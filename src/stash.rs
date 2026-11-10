//! Stash commands and bounded, machine-readable output. Background workers only.
use crate::{git, models::ChangeKind};
use std::path::{Component, Path};

const LIMIT: usize = 1_000;
const PATCH_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub oid: String,
    pub message: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SavePreview {
    pub include_untracked: bool,
    pub patch: String,
    head: String,
    status: Vec<u8>,
    index: Vec<u8>,
    untracked: Vec<(String, Vec<u8>)>,
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn list(root: &Path) -> Result<Vec<Entry>, String> {
    parse_list(&git::run(
        root,
        &["stash", "list", "-z", "-n", "1001", "--format=%H%x00%gs"],
    )?)
}

fn parse_list(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = bytes.strip_suffix(&[0]).ok_or("Truncated stash list")?;
    let fields: Vec<_> = bytes.split(|byte| *byte == 0).collect();
    if fields.len() % 2 != 0 {
        return Err("Malformed stash list".into());
    }
    if fields.len() / 2 > LIMIT {
        return Err("Stash list exceeds the 1,000 entry limit".into());
    }
    fields
        .as_chunks::<2>()
        .0
        .iter()
        .map(|fields| {
            let oid = git::text(fields[0].to_vec())?;
            if !valid_oid(&oid) {
                return Err("Invalid stash object ID".into());
            }
            Ok(Entry {
                oid,
                message: git::text(fields[1].to_vec())?,
            })
        })
        .collect()
}

fn diff(root: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = vec![
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--binary",
        "--full-index",
    ];
    command.extend_from_slice(args);
    git::text(git::run(root, &command)?)
}

pub fn preview_save(root: &Path, include_untracked: bool) -> Result<SavePreview, String> {
    git::require_no_operation(root)?;
    let snapshot = git::load(root)?;
    if snapshot
        .changes
        .iter()
        .any(|change| change.kind == ChangeKind::Conflicted)
    {
        return Err("Resolve conflicts outside SannyGit before saving a stash.".into());
    }
    if !snapshot
        .changes
        .iter()
        .any(|change| include_untracked || change.kind != ChangeKind::Untracked)
    {
        return Err("There are no selected changes to stash.".into());
    }
    let head = git::text(
        git::run(root, &["rev-parse", "--verify", "HEAD^{commit}"])
            .map_err(|_| "Create the first commit before using stash.".to_owned())?,
    )?;
    let status = git::run(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let index = diff(root, &["--cached", head.trim(), "--"])?.into_bytes();
    let mut patch = format!(
        "Staged changes\n{}\nUnstaged changes\n{}",
        String::from_utf8_lossy(&index),
        diff(root, &["--"])?
    );
    let mut untracked = Vec::new();
    if include_untracked {
        for change in snapshot
            .changes
            .iter()
            .filter(|change| change.kind == ChangeKind::Untracked)
        {
            let file = root.join(&change.path);
            let metadata = file
                .symlink_metadata()
                .map_err(|error| format!("Cannot inspect {}: {error}", change.path))?;
            let identity = if metadata.file_type().is_symlink() {
                let target = std::fs::read_link(&file).map_err(|error| {
                    format!("Cannot read symbolic link {}: {error}", change.path)
                })?;
                let target = target
                    .to_str()
                    .ok_or("Non-UTF-8 symbolic link targets are not supported")?;
                [b"link\0".as_slice(), target.as_bytes()].concat()
            } else if metadata.is_file() {
                let oid = git::run(root, &["hash-object", "--no-filters", "--", &change.path])?;
                [b"file\0".as_slice(), &oid].concat()
            } else {
                return Err(format!("Unsupported untracked file type: {}", change.path));
            };
            #[cfg(unix)]
            let identity = {
                use std::os::unix::fs::PermissionsExt;
                let mut identity = identity;
                identity.extend_from_slice(&(metadata.permissions().mode() & 0o111).to_le_bytes());
                identity
            };
            patch.push_str(&format!("\nNew file: {}\n", change.path));
            patch.push_str(&git::diff(root, &change.path, change.kind, None)?);
            untracked.push((change.path.clone(), identity));
            if patch.len() > PATCH_LIMIT {
                return Err("Stash preview exceeds the 2 MiB limit".into());
            }
        }
    }
    if patch.len() > PATCH_LIMIT {
        return Err("Stash preview exceeds the 2 MiB limit".into());
    }
    Ok(SavePreview {
        include_untracked,
        patch,
        head,
        status,
        index,
        untracked,
    })
}

pub fn save(root: &Path, message: &str, expected: &SavePreview) -> Result<(), String> {
    if message.contains('\0') || message.len() > 4_096 {
        return Err("Use a stash message without NUL characters and no larger than 4 KiB.".into());
    }
    if preview_save(root, expected.include_untracked)? != *expected {
        return Err("Changes have changed since the preview. Preview again before saving.".into());
    }
    let before = list(root)?;
    let message = if message.trim().is_empty() {
        "SannyGit stash"
    } else {
        message
    };
    let mut args = vec!["push", "--message", message];
    if expected.include_untracked {
        args.push("--include-untracked");
    }
    args.push("--");
    git::run_stash(root, &args).map_err(|error| format!("Stash save did not complete: {error}. Inspect the stash list and working tree before retrying; some changes may already be stored."))?;
    let after = list(root).map_err(|error| format!("Stash save completed, but its entry could not be verified: {error}. Reload before retrying."))?;
    if after.len() != before.len() + 1 || after.get(1..) != Some(before.as_slice()) {
        return Err("Stash save completed, but the stash list changed unexpectedly. Inspect the list and working tree before retrying.".into());
    }
    let remaining = git::load(root).map_err(|error| format!("Stash saved, but the working tree could not be checked: {error}. Inspect it before retrying."))?;
    if remaining
        .changes
        .iter()
        .any(|change| expected.include_untracked || change.kind != ChangeKind::Untracked)
    {
        return Err("Stash saved, but some selected changes remain in the working tree. Inspect the stash and local files before retrying.".into());
    }
    Ok(())
}

fn parents(root: &Path, entry: &Entry) -> Result<Vec<String>, String> {
    if !valid_oid(&entry.oid) {
        return Err("Invalid stash object ID".into());
    }
    let output = git::text(git::run(
        root,
        &["rev-list", "--parents", "-n", "1", &entry.oid, "--"],
    )?)?;
    let fields: Vec<_> = output.split_whitespace().map(str::to_owned).collect();
    if !(fields.len() == 3 || fields.len() == 4)
        || fields.first() != Some(&entry.oid)
        || fields.iter().any(|oid| !valid_oid(oid))
    {
        return Err(
            "The selected object is not a supported stash entry. Reload the stash list.".into(),
        );
    }
    Ok(fields)
}

pub fn preview_entry(root: &Path, entry: &Entry) -> Result<String, String> {
    let parents = parents(root, entry)?;
    let mut patch = format!(
        "Staged changes\n{}\nUnstaged changes\n{}",
        diff(root, &[&parents[1], &parents[2], "--"])?,
        diff(root, &[&parents[2], &entry.oid, "--"])?
    );
    if let Some(untracked) = parents.get(3) {
        let empty = git::text(git::run_with_input(
            root,
            &["hash-object", "-t", "tree", "--stdin"],
            Some(b""),
        )?)?;
        patch.push_str(&format!(
            "\nUntracked files\n{}",
            diff(root, &[empty.trim(), untracked, "--"])?
        ));
    }
    if patch.len() > PATCH_LIMIT {
        return Err("Stash preview exceeds the 2 MiB limit".into());
    }
    Ok(patch)
}

fn check_list<'a>(root: &Path, expected: &'a [Entry], index: usize) -> Result<&'a Entry, String> {
    let entry = expected.get(index).ok_or("Select a stash entry first.")?;
    if list(root)? != expected {
        return Err("The stash list has changed. Reload and select the entry again.".into());
    }
    Ok(entry)
}

pub fn drop_entry(root: &Path, expected: &[Entry], index: usize) -> Result<(), String> {
    git::require_no_operation(root)?;
    check_list(root, expected, index)?;
    git::run(root, &["stash", "drop", &format!("stash@{{{index}}}")]).map(|_| ())
}

pub fn apply(
    root: &Path,
    expected: &[Entry],
    index: usize,
    restore_index: bool,
    pop: bool,
) -> Result<(), String> {
    git::require_no_operation(root)?;
    let entry = check_list(root, expected, index)?;
    if !git::load(root)?.changes.is_empty() {
        return Err("Commit or move aside local changes before applying a stash.".into());
    }
    // Git may overwrite ignored paths. Check every affected path absent from the
    // current index, including ignored files and symlink ancestors, before apply.
    let parents = parents(root, entry)?;
    let mut paths = git::run(
        root,
        &[
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            &parents[1],
            &entry.oid,
            "--",
        ],
    )?;
    if let Some(untracked) = parents.get(3) {
        paths.extend(git::run(
            root,
            &["ls-tree", "-r", "--name-only", "-z", untracked, "--"],
        )?);
    }
    let tracked = git::run(root, &["ls-files", "-z"])?;
    for path in paths
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let name =
            std::str::from_utf8(path).map_err(|_| "Non-UTF-8 stash paths are not supported")?;
        if Path::new(name)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err("Unsupported path in stash entry".into());
        }
        if !tracked
            .split(|byte| *byte == 0)
            .any(|tracked| tracked == path)
        {
            let mut target = root.to_path_buf();
            for component in Path::new(name).components() {
                target.push(component);
                match target.symlink_metadata() {
                    Ok(metadata) if metadata.file_type().is_symlink() || target == root.join(name) => return Err(format!("Stash would overwrite an existing path: {name}. Move it aside before applying.")),
                    Ok(_) => {},
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                    Err(error) => return Err(format!("Cannot check stash destination {name}: {error}")),
                }
            }
        }
    }
    let mut args = vec!["apply"];
    if restore_index {
        args.push("--index");
    }
    args.push(&entry.oid);
    git::run_stash(root, &args).map_err(|error| format!("Stash apply did not complete: {error}. The stash entry is retained. Inspect the working tree for conflicts or partially applied changes before retrying."))?;
    if pop {
        drop_entry(root, expected, index).map_err(|error| format!("Stash applied, but its entry was retained: {error}. Do not apply it again; inspect the working tree and list."))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{tests::TempRepo, CommitContents};

    fn base() -> TempRepo {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        git::commit(&repo.0, "Base", CommitContents::All).unwrap();
        repo
    }

    #[test]
    fn stash_list_parser_rejects_truncated_invalid_non_utf8_and_oversized_records() {
        assert!(parse_list(b"").unwrap().is_empty());
        let oid = "a".repeat(40);
        assert_eq!(
            parse_list(format!("{oid}\0message\0").as_bytes()).unwrap()[0].message,
            "message"
        );
        for bytes in [
            b"hash\0message\0".to_vec(),
            format!("{oid}\0message").into_bytes(),
            format!("{oid}\0").into_bytes(),
            [format!("{oid}\0").as_bytes(), &[255, 0]].concat(),
            format!("{oid}\0message\0").repeat(1001).into_bytes(),
        ] {
            assert!(parse_list(&bytes).is_err());
        }
    }

    #[test]
    fn save_preview_and_apply_preserve_staged_unstaged_and_selected_untracked_files() {
        for include in [false, true] {
            let repo = base();
            repo.write("file", "staged\n");
            repo.git(&["add", "file"]);
            repo.write("file", "unstaged\n");
            repo.write("new file", "new contents\n");
            let head = repo.git(&["rev-parse", "HEAD"]);
            let index = repo.git(&["ls-files", "--stage"]);
            let preview = preview_save(&repo.0, include).unwrap();
            assert!(preview.patch.contains("+staged"));
            assert!(preview.patch.contains("+unstaged"));
            assert_eq!(preview.patch.contains("new contents"), include);
            assert_eq!(repo.git(&["ls-files", "--stage"]), index);
            assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
            save(&repo.0, "Work in progress", &preview).unwrap();
            let entries = list(&repo.0).unwrap();
            assert_eq!(entries.len(), 1);
            assert!(entries[0].message.contains("Work in progress"));
            for _ in 0..3 {
                assert_eq!(list(&repo.0).unwrap(), entries);
            }
            assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
            assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"base\n");
            assert_eq!(repo.0.join("new file").exists(), !include);
            let patch = preview_entry(&repo.0, &entries[0]).unwrap();
            assert!(patch.contains("+staged") && patch.contains("+unstaged"));
            assert_eq!(patch.contains("new contents"), include);
            if !include {
                std::fs::remove_file(repo.0.join("new file")).unwrap();
            }
            apply(&repo.0, &entries, 0, true, false).unwrap();
            assert_eq!(repo.git(&["ls-files", "--stage"]), index);
            assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
            assert_eq!(repo.0.join("new file").exists(), include);
            assert_eq!(list(&repo.0).unwrap(), entries);
            assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
            assert!(apply(&repo.0, &entries, 0, true, true)
                .unwrap_err()
                .contains("local changes"));
            assert_eq!(list(&repo.0).unwrap(), entries);
        }
    }

    #[test]
    fn stale_save_preview_rejects_content_changes_including_binary_new_files() {
        let repo = base();
        repo.write("file", "modified\n");
        repo.write("binary", "\0first");
        let preview = preview_save(&repo.0, true).unwrap();
        repo.write("binary", "\0second");
        assert!(save(&repo.0, "Stale", &preview)
            .unwrap_err()
            .contains("Preview again"));
        assert!(list(&repo.0).unwrap().is_empty());
        assert!(repo.git(&["diff", "--cached", "--name-only"]).is_empty());
        let preview = preview_save(&repo.0, true).unwrap();
        repo.git(&["add", "file"]);
        assert!(save(&repo.0, "Stale index", &preview).is_err());
        assert!(list(&repo.0).unwrap().is_empty());
        assert_eq!(std::fs::read(repo.0.join("binary")).unwrap(), b"\0second");
    }

    #[test]
    fn save_rejects_unborn_empty_and_in_progress_repositories_without_writes() {
        let unborn = TempRepo::new();
        unborn.write("new", "contents");
        assert!(preview_save(&unborn.0, true).is_err());
        let repo = base();
        assert!(preview_save(&repo.0, true).is_err());
        repo.write("new", "contents");
        assert!(preview_save(&repo.0, false).is_err());
        let preview = preview_save(&repo.0, true).unwrap();
        std::fs::create_dir(repo.0.join(".git/rebase-merge")).unwrap();
        assert!(save(&repo.0, "Blocked", &preview)
            .unwrap_err()
            .contains("Finish"));
        assert!(repo.git(&["diff", "--cached", "--name-only"]).is_empty());
        assert!(list(&repo.0).unwrap().is_empty());
    }

    #[test]
    fn successful_pop_restores_rename_deletion_and_untracked_files_then_drops_only_selected_entry()
    {
        let repo = base();
        repo.write("deleted", "remove me\n");
        git::commit(&repo.0, "More files", CommitContents::All).unwrap();
        repo.write("file", "older stash\n");
        save(&repo.0, "Older", &preview_save(&repo.0, false).unwrap()).unwrap();
        repo.git(&["mv", "file", "renamed"]);
        std::fs::remove_file(repo.0.join("deleted")).unwrap();
        repo.write("new", "untracked\n");
        let index = repo.git(&["ls-files", "--stage"]);
        save(&repo.0, "Newest", &preview_save(&repo.0, true).unwrap()).unwrap();
        let entries = list(&repo.0).unwrap();
        assert_eq!(entries.len(), 2);
        apply(&repo.0, &entries, 0, true, true).unwrap();
        assert_eq!(list(&repo.0).unwrap(), entries[1..]);
        assert_eq!(repo.git(&["ls-files", "--stage"]), index);
        assert!(!repo.0.join("file").exists() && !repo.0.join("deleted").exists());
        assert_eq!(std::fs::read(repo.0.join("renamed")).unwrap(), b"base\n");
        assert_eq!(std::fs::read(repo.0.join("new")).unwrap(), b"untracked\n");
    }

    #[test]
    fn conflicting_apply_and_pop_retain_stash_and_do_not_report_success() {
        for pop in [false, true] {
            let repo = base();
            repo.write("file", "stashed change\n");
            save(&repo.0, "Conflict", &preview_save(&repo.0, false).unwrap()).unwrap();
            let entries = list(&repo.0).unwrap();
            repo.write("file", "different commit\n");
            git::commit(&repo.0, "Diverged", CommitContents::All).unwrap();
            let head = repo.git(&["rev-parse", "HEAD"]);
            let error = apply(&repo.0, &entries, 0, false, pop).unwrap_err();
            assert!(error.contains("retained") && error.contains("conflicts"));
            assert_eq!(list(&repo.0).unwrap(), entries);
            assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
            assert!(git::load(&repo.0)
                .unwrap()
                .changes
                .iter()
                .any(|change| change.kind == ChangeKind::Conflicted));
        }
    }

    #[test]
    fn drop_rejects_stale_selection_and_preserves_worktree_and_other_entries() {
        let repo = base();
        repo.write("file", "first\n");
        save(&repo.0, "First", &preview_save(&repo.0, false).unwrap()).unwrap();
        let old = list(&repo.0).unwrap();
        repo.write("file", "second\n");
        save(&repo.0, "Second", &preview_save(&repo.0, false).unwrap()).unwrap();
        let current = list(&repo.0).unwrap();
        assert!(drop_entry(&repo.0, &old, 0)
            .unwrap_err()
            .contains("list has changed"));
        assert!(apply(&repo.0, &old, 0, true, true).is_err());
        repo.write("file", "local changes remain\n");
        drop_entry(&repo.0, &current, 1).unwrap();
        assert_eq!(list(&repo.0).unwrap(), current[..1]);
        assert_eq!(
            std::fs::read(repo.0.join("file")).unwrap(),
            b"local changes remain\n"
        );
        assert!(repo.git(&["diff", "--cached", "--name-only"]).is_empty());
    }

    #[test]
    fn ignored_destinations_are_never_overwritten_by_apply_or_pop() {
        let repo = base();
        repo.write("new", "stashed contents\n");
        save(&repo.0, "Untracked", &preview_save(&repo.0, true).unwrap()).unwrap();
        repo.write(".git/info/exclude", "new\n");
        repo.write("new", "private ignored contents\n");
        let entries = list(&repo.0).unwrap();
        assert!(git::load(&repo.0).unwrap().changes.is_empty());
        for pop in [false, true] {
            assert!(apply(&repo.0, &entries, 0, true, pop)
                .unwrap_err()
                .contains("overwrite"));
            assert_eq!(
                std::fs::read(repo.0.join("new")).unwrap(),
                b"private ignored contents\n"
            );
            assert_eq!(list(&repo.0).unwrap(), entries);
        }
    }

    #[test]
    fn applying_without_index_restoration_keeps_tracked_contents_unstaged() {
        let repo = base();
        repo.write("file", "staged\n");
        repo.git(&["add", "file"]);
        repo.write("file", "unstaged\n");
        save(
            &repo.0,
            "Unstaged apply",
            &preview_save(&repo.0, false).unwrap(),
        )
        .unwrap();
        let entries = list(&repo.0).unwrap();
        apply(&repo.0, &entries, 0, false, false).unwrap();
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
        assert!(repo.git(&["diff", "--cached", "--name-only"]).is_empty());
        assert_eq!(list(&repo.0).unwrap(), entries);
    }

    #[test]
    fn save_rejects_stale_head_and_locked_index_without_losing_files() {
        let repo = base();
        repo.write("file", "keep modified\n");
        let stale = preview_save(&repo.0, false).unwrap();
        repo.git(&["commit", "--allow-empty", "-m", "External HEAD"]);
        assert!(save(&repo.0, "Stale HEAD", &stale).is_err());
        let preview = preview_save(&repo.0, false).unwrap();
        repo.write(".git/index.lock", "busy");
        assert!(save(&repo.0, "Locked index", &preview).is_err());
        assert!(list(&repo.0).unwrap().is_empty());
        assert_eq!(
            std::fs::read(repo.0.join("file")).unwrap(),
            b"keep modified\n"
        );
        assert!(repo.git(&["diff", "--cached", "--name-only"]).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_previewed_without_following_targets_and_restore_as_links() {
        use std::os::unix::fs::symlink;
        let repo = base();
        symlink("missing-target", repo.0.join("link")).unwrap();
        let preview = preview_save(&repo.0, true).unwrap();
        save(&repo.0, "Link", &preview).unwrap();
        let entries = list(&repo.0).unwrap();
        assert!(repo.0.join("link").symlink_metadata().is_err());
        apply(&repo.0, &entries, 0, true, true).unwrap();
        assert_eq!(
            std::fs::read_link(repo.0.join("link")).unwrap(),
            Path::new("missing-target")
        );
        assert!(list(&repo.0).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn successful_git_exit_with_incomplete_cleanup_is_reported_as_partial_save() {
        std::thread::spawn(|| {
            use std::os::unix::fs::PermissionsExt;
            let repo = base();
            repo.write("file", "modified\n");
            repo.write("new", "new\n");
            let preview = preview_save(&repo.0, true).unwrap();
            let wrapper = repo.0.join(".git/literal-git");
            std::fs::write(
                &wrapper,
                "#!/bin/sh\nGIT_LITERAL_PATHSPECS=1 exec git \"$@\"\n",
            )
            .unwrap();
            std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
            git::set_executable(wrapper.to_string_lossy().into_owned());
            let error = save(&repo.0, "Partial cleanup", &preview).unwrap_err();
            assert!(error.contains("Stash saved") && error.contains("changes remain"));
            assert_eq!(list(&repo.0).unwrap().len(), 1);
            assert_eq!(std::fs::read(repo.0.join("new")).unwrap(), b"new\n");
            assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"base\n");
        })
        .join()
        .unwrap();
    }
}
