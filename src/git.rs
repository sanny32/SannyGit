//! Git command execution and parsing. Called only by the application's background workers.
use crate::models::{ChangeFile, ChangeKind, Repository};
use std::{
    cell::RefCell,
    fmt::Write,
    io::{Read, Write as IoWrite},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 2 * 1024 * 1024;
const FILE_LIMIT: usize = 5_000;
const TIMEOUT: Duration = Duration::from_secs(15);

thread_local! {
    static EXECUTABLE: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Set once at worker startup so every command in an operation uses the same Git.
pub(crate) fn set_executable(executable: String) {
    EXECUTABLE.with(|value| *value.borrow_mut() = executable);
}

/// Find the first executable Git in PATH without launching a process.
pub(crate) fn find_executable() -> Option<PathBuf> {
    find_executable_in(&std::env::var_os("PATH")?, &std::env::current_dir().ok()?)
}

fn find_executable_in(search_path: &std::ffi::OsStr, current_dir: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    std::env::split_paths(search_path).find_map(|directory| {
        let candidate = current_dir.join(directory).join(name);
        let metadata = candidate.metadata().ok()?;
        if !metadata.is_file() {
            return None;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return None;
            }
        }
        Some(candidate)
    })
}

fn executable() -> String {
    EXECUTABLE.with(|value| {
        let value = value.borrow();
        if value.is_empty() {
            "git".into()
        } else {
            value.clone()
        }
    })
}

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

pub(crate) fn run(path: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    run_controlled(path, args, TIMEOUT, None)
}

pub(crate) fn run_controlled(
    path: &Path,
    args: &[&str],
    timeout: Duration,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<u8>, String> {
    execute(path, args, None, timeout, cancel, false)
}

pub(crate) fn run_with_input(
    path: &Path,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    execute(path, args, input, TIMEOUT, None, false)
}

/// Successful diagnostic output for an explicit preview (never error logging).
pub(crate) fn run_diagnostic(path: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    execute(path, args, None, TIMEOUT, None, true)
}

fn execute(
    path: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
    diagnostics: bool,
) -> Result<Vec<u8>, String> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Err("Operation cancelled".into());
    }
    let executable = executable();
    let mut command = Command::new(&executable);
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
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start Git executable \"{executable}\": {e}. Check Preferences > Git > Executable."))?;
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
                Ok(None)
                    if start.elapsed() < timeout
                        && !cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) =>
                {
                    thread::sleep(Duration::from_millis(20))
                }
                result => {
                    #[cfg(unix)]
                    if let Ok(id) = i32::try_from(child.id()) {
                        // SAFETY: This child was started in its own process group.
                        unsafe {
                            libc::kill(-id, libc::SIGKILL);
                        }
                    }
                    #[cfg(target_os = "windows")]
                    {
                        let _ = Command::new("taskkill")
                            .args(["/PID", &child.id().to_string(), "/T", "/F"])
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status();
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(match result {
                        Err(e) => format!("Cannot wait for Git: {e}"),
                        _ if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) => {
                            "Operation cancelled; refresh local and remote state before retrying"
                                .into()
                        }
                        _ => format!(
                            "Git operation timed out after {} seconds",
                            timeout.as_secs()
                        ),
                    });
                }
            }
        };
        let mut output = out.join().map_err(|_| "Git output reader failed")??;
        let errors = err.join().map_err(|_| "Git error reader failed")??;
        let written = writer.join().map_err(|_| "Git input writer failed")?;
        if !status?.success() {
            return Err(command_error(args.first().copied().unwrap_or("command")));
        }
        written?;
        if diagnostics {
            output.extend_from_slice(&errors);
        }
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
    } else if operation == "clone" {
        "Check the URL, destination, network access, credential helpers and SSH configuration. A partial destination may remain."
    } else if matches!(operation, "fetch" | "push") {
        "Check network access, Git credential helpers, SSH configuration and remote permissions. Push may also be rejected because the remote branch advanced, a hook refused it, or the remote is configured as a mirror. Refresh before retrying."
    } else if operation == "worktree" {
        "Check branch availability, worktree locks, local files and destination permissions. Refresh before retrying."
    } else if operation == "config" {
        "Check Git config file permissions and configuration syntax."
    } else if operation == "switch" {
        "Check for a branch used by another worktree, an in-progress Git operation, file conflicts or repository permissions. Refresh before retrying."
    } else if operation == "branch" {
        "Check the branch name, existing refs, ref locks and repository permissions. Refresh before retrying."
    } else if matches!(operation, "add" | "restore" | "update-index") {
        "Check whether the index is locked by another Git operation and verify repository permissions. Refresh before retrying."
    } else {
        "Check the repository path, permissions and ownership settings. Refresh before retrying."
    };
    format!("Git {operation} failed. {reason}")
}

/// Stages or unstages the complete selected file; never changes working-tree bytes.
pub fn set_staged(root: &Path, path: &str, staged: bool) -> Result<(), String> {
    require_no_operation(root)?;
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

/// One content selection shared by commit, amend, preview and message generation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommitContents {
    #[default]
    All,
    Staged,
}

impl CommitContents {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All changes",
            Self::Staged => "Staged only",
        }
    }
}

/// Read-only preview of the resulting commit relative to its first parent.
/// For amend this includes the existing commit as well as the selected changes.
pub fn commit_patch(
    root: &Path,
    contents: CommitContents,
    amend_head: Option<&str>,
) -> Result<String, String> {
    let head = text(run(root, &["rev-parse", "--revs-only", "HEAD"])?)?;
    let base = if let Some(expected) = amend_head {
        check_amend_head(root, expected)?;
        let parents = text(run(root, &["rev-list", "--parents", "-n", "1", expected])?)?;
        parents.split_whitespace().nth(1).map(str::to_owned)
    } else {
        let head = head.trim();
        (!head.is_empty()).then(|| head.to_owned())
    };
    let base = match base {
        Some(base) => base,
        None => text(run_with_input(
            root,
            &["hash-object", "-t", "tree", "--stdin"],
            Some(b""),
        )?)?
        .trim()
        .to_owned(),
    };
    let mut args = vec!["diff", "--no-ext-diff", "--no-textconv"];
    if contents == CommitContents::Staged {
        args.push("--cached");
    }
    args.push(&base);
    args.push("--");
    let mut patch = text(run(root, &args)?)?;
    if contents == CommitContents::All {
        // Preview new files without staging them or modifying the real index.
        for change in load(root)?.changes {
            if change.kind == ChangeKind::Untracked {
                writeln!(patch, "\nNew file: {}", change.path).map_err(|e| e.to_string())?;
                patch.push_str(&diff(root, &change.path, change.kind, None)?);
                if patch.len() > OUTPUT_LIMIT {
                    return Err("Commit diff exceeds the 2 MiB limit".into());
                }
            }
        }
    }
    Ok(patch)
}

/// Commits explicitly selected contents after a UI action.
pub fn commit(root: &Path, message: &str, contents: CommitContents) -> Result<(), String> {
    commit_with_mode(root, message, false, contents)
}

/// Loads HEAD explicitly rather than relying on the filtered journal selection.
pub fn amend_message(root: &Path) -> Result<(String, String), String> {
    require_no_operation(root)?;
    let head = text(run(root, &["rev-parse", "--revs-only", "HEAD"])?)?;
    let head = head.trim();
    if head.is_empty() {
        return Err("There is no commit to amend.".into());
    }
    let message = text(run(
        root,
        &[
            "show",
            "--no-patch",
            "--no-notes",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--format=format:%B",
            head,
            "--",
        ],
    )?)?;
    validate_message(&message)?;
    Ok((head.to_owned(), message.trim_end_matches('\n').to_owned()))
}

pub fn amend_commit(
    root: &Path,
    message: &str,
    expected_head: &str,
    contents: CommitContents,
) -> Result<(), String> {
    check_amend_head(root, expected_head)?;
    commit_with_mode(root, message, true, contents)
}

fn check_amend_head(root: &Path, expected_head: &str) -> Result<(), String> {
    let head = text(run(root, &["rev-parse", "--verify", "HEAD"])?)?;
    if head.trim() != expected_head {
        return Err("HEAD has changed. Turn off Amend last commit and enable it again.".into());
    }
    require_no_operation(root)
}

/// Resolve markers through Git so linked worktrees use their own operation state.
/// SannyGit has no continue/abort workflow yet; leave unfinished operations to Git.
pub(crate) fn require_no_operation(root: &Path) -> Result<(), String> {
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        let path = text(run(root, &["rev-parse", "--git-path", marker])?)?;
        match root.join(path.strip_suffix('\n').unwrap_or(&path)).try_exists() {
            Ok(false) => {}
            Ok(true) => return Err("Finish the in-progress merge, rebase, cherry-pick or revert outside SannyGit before retrying this action.".into()),
            Err(error) => return Err(format!("Could not check unfinished Git operations: {error}")),
        }
    }
    Ok(())
}

fn commit_with_mode(
    root: &Path,
    message: &str,
    amend: bool,
    contents: CommitContents,
) -> Result<(), String> {
    validate_message(message)?;
    require_no_operation(root)?;
    let snapshot = load(root)?;
    if snapshot
        .changes
        .iter()
        .any(|c| c.kind == ChangeKind::Conflicted)
    {
        return Err("Resolve all conflicts before committing.".into());
    }
    if snapshot.changes.is_empty() && !amend {
        return Err("There are no changes to commit.".into());
    }
    if contents == CommitContents::Staged && !amend && !snapshot.changes.iter().any(|c| c.staged) {
        return Err("There are no staged changes to commit.".into());
    }
    if contents == CommitContents::All {
        run(root, &["add", "--all"])?;
    }
    // stdin preserves multiline messages without a shell or a message file.
    // Git's configured identity, hooks and signing remain in effect.
    let mut args = vec!["commit", "--file=-", "--cleanup=whitespace"];
    if amend {
        args.push("--amend");
    }
    run_with_input(root, &args, Some(message.as_bytes())).map(|_| ())
}

pub(crate) fn text(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| "Non-UTF-8 Git output is not supported yet".into())
}

pub fn load(path: &Path) -> Result<Snapshot, String> {
    let root = text(run(path, &["rev-parse", "--show-toplevel"])?)?;
    // Git appends exactly one line ending; preserve whitespace in directory names.
    let root = root.strip_suffix('\n').unwrap_or(&root);
    let path = PathBuf::from(root);
    let branch = match run(&path, &["symbolic-ref", "--quiet", "HEAD"]) {
        Ok(output) => {
            let name = text(output)?;
            name.trim_end_matches('\n')
                .strip_prefix("refs/heads/")
                .ok_or("Unexpected HEAD reference")?
                .to_owned()
        }
        Err(_) => format!(
            "Detached at {}",
            text(run(&path, &["rev-parse", "--short", "HEAD"])?)?.trim()
        ),
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

pub(crate) fn limit_diff(patch: String) -> Result<String, String> {
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
pub(crate) mod tests {
    #[test]
    fn path_discovery_returns_the_first_executable_as_an_absolute_path() {
        let repo = TempRepo::new();
        let first = repo.0.join("first Git directory");
        let second = repo.0.join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        let name = if cfg!(windows) { "git.exe" } else { "git" };
        let candidate = second.join(name);
        std::fs::write(&candidate, "test executable").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // A non-executable earlier PATH entry must be skipped.
            std::fs::write(first.join(name), "not executable").unwrap();
            std::fs::set_permissions(first.join(name), std::fs::Permissions::from_mode(0o600))
                .unwrap();
            std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let search =
            std::env::join_paths([Path::new("first Git directory"), Path::new("second")]).unwrap();
        assert_eq!(find_executable_in(&search, &repo.0), Some(candidate));
        assert_eq!(find_executable_in(first.as_os_str(), &repo.0), None);
    }

    #[test]
    fn configured_executable_reports_missing_file_without_falling_back() {
        thread::spawn(|| {
            let directory = std::env::temp_dir();
            let missing = directory.join(format!("missing Git executable {}", std::process::id()));
            set_executable(missing.to_string_lossy().into_owned());
            let error = run(&directory, &["--version"]).unwrap_err();
            assert!(error.contains(&*missing.to_string_lossy()));
            assert!(error.contains("Preferences > Git > Executable"));
            set_executable(String::new());
            assert!(run(&directory, &["--version"])
                .unwrap()
                .starts_with(b"git version"));
        })
        .join()
        .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn configured_executable_supports_spaces_and_preserves_arguments() {
        thread::spawn(|| {
            use std::os::unix::fs::PermissionsExt;
            let repo = TempRepo::new();
            let executable = repo.0.join("custom Git executable");
            std::fs::write(&executable, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            set_executable(executable.to_string_lossy().into_owned());
            let output = run(&repo.0, &["--version", "argument with spaces", "$(literal)"]).unwrap();
            assert_eq!(String::from_utf8(output).unwrap(),
                "--no-pager\n--no-optional-locks\n-c\ncolor.ui=false\n--version\nargument with spaces\n$(literal)\n");
        }).join().unwrap();
    }

    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    pub(crate) struct TempRepo(pub(crate) PathBuf);
    impl TempRepo {
        pub(crate) fn new() -> Self {
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
        pub(crate) fn git(&self, args: &[&str]) -> String {
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
        pub(crate) fn write(&self, path: &str, content: &str) {
            std::fs::write(self.0.join(path), content).unwrap();
        }
    }
    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn staged_commit_preview_and_commit_preserve_partial_file_and_untracked_contents() {
        for unborn in [true, false] {
            let repo = TempRepo::new();
            if !unborn {
                repo.write("file", "base\n");
                commit(&repo.0, "Base", CommitContents::All).unwrap();
            }
            repo.write("file", "selected\n");
            repo.git(&["add", "file"]);
            repo.write("file", "excluded\n");
            repo.write("new", "private untracked\n");
            let index = std::fs::read(repo.0.join(".git/index")).unwrap();
            let patch = commit_patch(&repo.0, CommitContents::Staged, None).unwrap();
            assert!(patch.contains("+selected"));
            assert!(!patch.contains("excluded"));
            assert!(!patch.contains("private untracked"));
            let all = commit_patch(&repo.0, CommitContents::All, None).unwrap();
            assert!(all.contains("+excluded"));
            assert!(!all.contains("+selected"));
            assert!(all.contains("private untracked"));
            assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
            commit(&repo.0, "Selected", CommitContents::Staged).unwrap();
            assert_eq!(repo.git(&["show", "HEAD:file"]), "selected\n");
            assert_eq!(repo.git(&["ls-tree", "--name-only", "HEAD"]), "file\n");
            assert!(repo.git(&["diff", "--cached"]).is_empty());
            assert_eq!(
                std::fs::read_to_string(repo.0.join("file")).unwrap(),
                "excluded\n"
            );
            assert_eq!(
                std::fs::read_to_string(repo.0.join("new")).unwrap(),
                "private untracked\n"
            );
            let head = repo.git(&["rev-parse", "HEAD"]);
            assert!(commit(&repo.0, "Empty", CommitContents::Staged)
                .unwrap_err()
                .contains("no staged"));
            assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
        }
    }

    #[test]
    fn staged_commit_handles_deletion_rename_and_recreated_working_file() {
        let repo = TempRepo::new();
        repo.write("old", "rename contents\n");
        repo.write("deleted", "delete contents\n");
        commit(&repo.0, "Base", CommitContents::All).unwrap();
        repo.git(&["mv", "old", "renamed"]);
        repo.git(&["rm", "deleted"]);
        repo.write("deleted", "recreated unstaged\n");
        repo.write("renamed", "changed after rename\n");
        let patch = commit_patch(&repo.0, CommitContents::Staged, None).unwrap();
        assert!(patch.contains("deleted"));
        assert!(patch.contains("renamed"));
        assert!(!patch.contains("recreated unstaged"));
        assert!(!patch.contains("changed after rename"));
        commit(&repo.0, "Rename and delete", CommitContents::Staged).unwrap();
        assert_eq!(repo.git(&["ls-tree", "--name-only", "HEAD"]), "renamed\n");
        assert_eq!(repo.git(&["show", "HEAD:renamed"]), "rename contents\n");
        assert!(repo.git(&["diff", "--cached"]).is_empty());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("deleted")).unwrap(),
            "recreated unstaged\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.0.join("renamed")).unwrap(),
            "changed after rename\n"
        );
    }

    #[test]
    fn staged_amend_preview_includes_original_commit_and_preserves_unstaged_changes() {
        for root_commit in [true, false] {
            let repo = TempRepo::new();
            if !root_commit {
                repo.write("parent", "parent content\n");
                commit(&repo.0, "Parent", CommitContents::All).unwrap();
            }
            repo.write("original", "original commit content\n");
            commit(&repo.0, "Original", CommitContents::All).unwrap();
            let (head, _) = amend_message(&repo.0).unwrap();
            let parents = repo.git(&["show", "-s", "--format=%P", "HEAD"]);
            repo.write("original", "unstaged private content\n");
            repo.write("new", "selected addition\n");
            repo.git(&["add", "new"]);
            repo.write("new", "excluded addition\n");
            let index = std::fs::read(repo.0.join(".git/index")).unwrap();
            let patch = commit_patch(&repo.0, CommitContents::Staged, Some(&head)).unwrap();
            assert!(patch.contains("+original commit content"));
            assert!(patch.contains("+selected addition"));
            assert!(!patch.contains("unstaged private"));
            assert!(!patch.contains("excluded addition"));
            assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
            amend_commit(&repo.0, "Amended", &head, CommitContents::Staged).unwrap();
            assert_eq!(repo.git(&["show", "-s", "--format=%P", "HEAD"]), parents);
            assert_eq!(
                repo.git(&["show", "HEAD:original"]),
                "original commit content\n"
            );
            assert_eq!(repo.git(&["show", "HEAD:new"]), "selected addition\n");
            let (next, _) = amend_message(&repo.0).unwrap();
            let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
            assert!(repo.git(&["diff", "--cached"]).is_empty());
            amend_commit(&repo.0, "Message only", &next, CommitContents::Staged).unwrap();
            assert_eq!(repo.git(&["rev-parse", "HEAD^{tree}"]), tree);
            assert_eq!(
                std::fs::read_to_string(repo.0.join("original")).unwrap(),
                "unstaged private content\n"
            );
            assert_eq!(
                std::fs::read_to_string(repo.0.join("new")).unwrap(),
                "excluded addition\n"
            );
            let before = std::fs::read(repo.0.join(".git/index")).unwrap();
            for mode in [CommitContents::All, CommitContents::Staged] {
                assert!(amend_commit(&repo.0, "Stale", &head, mode)
                    .unwrap_err()
                    .contains("HEAD has changed"));
                assert!(commit_patch(&repo.0, mode, Some(&head))
                    .unwrap_err()
                    .contains("HEAD has changed"));
            }
            assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), before);
        }
    }

    #[test]
    fn commit_patch_includes_new_files_without_changing_the_index() {
        let repo = TempRepo::new();
        repo.write("file.txt", "base\n");
        repo.git(&["add", "file.txt"]);
        repo.git(&["commit", "-m", "base"]);
        repo.write("file.txt", "unstaged\n");
        repo.write("untracked.txt", "new contents\n");
        repo.write(".gitignore", "ignored.txt\n");
        repo.write("ignored.txt", "ignored contents\n");
        let before = repo.git(&["status", "--porcelain=v1", "-z"]);

        let patch = commit_patch(&repo.0, CommitContents::All, None).unwrap();
        assert!(patch.contains("+unstaged"));
        assert!(patch.contains("untracked.txt"));
        assert!(patch.contains("+new contents"));
        assert!(!patch.contains("ignored contents"));
        assert_eq!(repo.git(&["status", "--porcelain=v1", "-z"]), before);
    }

    #[test]
    fn new_files_can_generate_a_patch_and_create_the_initial_commit() {
        let repo = TempRepo::new();
        repo.write("new file.txt", "initial contents\n");
        repo.write(".gitignore", "ignored\n");
        repo.write("ignored", "keep private\n");
        let patch = commit_patch(&repo.0, CommitContents::All, None).unwrap();
        assert!(patch.contains("new file.txt"));
        assert!(patch.contains("+initial contents"));
        assert!(!patch.contains("keep private"));
        assert!(load(&repo.0).unwrap().changes.iter().all(|c| !c.staged));
        commit(&repo.0, "Add initial files", CommitContents::All).unwrap();
        assert_eq!(
            repo.git(&["show", "HEAD:new file.txt"]),
            "initial contents\n"
        );
        assert!(!repo
            .git(&["ls-tree", "--name-only", "HEAD"])
            .lines()
            .any(|path| path == "ignored"));
        assert!(load(&repo.0).unwrap().changes.is_empty());
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
        assert!(commit(&repo.0, "Must not commit conflict", CommitContents::All).is_err());
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
        commit(&repo.0, "First commit", CommitContents::All).unwrap();
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
    fn commit_includes_tracked_unstaged_changes_and_new_files() {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        repo.git(&["add", "file"]);
        repo.git(&["commit", "-m", "base"]);
        repo.write("file", "working tree\n");
        repo.write("other", "untracked\n");
        let message = "Subject é\n\nBody with `text` and $(touch SHOULD_NOT_EXIST)\n# preserved";
        commit(&repo.0, message, CommitContents::All).unwrap();
        assert_eq!(repo.git(&["show", "HEAD:file"]), "working tree\n");
        assert_eq!(repo.git(&["log", "-1", "--format=%B"]).trim_end(), message);
        assert_eq!(
            repo.git(&["ls-tree", "--name-only", "HEAD"]),
            "file\nother\n"
        );
        assert_eq!(repo.git(&["show", "HEAD:other"]), "untracked\n");
        assert_eq!(
            std::fs::read_to_string(repo.0.join("other")).unwrap(),
            "untracked\n"
        );
        assert!(!repo.0.join("SHOULD_NOT_EXIST").exists());
        let changes = load(&repo.0).unwrap().changes;
        assert!(changes.is_empty());
    }

    #[test]
    fn amend_replaces_root_and_allows_message_only_changes() {
        let repo = TempRepo::new();
        assert!(amend_message(&repo.0).unwrap_err().contains("no commit"));
        repo.write("file", "original\n");
        commit(&repo.0, "Original\n\nBody é", CommitContents::All).unwrap();
        let (head, message) = amend_message(&repo.0).unwrap();
        assert_eq!(message, "Original\n\nBody é");
        let author = repo.git(&["show", "-s", "--format=%an <%ae> %aI", "HEAD"]);
        amend_commit(&repo.0, "Reworded\n\nNew body", &head, CommitContents::All).unwrap();
        assert_ne!(repo.git(&["rev-parse", "HEAD"]).trim(), head);
        assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]).trim(), "1");
        assert_eq!(repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(), "");
        assert_eq!(
            repo.git(&["show", "-s", "--format=%an <%ae> %aI", "HEAD"]),
            author
        );
        assert_eq!(
            repo.git(&["log", "-1", "--format=%B"]).trim_end(),
            "Reworded\n\nNew body"
        );
        assert_eq!(repo.git(&["show", "HEAD:file"]), "original\n");
    }

    #[test]
    fn amend_includes_all_changes_and_preserves_parents() {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        commit(&repo.0, "Base", CommitContents::All).unwrap();
        let parent = repo.git(&["rev-parse", "HEAD"]);
        repo.write("file", "second\n");
        commit(&repo.0, "Second", CommitContents::All).unwrap();
        let (head, _) = amend_message(&repo.0).unwrap();
        repo.write("file", "replacement\n");
        repo.write("new", "new file\n");
        amend_commit(&repo.0, "Replacement", &head, CommitContents::All).unwrap();
        assert_eq!(repo.git(&["rev-parse", "HEAD^"]), parent);
        assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]).trim(), "2");
        assert_eq!(repo.git(&["show", "HEAD:file"]), "replacement\n");
        assert_eq!(repo.git(&["show", "HEAD:new"]), "new file\n");
        assert!(load(&repo.0).unwrap().changes.is_empty());
    }

    #[test]
    fn amend_rejects_stale_head_invalid_message_and_in_progress_operations_before_staging() {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        commit(&repo.0, "Base", CommitContents::All).unwrap();
        let (old_head, _) = amend_message(&repo.0).unwrap();
        repo.write("file", "second\n");
        commit(&repo.0, "Second", CommitContents::All).unwrap();
        let (head, _) = amend_message(&repo.0).unwrap();
        repo.write("new", "not staged\n");
        assert!(
            amend_commit(&repo.0, "Stale", &old_head, CommitContents::All)
                .unwrap_err()
                .contains("HEAD has changed")
        );
        assert!(amend_commit(&repo.0, " ", &head, CommitContents::All).is_err());
        for marker in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
            "sequencer",
        ] {
            let path = repo.0.join(".git").join(marker);
            std::fs::write(&path, &head).unwrap();
            assert!(amend_commit(&repo.0, "Blocked", &head, CommitContents::All)
                .unwrap_err()
                .contains("Finish"));
            std::fs::remove_file(path).unwrap();
        }
        assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), head);
        assert!(repo.git(&["diff", "--cached", "--name-only"]).is_empty());
        assert_eq!(repo.git(&["status", "--porcelain"]).trim(), "?? new");
    }

    #[test]
    fn unfinished_operations_preserve_index_worktree_and_head_in_linked_worktrees() {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        commit(&repo.0, "Base", CommitContents::All).unwrap();
        repo.git(&["branch", "other"]);
        let linked = repo.0.join("linked");
        repo.git(&["worktree", "add", "--detach", linked.to_str().unwrap()]);
        for root in [&repo.0, &linked] {
            std::fs::write(root.join("file"), "staged\n").unwrap();
            set_staged(root, "file", true).unwrap();
            std::fs::write(root.join("file"), "unstaged\n").unwrap();
            std::fs::write(root.join("new"), "untracked\n").unwrap();
            let head = text(run(root, &["rev-parse", "HEAD"]).unwrap()).unwrap();
            let index = run(root, &["ls-files", "--stage", "-z"]).unwrap();
            for marker in [
                "MERGE_HEAD",
                "CHERRY_PICK_HEAD",
                "REVERT_HEAD",
                "rebase-merge",
                "rebase-apply",
                "sequencer",
            ] {
                let marker_path =
                    text(run(root, &["rev-parse", "--git-path", marker]).unwrap()).unwrap();
                let marker_path = root.join(marker_path.trim_end_matches('\n'));
                let directory = matches!(marker, "rebase-merge" | "rebase-apply" | "sequencer");
                if directory {
                    std::fs::create_dir(&marker_path).unwrap();
                } else {
                    std::fs::write(&marker_path, &head).unwrap();
                }
                for mode in [CommitContents::All, CommitContents::Staged] {
                    assert!(commit(root, "Blocked", mode)
                        .unwrap_err()
                        .contains("Finish"));
                    assert!(amend_commit(root, "Blocked", head.trim(), mode)
                        .unwrap_err()
                        .contains("Finish"));
                }
                assert!(amend_message(root).unwrap_err().contains("Finish"));
                assert!(set_staged(root, "file", true)
                    .unwrap_err()
                    .contains("Finish"));
                assert!(set_staged(root, "file", false)
                    .unwrap_err()
                    .contains("Finish"));
                assert!(crate::branches::switch(root, "refs/heads/other")
                    .unwrap_err()
                    .contains("Finish"));
                assert_eq!(run(root, &["ls-files", "--stage", "-z"]).unwrap(), index);
                assert_eq!(
                    text(run(root, &["rev-parse", "HEAD"]).unwrap()).unwrap(),
                    head
                );
                assert_eq!(std::fs::read(root.join("file")).unwrap(), b"unstaged\n");
                assert_eq!(std::fs::read(root.join("new")).unwrap(), b"untracked\n");
                if directory {
                    std::fs::remove_dir(marker_path).unwrap();
                } else {
                    std::fs::remove_file(marker_path).unwrap();
                }
            }
            require_no_operation(root).unwrap();
        }
    }

    #[test]
    fn stages_deletion_and_unstages_both_sides_of_rename() {
        let repo = TempRepo::new();
        repo.write(
            "old",
            "unchanged content line one\nunchanged content line two\n",
        );
        set_staged(&repo.0, "old", true).unwrap();
        commit(&repo.0, "initial", CommitContents::All).unwrap();
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
        assert!(commit(&repo.0, "Nothing changed", CommitContents::All)
            .unwrap_err()
            .contains("no changes"));
        repo.write("file", "content\n");
        std::fs::write(repo.0.join(".git/index.lock"), "locked").unwrap();
        assert!(commit(&repo.0, "Add new file", CommitContents::All)
            .unwrap_err()
            .contains("locked"));
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
        commit(&repo.0, "Base", CommitContents::All).unwrap();
        let (head, _) = amend_message(&repo.0).unwrap();
        repo.write("file", "updated\n");
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
        repo.write("file", "unstaged after selection\n");
        repo.write("untracked", "not selected\n");
        let index = repo.git(&["ls-files", "--stage", "-z"]);
        assert!(commit(&repo.0, "Rejected staged", CommitContents::Staged).is_err());
        assert!(amend_commit(
            &repo.0,
            "Rejected staged amend",
            &head,
            CommitContents::Staged
        )
        .is_err());
        assert_eq!(repo.git(&["ls-files", "--stage", "-z"]), index);
        assert_eq!(repo.git(&["show", ":file"]), "updated\n");
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "unstaged after selection\n"
        );
        assert!(commit(&repo.0, "Rejected", CommitContents::All)
            .unwrap_err()
            .contains("hooks"));
        assert!(load(&repo.0).unwrap().changes[0].staged);
        assert!(
            amend_commit(&repo.0, "Rejected amend", &head, CommitContents::All)
                .unwrap_err()
                .contains("hooks")
        );
        assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), head);
        assert!(load(&repo.0).unwrap().changes[0].staged);
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
