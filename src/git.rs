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

pub(crate) mod diagnostics;
use diagnostics::{CommandError, FailureKind};

/// A completed command, kept separate from user-facing diagnostics. In Sync
/// mode a reviewed porcelain rejection can have a non-success status.
struct CommandOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
}

impl std::fmt::Debug for CommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandOutput")
            .field("status", &self.status)
            .field("stdout_bytes", &self.stdout.len())
            .finish()
    }
}

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
    execute(path, args, None, timeout, cancel, CommandMode::Normal)
        .map(|output| output.stdout)
        .map_err(|error| error.to_string())
}

/// Return whether Sync needs to pull after a non-fast-forward push rejection.
/// Other failures retain the normal sanitized errors.
pub(crate) fn run_sync_push(
    path: &Path,
    args: &[&str],
    cancel: &AtomicBool,
) -> Result<bool, String> {
    execute(
        path,
        args,
        None,
        Duration::from_secs(600),
        Some(cancel),
        CommandMode::SyncPush,
    )
    .map(|output| push_needs_pull(&output.stdout))
    .map_err(|error| error.to_string())
}

fn push_needs_pull(output: &[u8]) -> bool {
    let Ok(output) = std::str::from_utf8(output) else {
        return false;
    };
    let mut statuses = output.lines().filter_map(|line| {
        let mut fields = line.split('\t');
        let flag = fields.next()?;
        let refs = fields.next()?;
        let summary = fields.next()?;
        (flag.len() == 1 && refs.contains(":refs/heads/")).then_some((flag, summary))
    });
    matches!(
        statuses.next(),
        Some((
            "!",
            "[rejected] (fetch first)" | "[rejected] (non-fast-forward)"
        ))
    ) && statuses.next().is_none()
}

pub(crate) fn run_with_input(
    path: &Path,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    execute(path, args, input, TIMEOUT, None, CommandMode::Normal)
        .map(|output| output.stdout)
        .map_err(|error| error.to_string())
}

/// Read-only commands with bounded stdin and the same cancellation/timeout handling.
pub(crate) fn run_with_input_controlled(
    path: &Path,
    args: &[&str],
    input: &[u8],
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    execute(
        path,
        args,
        Some(input),
        TIMEOUT,
        Some(cancel),
        CommandMode::Normal,
    )
    .map(|output| output.stdout)
    .map_err(|error| error.to_string())
}

/// Successful diagnostic output for an explicit preview (never error logging).
pub(crate) fn run_diagnostic(path: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    execute(path, args, None, TIMEOUT, None, CommandMode::Diagnostic)
        .map(|output| output.stdout)
        .map_err(|error| error.to_string())
}

/// Whole-repository stash operations have no user pathspecs. Git's stash cleanup
/// internally uses :/; inheriting literal pathspecs leaves untracked files behind.
pub(crate) fn run_stash(path: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = vec!["-c", "submodule.recurse=false", "stash"];
    command.extend_from_slice(args);
    execute(path, &command, None, TIMEOUT, None, CommandMode::Stash)
        .map(|output| output.stdout)
        .map_err(|error| error.to_string())
}

enum CommandMode {
    Normal,
    Diagnostic,
    SyncPush,
    Stash,
}

fn execute(
    path: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
    mode: CommandMode,
) -> Result<CommandOutput, CommandError> {
    let operation = if matches!(mode, CommandMode::Stash) {
        "stash"
    } else {
        diagnostics::operation(args)
    };
    let fail = |kind, message: String| CommandError::new(operation, kind, message);
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Err(fail(FailureKind::Cancelled, "Operation cancelled".into()));
    }
    let executable = executable();
    let mut command = Command::new(&executable);
    command
        .current_dir(path)
        .args(["--no-pager", "--no-optional-locks", "-c", "color.ui=false"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env(
            "GIT_LITERAL_PATHSPECS",
            if matches!(mode, CommandMode::Stash) {
                "0"
            } else {
                "1"
            },
        )
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if matches!(mode, CommandMode::SyncPush) {
        // Porcelain rejection summaries must be stable across Git locales.
        command.env("LC_ALL", "C");
    }
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
        .map_err(|e| fail(FailureKind::Start, format!("Cannot start Git executable \"{executable}\": {e}. Check Preferences > Git > Executable.")))?;
    #[cfg(test)]
    crate::test_support::record_git_command(path);
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| fail(FailureKind::Io, "Cannot read Git output".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| fail(FailureKind::Io, "Cannot read Git errors".into()))?;
    let stdin = child.stdin.take();
    let result = thread::scope(|scope| {
        let writer = scope.spawn(move || -> Result<(), String> {
            if let (Some(mut pipe), Some(bytes)) = (stdin, input) {
                pipe.write_all(bytes)
                    .map_err(|e| format!("Cannot send input to Git: {e}"))?;
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
                        use std::os::windows::process::CommandExt;
                        let _ = Command::new("taskkill")
                            .args(["/PID", &child.id().to_string(), "/T", "/F"])
                            .creation_flags(0x08000000) // CREATE_NO_WINDOW
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status();
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(match result {
                        Err(e) => fail(FailureKind::Io, format!("Cannot wait for Git: {e}")),
                        _ if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) => fail(
                            FailureKind::Cancelled,
                            "Operation cancelled; refresh local and remote state before retrying"
                                .into(),
                        ),
                        _ => fail(
                            FailureKind::TimedOut,
                            format!(
                                "Git operation timed out after {} seconds",
                                timeout.as_secs()
                            ),
                        ),
                    });
                }
            }
        };
        let mut output = out
            .join()
            .map_err(|_| fail(FailureKind::Worker, "Git output reader failed".into()))?
            .map_err(|error| fail(FailureKind::Io, error))?;
        let errors = err
            .join()
            .map_err(|_| fail(FailureKind::Worker, "Git error reader failed".into()))?
            .map_err(|error| fail(FailureKind::Io, error))?;
        let written = writer
            .join()
            .map_err(|_| fail(FailureKind::Worker, "Git input writer failed".into()))?;
        let status = status?;
        if !status.success() && !(matches!(mode, CommandMode::SyncPush) && push_needs_pull(&output))
        {
            return Err(CommandError::exited(operation, status.code(), &errors));
        }
        written.map_err(|error| fail(FailureKind::Io, error).with_exit_code(status.code()))?;
        if matches!(mode, CommandMode::Diagnostic) {
            output.extend_from_slice(&errors);
        }
        if output.len() > OUTPUT_LIMIT {
            return Err(fail(
                FailureKind::OutputLimit,
                "Git output exceeds the 2 MiB display limit".into(),
            )
            .with_exit_code(status.code()));
        }
        drop(errors);
        Ok(CommandOutput {
            status,
            stdout: output,
        })
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
    } else if operation == "stash" {
        "Check for conflicts, index/ref locks, file permissions and Git identity. Inspect the working tree and stash list before retrying."
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
    set_staged_paths(root, &[path.to_owned()], staged)
}

pub fn set_staged_paths(root: &Path, paths: &[String], staged: bool) -> Result<(), String> {
    if paths.is_empty() {
        return Err("Select files first.".into());
    }
    require_no_operation(root)?;
    let snapshot = load(root)?;
    let changes = paths
        .iter()
        .map(|path| {
            snapshot.changes.iter().find(|c| &c.path == path).ok_or(
                "A selected file changed since the last refresh. Refresh and select it again.",
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    if changes
        .iter()
        .any(|change| change.kind == ChangeKind::Conflicted)
    {
        return Err("Resolve conflicts outside SannyGit before staging this file.".into());
    }
    if changes
        .iter()
        .any(|change| (staged && !change.unstaged) || (!staged && !change.staged))
    {
        return Err("The file's staging state has changed. Refresh before retrying.".into());
    }
    // --revs-only succeeds with empty output for an unborn HEAD; other command
    // failures must propagate rather than being mistaken for a new repository.
    let has_head = !staged && !run(root, &["rev-parse", "--revs-only", "HEAD"])?.is_empty();
    let mut args = if staged {
        vec!["add", "--all", "--"]
    } else if has_head {
        vec!["restore", "--source=HEAD", "--staged", "--"]
    } else {
        // Before the first commit there is no HEAD to restore. Remove only index entries.
        vec!["update-index", "--force-remove", "--"]
    };
    for change in changes {
        args.push(&change.path);
        if let Some(source) = &change.original_path {
            // A copy must not stage or unstage its source; porcelain R is the only
            // status currently displayed as Renamed.
            if !staged && change.kind == ChangeKind::Renamed {
                args.push(source);
            }
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
    let branch = match execute(
        &path,
        &["symbolic-ref", "--quiet", "HEAD"],
        None,
        TIMEOUT,
        None,
        CommandMode::Normal,
    ) {
        Ok(output) => {
            let name = text(output.stdout)?;
            name.trim_end_matches('\n')
                .strip_prefix("refs/heads/")
                .ok_or("Unexpected HEAD reference")?
                .to_owned()
        }
        Err(error) if error.kind == FailureKind::Exit && error.exit_code == Some(1) => format!(
            "Detached at {}",
            text(run(&path, &["rev-parse", "--short", "HEAD"])?)?.trim()
        ),
        Err(error) => return Err(error.to_string()),
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
mod tests;
