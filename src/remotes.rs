//! Explicit remote operations. No credentials are read, stored or logged here.
use crate::{branches, git, models::RefKind};
use std::{path::Path, sync::atomic::AtomicBool, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Fetch,
    Pull,
    Push,
}
impl Operation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fetch => "Fetch",
            Self::Pull => "Pull",
            Self::Push => "Push",
        }
    }
    pub fn success(self) -> &'static str {
        match self {
            Self::Fetch => "Fetch complete",
            Self::Pull => "Pull complete",
            Self::Push => "Push complete",
        }
    }
}

#[derive(Clone, Default)]
pub struct State {
    pub remotes: Vec<String>,
    pub branch: Option<String>,
    pub upstream_remote: Option<String>,
    pub upstream_ref: Option<String>,
}

pub fn load(root: &Path) -> Result<State, String> {
    // `remote` lists only names, never URLs or credentials.
    let names = git::text(git::run(root, &["remote"])?)?;
    let remotes = names.lines().map(str::to_owned).collect::<Vec<_>>();
    if remotes
        .iter()
        .any(|name| name.is_empty() || name.starts_with('-') || name.chars().any(char::is_control))
    {
        return Err("Unsupported remote name".into());
    }
    let refs = branches::load(root)?;
    let branch = refs
        .refs
        .iter()
        .find(|r| r.kind == RefKind::Local && r.current)
        .map(|r| r.name.clone());
    let mut state = State {
        remotes,
        branch,
        ..Default::default()
    };
    if let Some(branch) = &state.branch {
        let output = git::text(git::run(
            root,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(upstream:remotename)%00%(upstream:remoteref)%00",
                &format!("refs/heads/{branch}"),
            ],
        )?)?;
        for line in output.lines() {
            let parts = line.split('\0').collect::<Vec<_>>();
            if parts.len() != 4 {
                return Err("Invalid upstream metadata".into());
            }
            if parts[0] == format!("refs/heads/{branch}")
                && !parts[1].is_empty()
                && !parts[2].is_empty()
            {
                state.upstream_remote = Some(parts[1].into());
                state.upstream_ref = parts[2].strip_prefix("refs/heads/").map(str::to_owned);
            }
        }
    }
    Ok(state)
}

fn head(root: &Path) -> Result<String, String> {
    let oid = git::text(git::run(root, &["rev-parse", "--verify", "HEAD^{commit}"])?)?;
    Ok(oid.trim().into())
}

fn require_pull_ready(root: &Path) -> Result<(), String> {
    if !git::load(root)?.changes.is_empty() {
        return Err("Commit or move aside local changes before pulling.".into());
    }
    git::require_no_operation(root)
}

pub fn run(
    root: &Path,
    operation: Operation,
    remote: &str,
    target: &str,
    expected_branch: Option<&str>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let state = load(root)?;
    if !state.remotes.iter().any(|name| name == remote) {
        return Err("Choose a configured remote, then refresh before retrying.".into());
    }
    let timeout = Duration::from_secs(600);
    if operation == Operation::Fetch {
        git::run_controlled(
            root,
            &["fetch", "--no-recurse-submodules", "--", remote],
            timeout,
            Some(cancel),
        )?;
        return Ok(());
    }
    let branch = state
        .branch
        .as_deref()
        .ok_or("Check out a local branch with commits first.")?;
    if Some(branch) != expected_branch {
        return Err("The current branch changed. Reopen the operation dialog.".into());
    }
    if let Some(reason) = branches::name_hint(target) {
        return Err(reason.into());
    }
    let destination = format!("refs/heads/{target}");
    git::run(root, &["check-ref-format", &destination])?;
    let original = head(root)?;
    match operation {
        Operation::Fetch => unreachable!(),
        Operation::Push => {
            // Push only the reviewed branch tip, never configured mirror/force/tag refspecs.
            git::run_controlled(
                root,
                &[
                    "push",
                    "--porcelain",
                    "--no-force",
                    "--no-mirror",
                    "--no-follow-tags",
                    "--recurse-submodules=no",
                    "--",
                    remote,
                    &format!("{original}:{destination}"),
                ],
                timeout,
                Some(cancel),
            )?;
        }
        Operation::Pull => {
            require_pull_ready(root)?;
            git::run_controlled(
                root,
                &[
                    "fetch",
                    "--no-recurse-submodules",
                    "--no-tags",
                    "--",
                    remote,
                    &destination,
                ],
                timeout,
                Some(cancel),
            )?;
            let fetched = git::text(git::run(
                root,
                &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
            )?)?;
            let current = load(root)?;
            if current.branch.as_deref() != Some(branch) || head(root)? != original {
                return Err(
                    "The local branch changed while fetching. Refresh before retrying.".into(),
                );
            }
            require_pull_ready(root)?;
            if git::run(
                root,
                &["merge-base", "--is-ancestor", &original, fetched.trim()],
            )
            .is_err()
            {
                return Err("Pull requires a fast-forward. The branches diverged; merge or rebase them separately.".into());
            }
            git::run_controlled(root, &["-c", "submodule.recurse=false", "merge", "--ff-only", "--no-edit", "--no-autostash", "--no-overwrite-ignore", "--", fetched.trim()], timeout, Some(cancel))
                .map_err(|error| format!("Could not fast-forward: {error} Check ignored-file conflicts, Git hooks, repository locks and permissions; refresh before retrying."))?;
        }
    }
    Ok(())
}

pub fn set_upstream(root: &Path, local: &str, remote: &str, target: &str) -> Result<(), String> {
    let state = load(root)?;
    if state.branch.as_deref() != Some(local) || !state.remotes.iter().any(|r| r == remote) {
        return Err("The branch or remote changed. Refresh before setting upstream.".into());
    }
    if let Some(reason) = branches::name_hint(target) {
        return Err(reason.into());
    }
    git::run(
        root,
        &[
            "branch",
            &format!("--set-upstream-to=refs/remotes/{remote}/{target}"),
            "--",
            local,
        ],
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::TempRepo;
    use std::{fs, path::PathBuf};

    fn command(root: &Path, args: &[&str]) -> Vec<u8> {
        let mut configured = vec!["-c", "core.autocrlf=false"];
        configured.extend_from_slice(args);
        git::run(root, &configured).unwrap()
    }
    fn identity(root: &Path) {
        command(root, &["config", "core.autocrlf", "false"]);
        command(root, &["config", "user.name", "Remote Test"]);
        command(
            root,
            &["config", "user.email", "remote-test@example.invalid"],
        );
    }
    fn commit(root: &Path, text: &str) {
        fs::write(root.join("file"), text).unwrap();
        command(root, &["add", "file"]);
        command(root, &["commit", "-m", text]);
    }
    fn fixture() -> (TempRepo, PathBuf, PathBuf) {
        let temp = TempRepo::new();
        let remote = temp.0.join("remote.git");
        let source = temp.0.join("source");
        let other = temp.0.join("other");
        command(
            &temp.0,
            &[
                "init",
                "--bare",
                "--initial-branch=main",
                remote.to_str().unwrap(),
            ],
        );
        command(
            &temp.0,
            &[
                "clone",
                "--",
                remote.to_str().unwrap(),
                source.to_str().unwrap(),
            ],
        );
        identity(&source);
        commit(&source, "base\n");
        run(
            &source,
            Operation::Push,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false),
        )
        .unwrap();
        command(
            &temp.0,
            &[
                "clone",
                "--",
                remote.to_str().unwrap(),
                other.to_str().unwrap(),
            ],
        );
        identity(&other);
        (temp, source, other)
    }

    #[test]
    fn explicit_push_and_fetch_preserve_worktree_and_do_not_push_tags_or_other_branches() {
        let (temp, source, other) = fixture();
        let state = load(&other).unwrap();
        assert_eq!(state.branch.as_deref(), Some("main"));
        assert_eq!(state.upstream_remote.as_deref(), Some("origin"));
        assert_eq!(state.upstream_ref.as_deref(), Some("main"));
        command(&source, &["branch", "private"]);
        commit(&source, "next\n");
        command(&source, &["tag", "-a", "v1", "-m", "private tag"]);
        command(&source, &["config", "push.followTags", "true"]);
        command(
            &source,
            &["config", "remote.origin.push", "+refs/heads/*:refs/heads/*"],
        );
        command(&source, &["config", "remote.origin.mirror", "true"]);
        assert!(run(
            &source,
            Operation::Push,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false)
        )
        .is_err());
        command(&source, &["config", "--unset", "remote.origin.mirror"]);
        run(
            &source,
            Operation::Push,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false),
        )
        .unwrap();
        let remote = temp.0.join("remote.git");
        assert!(command(&remote, &["tag", "--list"]).is_empty());
        assert!(!git::text(command(
            &remote,
            &["for-each-ref", "--format=%(refname)", "refs/heads"]
        ))
        .unwrap()
        .contains("private"));
        let old = head(&other).unwrap();
        fs::write(other.join("untracked"), "keep").unwrap();
        run(
            &other,
            Operation::Fetch,
            "origin",
            "",
            None,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(head(&other).unwrap(), old);
        assert_eq!(fs::read_to_string(other.join("file")).unwrap(), "base\n");
        assert_eq!(fs::read_to_string(other.join("untracked")).unwrap(), "keep");
        assert_eq!(
            command(&other, &["rev-parse", "origin/main"]),
            command(&source, &["rev-parse", "HEAD"])
        );
        assert!(run(
            &other,
            Operation::Fetch,
            "origin",
            "",
            None,
            &AtomicBool::new(true)
        )
        .is_err());
    }

    #[test]
    fn pull_fast_forwards_and_refuses_dirty_or_diverged_worktrees() {
        let (_temp, source, other) = fixture();
        commit(&source, "next\n");
        run(
            &source,
            Operation::Push,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false),
        )
        .unwrap();
        fs::write(other.join("file"), "local edits\n").unwrap();
        assert!(run(
            &other,
            Operation::Pull,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(other.join("file")).unwrap(),
            "local edits\n"
        );
        fs::write(other.join("file"), "base\n").unwrap();
        run(
            &other,
            Operation::Pull,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(head(&other).unwrap(), head(&source).unwrap());
        assert_eq!(fs::read_to_string(other.join("file")).unwrap(), "next\n");
        commit(&other, "local commit\n");
        commit(&source, "remote commit\n");
        run(
            &source,
            Operation::Push,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false),
        )
        .unwrap();
        let old = head(&other).unwrap();
        assert!(run(
            &other,
            Operation::Pull,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false)
        )
        .unwrap_err()
        .contains("fast-forward"));
        assert_eq!(head(&other).unwrap(), old);
        assert_eq!(
            fs::read_to_string(other.join("file")).unwrap(),
            "local commit\n"
        );
    }

    #[test]
    fn pull_preserves_ignored_files_and_refuses_in_progress_operations() {
        let (_temp, source, other) = fixture();
        let original = head(&other).unwrap();
        fs::write(other.join(".git/MERGE_HEAD"), format!("{original}\n")).unwrap();
        assert!(run(
            &other,
            Operation::Pull,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false)
        )
        .unwrap_err()
        .contains("in-progress"));
        fs::remove_file(other.join(".git/MERGE_HEAD")).unwrap();
        fs::write(other.join(".git/info/exclude"), "secret\n").unwrap();
        fs::write(other.join("secret"), "private local contents\n").unwrap();
        fs::write(source.join("secret"), "remote contents\n").unwrap();
        command(&source, &["add", "secret"]);
        command(&source, &["commit", "-m", "remote file"]);
        run(
            &source,
            Operation::Push,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(run(
            &other,
            Operation::Pull,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(head(&other).unwrap(), original);
        assert_eq!(
            fs::read_to_string(other.join("secret")).unwrap(),
            "private local contents\n"
        );
    }

    #[test]
    fn push_rejects_non_fast_forward_and_stale_or_invalid_selections() {
        let (temp, source, other) = fixture();
        commit(&other, "remote advance\n");
        run(
            &other,
            Operation::Push,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false),
        )
        .unwrap();
        commit(&source, "local advance\n");
        let before = command(&temp.0.join("remote.git"), &["rev-parse", "main"]);
        assert!(run(
            &source,
            Operation::Push,
            "origin",
            "main",
            Some("main"),
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(
            command(&temp.0.join("remote.git"), &["rev-parse", "main"]),
            before
        );
        for (remote, target, branch) in [
            ("--all", "main", Some("main")),
            ("origin", ":main", Some("main")),
            ("origin", "main", Some("other")),
        ] {
            assert!(run(
                &source,
                Operation::Push,
                remote,
                target,
                branch,
                &AtomicBool::new(false)
            )
            .is_err());
        }
        command(&source, &["switch", "--detach"]);
        assert!(run(
            &source,
            Operation::Push,
            "origin",
            "main",
            None,
            &AtomicBool::new(false)
        )
        .is_err());
    }
}
