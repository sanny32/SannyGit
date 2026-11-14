//! Destructive operations on exactly one reviewed path; never change the index.
use crate::{git, models::ChangeKind};
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

const SNAPSHOT_LIMIT: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Restore,
    DeleteUntracked,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Self::Restore => "Restore File",
            Self::DeleteUntracked => "Delete File",
        }
    }
    pub fn explanation(self) -> &'static str {
        match self {
            Self::Restore => "Replace this file's working copy with its index version. Unstaged edits are lost. Staged contents and other files are kept. Use Unstage separately to remove changes from the index.",
            Self::DeleteUntracked => "Delete this untracked file or symbolic link directly from disk, bypassing the Trash. The index does not protect its current contents. Staged contents and other paths are kept. A link's target is not deleted.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Contents {
    Missing,
    File(Vec<u8>),
    Link(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileState {
    contents: Contents,
    modified: Option<SystemTime>,
    readonly: bool,
    #[cfg(unix)]
    identity: (u64, u64, u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepared {
    pub path: String,
    pub action: Action,
    pub summary: String,
    head: Vec<u8>,
    index: Vec<u8>,
    status: Vec<u8>,
    file: FileState,
}

fn normal_directory(metadata: &std::fs::Metadata) -> bool {
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Also reject directory junctions and other reparse points, which are
        // not necessarily classified as symbolic links by FileType.
        const REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & REPARSE_POINT != 0 {
            return false;
        }
    }
    true
}

fn safe_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty()
        || name.contains('\0')
        || Path::new(name)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("Select one repository-relative file path.".into());
    }
    let path = root.join(name);
    let mut parent = root.to_path_buf();
    let components: Vec<_> = Path::new(name).components().collect();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        parent.push(component);
        match parent.symlink_metadata() {
            Ok(metadata) if normal_directory(&metadata) => {}
            Ok(_) => {
                return Err(
                    "A parent path is not a normal directory. Resolve it outside SannyGit.".into(),
                )
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Cannot inspect the file's parent directory: {error}"
                ))
            }
        }
    }
    Ok(path)
}

fn file_state(path: &Path) -> Result<FileState, String> {
    let metadata = match path.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FileState {
                contents: Contents::Missing,
                modified: None,
                readonly: false,
                #[cfg(unix)]
                identity: (0, 0, 0),
            })
        }
        Err(error) => return Err(format!("Cannot inspect selected file: {error}")),
    };
    let contents = if metadata.file_type().is_symlink() {
        Contents::Link(
            std::fs::read_link(path)
                .map_err(|error| format!("Cannot read symbolic link: {error}"))?,
        )
    } else if metadata.is_file() {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|error| format!("Cannot open selected file: {error}"))?
            .take(SNAPSHOT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Cannot read selected file: {error}"))?;
        if bytes.len() as u64 > SNAPSHOT_LIMIT {
            return Err(
                "Selected file exceeds the 2 MiB discard review limit. Handle it outside SannyGit."
                    .into(),
            );
        }
        Contents::File(bytes)
    } else {
        return Err(
            "Discard supports one regular file or symbolic link, not directories or submodules."
                .into(),
        );
    };
    Ok(FileState {
        contents,
        modified: metadata.modified().ok(),
        readonly: metadata.permissions().readonly(),
        #[cfg(unix)]
        identity: {
            use std::os::unix::fs::MetadataExt;
            (metadata.dev(), metadata.ino(), metadata.mode())
        },
    })
}

fn index_entry(root: &Path, path: &str) -> Result<Vec<u8>, String> {
    let entry = git::run(root, &["ls-files", "--stage", "-z", "--", path])?;
    if entry.is_empty() {
        return Ok(entry);
    }
    let record = entry.strip_suffix(&[0]).ok_or("Truncated index entry")?;
    let tab = record
        .iter()
        .position(|byte| *byte == b'\t')
        .ok_or("Invalid index entry")?;
    let header = &record[..tab];
    let name = &record[tab + 1..];
    let fields: Vec<_> = header.split(|byte| *byte == b' ').collect();
    if name != path.as_bytes() || fields.len() != 3 || fields[2] != b"0" {
        return Err("Select one non-conflicted file, not a directory.".into());
    }
    if !matches!(fields[0], b"100644" | b"100755" | b"120000") {
        return Err("Discarding submodules or unsupported index modes is not supported.".into());
    }
    Ok(entry)
}

pub fn prepare(root: &Path, path: &str) -> Result<Prepared, String> {
    let target = safe_path(root, path)?;
    git::require_no_operation(root)?;
    let snapshot = git::load(root)?;
    let changes: Vec<_> = snapshot
        .changes
        .iter()
        .filter(|change| change.path == path)
        .collect();
    if changes.is_empty() {
        return Err("This file no longer has changes. Refresh before retrying.".into());
    }
    if changes
        .iter()
        .any(|change| change.kind == ChangeKind::Conflicted)
    {
        return Err("Resolve conflicts outside SannyGit before discarding changes.".into());
    }
    let index = index_entry(root, path)?;
    let action = if index.is_empty()
        && changes
            .iter()
            .any(|change| change.kind == ChangeKind::Untracked)
    {
        Action::DeleteUntracked
    } else if !index.is_empty() && changes.iter().any(|change| change.unstaged) {
        Action::Restore
    } else {
        return Err("There are no supported unstaged changes for this path. Use Unstage separately for staged changes.".into());
    };
    let file = file_state(&target)?;
    if action == Action::DeleteUntracked && file.contents == Contents::Missing {
        return Err("The untracked file no longer exists. Refresh before retrying.".into());
    }
    let summary = match &file.contents {
        Contents::Missing => "Missing file; its index version will be restored.".into(),
        Contents::File(bytes) => format!("{} bytes in the current working copy.", bytes.len()),
        Contents::Link(target) => format!(
            "Symbolic link to {:?}; the target will not be followed.",
            target
        ),
    };
    Ok(Prepared {
        path: path.into(),
        action,
        summary,
        file,
        index,
        head: git::run(root, &["rev-parse", "--revs-only", "HEAD"])?,
        status: git::run(
            root,
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
                "--",
                path,
            ],
        )?,
    })
}

pub fn execute(root: &Path, expected: &Prepared) -> Result<(), String> {
    if prepare(root, &expected.path)? != *expected {
        return Err(
            "The file, index or HEAD changed after review. Reload details and confirm again."
                .into(),
        );
    }
    let target = safe_path(root, &expected.path)?;
    match expected.action {
        Action::Restore => {
            git::run(root, &["restore", "--worktree", "--no-recurse-submodules", "--", &expected.path])
            .map_err(|error| format!("File restoration did not complete: {error}. Inspect the refreshed file before retrying."))?;
        }
        Action::DeleteUntracked => {
            #[cfg(windows)]
            let result = {
                use std::os::windows::fs::FileTypeExt;
                let metadata = target
                    .symlink_metadata()
                    .map_err(|error| format!("Cannot inspect file before deletion: {error}"))?;
                if metadata.file_type().is_symlink_dir() {
                    std::fs::remove_dir(&target)
                } else {
                    std::fs::remove_file(&target)
                }
            };
            #[cfg(not(windows))]
            let result = std::fs::remove_file(&target);
            result.map_err(|error| format!("Could not delete the selected file: {error}"))?;
        }
    }
    if index_entry(root, &expected.path)? != expected.index {
        return Err("The file operation completed, but its index entry changed externally. Inspect the repository before retrying.".into());
    }
    Ok(())
}

/// Validate every reviewed path before starting; report partial completion on I/O failure.
pub fn execute_many(root: &Path, expected: &[Prepared]) -> Result<(), String> {
    if expected.is_empty() {
        return Err("Select files first.".into());
    }
    for entry in expected {
        if prepare(root, &entry.path)? != *entry {
            return Err(format!("{} changed after review. Reload details and confirm again. No files were discarded.", entry.path));
        }
    }
    for (completed, entry) in expected.iter().enumerate() {
        execute(root, entry).map_err(|error| {
            format!(
                "Discard stopped at {} after {completed} of {} files completed. {error}",
                entry.path,
                expected.len()
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{git::CommitContents, test_support::TempRepo};

    #[test]
    fn batch_preflights_every_path_and_preserves_index_and_unselected_files() {
        let repo = base();
        repo.write("file", "staged\n");
        repo.git(&["add", "file"]);
        repo.write("file", "unstaged\n");
        repo.write("new", "new\n");
        repo.write("other", "keep\n");
        let index = repo.git(&["ls-files", "--stage"]);
        let entries = [
            prepare(&repo.0, "file").unwrap(),
            prepare(&repo.0, "new").unwrap(),
        ];
        repo.write("new", "changed\n");
        assert!(execute_many(&repo.0, &entries).is_err());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "unstaged\n"
        );
        let entries = [
            prepare(&repo.0, "file").unwrap(),
            prepare(&repo.0, "new").unwrap(),
        ];
        execute_many(&repo.0, &entries).unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "staged\n"
        );
        assert!(!repo.0.join("new").exists());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("other")).unwrap(),
            "keep\n"
        );
        assert_eq!(repo.git(&["ls-files", "--stage"]), index);
    }

    fn base() -> TempRepo {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        repo.write("other", "other base\n");
        git::commit(&repo.0, "Base", CommitContents::All).unwrap();
        repo
    }

    #[test]
    fn review_is_read_only_and_restore_preserves_all_staged_and_unrelated_contents() {
        let repo = base();
        repo.write("file", "staged\n");
        repo.git(&["add", "file"]);
        repo.write("file", "unstaged\n");
        repo.write("other", "other staged\n");
        repo.git(&["add", "other"]);
        repo.write("other", "other unstaged\n");
        repo.write("new", "new contents\n");
        let index = repo.git(&["ls-files", "--stage"]);
        let head = repo.git(&["rev-parse", "HEAD"]);
        let prepared = prepare(&repo.0, "file").unwrap();
        assert_eq!(prepared.action, Action::Restore);
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
        assert_eq!(repo.git(&["ls-files", "--stage"]), index);
        execute(&repo.0, &prepared).unwrap();
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"staged\n");
        assert_eq!(
            std::fs::read(repo.0.join("other")).unwrap(),
            b"other unstaged\n"
        );
        assert_eq!(
            std::fs::read(repo.0.join("new")).unwrap(),
            b"new contents\n"
        );
        assert_eq!(repo.git(&["ls-files", "--stage"]), index);
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
        assert!(prepare(&repo.0, "file").unwrap_err().contains("Unstage"));
    }

    #[test]
    fn deleting_one_untracked_file_preserves_staged_deletion_and_other_files() {
        let repo = base();
        repo.git(&["rm", "file"]);
        repo.write("file", "recreated untracked\n");
        repo.write("new", "keep new\n");
        let index = repo.git(&["ls-files", "--stage"]);
        let prepared = prepare(&repo.0, "file").unwrap();
        assert_eq!(prepared.action, Action::DeleteUntracked);
        execute(&repo.0, &prepared).unwrap();
        assert!(!repo.0.join("file").exists());
        assert_eq!(repo.git(&["ls-files", "--stage"]), index);
        assert!(repo
            .git(&["diff", "--cached", "--name-status"])
            .starts_with("D\tfile"));
        assert_eq!(std::fs::read(repo.0.join("new")).unwrap(), b"keep new\n");
    }

    #[test]
    fn restore_handles_missing_files_staged_renames_and_unborn_index_versions() {
        let repo = base();
        repo.git(&["mv", "file", "renamed"]);
        repo.write("renamed", "unstaged rename edit\n");
        let index = repo.git(&["ls-files", "--stage"]);
        execute(&repo.0, &prepare(&repo.0, "renamed").unwrap()).unwrap();
        assert!(!repo.0.join("file").exists());
        assert_eq!(std::fs::read(repo.0.join("renamed")).unwrap(), b"base\n");
        assert_eq!(repo.git(&["ls-files", "--stage"]), index);
        std::fs::remove_file(repo.0.join("other")).unwrap();
        execute(&repo.0, &prepare(&repo.0, "other").unwrap()).unwrap();
        assert_eq!(
            std::fs::read(repo.0.join("other")).unwrap(),
            b"other base\n"
        );
        let unborn = TempRepo::new();
        unborn.write("new", "staged\n");
        unborn.git(&["add", "new"]);
        unborn.write("new", "unstaged\n");
        execute(&unborn.0, &prepare(&unborn.0, "new").unwrap()).unwrap();
        assert_eq!(std::fs::read(unborn.0.join("new")).unwrap(), b"staged\n");
    }

    #[test]
    fn unstaged_moves_are_handled_as_two_independent_paths() {
        let repo = base();
        std::fs::rename(repo.0.join("file"), repo.0.join("moved")).unwrap();
        execute(&repo.0, &prepare(&repo.0, "file").unwrap()).unwrap();
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"base\n");
        assert_eq!(std::fs::read(repo.0.join("moved")).unwrap(), b"base\n");
        execute(&repo.0, &prepare(&repo.0, "moved").unwrap()).unwrap();
        assert!(!repo.0.join("moved").exists());
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"base\n");
    }

    #[test]
    fn changes_after_review_are_rejected_without_overwriting_new_work() {
        for change in ["contents", "index", "head", "directory", "tracking"] {
            let repo = base();
            repo.write("file", "reviewed\n");
            if change == "tracking" {
                repo.write("untracked", "reviewed\n");
            }
            let path = if change == "tracking" {
                "untracked"
            } else {
                "file"
            };
            let prepared = prepare(&repo.0, path).unwrap();
            match change {
                "contents" => repo.write("file", "new work\n"),
                "index" => {
                    repo.git(&["add", "file"]);
                }
                "head" => {
                    repo.git(&["commit", "--allow-empty", "-m", "External"]);
                }
                "directory" => {
                    std::fs::remove_file(repo.0.join("file")).unwrap();
                    std::fs::create_dir(repo.0.join("file")).unwrap();
                    repo.write("file/child", "keep child\n");
                }
                _ => {
                    repo.git(&["add", "untracked"]);
                }
            }
            let index = repo.git(&["ls-files", "--stage"]);
            let status = repo.git(&["status", "--porcelain"]);
            assert!(
                execute(&repo.0, &prepared).is_err(),
                "Failed to reject {change}"
            );
            assert_eq!(repo.git(&["ls-files", "--stage"]), index);
            assert_eq!(repo.git(&["status", "--porcelain"]), status);
            if change == "contents" {
                assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"new work\n");
            }
            if change == "directory" {
                assert_eq!(
                    std::fs::read(repo.0.join("file/child")).unwrap(),
                    b"keep child\n"
                );
            }
        }
    }

    #[test]
    fn binary_contents_and_literal_pathspecs_restore_only_the_selected_file() {
        let repo = base();
        std::fs::write(repo.0.join("file[ab].bin"), [0, 255, 1]).unwrap();
        repo.write("filea.bin", "base\n");
        git::commit(&repo.0, "Binary", CommitContents::All).unwrap();
        std::fs::write(repo.0.join("file[ab].bin"), [0, 255, 2]).unwrap();
        repo.write("filea.bin", "keep changed\n");
        execute(&repo.0, &prepare(&repo.0, "file[ab].bin").unwrap()).unwrap();
        assert_eq!(
            std::fs::read(repo.0.join("file[ab].bin")).unwrap(),
            [0, 255, 1]
        );
        assert_eq!(
            std::fs::read(repo.0.join("filea.bin")).unwrap(),
            b"keep changed\n"
        );
    }

    #[test]
    fn operation_markers_unsafe_paths_and_large_files_block_destructive_actions() {
        let repo = base();
        repo.write("file", "keep modified\n");
        for path in ["", "../file", ".git/../file", "/file", "file/child"] {
            assert!(prepare(&repo.0, path).is_err());
        }
        for marker in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
            "sequencer",
        ] {
            let prepared = prepare(&repo.0, "file").unwrap();
            repo.write(&format!(".git/{marker}"), "busy");
            assert!(execute(&repo.0, &prepared).unwrap_err().contains("Finish"));
            std::fs::remove_file(repo.0.join(".git").join(marker)).unwrap();
        }
        std::fs::write(repo.0.join("large"), vec![0; SNAPSHOT_LIMIT as usize + 1]).unwrap();
        assert!(prepare(&repo.0, "large").unwrap_err().contains("2 MiB"));
        assert!(repo.0.join("large").exists());
        assert_eq!(
            std::fs::read(repo.0.join("file")).unwrap(),
            b"keep modified\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_restore_and_deletion_never_touch_link_targets() {
        use std::os::unix::fs::symlink;
        let repo = base();
        let outside = TempRepo::new();
        outside.write("first", "first target\n");
        outside.write("second", "second target\n");
        symlink(outside.0.join("first"), repo.0.join("link")).unwrap();
        git::commit(&repo.0, "Link", CommitContents::All).unwrap();
        std::fs::remove_file(repo.0.join("link")).unwrap();
        symlink(outside.0.join("second"), repo.0.join("link")).unwrap();
        execute(&repo.0, &prepare(&repo.0, "link").unwrap()).unwrap();
        assert_eq!(
            std::fs::read_link(repo.0.join("link")).unwrap(),
            outside.0.join("first")
        );
        std::fs::remove_file(repo.0.join("file")).unwrap();
        symlink(outside.0.join("second"), repo.0.join("file")).unwrap();
        execute(&repo.0, &prepare(&repo.0, "file").unwrap()).unwrap();
        assert!(!repo
            .0
            .join("file")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"base\n");
        symlink(&outside.0, repo.0.join("directory-link")).unwrap();
        execute(&repo.0, &prepare(&repo.0, "directory-link").unwrap()).unwrap();
        assert!(repo.0.join("directory-link").symlink_metadata().is_err());
        assert_eq!(
            std::fs::read(outside.0.join("first")).unwrap(),
            b"first target\n"
        );
        assert_eq!(
            std::fs::read(outside.0.join("second")).unwrap(),
            b"second target\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_parent_directory_replaced_by_a_link_blocks_deletion() {
        use std::os::unix::fs::symlink;
        let repo = base();
        let outside = TempRepo::new();
        outside.write("file", "outside\n");
        std::fs::create_dir(repo.0.join("dir")).unwrap();
        repo.write("dir/file", "reviewed\n");
        let prepared = prepare(&repo.0, "dir/file").unwrap();
        std::fs::remove_file(repo.0.join("dir/file")).unwrap();
        std::fs::remove_dir(repo.0.join("dir")).unwrap();
        symlink(&outside.0, repo.0.join("dir")).unwrap();
        assert!(execute(&repo.0, &prepared)
            .unwrap_err()
            .contains("parent path"));
        assert_eq!(std::fs::read(outside.0.join("file")).unwrap(), b"outside\n");
    }
}
