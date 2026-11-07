//! Git command execution and parsing. Called only by the application's background workers.
use crate::models::{ChangeFile, ChangeKind, Repository};
use std::{
    fmt::Write,
    io::{Read, Write as IoWrite},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 2 * 1024 * 1024;
const FILE_LIMIT: usize = 5_000;
const TIMEOUT: Duration = Duration::from_secs(15);

pub struct Snapshot {
    pub repository: Repository,
    pub branch: String,
    pub changes: Vec<ChangeFile>,
}

fn capture(mut reader: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        // Continue draining the pipe so Git cannot block on a full pipe.
        let remaining = (OUTPUT_LIMIT + 1).saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    Ok(bytes)
}

fn run(path: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    run_with_input(path, args, None)
}

fn run_with_input(path: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let mut command = Command::new("git");
    command
        .current_dir(path)
        .args(["--no-pager", "--no-optional-locks", "-c", "color.ui=false"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start Git: {e}"))?;
    let stdout = child.stdout.take().ok_or("Cannot read Git output")?;
    let stderr = child.stderr.take().ok_or("Cannot read Git errors")?;
    let stdin = child.stdin.take();
    let result = thread::scope(|scope| {
        let writer = scope.spawn(move || -> Result<(), String> {
            if let (Some(mut pipe), Some(bytes)) = (stdin, input) {
                pipe.write_all(bytes)
                    .map_err(|e| format!("Cannot send commit message to Git: {e}"))?;
            }
            Ok(())
        });
        let out = scope.spawn(|| capture(stdout));
        let err = scope.spawn(|| capture(stderr));
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if start.elapsed() < TIMEOUT => thread::sleep(Duration::from_millis(20)),
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(match result {
                        Err(e) => format!("Cannot wait for Git: {e}"),
                        _ => "Git operation timed out after 15 seconds".into(),
                    });
                }
            }
        };
        let output = out.join().map_err(|_| "Git output reader failed")??;
        let errors = err.join().map_err(|_| "Git error reader failed")??;
        let written = writer.join().map_err(|_| "Git input writer failed")?;
        if !status?.success() {
            return Err(command_error(args.first().copied().unwrap_or("command")));
        }
        written?;
        if output.len() > OUTPUT_LIMIT {
            return Err("Git output exceeds the 2 MiB display limit".into());
        }
        drop(errors);
        Ok(output)
    });
    result
}

fn command_error(operation: &str) -> String {
    // Do not parse localized diagnostics or echo private URLs and hook output.
    let reason = if operation == "commit" {
        "Check your Git identity, signing configuration and commit hooks. Refresh before retrying."
    } else if matches!(operation, "add" | "restore" | "update-index") {
        "Check whether the index is locked by another Git operation and verify repository permissions. Refresh before retrying."
    } else {
        "Check the repository path, permissions and ownership settings. Refresh before retrying."
    };
    format!("Git {operation} failed. {reason}")
}

/// Stages or unstages the complete selected file; never changes working-tree bytes.
pub fn set_staged(root: &Path, path: &str, staged: bool) -> Result<(), String> {
    let snapshot = load(root)?;
    let change = snapshot
        .changes
        .iter()
        .find(|c| c.path == path)
        .ok_or("The file has changed since the last refresh. Refresh and select it again.")?;
    if change.kind == ChangeKind::Conflicted {
        return Err("Resolve conflicts outside SannyGit before staging this file.".into());
    }
    if (staged && !change.unstaged) || (!staged && !change.staged) {
        return Err("The file's staging state has changed. Refresh before retrying.".into());
    }
    // --revs-only succeeds with empty output for an unborn HEAD; other command
    // failures must propagate rather than being mistaken for a new repository.
    let has_head = !staged && !run(root, &["rev-parse", "--revs-only", "HEAD"])?.is_empty();
    let mut args = if staged {
        vec!["add", "--all", "--", path]
    } else if has_head {
        vec!["restore", "--source=HEAD", "--staged", "--", path]
    } else {
        // Before the first commit there is no HEAD to restore. Remove only index entries.
        vec!["update-index", "--force-remove", "--", path]
    };
    if let Some(source) = &change.original_path {
        // A copy must not stage or unstage its source; porcelain R is the only
        // status currently displayed as Renamed.
        if !staged && change.kind == ChangeKind::Renamed {
            args.push(source);
        }
    }
    run(root, &args).map(|_| ())
}

pub fn validate_message(message: &str) -> Result<(), String> {
    if message.trim().is_empty() {
        return Err("Enter a commit message.".into());
    }
    if message.contains('\0') {
        return Err("Commit messages cannot contain NUL characters.".into());
    }
    if message.len() > 65_536 {
        return Err("Commit message exceeds the 64 KiB limit.".into());
    }
    Ok(())
}

/// Commits only the existing index, after an explicit Commit action in the UI.
pub fn commit(root: &Path, message: &str) -> Result<(), String> {
    validate_message(message)?;
    let snapshot = load(root)?;
    if snapshot
        .changes
        .iter()
        .any(|c| c.kind == ChangeKind::Conflicted)
    {
        return Err("Resolve all conflicts before committing.".into());
    }
    if !snapshot.changes.iter().any(|c| c.staged) {
        return Err("Stage at least one change before committing.".into());
    }
    // stdin preserves multiline messages without a shell or a message file.
    // Git's configured identity, hooks and signing remain in effect.
    run_with_input(
        root,
        &["commit", "--file=-", "--cleanup=whitespace"],
        Some(message.as_bytes()),
    )
    .map(|_| ())
}

fn text(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| "Non-UTF-8 Git output is not supported yet".into())
}

pub fn load(path: &Path) -> Result<Snapshot, String> {
    let root = text(run(path, &["rev-parse", "--show-toplevel"])?)?;
    // Git appends exactly one line ending; preserve whitespace in directory names.
    let root = root.strip_suffix('\n').unwrap_or(&root);
    let path = PathBuf::from(root);
    let branch = run(&path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .and_then(text)
        .map(|value| value.trim().to_owned())
        .or_else(|_| {
            text(run(&path, &["symbolic-ref", "--short", "HEAD"])?).map(|s| s.trim().to_owned())
        })?;
    let branch = if branch == "HEAD" {
        format!(
            "Detached at {}",
            text(run(&path, &["rev-parse", "--short", "HEAD"])?)?.trim()
        )
    } else {
        branch
    };
    let changes = parse_status(&run(
        &path,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?)?;
    Ok(Snapshot {
        repository: Repository {
            name: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.into()),
            path: root.into(),
        },
        branch,
        changes,
    })
}

pub fn parse_status(bytes: &[u8]) -> Result<Vec<ChangeFile>, String> {
    if !bytes.is_empty() && bytes.last() != Some(&0) {
        return Err("Truncated Git status output".into());
    }
    let mut records = bytes.split(|b| *b == 0);
    let mut changes = Vec::new();
    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        if record.len() < 4 || record[2] != b' ' {
            return Err("Malformed Git status record".into());
        }
        let x = record[0];
        let y = record[1];
        let valid = |value| b" MADRCU?!T".contains(&value);
        if !valid(x) || !valid(y) {
            return Err("Unknown Git status code".into());
        }
        if x == b'!' && y == b'!' {
            continue;
        }
        let kind = if x == b'?' && y == b'?' {
            ChangeKind::Untracked
        } else if x == b'U' || y == b'U' || matches!([x, y], [b'A', b'A'] | [b'D', b'D']) {
            ChangeKind::Conflicted
        } else if x == b'R' || y == b'R' {
            ChangeKind::Renamed
        } else if x == b'D' || y == b'D' {
            ChangeKind::Deleted
        } else if x == b'A' || y == b'A' || x == b'C' {
            ChangeKind::Added
        } else {
            ChangeKind::Modified
        };
        let path = std::str::from_utf8(&record[3..])
            .map_err(|_| "Non-UTF-8 file names are not supported yet")?
            .to_owned();
        let original_path = if x == b'R' || y == b'R' || x == b'C' || y == b'C' {
            let source = records
                .next()
                .filter(|source| !source.is_empty())
                .ok_or("Missing source path in Git rename record")?;
            Some(
                std::str::from_utf8(source)
                    .map_err(|_| "Non-UTF-8 file names are not supported yet")?
                    .to_owned(),
            )
        } else {
            None
        };
        changes.push(ChangeFile {
            path,
            original_path,
            kind,
            staged: kind != ChangeKind::Conflicted && x != b' ' && x != b'?',
            unstaged: y != b' ',
            diff: String::new(),
        });
        if changes.len() > FILE_LIMIT {
            return Err("Repository exceeds the 5,000 changed-file display limit".into());
        }
    }
    Ok(changes)
}

pub fn diff(
    root: &Path,
    path: &str,
    kind: ChangeKind,
    original_path: Option<&str>,
) -> Result<String, String> {
    if kind == ChangeKind::Untracked {
        let file = root.join(path);
        if !std::fs::symlink_metadata(&file)
            .map_err(|e| e.to_string())?
            .is_file()
        {
            return Ok("Preview is unavailable for directories and symbolic links".into());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(file)
            .map_err(|e| e.to_string())?
            .take((OUTPUT_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > OUTPUT_LIMIT {
            return Err("File exceeds the 2 MiB preview limit".into());
        }
        if bytes.contains(&0) {
            return Ok("Binary file — text preview unavailable".into());
        }
        let content = text(bytes)?;
        let mut patch = format!(
            "Untracked file\n--- /dev/null\n+++ {path}\n@@ -0,0 +1,{} @@\n",
            content.lines().count()
        );
        for line in content.lines() {
            writeln!(patch, "+{line}").map_err(|e| e.to_string())?;
        }
        return limit_diff(patch);
    }
    // Keep index and working-tree patches separate, including files changed in both.
    let mut index_args = vec![
        "diff",
        "--cached",
        "--no-ext-diff",
        "--no-textconv",
        "--",
        path,
    ];
    if let Some(source) = original_path {
        index_args.push(source);
    }
    let index = text(run(root, &index_args)?)?;
    let mut worktree_args = if kind == ChangeKind::Conflicted {
        vec![
            "diff",
            "--ours",
            "--no-ext-diff",
            "--no-textconv",
            "--",
            path,
        ]
    } else {
        vec!["diff", "--no-ext-diff", "--no-textconv", "--", path]
    };
    if let Some(source) = original_path {
        worktree_args.push(source);
    }
    let worktree = text(run(root, &worktree_args)?)?;
    let mut result = String::new();
    if !index.is_empty() {
        result.push_str("Staged changes (HEAD → index)\n");
        result.push_str(&index);
    }
    if !worktree.is_empty() {
        result.push_str(if kind == ChangeKind::Conflicted {
            "Conflict: working tree compared with ours\n"
        } else {
            "Working tree changes (index → working tree)\n"
        });
        result.push_str(&worktree);
    }
    if result.is_empty() {
        result
            .push_str("No text diff available. The file may have changed; refresh the repository.");
    }
    limit_diff(result)
}

fn limit_diff(patch: String) -> Result<String, String> {
    if patch.len() > OUTPUT_LIMIT
        || patch.lines().count() > 10_000
        || patch.lines().any(|line| line.len() > 16_384)
    {
        Err("Diff exceeds the display limit (2 MiB, 10,000 lines or 16 KiB per line)".into())
    } else {
        Ok(patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct TempRepo(PathBuf);
    impl TempRepo {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "sannygit-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            let repo = Self(path);
            repo.git(&["init", "--initial-branch=main"]);
            // Isolate backend write tests from the developer's identity, hooks and signing.
            for (key, value) in [
                ("user.name", "SannyGit Test"),
                ("user.email", "test@example.invalid"),
                ("commit.gpgsign", "false"),
                ("core.hooksPath", "disabled-test-hooks"),
                ("core.autocrlf", "false"),
            ] {
                repo.git(&["config", "--local", key, value]);
            }
            repo
        }
        fn git(&self, args: &[&str]) -> String {
            let output = Command::new("git")
                .current_dir(&self.0)
                .args([
                    "-c",
                    "user.name=SannyGit Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "core.hooksPath=disabled-test-hooks",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        }
        fn write(&self, path: &str, content: &str) {
            std::fs::write(self.0.join(path), content).unwrap();
        }
    }
    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parses_nul_paths_renames_mixed_and_conflicted_status() {
        let files = parse_status(b"MM a file\0R  new\nname\0old name\0?? unicode-\xc3\xa9\0UU conflict\0 D deleted\0AA both\0").unwrap();
        assert_eq!(files.len(), 6);
        assert_eq!(files[0].kind, ChangeKind::Modified);
        assert!(files[0].staged);
        assert_eq!(files[1].path, "new\nname");
        assert_eq!(files[1].kind, ChangeKind::Renamed);
        assert_eq!(files[1].original_path.as_deref(), Some("old name"));
        assert_eq!(files[2].kind, ChangeKind::Untracked);
        assert!(!files[2].staged);
        assert_eq!(files[3].kind, ChangeKind::Conflicted);
        assert!(!files[3].staged);
        assert_eq!(files[4].kind, ChangeKind::Deleted);
        assert_eq!(files[5].kind, ChangeKind::Conflicted);
    }

    #[test]
    fn rejects_malformed_oversized_and_non_utf8_status() {
        for input in [
            b"M\0".as_slice(),
            b"M  missing terminator",
            b"R  new\0",
            b"ZZ file\0",
            b"?? \xff\0",
        ] {
            assert!(parse_status(input).is_err());
        }
        assert!(parse_status(&b"?? file\0".repeat(FILE_LIMIT + 1)).is_err());
        assert!(limit_diff("+line\n".repeat(10_001)).is_err());
        assert!(limit_diff("a".repeat(OUTPUT_LIMIT + 1)).is_err());
    }

    #[test]
    fn loads_unborn_repository_and_previews_untracked_files() {
        let repo = TempRepo::new();
        repo.write("hello é.txt", "hello\nworld\n");
        repo.write("binary", "a\0b");
        let snapshot = load(&repo.0).unwrap();
        assert_eq!(snapshot.branch, "main");
        assert_eq!(snapshot.changes.len(), 2);
        let patch = diff(&repo.0, "hello é.txt", ChangeKind::Untracked, None).unwrap();
        assert!(patch.contains("+hello\n+world"));
        assert!(diff(&repo.0, "binary", ChangeKind::Untracked, None)
            .unwrap()
            .contains("Binary file"));
        repo.git(&["add", "--", "hello é.txt"]);
        assert!(diff(&repo.0, "hello é.txt", ChangeKind::Added, None)
            .unwrap()
            .contains("Staged changes"));
    }

    #[test]
    fn loads_real_index_worktree_rename_deletion_and_detached_head() {
        let repo = TempRepo::new();
        repo.write("file.txt", "original\n");
        repo.write("delete.txt", "delete\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "fixture"]);
        repo.write("file.txt", "staged\n");
        repo.git(&["add", "file.txt"]);
        repo.write("file.txt", "unstaged\n");
        repo.git(&["mv", "delete.txt", "renamed.txt"]);
        let snapshot = load(&repo.0).unwrap();
        assert!(snapshot
            .changes
            .iter()
            .any(|c| c.kind == ChangeKind::Renamed));
        let rename_patch = diff(
            &repo.0,
            "renamed.txt",
            ChangeKind::Renamed,
            Some("delete.txt"),
        )
        .unwrap();
        assert!(rename_patch.contains("rename from delete.txt"));
        assert!(rename_patch.contains("rename to renamed.txt"));
        let patch = diff(&repo.0, "file.txt", ChangeKind::Modified, None).unwrap();
        assert!(patch.contains("+staged"));
        assert!(patch.contains("+unstaged"));
        assert!(patch.contains("Staged changes"));
        assert!(patch.contains("Working tree changes"));
        std::fs::remove_file(repo.0.join("file.txt")).unwrap();
        assert!(load(&repo.0)
            .unwrap()
            .changes
            .iter()
            .any(|c| c.kind == ChangeKind::Deleted));
        repo.git(&["checkout", "--detach"]);
        assert!(load(&repo.0).unwrap().branch.starts_with("Detached at "));
    }

    #[test]
    fn literal_pathspec_does_not_include_other_files() {
        let repo = TempRepo::new();
        repo.write("[x].txt", "one\n");
        repo.write("x.txt", "two\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "fixture"]);
        repo.write("[x].txt", "literal\n");
        repo.write("x.txt", "unrelated\n");
        let patch = diff(&repo.0, "[x].txt", ChangeKind::Modified, None).unwrap();
        assert!(patch.contains("+literal"));
        assert!(!patch.contains("unrelated"));
    }

    #[test]
    fn loads_linked_worktree_from_subdirectory() {
        let repo = TempRepo::new();
        repo.write("file", "fixture");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "fixture"]);
        let linked = repo.0.join("linked");
        repo.git(&["worktree", "add", "--detach", linked.to_str().unwrap()]);
        std::fs::create_dir(linked.join("nested")).unwrap();
        let snapshot = load(&linked.join("nested")).unwrap();
        assert_eq!(
            PathBuf::from(snapshot.repository.path)
                .canonicalize()
                .unwrap(),
            linked.canonicalize().unwrap()
        );
        assert!(snapshot.changes.is_empty());
        assert!(snapshot.branch.starts_with("Detached at "));
    }

    #[test]
    fn previews_conflict_against_ours_without_combined_diff() {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "base"]);
        repo.git(&["checkout", "-b", "other"]);
        repo.write("file", "theirs\n");
        repo.git(&["commit", "-am", "theirs"]);
        repo.git(&["checkout", "main"]);
        repo.write("file", "ours\n");
        repo.git(&["commit", "-am", "ours"]);
        let output = Command::new("git")
            .current_dir(&repo.0)
            .args([
                "-c",
                "core.hooksPath=disabled-test-hooks",
                "merge",
                "--no-commit",
                "other",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(
            load(&repo.0).unwrap().changes[0].kind,
            ChangeKind::Conflicted
        );
        let index_before = std::fs::read(repo.0.join(".git/index")).unwrap();
        assert!(set_staged(&repo.0, "file", true).is_err());
        assert!(commit(&repo.0, "Must not commit conflict").is_err());
        assert_eq!(
            std::fs::read(repo.0.join(".git/index")).unwrap(),
            index_before
        );
        let patch = diff(&repo.0, "file", ChangeKind::Conflicted, None).unwrap();
        assert!(patch.contains("Conflict: working tree compared with ours"));
        assert!(patch.contains("+<<<<<<<"));
        assert!(!patch.contains("@@@"));
    }

    #[test]
    fn stage_and_unstage_preserve_worktree_before_and_after_first_commit() {
        let repo = TempRepo::new();
        let path = "[x] é.txt";
        repo.write(path, "new\n");
        repo.write("x é.txt", "unrelated\n");
        set_staged(&repo.0, path, true).unwrap();
        let snapshot = load(&repo.0).unwrap();
        assert!(
            snapshot
                .changes
                .iter()
                .find(|c| c.path == path)
                .unwrap()
                .staged
        );
        assert!(
            !snapshot
                .changes
                .iter()
                .find(|c| c.path == "x é.txt")
                .unwrap()
                .staged
        );
        repo.write(path, "new and unstaged\n");
        set_staged(&repo.0, path, false).unwrap();
        assert!(load(&repo.0).unwrap().changes.iter().all(|c| !c.staged));
        assert_eq!(
            std::fs::read_to_string(repo.0.join(path)).unwrap(),
            "new and unstaged\n"
        );
        set_staged(&repo.0, path, true).unwrap();
        commit(&repo.0, "First commit").unwrap();
        repo.write(path, "index version\n");
        set_staged(&repo.0, path, true).unwrap();
        repo.write(path, "working version\n");
        let snapshot = load(&repo.0).unwrap();
        let mixed = snapshot.changes.iter().find(|c| c.path == path).unwrap();
        assert!(mixed.staged && mixed.unstaged);
        set_staged(&repo.0, path, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.0.join(path)).unwrap(),
            "working version\n"
        );
        assert!(
            !load(&repo.0)
                .unwrap()
                .changes
                .iter()
                .find(|c| c.path == path)
                .unwrap()
                .staged
        );
    }

    #[test]
    fn commit_uses_only_index_and_preserves_multiline_message() {
        let repo = TempRepo::new();
        repo.write("file", "staged\n");
        repo.write("other", "not staged\n");
        set_staged(&repo.0, "file", true).unwrap();
        repo.write("file", "working tree\n");
        let message = "Subject é\n\nBody with `text` and $(touch SHOULD_NOT_EXIST)\n# preserved";
        commit(&repo.0, message).unwrap();
        assert_eq!(repo.git(&["show", "HEAD:file"]), "staged\n");
        assert_eq!(repo.git(&["log", "-1", "--format=%B"]).trim_end(), message);
        assert_eq!(repo.git(&["ls-tree", "--name-only", "HEAD"]), "file\n");
        assert!(!repo.0.join("SHOULD_NOT_EXIST").exists());
        let changes = load(&repo.0).unwrap().changes;
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|c| !c.staged));
    }

    #[test]
    fn stages_deletion_and_unstages_both_sides_of_rename() {
        let repo = TempRepo::new();
        repo.write(
            "old",
            "unchanged content line one\nunchanged content line two\n",
        );
        set_staged(&repo.0, "old", true).unwrap();
        commit(&repo.0, "initial").unwrap();
        repo.git(&["mv", "old", "new"]);
        repo.write(
            "new",
            "unchanged content line one\nunchanged content line two\nupdated\n",
        );
        set_staged(&repo.0, "new", true).unwrap();
        set_staged(&repo.0, "new", false).unwrap();
        assert!(load(&repo.0).unwrap().changes.iter().all(|c| !c.staged));
        assert!(!repo.0.join("old").exists());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("new")).unwrap(),
            "unchanged content line one\nunchanged content line two\nupdated\n"
        );
        set_staged(&repo.0, "old", true).unwrap();
        assert!(load(&repo.0)
            .unwrap()
            .changes
            .iter()
            .any(|c| c.kind == ChangeKind::Deleted && c.staged));
        set_staged(&repo.0, "old", false).unwrap();
        assert!(!repo.0.join("old").exists());
    }

    #[test]
    fn refuses_empty_commit_and_reports_index_lock_without_touching_worktree() {
        let repo = TempRepo::new();
        for message in ["", " \n\t", "bad\0message"] {
            assert!(validate_message(message).is_err());
        }
        assert!(validate_message(&"a".repeat(65_537)).is_err());
        repo.write("file", "content\n");
        assert!(commit(&repo.0, "Nothing staged")
            .unwrap_err()
            .contains("Stage at least one"));
        std::fs::write(repo.0.join(".git/index.lock"), "locked").unwrap();
        assert!(set_staged(&repo.0, "file", true)
            .unwrap_err()
            .contains("locked"));
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "content\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn commit_respects_rejecting_hook_and_keeps_index() {
        use std::os::unix::fs::PermissionsExt;
        let repo = TempRepo::new();
        repo.write("file", "content\n");
        set_staged(&repo.0, "file", true).unwrap();
        let hooks = repo.0.join("test-hooks");
        std::fs::create_dir(&hooks).unwrap();
        let hook = hooks.join("pre-commit");
        std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        repo.git(&[
            "config",
            "--local",
            "core.hooksPath",
            hooks.to_str().unwrap(),
        ]);
        assert!(commit(&repo.0, "Rejected").unwrap_err().contains("hooks"));
        assert!(load(&repo.0).unwrap().changes[0].staged);
        assert!(run(&repo.0, &["rev-parse", "--verify", "HEAD"]).is_err());
    }

    #[test]
    fn rejects_non_repository_and_bare_repository() {
        let repo = TempRepo::new();
        let missing = repo.0.join("missing");
        assert!(load(&missing).is_err());
        let bare = repo.0.join("bare.git");
        repo.git(&["init", "--bare", bare.to_str().unwrap()]);
        assert!(load(&bare).is_err());
    }
}
