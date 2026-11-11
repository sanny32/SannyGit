//! Explicit remote operations. Authentication stays with Git; diagnostics omit credentials.
use crate::{branches, git, models::RefKind};
use std::{path::Path, sync::atomic::AtomicBool, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Fetch,
    Sync,
    Pull,
    Push,
}
impl Operation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fetch => "Fetch",
            Self::Sync => "Sync",
            Self::Pull => "Pull",
            Self::Push => "Push",
        }
    }
    pub fn success(self) -> &'static str {
        match self {
            Self::Fetch => "Fetch complete",
            Self::Sync => "Sync complete",
            Self::Pull => "Pull complete",
            Self::Push => "Push complete",
        }
    }
}

#[derive(Clone, Default)]
pub struct State {
    pub remotes: Vec<String>,
    pub remote_urls: std::collections::BTreeMap<String, String>,
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
        remote_urls: crate::remote_config::load(root)?
            .entries
            .into_iter()
            .map(|entry| (entry.name, entry.fetch_display))
            .collect(),
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

#[cfg(test)]
pub fn run(
    root: &Path,
    operation: Operation,
    remote: &str,
    target: &str,
    expected_branch: Option<&str>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    run_with_tags(
        root,
        operation,
        remote,
        target,
        expected_branch,
        cancel,
        None,
    )
}

/// `None` preserves normal Fetch behavior; Pull supplies an explicit tag choice.
pub fn run_with_tags(
    root: &Path,
    operation: Operation,
    remote: &str,
    target: &str,
    expected_branch: Option<&str>,
    cancel: &AtomicBool,
    update_tags: Option<bool>,
) -> Result<(), String> {
    let state = load(root)?;
    if !state.remotes.iter().any(|name| name == remote) {
        return Err("Choose a configured remote, then refresh before retrying.".into());
    }
    let timeout = Duration::from_secs(600);
    if operation == Operation::Fetch {
        if update_tags == Some(true) {
            fetch_tags(root, remote, cancel)?;
        }
        let mut args = vec!["fetch", "--no-recurse-submodules"];
        if update_tags.is_some() {
            args.push("--no-tags");
        }
        args.extend(["--", remote]);
        git::run_controlled(root, &args, timeout, Some(cancel))?;
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
    if operation == Operation::Sync {
        require_pull_ready(root)?;
        // Only a non-fast-forward rejection permits continuing after a failed push.
        let rejected = git::run_sync_push(
            root,
            &push_arguments(remote, &destination, &original)
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            cancel,
        )?;
        if rejected {
            // Pulling from a different endpoint cannot resolve a rejected push.
            // Compare URLs internally; never expose credentials in diagnostics.
            let fetch_urls = git::run(root, &["remote", "get-url", "--all", remote])?;
            let push_urls = git::run(root, &["remote", "get-url", "--push", "--all", remote])?;
            if fetch_urls != push_urls {
                return Err("Sync push was rejected and the fetch and push URLs differ. Pull and push separately.".into());
            }
        }
    }
    match operation {
        Operation::Fetch => unreachable!(),
        Operation::Push => {
            push_tip(root, remote, &destination, &original, cancel)?;
        }
        Operation::Pull | Operation::Sync => {
            require_pull_ready(root)?;
            if update_tags == Some(true) {
                // Update tag refs separately so FETCH_HEAD still identifies only
                // the reviewed branch in the following fetch.
                fetch_tags(root, remote, cancel)?;
            }
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
            let remote_is_ahead = git::run(
                root,
                &["merge-base", "--is-ancestor", &original, fetched.trim()],
            )
            .is_ok();
            let local_is_ahead = operation == Operation::Sync
                && git::run(
                    root,
                    &["merge-base", "--is-ancestor", fetched.trim(), &original],
                )
                .is_ok();
            if !remote_is_ahead && !local_is_ahead {
                return Err(format!("{} requires a fast-forward. The branches diverged; merge or rebase them separately.", operation.label()));
            }
            if remote_is_ahead {
                git::run_controlled(root, &["-c", "submodule.recurse=false", "merge", "--ff-only", "--no-edit", "--no-autostash", "--no-overwrite-ignore", "--", fetched.trim()], timeout, Some(cancel))
                    .map_err(|error| format!("Could not fast-forward: {error} Check ignored-file conflicts, Git hooks, repository locks and permissions; refresh before retrying."))?;
            }
        }
    }
    Ok(())
}

fn fetch_tags(root: &Path, remote: &str, cancel: &AtomicBool) -> Result<(), String> {
    git::run_controlled(
        root,
        &[
            "fetch",
            "--no-recurse-submodules",
            "--no-tags",
            "--",
            remote,
            "+refs/tags/*:refs/tags/*",
        ],
        Duration::from_secs(600),
        Some(cancel),
    )
    .map(|_| ())
}

fn push_tip(
    root: &Path,
    remote: &str,
    destination: &str,
    original: &str,
    cancel: &AtomicBool,
) -> Result<(), String> {
    git::run_controlled(
        root,
        &push_arguments(remote, destination, original)
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        Duration::from_secs(600),
        Some(cancel),
    )?;
    Ok(())
}

fn push_arguments(remote: &str, destination: &str, original: &str) -> Vec<String> {
    // Push only the reviewed branch tip, never configured mirror/force/tag refspecs.
    [
        "push",
        "--porcelain",
        "--no-force",
        "--no-mirror",
        "--no-follow-tags",
        "--recurse-submodules=no",
        "--",
        remote,
    ]
    .into_iter()
    .map(str::to_owned)
    .chain([format!("{original}:{destination}")])
    .collect()
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

#[derive(Clone, PartialEq, Eq)]
pub struct TagPushPreview {
    pub full_name: String,
    pub oid: String,
    pub remote: String,
    pub endpoint_display: String,
    endpoint: String,
    config: crate::remote_config::Snapshot,
}

pub fn preview_tag_push(
    root: &Path,
    full_name: &str,
    remote: &str,
) -> Result<TagPushPreview, String> {
    let refs = branches::load(root)?;
    let tag = refs
        .refs
        .iter()
        .find(|entry| entry.kind == RefKind::Tag && entry.full_name == full_name)
        .ok_or("Select an existing tag and reload details.")?;
    if let Some(reason) = branches::tag_hint(&tag.name, "") {
        return Err(reason.into());
    }
    let (endpoint, config) = crate::remote_config::push_endpoint(root, remote)?;
    Ok(TagPushPreview {
        full_name: tag.full_name.clone(),
        oid: tag.hash.clone(),
        remote: remote.into(),
        endpoint_display: crate::remote_config::redact_url(&endpoint),
        endpoint,
        config,
    })
}

pub fn push_tag(root: &Path, expected: &TagPushPreview, cancel: &AtomicBool) -> Result<(), String> {
    if preview_tag_push(root, &expected.full_name, &expected.remote)? != *expected {
        return Err(
            "The tag or remote configuration changed after review. Reload and confirm again."
                .into(),
        );
    }
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
            &expected.endpoint,
            &format!("{}:{}", expected.oid, expected.full_name),
        ],
        Duration::from_secs(600),
        Some(cancel),
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests;
