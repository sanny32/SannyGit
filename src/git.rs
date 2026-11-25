//! Git command execution and parsing. Called only by the application's background workers.
use crate::{
    logging::{self, Level},
    models::{ChangeFile, ChangeKind, Repository},
};
#[cfg(windows)]
use std::sync::Arc;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 2 * 1024 * 1024;
const FILE_LIMIT: usize = 5_000;
const TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) mod diagnostics;
mod pipe;
use diagnostics::{CommandError, FailureKind};

/// A completed command, kept separate from user-facing diagnostics. In Sync
/// mode a reviewed porcelain rejection can have a non-success status.
struct CommandOutput {
    status: ExitStatus,
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

#[derive(Clone, Debug)]
pub(crate) struct GitContext {
    executable: String,
}

impl GitContext {
    pub(crate) fn new(executable: String) -> Self {
        Self {
            executable: if executable.is_empty() {
                "git".into()
            } else {
                executable
            },
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Sequencing {
    #[default]
    None,
    Merge,
    CherryPick,
    Revert,
    Rebase,
}

impl Sequencing {
    pub fn banner(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Merge => "Merge in progress",
            Self::CherryPick => "Cherry-pick in progress",
            Self::Revert => "Revert in progress",
            Self::Rebase => "Rebase in progress",
        }
    }

    pub fn abort_help(self) -> &'static str {
        match self {
            Self::None => "Abort the current operation",
            Self::Merge => "Abort the current merge",
            Self::CherryPick => "Abort the current cherry-pick",
            Self::Revert => "Abort the current revert",
            Self::Rebase => "Abort the current rebase",
        }
    }

    pub fn supports_skip(self) -> bool {
        matches!(self, Self::CherryPick | Self::Revert | Self::Rebase)
    }
}

pub struct Snapshot {
    pub repository: Repository,
    pub branch: String,
    /// Full HEAD commit id. Empty when the branch has no commits yet.
    pub head: String,
    pub changes: Vec<ChangeFile>,
    pub sequencing: Sequencing,
    pub merging: bool,
    pub merge_message: Option<String>,
}

pub(crate) fn run(git: &GitContext, path: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    run_controlled(git, path, args, TIMEOUT, None)
}

pub(crate) fn is_ancestor(
    git: &GitContext,
    path: &Path,
    ancestor: &str,
    descendant: &str,
) -> Result<bool, String> {
    match execute(
        git,
        path,
        &["merge-base", "--is-ancestor", ancestor, descendant],
        None,
        TIMEOUT,
        None,
        CommandMode::Normal,
    ) {
        Ok(_) => Ok(true),
        Err(error) if error.kind == FailureKind::Exit && error.exit_code == Some(1) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn run_controlled(
    git: &GitContext,
    path: &Path,
    args: &[&str],
    timeout: Duration,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<u8>, String> {
    execute(git, path, args, None, timeout, cancel, CommandMode::Normal)
        .map(|output| output.stdout)
        .map_err(|error| error.to_string())
}

/// Return whether Sync needs to pull after a non-fast-forward push rejection.
/// Other failures retain the normal sanitized errors.
pub(crate) fn run_sync_push(
    git: &GitContext,
    path: &Path,
    args: &[&str],
    cancel: &AtomicBool,
) -> Result<bool, String> {
    execute(
        git,
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
    git: &GitContext,
    path: &Path,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    execute(git, path, args, input, TIMEOUT, None, CommandMode::Normal)
        .map(|output| output.stdout)
        .map_err(|error| error.to_string())
}

pub(crate) fn refresh_checkout_line_endings(
    git: &GitContext,
    root: &Path,
    target: &str,
) -> Result<(), String> {
    let tracked = run(git, root, &["ls-files", "--stage", "-t", "-z"])?;
    let mut paths = Vec::new();
    for entry in tracked
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let tab = entry
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("Invalid tracked file metadata while refreshing line endings.")?;
        let (metadata, path) = entry.split_at(tab);
        // H marks ordinary cached entries. Sparse entries, symlinks and submodules
        // do not need line-ending conversion in this worktree.
        if (metadata.starts_with(b"H 100644 ") || metadata.starts_with(b"H 100755 "))
            && metadata.ends_with(b" 0")
        {
            paths.extend_from_slice(&path[1..]);
            paths.push(0);
        }
    }
    if paths.is_empty() {
        return Ok(());
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("Cannot create a temporary checkout index: {error}"))?
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("sannygit-reset-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory)
        .map_err(|error| format!("Cannot create a temporary checkout index: {error}"))?;
    let index = directory.join("index");
    let result = (|| {
        // A fresh index has no cached file stat information for checkout-index to trust.
        execute(
            git,
            root,
            &[
                "-c",
                "core.splitIndex=false",
                "read-tree",
                "--reset",
                "-i",
                target,
            ],
            None,
            TIMEOUT,
            None,
            CommandMode::AlternateIndex(&index),
        )?;
        execute(
            git,
            root,
            &["checkout-index", "--force", "--stdin", "-z"],
            Some(&paths),
            TIMEOUT,
            None,
            CommandMode::AlternateIndex(&index),
        )?;
        Ok::<(), CommandError>(())
    })();
    let _ = std::fs::remove_file(&index);
    let _ = std::fs::remove_file(directory.join("index.lock"));
    let _ = std::fs::remove_dir(&directory);
    result.map_err(|error| error.to_string())
}

/// Read-only commands with bounded stdin and the same cancellation/timeout handling.
pub(crate) fn run_with_input_controlled(
    git: &GitContext,
    path: &Path,
    args: &[&str],
    input: &[u8],
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    execute(
        git,
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
pub(crate) fn run_diagnostic(
    git: &GitContext,
    path: &Path,
    args: &[&str],
) -> Result<Vec<u8>, String> {
    execute(
        git,
        path,
        args,
        None,
        TIMEOUT,
        None,
        CommandMode::Diagnostic,
    )
    .map(|output| output.stdout)
    .map_err(|error| error.to_string())
}

/// Whole-repository stash operations have no user pathspecs. Git's stash cleanup
/// internally uses :/; inheriting literal pathspecs leaves untracked files behind.
pub(crate) fn run_stash(git: &GitContext, path: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = vec!["-c", "submodule.recurse=false", "stash"];
    command.extend_from_slice(args);
    execute(git, path, &command, None, TIMEOUT, None, CommandMode::Stash)
        .map(|output| output.stdout)
        .map_err(|error| error.to_string())
}

enum CommandMode<'a> {
    Normal,
    Diagnostic,
    SyncPush,
    Stash,
    AlternateIndex(&'a Path),
}

fn execute(
    git: &GitContext,
    path: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
    mode: CommandMode<'_>,
) -> Result<CommandOutput, CommandError> {
    let operation = if matches!(mode, CommandMode::Stash) {
        "stash"
    } else {
        diagnostics::operation(args)
    };
    let started = Instant::now();
    let result = execute_command(git, path, args, input, timeout, cancel, mode, operation);
    let milliseconds = started.elapsed().as_millis();
    match &result {
        Ok(output) => logging::record(
            Level::Debug,
            format_args!(
                "Git {operation} finished in {milliseconds} ms (exit code {:?})",
                output.status.code()
            ),
        ),
        Err(error) => logging::record(
            Level::Debug,
            format_args!(
                "Git {operation} failed in {milliseconds} ms ({:?}, exit code {:?})",
                error.kind, error.exit_code
            ),
        ),
    }
    result
}

#[expect(
    clippy::too_many_arguments,
    reason = "pass the worker Git context explicitly"
)]
fn execute_command(
    git: &GitContext,
    path: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
    mode: CommandMode<'_>,
    operation: &'static str,
) -> Result<CommandOutput, CommandError> {
    let fail = |kind, message: String| CommandError::new(operation, kind, message);
    if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) {
        return Err(fail(FailureKind::Cancelled, "Operation cancelled".into()));
    }
    let executable = &git.executable;
    let mut command = Command::new(executable);
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
    if let CommandMode::AlternateIndex(index) = mode {
        command.env("GIT_INDEX_FILE", index);
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
    // Kill the child if pipe setup fails before the wait thread owns it.
    // After the hand-off, Drop is a no-op and the waiter reaps the process.
    let mut child = ChildGuard(Some(child));
    let result = (|| {
        let input = input.map(<[u8]>::to_vec);
        let started = Instant::now();
        let deadline = started + timeout;
        let spawn_error = |error: std::io::Error| {
            fail(
                FailureKind::Worker,
                format!("Cannot start Git pipe worker: {error}"),
            )
        };
        let (wake_tx, wake_rx) = mpsc::channel();
        let (child_tx, child_rx) = mpsc::channel::<Child>();
        // Once Git has exited, taskkill must not target its PID: the id may
        // already belong to another process. Unix signals the process group.
        #[cfg(windows)]
        let parent_exited = Arc::new(AtomicBool::new(false));
        #[cfg(windows)]
        let exited_flag = Arc::clone(&parent_exited);
        let wait_tx = wake_tx.clone();
        let mut waiter = JoinGuard(Some(
            thread::Builder::new()
                .name("sannygit-git-wait".into())
                .spawn(move || {
                    let Ok(mut child) = child_rx.recv() else {
                        return;
                    };
                    let status = child.wait();
                    #[cfg(windows)]
                    if status.is_ok() {
                        exited_flag.store(true, Ordering::Release);
                    }
                    let _ = wait_tx.send(GitWake::Exit(status));
                })
                .map_err(spawn_error)?,
        ));
        // Shadow the sender so it drops before `waiter`. An early return then
        // unblocks the thread before the join, instead of waiting forever.
        let child_tx = child_tx;
        let writer = match (stdin, input) {
            (Some(stdin), Some(bytes)) => {
                let wake = wake_tx.clone();
                Some(
                    pipe::Worker::spawn(stdin, move |pipe, stop| {
                        let result = pipe::write(pipe, &bytes, stop);
                        let _ = wake.send(GitWake::Pipe);
                        result
                    })
                    .map_err(spawn_error)?,
                )
            }
            _ => None,
        };
        fn spawn_capture<P: pipe::Pipe + Read + Send + 'static>(
            pipe: P,
            wake_tx: &mpsc::Sender<GitWake>,
            spawn_error: &impl Fn(std::io::Error) -> CommandError,
        ) -> Result<pipe::Worker<Vec<u8>>, CommandError> {
            let wake = wake_tx.clone();
            pipe::Worker::spawn(pipe, move |pipe, stop| {
                let result = pipe::capture(pipe, stop);
                let _ = wake.send(GitWake::Pipe);
                result
            })
            .map_err(spawn_error)
        }
        let out = spawn_capture(stdout, &wake_tx, &spawn_error)?;
        let err = spawn_capture(stderr, &wake_tx, &spawn_error)?;
        let required_pipes = 2 + u8::from(writer.is_some());
        let Some(git_child) = child.0.take() else {
            return Err(fail(
                FailureKind::Io,
                "Cannot wait for Git: process is unavailable".into(),
            ));
        };
        #[cfg(windows)]
        let handle = {
            use std::os::windows::io::AsHandle;
            match git_child.as_handle().try_clone_to_owned() {
                Ok(handle) => handle,
                Err(error) => {
                    let mut git_child = git_child;
                    let _ = git_child.kill();
                    let _ = git_child.wait();
                    return Err(fail(
                        FailureKind::Io,
                        format!("Cannot wait for Git: {error}"),
                    ));
                }
            }
        };
        let pid = git_child.id();
        if let Err(mpsc::SendError(mut git_child)) = child_tx.send(git_child) {
            let _ = git_child.kill();
            let _ = git_child.wait();
            return Err(fail(
                FailureKind::Io,
                "Cannot wait for Git: wait thread stopped".into(),
            ));
        }
        let running = RunningGit {
            pid,
            #[cfg(windows)]
            parent_exited,
            #[cfg(windows)]
            handle,
        };
        let Some(waiter) = waiter.take() else {
            return Err(fail(
                FailureKind::Io,
                "Cannot wait for Git: wait thread stopped".into(),
            ));
        };
        let status = await_git(
            &wake_rx,
            &wake_tx,
            waiter,
            running,
            deadline,
            timeout,
            cancel,
            required_pipes,
            operation,
        )?;
        // Stop pipe workers on failure before joining: descendants may have
        // escaped process-tree termination or outlived the original Git PID.
        let mut output = out
            .finish()
            .map_err(|_| fail(FailureKind::Worker, "Git output reader failed".into()))?
            .map_err(|error| fail(FailureKind::Io, error))?;
        let errors = err
            .finish()
            .map_err(|_| fail(FailureKind::Worker, "Git error reader failed".into()))?
            .map_err(|error| fail(FailureKind::Io, error))?;
        let written = writer
            .map_or(Ok(Ok(())), pipe::Worker::finish)
            .map_err(|_| fail(FailureKind::Worker, "Git input writer failed".into()))?;
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
    })();
    result
}

enum GitWake {
    Exit(std::io::Result<ExitStatus>),
    Pipe,
    Cancel,
}

struct CancelWaiter {
    key: usize,
    id: u64,
    sender: mpsc::Sender<GitWake>,
}

static CANCEL_WAITERS: std::sync::Mutex<Vec<CancelWaiter>> = std::sync::Mutex::new(Vec::new());
static CANCEL_WAITER_ID: AtomicU64 = AtomicU64::new(1);

struct CancelWatch {
    id: u64,
}

impl Drop for CancelWatch {
    fn drop(&mut self) {
        CANCEL_WAITERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|waiter| waiter.id != self.id);
    }
}

fn watch_cancel(flag: &AtomicBool, sender: mpsc::Sender<GitWake>) -> CancelWatch {
    let id = CANCEL_WAITER_ID.fetch_add(1, Ordering::Relaxed);
    CANCEL_WAITERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(CancelWaiter {
            key: std::ptr::from_ref(flag) as usize,
            id,
            sender,
        });
    CancelWatch { id }
}

/// Mark a Git operation canceled and wake any command waiting on this flag.
pub(crate) fn signal_cancel(flag: &AtomicBool) {
    flag.store(true, Ordering::Release);
    let key = std::ptr::from_ref(flag) as usize;
    let senders: Vec<_> = CANCEL_WAITERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|waiter| waiter.key == key)
        .map(|waiter| waiter.sender.clone())
        .collect();
    for sender in senders {
        let _ = sender.send(GitWake::Cancel);
    }
}

struct ChildGuard(Option<Child>);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

struct JoinGuard(Option<thread::JoinHandle<()>>);

impl JoinGuard {
    fn take(&mut self) -> Option<thread::JoinHandle<()>> {
        self.0.take()
    }
}

impl Drop for JoinGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            let _ = handle.join();
        }
    }
}

struct RunningGit {
    pid: u32,
    #[cfg(windows)]
    parent_exited: Arc<AtomicBool>,
    #[cfg(windows)]
    handle: std::os::windows::io::OwnedHandle,
}

fn stop_git_tree(git: &RunningGit) {
    #[cfg(unix)]
    if let Ok(id) = i32::try_from(git.pid) {
        // SAFETY: this child was started in its own process group. The direct
        // signal matches Child::kill when the group id cannot be targeted.
        unsafe {
            libc::kill(-id, libc::SIGKILL);
            libc::kill(id, libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        if !git.parent_exited.load(Ordering::Acquire) {
            // An exited PID may already have been reused. Its pipes are
            // canceled by the workers without targeting that PID.
            use std::os::windows::process::CommandExt;
            let _ = Command::new("taskkill")
                .args(["/PID", &git.pid.to_string(), "/T", "/F"])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        use std::os::windows::io::AsRawHandle;
        #[link(name = "kernel32")]
        extern "system" {
            fn TerminateProcess(process: *mut std::ffi::c_void, exit_code: u32) -> i32;
        }
        // SAFETY: the handle was duplicated from this child and stays open
        // until the wait returns. It names the original process, not a reused PID.
        unsafe {
            TerminateProcess(git.handle.as_raw_handle(), 1);
        }
    }
}

/// Block until Git has exited and every pipe worker has reported completion.
///
/// Process exit, pipe completion and cancellation all arrive on `rx`.
#[expect(
    clippy::too_many_arguments,
    reason = "the wait, process, deadline and cancellation stay explicit at the call"
)]
fn await_git(
    rx: &mpsc::Receiver<GitWake>,
    wake_tx: &mpsc::Sender<GitWake>,
    waiter: thread::JoinHandle<()>,
    running: RunningGit,
    deadline: Instant,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
    required_pipes: u8,
    operation: &'static str,
) -> Result<ExitStatus, CommandError> {
    let fail = |kind, message: String| CommandError::new(operation, kind, message);
    let mut waiter = JoinGuard(Some(waiter));
    // Register before reading the flag so a cancel stored during registration
    // is still visible, and a later store wakes this wait immediately.
    let _watch = cancel.map(|flag| watch_cancel(flag, wake_tx.clone()));
    let mut status = None;
    let mut wait_error = None;
    let mut pipes = 0u8;
    let note = |wake: GitWake,
                status: &mut Option<ExitStatus>,
                wait_error: &mut Option<std::io::Error>,
                pipes: &mut u8| {
        match wake {
            GitWake::Exit(Ok(exit)) => *status = Some(exit),
            GitWake::Exit(Err(error)) => *wait_error = Some(error),
            GitWake::Pipe => *pipes = pipes.saturating_add(1),
            GitWake::Cancel => {}
        }
    };
    let join = |waiter: &mut JoinGuard| {
        if let Some(handle) = waiter.take() {
            let _ = handle.join();
        }
    };
    loop {
        while let Ok(wake) = rx.try_recv() {
            note(wake, &mut status, &mut wait_error, &mut pipes);
        }
        if pipes >= required_pipes {
            if let Some(exit) = status.take() {
                join(&mut waiter);
                return Ok(exit);
            }
        }
        let cancelled = cancel.is_some_and(|flag| flag.load(Ordering::Acquire));
        let timed_out = Instant::now() >= deadline;
        if cancelled || timed_out || wait_error.is_some() {
            stop_git_tree(&running);
            join(&mut waiter);
            return Err(match wait_error.take() {
                Some(error) => fail(FailureKind::Io, format!("Cannot wait for Git: {error}")),
                None if cancelled => fail(
                    FailureKind::Cancelled,
                    "Operation cancelled; refresh local and remote state before retrying".into(),
                ),
                None => fail(
                    FailureKind::TimedOut,
                    format!(
                        "Git operation timed out after {} seconds",
                        timeout.as_secs()
                    ),
                ),
            });
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(wake) => note(wake, &mut status, &mut wait_error, &mut pipes),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                stop_git_tree(&running);
                join(&mut waiter);
                return Err(fail(
                    FailureKind::Io,
                    "Cannot wait for Git: wait thread stopped".into(),
                ));
            }
        }
    }
}

fn command_error(operation: &str) -> String {
    // Do not parse localized diagnostics or echo private URLs and hook output.
    let reason = if operation == "commit" {
        "Check your Git identity, signing configuration and commit hooks. Refresh before retrying."
    } else if operation == "merge" {
        "Check local changes, repository permissions and Git identity. Refresh before retrying."
    } else if operation == "clone" {
        "Check the URL, destination, network access, credential helpers and SSH configuration. A partial destination may remain."
    } else if matches!(operation, "fetch" | "push") {
        "Check network access, Git credential helpers, SSH configuration and remote permissions. Push may also be rejected because the remote branch advanced, a hook refused it, or the remote is configured as a mirror. Refresh before retrying."
    } else if operation == "worktree" {
        "Check branch availability, worktree locks, local files and destination permissions. Refresh before retrying."
    } else if operation == "format-patch" {
        "Check that the tag points to a commit and that the selected directory is writable."
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

pub(crate) fn ignore_pattern(path: &str) -> Result<String, &'static str> {
    if path.is_empty()
        || path.ends_with('/')
        || path.contains(['\0', '\r', '\n'])
        || path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(".gitignore"))
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err("The selected path cannot be added to .gitignore.");
    }
    Ok(format!("/{}", escape_ignore_path(path)))
}

fn escape_ignore_path(path: &str) -> String {
    let mut pattern = String::new();
    for character in path.chars() {
        if matches!(character, '\\' | '*' | '?' | '[' | ']' | ' ') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}

pub(crate) fn valid_ignore_glob(pattern: &str) -> bool {
    !pattern.trim().is_empty()
        && pattern.trim_end() == pattern
        && !pattern.ends_with('\\')
        && !pattern.contains(['/', '\0', '\r', '\n'])
}

pub(crate) fn ignore_paths(
    git: &GitContext,
    root: &Path,
    paths: &[String],
    directory: &Path,
    glob: Option<&str>,
) -> Result<(), String> {
    if paths.is_empty() {
        return Err("Select untracked files to ignore.".into());
    }
    let mut patterns = Vec::new();
    for path in paths {
        let exact = ignore_pattern(path)?;
        if directory.as_os_str().is_empty() {
            patterns.push(exact);
        } else {
            let file = Path::new(path);
            if file.parent() != Some(directory) {
                return Err("The selected files must share the ignore file's folder.".into());
            }
            let name = path.rsplit('/').next().unwrap_or(path);
            patterns.push(ignore_pattern(name)?);
        }
    }
    if let Some(glob) = glob {
        let parent = Path::new(&paths[0]).parent();
        if !valid_ignore_glob(glob) || paths.iter().any(|path| Path::new(path).parent() != parent) {
            return Err("Enter a filename pattern for files from one folder.".into());
        }
        let prefix = if directory.as_os_str().is_empty() {
            paths[0]
                .rsplit_once('/')
                .map(|(folder, _)| escape_ignore_path(folder))
        } else {
            None
        };
        patterns = vec![match prefix {
            Some(folder) => format!("/{folder}/{glob}"),
            None => format!("/{glob}"),
        }];
    }
    let untracked: BTreeSet<_> = load_changes(git, root)?
        .into_iter()
        .filter(|change| change.kind == ChangeKind::Untracked)
        .map(|change| change.path)
        .collect();
    if paths.iter().any(|path| !untracked.contains(path)) {
        return Err("A selected file is no longer untracked. Refresh and select it again.".into());
    }

    use std::io::{Seek, SeekFrom, Write as _};

    let relative_target = directory.join(".gitignore");
    let target = crate::discard::safe_path(
        root,
        relative_target.to_str().ok_or("Invalid .gitignore path")?,
    )?;
    let update = || -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.create(true).read(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
        }
        let mut file = options.open(target)?;
        let metadata = file.metadata()?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(std::io::Error::other(
                    "The .gitignore cannot be a reparse point",
                ));
            }
        }
        if !metadata.is_file() {
            return Err(std::io::Error::other(
                "The .gitignore must be a regular file",
            ));
        }

        let mut contents = String::new();
        if metadata.len() > 0 {
            file.seek(SeekFrom::End(-1))?;
            let mut last = [0];
            file.read_exact(&mut last)?;
            if last[0] != b'\n' {
                contents.push('\n');
            }
        }
        for pattern in patterns {
            contents.push_str(&pattern);
            contents.push('\n');
        }
        file.write_all(contents.as_bytes())
    };
    update().map_err(|error| format!("Cannot update .gitignore: {error}"))
}

#[cfg(test)]
pub fn set_staged(git: &GitContext, root: &Path, path: &str, staged: bool) -> Result<(), String> {
    set_staged_paths(git, root, &[path.to_owned()], staged)
}

/// Stages or unstages the selected paths while preserving working-tree bytes.
pub fn set_staged_paths(
    git: &GitContext,
    root: &Path,
    paths: &[String],
    staged: bool,
) -> Result<(), String> {
    if paths.is_empty() {
        return Err("Select files first.".into());
    }
    let merging = check_operation(git, root, staged)?;
    let mut status = load_path_changes(git, root, paths)?;
    // A scoped status can split a rename into a deletion or an addition.
    // Resolve those cases globally before allowing either side to change.
    if status.iter().any(|change| {
        matches!(change.kind, ChangeKind::Deleted | ChangeKind::Renamed)
            || (!staged && change.staged && change.kind == ChangeKind::Added)
    }) {
        status = load_changes(git, root)?;
    }
    let by_path: BTreeMap<_, _> = status.iter().map(|c| (c.path.as_str(), c)).collect();
    let mut changes = Vec::new();
    let mut ignored = Vec::new();
    for path in paths {
        if let Some(change) = by_path.get(path.as_str()).copied() {
            changes.push(change);
        } else {
            ignored.push(path.as_str());
        }
    }
    if !ignored.is_empty()
        && (!staged || {
            let ignored_now = ignored_paths(git, root, &ignored)?;
            ignored.iter().any(|path| !ignored_now.contains(*path))
        })
    {
        return Err(
            "A selected file changed since the last refresh. Refresh and select it again.".into(),
        );
    }
    if changes
        .iter()
        .any(|change| change.kind == ChangeKind::Conflicted && !merging)
    {
        return Err("Resolve conflicts outside SannyGit before staging this file.".into());
    }
    if !changes.is_empty()
        && changes.iter().any(|change| {
            (staged && !change.unstaged && change.kind != ChangeKind::Conflicted)
                || (!staged && !change.staged)
        })
    {
        return Err("The file's staging state has changed. Refresh before retrying.".into());
    }
    if changes.is_empty() {
        return force_add_ignored(git, root, &ignored);
    }
    // --revs-only succeeds with empty output for an unborn HEAD; other command
    // failures must propagate rather than being mistaken for a new repository.
    let has_head = !staged && !run(git, root, &["rev-parse", "--revs-only", "HEAD"])?.is_empty();
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
    run(git, root, &args)?;
    if !ignored.is_empty() {
        force_add_ignored(git, root, &ignored)?;
    }
    Ok(())
}

/// Unstage paths the way SmartGit does: one Git command and no status read.
/// `has_head` comes from the snapshot already shown in the file list.
pub(crate) fn unstage_paths(
    git: &GitContext,
    root: &Path,
    paths: &[String],
    has_head: bool,
) -> Result<(), String> {
    if paths.is_empty() {
        return Err("Select files first.".into());
    }
    let mut args = if has_head {
        vec!["reset", "-q", "HEAD", "--"]
    } else {
        vec!["rm", "--cached", "--force", "--"]
    };
    for path in paths {
        args.push(path);
    }
    run(git, root, &args).map(|_| ())
}

fn ignored_paths(
    git: &GitContext,
    root: &Path,
    paths: &[&str],
) -> Result<BTreeSet<String>, String> {
    let mut args = vec![
        "ls-files",
        "-z",
        "--others",
        "--ignored",
        "--exclude-standard",
    ];
    if !paths.is_empty() {
        args.push("--");
        args.extend_from_slice(paths);
    }
    let bytes = run(git, root, &args)?;
    if bytes.is_empty() {
        return Ok(BTreeSet::new());
    }
    let bytes = bytes
        .strip_suffix(&[0])
        .ok_or("Truncated ignored-file list")?;
    bytes
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            std::str::from_utf8(path)
                .map(str::to_owned)
                .map_err(|_| "Non-UTF-8 file names are not supported yet".to_owned())
        })
        .collect()
}

pub fn ignored_changes(git: &GitContext, root: &Path) -> Result<Vec<ChangeFile>, String> {
    let mut changes = Vec::new();
    for path in ignored_paths(git, root, &[])? {
        changes.push(ChangeFile {
            path,
            original_path: None,
            kind: ChangeKind::Ignored,
            conflict_status: None,
            staged: false,
            unstaged: true,
            diff: String::new(),
        });
        if changes.len() > FILE_LIMIT {
            return Err("Repository exceeds the 5,000 changed-file display limit".into());
        }
    }
    Ok(changes)
}

fn force_add_ignored(git: &GitContext, root: &Path, paths: &[&str]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut args = vec!["add", "--force", "--"];
    args.extend_from_slice(paths);
    run(git, root, &args).map(|_| ())
}

/// Move or rename one file inside the repository. Tracked files stay tracked.
pub fn move_path(git: &GitContext, root: &Path, from: &str, to: &str) -> Result<(), String> {
    if from == to {
        return Err("Choose a different path.".into());
    }
    require_no_operation(git, root)?;
    let source = crate::discard::safe_path(root, from)?;
    let destination = crate::discard::safe_path(root, to)?;
    if destination.symlink_metadata().is_ok() {
        return Err("The destination already exists.".into());
    }
    if let Some(parent) = destination.parent() {
        match parent.symlink_metadata() {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err("The destination directory is not a normal directory.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err("Create the destination directory before moving the file.".into());
            }
            Err(error) => return Err(format!("Cannot inspect the destination directory: {error}")),
        }
    }
    let status = load_changes(git, root)?;
    let change = status.iter().find(|change| change.path == from);
    let ignored = change.is_none() && ignored_paths(git, root, &[])?.contains(from);
    match change {
        Some(change) if change.kind == ChangeKind::Conflicted => {
            Err("Resolve the conflict before renaming this file.".into())
        }
        Some(change) if change.kind == ChangeKind::Deleted => {
            Err("A deleted file has no working copy to rename.".into())
        }
        Some(change) if change.kind == ChangeKind::Untracked || ignored => {
            std::fs::rename(&source, &destination)
                .map_err(|error| format!("Could not rename {from}: {error}"))
        }
        Some(_) => run(git, root, &["mv", "--", from, to]).map(|_| ()),
        None if ignored => std::fs::rename(&source, &destination)
            .map_err(|error| format!("Could not rename {from}: {error}")),
        None => Err(
            "The selected file changed since the last refresh. Refresh and select it again.".into(),
        ),
    }
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

#[derive(Clone, Debug)]
pub struct CommitPatchOptions {
    pub max_new_file_content_bytes: usize,
    pub path_excludes: Vec<String>,
}

impl Default for CommitPatchOptions {
    fn default() -> Self {
        Self {
            max_new_file_content_bytes: crate::llm::DEFAULT_MAX_NEW_FILE_CONTENT_BYTES as usize,
            path_excludes: Vec::new(),
        }
    }
}

impl From<&crate::llm::Config> for CommitPatchOptions {
    fn from(config: &crate::llm::Config) -> Self {
        Self {
            path_excludes: config.patch_path_excludes(),
            ..Self::default()
        }
    }
}

/// Read-only preview of the resulting commit relative to its first parent.
/// For amend this includes the existing commit as well as the selected changes.
pub fn commit_patch(
    git: &GitContext,
    root: &Path,
    contents: CommitContents,
    amend_head: Option<&str>,
    options: CommitPatchOptions,
) -> Result<String, String> {
    let head = text(run(git, root, &["rev-parse", "--revs-only", "HEAD"])?)?;
    let base = if let Some(expected) = amend_head {
        check_amend_head(git, root, expected)?;
        let parents = text(run(
            git,
            root,
            &["rev-list", "--parents", "-n", "1", expected],
        )?)?;
        parents.split_whitespace().nth(1).map(str::to_owned)
    } else {
        let head = head.trim();
        (!head.is_empty()).then(|| head.to_owned())
    };
    let base = match base {
        Some(base) => base,
        None => text(run_with_input(
            git,
            root,
            &["hash-object", "-t", "tree", "--stdin"],
            Some(b""),
        )?)?
        .trim()
        .to_owned(),
    };
    let mut arg_storage = vec![
        "diff".to_owned(),
        "--no-ext-diff".to_owned(),
        "--no-textconv".to_owned(),
    ];
    if contents == CommitContents::Staged {
        arg_storage.push("--cached".to_owned());
    }
    arg_storage.push(base);
    arg_storage.push("--".to_owned());
    arg_storage.push(".".to_owned());
    for pattern in &options.path_excludes {
        arg_storage.push(format!(":(exclude){pattern}"));
    }
    let args: Vec<&str> = arg_storage.iter().map(String::as_str).collect();
    let mut patch = text(run(git, root, &args)?)?;
    if contents == CommitContents::All {
        // Preview new files without staging them or modifying the real index.
        for change in load(git, root)?.changes {
            if change.kind == ChangeKind::Untracked {
                if path_excluded(&change.path, &options.path_excludes) {
                    continue;
                }
                if omit_untracked_patch_content(
                    root,
                    &change.path,
                    options.max_new_file_content_bytes,
                ) {
                    writeln!(
                        patch,
                        "\nNew file: {} (content omitted; use the path only)",
                        change.path
                    )
                    .map_err(|e| e.to_string())?;
                } else {
                    writeln!(patch, "\nNew file: {}", change.path).map_err(|e| e.to_string())?;
                    patch.push_str(&diff(git, root, &change.path, change.kind, None)?);
                }
                if patch.len() > OUTPUT_LIMIT {
                    return Err("Commit diff exceeds the 2 MiB limit".into());
                }
            }
        }
    }
    Ok(patch)
}

/// Commits explicitly selected contents after a UI action.
pub fn commit(
    git: &GitContext,
    root: &Path,
    message: &str,
    contents: CommitContents,
) -> Result<(), String> {
    commit_with_mode(git, root, message, false, contents)
}

/// Loads HEAD explicitly rather than relying on the filtered journal selection.
pub fn amend_message(git: &GitContext, root: &Path) -> Result<(String, String), String> {
    require_no_operation(git, root)?;
    let head = text(run(git, root, &["rev-parse", "--revs-only", "HEAD"])?)?;
    let head = head.trim();
    if head.is_empty() {
        return Err("There is no commit to amend.".into());
    }
    let message = text(run(
        git,
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
    git: &GitContext,
    root: &Path,
    message: &str,
    expected_head: &str,
    contents: CommitContents,
) -> Result<(), String> {
    check_amend_head(git, root, expected_head)?;
    commit_with_mode(git, root, message, true, contents)
}

fn check_amend_head(git: &GitContext, root: &Path, expected_head: &str) -> Result<(), String> {
    let head = text(run(git, root, &["rev-parse", "--verify", "HEAD"])?)?;
    if head.trim() != expected_head {
        return Err("HEAD has changed. Turn off Amend last commit and enable it again.".into());
    }
    require_no_operation(git, root)
}

/// Resolve markers through Git so linked worktrees use their own operation state.
pub(crate) fn require_no_operation(git: &GitContext, root: &Path) -> Result<(), String> {
    check_operation(git, root, false).map(|_| ())
}

pub(crate) fn operation_directory(git: &GitContext, root: &Path) -> Result<PathBuf, String> {
    let directory = text(run(git, root, &["rev-parse", "--absolute-git-dir"])?)?;
    Ok(PathBuf::from(
        directory.strip_suffix('\n').unwrap_or(&directory),
    ))
}

pub(crate) fn check_operation(
    git: &GitContext,
    root: &Path,
    allow_merge: bool,
) -> Result<bool, String> {
    // These operation markers belong to the per-worktree Git directory, not
    // the shared common directory. Resolve it once, including for linked trees.
    let directory = operation_directory(git, root)?;
    let mut merging = false;
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        match directory.join(marker).try_exists() {
            Ok(false) => {}
            Ok(true)
                if allow_merge && matches!(marker, "MERGE_HEAD" | "CHERRY_PICK_HEAD" | "REVERT_HEAD") =>
            {
                merging = true;
            }
            Ok(true) => return Err("Finish the in-progress merge, rebase, cherry-pick or revert outside SannyGit before retrying this action.".into()),
            Err(error) => return Err(format!("Could not check unfinished Git operations: {error}")),
        }
    }
    Ok(merging)
}

/// Porcelain status omits ignored files; reject collisions before Git can
/// replace them during a merge or a rebase's internal reset.
pub(crate) fn require_no_ignored_collisions(
    git: &GitContext,
    root: &Path,
    target: &str,
) -> Result<(), String> {
    let ignored = text(run(
        git,
        root,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
        ],
    )?)?;
    require_no_path_collisions(git, root, target, &ignored, "Ignored local files")
}

pub(crate) fn require_no_path_collisions(
    git: &GitContext,
    root: &Path,
    target: &str,
    local_paths: &str,
    file_kind: &str,
) -> Result<(), String> {
    if !local_paths.is_empty() {
        let paths = text(run(
            git,
            root,
            &["ls-tree", "-r", "--name-only", "-z", target],
        )?)?;
        let collisions = path_collisions(local_paths, &paths);
        if !collisions.is_empty() {
            const DISPLAY_LIMIT: usize = 20;
            let mut message = format!(
                "{file_kind} overlap paths in the selected commit. Move the listed files outside the repository and retry.\n\nGit diagnostic:\n"
            );
            for path in collisions.iter().take(DISPLAY_LIMIT) {
                let _ = writeln!(message, "{path:?}");
            }
            if collisions.len() > DISPLAY_LIMIT {
                let _ = write!(
                    message,
                    "\n{} more paths.",
                    collisions.len() - DISPLAY_LIMIT
                );
            }
            return Err(message);
        }
    }
    Ok(())
}

fn path_collisions<'a>(local_paths: &'a str, target_paths: &str) -> Vec<&'a str> {
    let normalize = |path: &str| {
        if cfg!(windows) {
            path.to_lowercase()
        } else {
            path.to_owned()
        }
    };
    let targets: BTreeSet<_> = target_paths.split_terminator('\0').map(normalize).collect();
    local_paths
        .split_terminator('\0')
        .filter(|path| {
            let path = normalize(path);
            let mut ancestor = path.as_str();
            loop {
                if targets.contains(ancestor) {
                    return true;
                }
                let Some((parent, _)) = ancestor.rsplit_once('/') else {
                    break;
                };
                ancestor = parent;
            }
            let prefix = format!("{path}/");
            targets
                .range(prefix.clone()..)
                .next()
                .is_some_and(|entry| entry.starts_with(&prefix))
        })
        .collect()
}

fn commit_with_mode(
    git: &GitContext,
    root: &Path,
    message: &str,
    amend: bool,
    contents: CommitContents,
) -> Result<(), String> {
    validate_message(message)?;
    let merging = check_operation(git, root, !amend)?;
    let snapshot = load(git, root)?;
    if snapshot
        .changes
        .iter()
        .any(|c| c.kind == ChangeKind::Conflicted)
    {
        return Err("Resolve all conflicts before committing.".into());
    }
    if snapshot.changes.is_empty() && !amend && !merging {
        return Err("There are no changes to commit.".into());
    }
    if contents == CommitContents::Staged
        && !amend
        && !merging
        && !snapshot.changes.iter().any(|c| c.staged)
    {
        return Err("There are no staged changes to commit.".into());
    }
    let cleaned;
    let message = if merging {
        cleaned = text(run_with_input(
            git,
            root,
            &["stripspace", "--strip-comments"],
            Some(message.as_bytes()),
        )?)?;
        validate_message(&cleaned)?;
        &cleaned
    } else {
        message
    };
    if contents == CommitContents::All {
        run(git, root, &["add", "--all"])?;
    }
    // stdin preserves multiline messages without a shell or a message file.
    // Git's configured identity, hooks and signing remain in effect.
    let mut args = vec!["commit", "--file=-", "--cleanup=whitespace"];
    if amend {
        args.push("--amend");
    }
    run_with_input(git, root, &args, Some(message.as_bytes())).map(|_| ())
}

pub(crate) fn text(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| "Non-UTF-8 Git output is not supported yet".into())
}

pub fn load(git: &GitContext, path: &Path) -> Result<Snapshot, String> {
    let root = text(run(git, path, &["rev-parse", "--show-toplevel"])?)?;
    // Git appends exactly one line ending; preserve whitespace in directory names.
    let root = root.strip_suffix('\n').unwrap_or(&root);
    let path = PathBuf::from(root);
    let branch = match execute(
        git,
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
            text(run(git, &path, &["rev-parse", "--short", "HEAD"])?)?.trim()
        ),
        Err(error) => return Err(error.to_string()),
    };
    // The branch name does not change when another program commits on this branch.
    let head = text(run(git, &path, &["rev-parse", "--revs-only", "HEAD"])?)?
        .trim()
        .to_owned();
    let changes = load_changes(git, &path)?;
    let directory = operation_directory(git, &path)?;
    let sequencing = detect_sequencing(&directory)?;
    let merging = sequencing != Sequencing::None;
    let merge_message = if merging {
        read_merge_message(&directory)?
    } else {
        None
    };
    Ok(Snapshot {
        repository: Repository {
            name: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.into()),
            path: root.into(),
        },
        branch,
        head,
        changes,
        sequencing,
        merging,
        merge_message,
    })
}

fn detect_sequencing(directory: &Path) -> Result<Sequencing, String> {
    for marker in ["rebase-merge", "rebase-apply"] {
        if directory
            .join(marker)
            .try_exists()
            .map_err(|error| format!("Could not check rebase state: {error}"))?
        {
            return Ok(Sequencing::Rebase);
        }
    }
    for (marker, sequencing) in [
        ("MERGE_HEAD", Sequencing::Merge),
        ("CHERRY_PICK_HEAD", Sequencing::CherryPick),
        ("REVERT_HEAD", Sequencing::Revert),
    ] {
        if directory
            .join(marker)
            .try_exists()
            .map_err(|error| format!("Could not check merge state: {error}"))?
        {
            return Ok(sequencing);
        }
    }
    Ok(Sequencing::None)
}

/// Shared preflight for cherry-pick, revert, and similar apply-commit operations.
pub(crate) fn preflight_apply_commit(
    git: &GitContext,
    root: &Path,
    expected_head: &str,
    expected_branch: &str,
    target: &str,
) -> Result<(), String> {
    if !matches!(target.len(), 40 | 64) || !target.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Select a valid commit before continuing.".into());
    }
    require_no_operation(git, root)?;
    let head = text(run(git, root, &["rev-parse", "--verify", "HEAD^{commit}"])?)?;
    if head.trim() != expected_head {
        return Err("HEAD changed. Refresh the repository and retry.".into());
    }
    let snapshot = load(git, root)?;
    if snapshot.branch != expected_branch {
        return Err("The current branch changed. Refresh the repository and retry.".into());
    }
    if !snapshot.changes.is_empty() {
        let untracked = snapshot
            .changes
            .iter()
            .filter(|change| change.kind == ChangeKind::Untracked)
            .map(|change| change.path.as_str())
            .collect::<Vec<_>>()
            .join("\0");
        if !untracked.is_empty() {
            require_no_path_collisions(git, root, target, &untracked, "Untracked files")?;
        }
        if snapshot
            .changes
            .iter()
            .any(|change| change.kind != ChangeKind::Untracked)
        {
            return Err("Commit or stash local changes before continuing.".into());
        }
    }
    require_no_ignored_collisions(git, root, target)?;
    Ok(())
}

fn read_merge_message(directory: &Path) -> Result<Option<String>, String> {
    let file = match std::fs::File::open(directory.join("MERGE_MSG")) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not open the merge message: {error}")),
    };
    let mut raw = Vec::new();
    file.take(OUTPUT_LIMIT as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|error| format!("Could not read the merge message: {error}"))?;
    if raw.len() > OUTPUT_LIMIT {
        return Ok(None);
    }
    let Ok(message) = String::from_utf8(raw) else {
        return Ok(None);
    };
    let message = message.trim_end_matches(['\r', '\n']).replace("\r\n", "\n");
    Ok(validate_message(&message).is_ok().then_some(message))
}

pub(crate) fn load_changes(git: &GitContext, path: &Path) -> Result<Vec<ChangeFile>, String> {
    load_path_changes(git, path, &[])
}

pub(crate) fn load_path_changes(
    git: &GitContext,
    root: &Path,
    paths: &[String],
) -> Result<Vec<ChangeFile>, String> {
    let mut args = vec![
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--ignore-submodules=none",
    ];
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    parse_status(&run(git, root, &args)?)
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
            conflict_status: match [x, y] {
                [b'D', b'D'] => Some("Both deleted"),
                [b'A', b'U'] => Some("Added by us"),
                [b'U', b'D'] => Some("Deleted by them"),
                [b'U', b'A'] => Some("Added by them"),
                [b'D', b'U'] => Some("Deleted by us"),
                [b'A', b'A'] => Some("Both added"),
                [b'U', b'U'] => Some("Both modified"),
                _ => None,
            },
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

fn path_excluded(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return false;
        }
        if let Some(suffix) = pattern.strip_prefix('*') {
            path.ends_with(suffix)
        } else {
            path == pattern || path.ends_with(&format!("/{pattern}"))
        }
    })
}

fn omit_untracked_patch_content(root: &Path, path: &str, max_content_bytes: usize) -> bool {
    if max_content_bytes == 0 || untracked_binary_path(path) {
        return true;
    }
    let file = root.join(path);
    let metadata = match std::fs::symlink_metadata(&file) {
        Ok(metadata) => metadata,
        Err(_) => return true,
    };
    if !metadata.is_file() {
        return true;
    }
    metadata.len() > max_content_bytes as u64
}

fn untracked_binary_path(path: &str) -> bool {
    let Some(ext) = Path::new(path).extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    [
        "otf", "ttf", "woff", "woff2", "eot", "png", "jpg", "jpeg", "gif", "webp", "ico", "icns",
        "pdf", "zip", "gz", "exe", "dll", "dylib", "so", "wasm",
    ]
    .iter()
    .any(|kind| ext.eq_ignore_ascii_case(kind))
}

pub fn diff(
    git: &GitContext,
    root: &Path,
    path: &str,
    kind: ChangeKind,
    original_path: Option<&str>,
) -> Result<String, String> {
    file_diff(git, root, path, kind, original_path, false)
}

pub fn file_diff(
    git: &GitContext,
    root: &Path,
    path: &str,
    kind: ChangeKind,
    original_path: Option<&str>,
    ignore_whitespace: bool,
) -> Result<String, String> {
    if kind == ChangeKind::Untracked || kind == ChangeKind::Ignored {
        let file = root.join(path);
        let metadata = std::fs::symlink_metadata(&file).map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Ok("Preview is unavailable for directories and symbolic links".into());
        }
        if metadata.len() > OUTPUT_LIMIT as u64 {
            return Ok("File exceeds the 2 MiB preview limit — preview unavailable".into());
        }
        if untracked_binary_path(path) {
            return Ok("Binary file — text preview unavailable".into());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(file)
            .map_err(|e| e.to_string())?
            .take((OUTPUT_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > OUTPUT_LIMIT {
            return Ok("File exceeds the 2 MiB preview limit — preview unavailable".into());
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
    if ignore_whitespace {
        index_args.insert(1, "--ignore-space-change");
    }
    if let Some(source) = original_path {
        index_args.push(source);
    }
    let index = text(run(git, root, &index_args)?)?;
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
    if ignore_whitespace {
        worktree_args.insert(1, "--ignore-space-change");
    }
    if let Some(source) = original_path {
        worktree_args.push(source);
    }
    let worktree = text(run(git, root, &worktree_args)?)?;
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
