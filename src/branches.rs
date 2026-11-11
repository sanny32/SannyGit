//! Ref listing and explicit branch/tag operations, called on background workers.
use crate::{
    git,
    models::{BranchRef, RefKind, RefSnapshot},
};
use std::{collections::HashSet, path::Path};

const REF_LIMIT: usize = 5_000;

pub fn load(root: &Path) -> Result<RefSnapshot, String> {
    let output = git::run(
        root,
        &[
            "for-each-ref",
            "--sort=refname",
            "--count=5001",
            "--format=%(refname)%00%(objectname)%00%(upstream)%00%(HEAD)%00%(symref)%00",
            "refs/heads/",
            "refs/remotes/",
            "refs/tags/",
        ],
    )?;
    let refs = parse(&output)?;
    let has_head = !git::run(root, &["rev-parse", "--revs-only", "HEAD"])?.is_empty();
    Ok(RefSnapshot { refs, has_head })
}

fn parse(bytes: &[u8]) -> Result<Vec<BranchRef>, String> {
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err("Truncated Git ref output".into());
    }
    let mut refs = Vec::new();
    let mut seen = HashSet::new();
    for (index, record) in bytes
        .split(|b| *b == b'\n')
        .filter(|r| !r.is_empty())
        .enumerate()
    {
        if index >= REF_LIMIT {
            return Err("Repository exceeds the 5,000-ref display limit".into());
        }
        let record = record
            .strip_suffix(&[0])
            .ok_or("Malformed Git ref record")?;
        let fields: Vec<_> = record
            .split(|b| *b == 0)
            .map(|s| std::str::from_utf8(s).map_err(|_| "Non-UTF-8 ref names are not supported"))
            .collect::<Result<_, _>>()?;
        if fields.len() != 5 {
            return Err("Malformed Git ref record".into());
        }
        if !seen.insert(fields[0]) {
            return Err("Duplicate Git ref record".into());
        }
        if !matches!(fields[1].len(), 40 | 64)
            || !fields[1].bytes().all(|c| c.is_ascii_hexdigit())
            || !matches!(fields[3], "*" | " ")
        {
            return Err("Invalid Git ref metadata".into());
        }
        // Hide symbolic aliases such as origin/HEAD; they are not independent branches.
        if !fields[4].is_empty() {
            continue;
        }
        let (kind, name) = if let Some(name) = fields[0].strip_prefix("refs/heads/") {
            (RefKind::Local, name)
        } else if let Some(name) = fields[0].strip_prefix("refs/remotes/") {
            (RefKind::Remote, name)
        } else if let Some(name) = fields[0].strip_prefix("refs/tags/") {
            (RefKind::Tag, name)
        } else {
            return Err("Unexpected ref namespace".into());
        };
        if name.is_empty() {
            return Err("Empty Git ref name".into());
        }
        refs.push(BranchRef {
            name: name.into(),
            full_name: fields[0].into(),
            hash: fields[1].into(),
            upstream: (!fields[2].is_empty()).then(|| {
                fields[2]
                    .strip_prefix("refs/remotes/")
                    .or_else(|| fields[2].strip_prefix("refs/heads/"))
                    .unwrap_or(fields[2])
                    .to_owned()
            }),
            current: fields[3] == "*",
            kind,
        });
    }
    Ok(refs)
}

/// Lightweight UI validation; Git performs its complete validation when an operation is submitted.
pub fn name_hint(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        Some("Enter a branch name")
    } else if name.len() > 1024 {
        Some("Branch name exceeds the 1 KiB limit")
    } else if name == "HEAD"
        || name.starts_with('-')
        || name.starts_with("refs/")
        || name.contains("@{")
        || name.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        Some("Use a short branch name without whitespace, control characters, refs/ or revision expressions")
    } else {
        None
    }
}

fn require_clean(root: &Path) -> Result<(), String> {
    git::require_no_operation(root)?;
    if !git::load(root)?.changes.is_empty() {
        return Err("Commit or move aside staged, unstaged and untracked changes before switching branches.".into());
    }
    Ok(())
}

pub fn tag_hint(name: &str, message: &str) -> Option<&'static str> {
    if name.is_empty() {
        Some("Enter a tag name")
    } else if name.len() > 1024 {
        Some("Tag name exceeds the 1 KiB limit")
    } else if name.starts_with('-')
        || name.starts_with("refs/")
        || name.contains("..")
        || name.contains("@{")
        || name.ends_with('.')
        || name
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "~^:?*[\\".contains(c))
        || name
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
    {
        Some("Use a valid short tag name without whitespace or revision expressions")
    } else if message.len() > 1024 * 1024 || message.contains('\0') {
        Some("Tag message must be at most 1 MiB and contain no NUL characters")
    } else {
        None
    }
}

/// Creates a tag at the current commit, never replacing an existing tag.
pub fn create_tag(root: &Path, name: &str, message: &str) -> Result<(), String> {
    if let Some(reason) = tag_hint(name, message) {
        return Err(reason.into());
    }
    git::run(root, &["check-ref-format", &format!("refs/tags/{name}")])?;
    let refs = load(root)?;
    if !refs.has_head {
        return Err("Create the first commit before creating a tag.".into());
    }
    if refs
        .refs
        .iter()
        .any(|r| r.full_name == format!("refs/tags/{name}"))
    {
        return Err("A tag with this name already exists.".into());
    }
    // Resolve HEAD once so both forms refer to the same exact commit.
    let commit = git::text(git::run(root, &["rev-parse", "--verify", "HEAD^{commit}"])?)?;
    if message.trim().is_empty() {
        // Override tag.gpgSign so an empty message always creates a lightweight tag.
        git::run(
            root,
            &["-c", "tag.gpgSign=false", "tag", "--", name, commit.trim()],
        )
    } else {
        git::run_with_input(
            root,
            &[
                "tag",
                "--annotate",
                "--file=-",
                "--cleanup=whitespace",
                "--",
                name,
                commit.trim(),
            ],
            Some(message.as_bytes()),
        )
    }
    .map(|_| ())
}

pub fn switch(root: &Path, full_name: &str) -> Result<(), String> {
    let refs = load(root)?;
    let target = refs
        .refs
        .iter()
        .find(|r| r.full_name == full_name)
        .ok_or("Select an existing branch or tag.")?;
    if target.current {
        return Err("This branch is already checked out.".into());
    }
    if target.kind == RefKind::Local {
        if let Some(reason) = name_hint(&target.name) {
            return Err(reason.into());
        }
    }
    require_clean(root)?;
    if target.kind != RefKind::Local {
        // Resolve the exact ref to a commit, including annotated tags. Never guess
        // a local branch or create tracking branches as a side effect.
        let commit = git::text(git::run(
            root,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{}^{{commit}}", target.full_name),
            ],
        )?)?;
        return git::run(
            root,
            &[
                "switch",
                "--detach",
                "--no-recurse-submodules",
                "--no-overwrite-ignore",
                "--",
                commit.trim(),
            ],
        )
        .map(|_| ());
    }
    // No force, auto-stash, remote guessing, submodule recursion or overwriting ignored files.
    git::run(
        root,
        &[
            "switch",
            "--no-guess",
            "--no-recurse-submodules",
            "--no-overwrite-ignore",
            "--",
            &target.name,
        ],
    )
    .map(|_| ())
}

#[derive(Clone)]
pub enum RemoteChoice {
    Local(String),
    Create(String),
    Detached,
}

/// Prefer an unambiguous tracking branch, then an untracked same-name branch.
pub fn local_for_remote<'a>(refs: &'a [BranchRef], remote: &BranchRef) -> Option<&'a BranchRef> {
    let mut tracking = refs.iter().filter(|r| {
        r.kind == RefKind::Local && r.upstream.as_deref() == Some(remote.name.as_str())
    });
    if let Some(first) = tracking.next() {
        return tracking.next().is_none().then_some(first);
    }
    let (_, name) = remote.name.split_once('/')?;
    refs.iter()
        .find(|r| r.kind == RefKind::Local && r.name == name && r.upstream.is_none())
}

pub fn checkout_remote(root: &Path, remote: &str, choice: &RemoteChoice) -> Result<(), String> {
    let refs = load(root)?;
    let target = refs
        .refs
        .iter()
        .find(|r| r.full_name == remote && r.kind == RefKind::Remote)
        .ok_or("The selected remote branch no longer exists. Refresh the repository.")?;
    match choice {
        RemoteChoice::Local(local) => {
            if !refs
                .refs
                .iter()
                .any(|r| r.kind == RefKind::Local && r.full_name == *local)
            {
                return Err("Select an existing local branch.".into());
            }
            switch(root, local)
        }
        RemoteChoice::Detached => switch(root, remote),
        RemoteChoice::Create(name) => {
            if let Some(reason) = name_hint(name) {
                return Err(reason.into());
            }
            git::run(root, &["check-ref-format", "--branch", name])?;
            if refs
                .refs
                .iter()
                .any(|r| r.full_name == format!("refs/heads/{name}"))
            {
                return Err("A local branch with this name already exists.".into());
            }
            require_clean(root)?;
            git::run(
                root,
                &[
                    "switch",
                    "--no-guess",
                    "--no-recurse-submodules",
                    "--no-overwrite-ignore",
                    "--track=direct",
                    "-c",
                    name,
                    &target.full_name,
                    "--",
                ],
            )
            .map(|_| ())
        }
    }
}

pub fn create(root: &Path, name: &str, switch_to: bool) -> Result<(), String> {
    if let Some(reason) = name_hint(name) {
        return Err(reason.into());
    }
    let checked = git::text(
        git::run(root, &["check-ref-format", "--branch", name])
            .map_err(|_| "Invalid branch name. Check Git branch naming rules.")?,
    )?;
    if checked.strip_suffix('\n').unwrap_or(&checked) != name {
        return Err("Branch name must be literal, not a revision expression.".into());
    }
    let refs = load(root)?;
    if !refs.has_head {
        return Err("Create the first commit before creating another branch.".into());
    }
    if refs
        .refs
        .iter()
        .any(|r| r.full_name == format!("refs/heads/{name}"))
    {
        return Err("A local branch with this name already exists.".into());
    }
    if switch_to {
        require_clean(root)?;
        git::run(
            root,
            &[
                "switch",
                "--no-guess",
                "--no-track",
                "--no-recurse-submodules",
                "--no-overwrite-ignore",
                "-c",
                name,
                "HEAD",
                "--",
            ],
        )
        .map(|_| ())
    } else {
        git::run(root, &["branch", "--no-track", "--", name, "HEAD"]).map(|_| ())
    }
}

/// Renames one exact local ref without overwriting another branch or touching files.
pub fn rename(root: &Path, full_name: &str, name: &str) -> Result<(), String> {
    if let Some(reason) = name_hint(name) {
        return Err(reason.into());
    }
    let checked = git::text(
        git::run(root, &["check-ref-format", "--branch", name])
            .map_err(|_| "Invalid branch name. Check Git branch naming rules.")?,
    )?;
    if checked.strip_suffix('\n').unwrap_or(&checked) != name {
        return Err("Branch name must be literal, not a revision expression.".into());
    }
    let refs = load(root)?;
    let target = refs
        .refs
        .iter()
        .find(|r| r.full_name == full_name && r.kind == RefKind::Local)
        .ok_or("The selected local branch no longer exists. Refresh the repository.")?;
    if let Some(reason) = name_hint(&target.name) {
        return Err(reason.into());
    }
    if refs
        .refs
        .iter()
        .any(|r| r.full_name == format!("refs/heads/{name}"))
    {
        return Err("A local branch with this name already exists.".into());
    }
    // Lowercase -m refuses an existing destination; Git retains upstream config
    // and reflogs, and updates HEAD if the renamed branch is checked out.
    git::run(root, &["branch", "-m", "--", &target.name, name]).map(|_| ())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeletePreview {
    pub full_name: String,
    pub name: String,
    pub oid: String,
    pub head: String,
}

pub fn preview_delete(root: &Path, full_name: &str) -> Result<DeletePreview, String> {
    git::require_no_operation(root)?;
    let refs = load(root)?;
    let target = refs
        .refs
        .iter()
        .find(|entry| entry.kind == RefKind::Local && entry.full_name == full_name)
        .ok_or("Select an existing local branch, then reload details.")?;
    if target.current {
        return Err("The current branch cannot be deleted.".into());
    }
    if let Some(reason) = name_hint(&target.name) {
        return Err(reason.into());
    }
    if crate::repository::worktrees(root)?
        .iter()
        .any(|tree| tree.branch.as_deref() == Some(&target.name))
    {
        return Err("This branch is checked out in a worktree and cannot be deleted.".into());
    }
    let head = git::text(git::run(root, &["rev-parse", "--verify", "HEAD^{commit}"])?)?
        .trim()
        .to_owned();
    git::run(root, &["merge-base", "--is-ancestor", &target.hash, &head])
        .map_err(|_| "The branch is not fully merged into the current HEAD, or its ancestry could not be verified. Force deletion is not available.".to_owned())?;
    Ok(DeletePreview {
        full_name: target.full_name.clone(),
        name: target.name.clone(),
        oid: target.hash.clone(),
        head,
    })
}

pub fn delete(root: &Path, expected: &DeletePreview) -> Result<(), String> {
    if preview_delete(root, &expected.full_name)? != *expected {
        return Err(
            "The branch or HEAD changed after review. Reload details and confirm again.".into(),
        );
    }
    git::run(root, &["branch", "-d", "--", &expected.name])
        .map_err(|error| format!("Branch deletion did not complete: {error} Only merged branches can be deleted; no force option is used."))?;
    Ok(())
}

#[cfg(test)]
mod tests;
