//! Ref listing and explicit branch/tag operations, called on background workers.
use crate::git::GitContext;
use crate::{
    git,
    models::{BranchRef, RefKind, RefSnapshot},
};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const REF_LIMIT: usize = 5_000;

pub fn load(git: &GitContext, root: &Path) -> Result<RefSnapshot, String> {
    let output = git::run(git,
        root,
        &[
            "for-each-ref",
            "--sort=refname",
            "--count=5001",
            "--format=%(refname)%00%(objectname)%00%(upstream)%00%(upstream:remotename)%00%(HEAD)%00%(symref)%00%(upstream:track,nobracket)%00%(if:equals=tag)%(objecttype)%(then)%(contents:subject)%(end)%00",
            "refs/heads/",
            "refs/remotes/",
            "refs/tags/",
        ],
    )?;
    let refs = parse(&output)?;
    let has_head = !git::run(git, root, &["rev-parse", "--revs-only", "HEAD"])?.is_empty();
    Ok(RefSnapshot { refs, has_head })
}

pub(crate) fn is_commit_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn read_commit_message(git: &GitContext, root: &Path, oid: &str) -> Result<String, String> {
    if !is_commit_id(oid) {
        return Err("Select a commit before copying its message.".into());
    }
    let output = git::text(
        git::run(
            git,
            root,
            &["log", "-1", "--format=%B", "--end-of-options", oid],
        )
        .map_err(|error| format!("Could not read the commit message.\n{error}"))?,
    )?;
    Ok(output.trim_end_matches(['\r', '\n']).to_owned())
}

pub fn read_tag_message(git: &GitContext, root: &Path, oid: &str) -> Result<String, String> {
    let object = git::run(git, root, &["cat-file", "tag", "--", oid])?;
    let body = object
        .windows(2)
        .position(|window| window == b"\n\n")
        .ok_or("Malformed Git tag object")?
        + 2;
    Ok(String::from_utf8_lossy(&object[body..]).into_owned())
}

pub fn format_tag_patch(
    git: &GitContext,
    root: &Path,
    oid: &str,
    directory: &Path,
) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Cannot prepare the patch export: {error}"))?
        .as_nanos();
    // Generate beside the destination so publishing the complete file can use a single rename.
    let staging = directory.join(format!(".sannygit-patch-{}-{nonce}", std::process::id()));
    let output_directory = staging
        .to_str()
        .ok_or("The selected directory path is not valid UTF-8")?;
    fs::create_dir(&staging).map_err(|error| {
        format!("Cannot create a temporary folder in the selected directory: {error}")
    })?;
    let result = (|| {
        let revision = format!("{oid}^{{commit}}");
        let output = git::text(git::run(
            git,
            root,
            &[
                "format-patch",
                "-1",
                "--no-cover-letter",
                "--no-ext-diff",
                "--output-directory",
                output_directory,
                &revision,
                "--",
            ],
        )?)?;
        let path = output.strip_suffix('\n').unwrap_or(&output);
        let name = Path::new(path).file_name().ok_or(
            "Git did not produce a patch. The tagged commit may be a merge or contain no changes.",
        )?;
        let destination = directory.join(name);
        fs::rename(staging.join(name), &destination)
            .map_err(|error| format!("Cannot save the patch in the selected directory: {error}"))?;
        Ok(destination)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

fn parse(bytes: &[u8]) -> Result<Vec<BranchRef>, String> {
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err("Truncated Git ref output".into());
    }
    let mut refs = Vec::new();
    let mut upstream_refs = Vec::new();
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
        let mut parts = record.splitn(8, |b| *b == 0);
        let fields: Vec<_> = parts
            .by_ref()
            .take(7)
            .map(|s| std::str::from_utf8(s).map_err(|_| "Non-UTF-8 ref names are not supported"))
            .collect::<Result<_, _>>()?;
        let subject = parts.next().ok_or("Malformed Git ref record")?;
        if subject.contains(&0) {
            return Err("Malformed Git ref record".into());
        }
        if !seen.insert(fields[0]) {
            return Err("Duplicate Git ref record".into());
        }
        if !matches!(fields[1].len(), 40 | 64)
            || !fields[1].bytes().all(|c| c.is_ascii_hexdigit())
            || !matches!(fields[4], "*" | " ")
        {
            return Err("Invalid Git ref metadata".into());
        }
        // Hide symbolic aliases such as origin/HEAD; they are not independent branches.
        if !fields[5].is_empty() {
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
        let tag_subject = if kind == RefKind::Tag {
            let subject = String::from_utf8_lossy(subject);
            let subject = subject.trim();
            (!subject.is_empty()).then(|| subject.to_owned())
        } else {
            None
        };
        upstream_refs.push((!fields[2].is_empty()).then(|| fields[2].to_owned()));
        refs.push(BranchRef {
            tag_subject,
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
            // `.` means the upstream is a local branch, not a remote one. Without
            // a resolved upstream ref there is nothing to show on the row.
            upstream_remote: (!fields[2].is_empty() && !matches!(fields[3], "" | "."))
                .then(|| fields[3].to_owned()),
            upstream_delta: (!fields[2].is_empty())
                .then(|| parse_delta(fields[6]))
                .flatten(),
            upstream_gone: false,
            current: fields[4] == "*",
            kind,
        });
    }
    mark_missing_upstreams(&mut refs, upstream_refs);
    Ok(refs)
}

fn parse_delta(track: &str) -> Option<(usize, usize)> {
    let mut counts = (0, 0);
    if !track.is_empty() {
        for part in track.split(", ") {
            let (direction, count) = part.split_once(' ')?;
            match direction {
                "ahead" => counts.0 = count.parse().ok()?,
                "behind" => counts.1 = count.parse().ok()?,
                _ => return None,
            }
        }
    }
    Some(counts)
}

/// A configured upstream counts as gone only when its remote already has loaded
/// remote-tracking refs but the tracked one is absent. A remote with no refs at
/// all was probably never fetched, so its upstream state stays unknown.
fn mark_missing_upstreams(refs: &mut [BranchRef], upstream_refs: Vec<Option<String>>) {
    let mut remotes = HashSet::new();
    let mut loaded = HashSet::new();
    for entry in refs.iter().filter(|entry| entry.kind == RefKind::Remote) {
        loaded.insert(entry.full_name.clone());
        if let Some((remote, _)) = entry
            .full_name
            .strip_prefix("refs/remotes/")
            .and_then(|name| name.split_once('/'))
        {
            remotes.insert(remote.to_owned());
        }
    }
    for (entry, upstream) in refs.iter_mut().zip(upstream_refs) {
        let (Some(upstream), Some(remote)) = (upstream, entry.upstream_remote.as_deref()) else {
            continue;
        };
        entry.upstream_gone = remotes.contains(remote) && !loaded.contains(upstream.as_str());
    }
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
pub fn create_tag(git: &GitContext, root: &Path, name: &str, message: &str) -> Result<(), String> {
    if let Some(reason) = tag_hint(name, message) {
        return Err(reason.into());
    }
    git::run(
        git,
        root,
        &["check-ref-format", &format!("refs/tags/{name}")],
    )?;
    let refs = load(git, root)?;
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
    let commit = git::text(git::run(
        git,
        root,
        &["rev-parse", "--verify", "HEAD^{commit}"],
    )?)?;
    if message.trim().is_empty() {
        // Override tag.gpgSign so an empty message always creates a lightweight tag.
        git::run(
            git,
            root,
            &["-c", "tag.gpgSign=false", "tag", "--", name, commit.trim()],
        )
    } else {
        git::run_with_input(
            git,
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

pub fn switch(git: &GitContext, root: &Path, full_name: &str) -> Result<(), String> {
    let refs = load(git, root)?;
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
    git::require_no_operation(git, root)?;
    if target.kind != RefKind::Local {
        // Resolve the exact ref to a commit, including annotated tags. Never guess
        // a local branch or create tracking branches as a side effect.
        let commit = git::text(git::run(
            git,
            root,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{}^{{commit}}", target.full_name),
            ],
        )?)?;
        return git::run(
            git,
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
        git,
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

pub fn checkout_remote(
    git: &GitContext,
    root: &Path,
    remote: &str,
    choice: &RemoteChoice,
) -> Result<(), String> {
    let refs = load(git, root)?;
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
            switch(git, root, local)
        }
        RemoteChoice::Detached => switch(git, root, remote),
        RemoteChoice::Create(name) => {
            if let Some(reason) = name_hint(name) {
                return Err(reason.into());
            }
            git::run(git, root, &["check-ref-format", "--branch", name])?;
            if refs
                .refs
                .iter()
                .any(|r| r.full_name == format!("refs/heads/{name}"))
            {
                return Err("A local branch with this name already exists.".into());
            }
            git::require_no_operation(git, root)?;
            git::run(
                git,
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

pub fn checkout_commit(git: &GitContext, root: &Path, commit: &str) -> Result<(), String> {
    if !is_commit_id(commit) {
        return Err("Select a valid commit before checking out.".into());
    }
    git::require_no_operation(git, root)?;
    git::run(
        git,
        root,
        &[
            "switch",
            "--detach",
            "--no-recurse-submodules",
            "--no-guess",
            commit,
        ],
    )
    .map(|_| ())
}

pub fn create(
    git: &GitContext,
    root: &Path,
    name: &str,
    switch_to: bool,
    start: Option<&str>,
) -> Result<(), String> {
    if let Some(reason) = name_hint(name) {
        return Err(reason.into());
    }
    let checked = git::text(
        git::run(git, root, &["check-ref-format", "--branch", name])
            .map_err(|_| "Invalid branch name. Check Git branch naming rules.")?,
    )?;
    if checked.strip_suffix('\n').unwrap_or(&checked) != name {
        return Err("Branch name must be literal, not a revision expression.".into());
    }
    let refs = load(git, root)?;
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
    let start = start.unwrap_or("HEAD");
    if switch_to {
        git::require_no_operation(git, root)?;
        git::run(
            git,
            root,
            &[
                "switch",
                "--no-guess",
                "--no-track",
                "--no-recurse-submodules",
                "--no-overwrite-ignore",
                "-c",
                name,
                start,
                "--",
            ],
        )
        .map(|_| ())
    } else {
        git::run(git, root, &["branch", "--no-track", "--", name, start]).map(|_| ())
    }
}

/// Renames one exact local ref without overwriting another branch or touching files.
pub fn rename(git: &GitContext, root: &Path, full_name: &str, name: &str) -> Result<(), String> {
    if let Some(reason) = name_hint(name) {
        return Err(reason.into());
    }
    let checked = git::text(
        git::run(git, root, &["check-ref-format", "--branch", name])
            .map_err(|_| "Invalid branch name. Check Git branch naming rules.")?,
    )?;
    if checked.strip_suffix('\n').unwrap_or(&checked) != name {
        return Err("Branch name must be literal, not a revision expression.".into());
    }
    let refs = load(git, root)?;
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
    git::run(git, root, &["branch", "-m", "--", &target.name, name]).map(|_| ())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeletePreview {
    pub full_name: String,
    pub name: String,
    pub oid: String,
    pub head: String,
    /// False when the tip is not contained in the current HEAD. Deletion then
    /// uses `branch -D` and still requires the reviewed preview.
    pub merged: bool,
}

pub fn preview_delete(
    git: &GitContext,
    root: &Path,
    full_name: &str,
) -> Result<DeletePreview, String> {
    git::require_no_operation(git, root)?;
    let refs = load(git, root)?;
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
    if crate::repository::worktrees(git, root)?
        .iter()
        .any(|tree| tree.branch.as_deref() == Some(&target.name))
    {
        return Err("This branch is checked out in a worktree and cannot be deleted.".into());
    }
    let head = git::text(git::run(
        git,
        root,
        &["rev-parse", "--verify", "HEAD^{commit}"],
    )?)?
    .trim()
    .to_owned();
    let merged = git::is_ancestor(git, root, &target.hash, &head)?;
    Ok(DeletePreview {
        full_name: target.full_name.clone(),
        name: target.name.clone(),
        oid: target.hash.clone(),
        head,
        merged,
    })
}

pub fn delete(git: &GitContext, root: &Path, expected: &DeletePreview) -> Result<(), String> {
    if preview_delete(git, root, &expected.full_name)? != *expected {
        return Err(
            "The branch or HEAD changed after review. Reload details and confirm again.".into(),
        );
    }
    let flag = if expected.merged { "-d" } else { "-D" };
    git::run(git, root, &["branch", flag, "--", &expected.name])
        .map_err(|error| format!("Branch deletion did not complete: {error}"))?;
    Ok(())
}

#[derive(Clone)]
pub struct ResetPreview {
    pub tag: String,
    pub target: String,
    pub has_tracked_changes: bool,
    tag_oid: String,
    branch: String,
    head: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    Clean,
    SaveStash,
    Discard,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ResetStrategy {
    #[default]
    Mixed,
    Soft,
    Hard,
    Keep,
    Merge,
}

impl ResetStrategy {
    fn flag(self) -> &'static str {
        match self {
            Self::Mixed => "--mixed",
            Self::Soft => "--soft",
            Self::Hard => "--hard",
            Self::Keep => "--keep",
            Self::Merge => "--merge",
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct AdvancedReset {
    pub strategy: ResetStrategy,
    pub fix_line_endings: bool,
}

fn reset_head(git: &GitContext, root: &Path) -> Result<(String, String), String> {
    let head = git::text(git::run(
        git,
        root,
        &["rev-parse", "--verify", "HEAD^{commit}"],
    )?)?;
    let reference = git::text(git::run(
        git,
        root,
        &["rev-parse", "--symbolic-full-name", "HEAD"],
    )?)?;
    Ok((head.trim().to_owned(), reference.trim().to_owned()))
}

pub fn preview_reset(git: &GitContext, root: &Path, tag: &str) -> Result<ResetPreview, String> {
    git::require_no_operation(git, root)?;
    let refs = load(git, root)?;
    let (label, object) = if let Some(source) =
        refs.refs.iter().find(|entry| entry.full_name == tag)
    {
        (source.name.clone(), source.hash.clone())
    } else if is_commit_id(tag) {
        (tag.to_owned(), tag.to_owned())
    } else {
        return Err("The selected reference no longer exists. Refresh branches and tags.".into());
    };
    let revision = format!("{object}^{{commit}}");
    let target = git::text(
        git::run(
            git,
            root,
            &["rev-parse", "--verify", "--end-of-options", &revision],
        )
        .map_err(|error| format!("Could not resolve '{label}' to a commit.\n{error}"))?,
    )?;
    let (head, branch) = reset_head(git, root)?;
    let target = target.trim();
    if is_commit_id(tag) && !target.eq_ignore_ascii_case(tag) {
        return Err("The selected object is not a commit.".into());
    }
    if [head.as_str(), target].iter().any(|oid| !is_commit_id(oid)) {
        return Err("Git returned an invalid commit object ID.".into());
    }
    let target = target.to_owned();
    // A commit is pinned by its own id. A ref keeps the reviewed object id, so moving an annotated tag is detected.
    let (stored_tag, tag_oid) = if is_commit_id(tag) {
        (target.clone(), target.clone())
    } else {
        (tag.to_owned(), object)
    };
    let snapshot = git::load(git, root)?;
    Ok(ResetPreview {
        tag: stored_tag,
        target,
        tag_oid,
        branch,
        head,
        has_tracked_changes: reset_has_tracked_changes(&snapshot),
    })
}

fn check_reset(
    git: &GitContext,
    root: &Path,
    expected: &ResetPreview,
) -> Result<ResetPreview, String> {
    let current = preview_reset(git, root, &expected.tag)?;
    if current.tag_oid != expected.tag_oid
        || current.target != expected.target
        || current.branch != expected.branch
        || current.head != expected.head
    {
        let selected = if is_commit_id(&expected.tag) {
            "commit"
        } else {
            "reference"
        };
        return Err(format!(
            "The selected {selected}, HEAD or current branch changed. Reload details and confirm Reset again."
        ));
    }
    Ok(current)
}

fn reset_has_tracked_changes(snapshot: &git::Snapshot) -> bool {
    snapshot
        .changes
        .iter()
        .any(|change| change.kind != crate::models::ChangeKind::Untracked)
}

pub fn reset_tag(
    git: &GitContext,
    root: &Path,
    expected: &ResetPreview,
    mode: ResetMode,
    advanced: Option<AdvancedReset>,
) -> Result<(), String> {
    let current = check_reset(git, root, expected)?;
    let strategy = advanced.map_or(ResetStrategy::Hard, |options| options.strategy);
    if strategy == ResetStrategy::Hard && mode == ResetMode::Clean && current.has_tracked_changes {
        return Err(
            "Tracked changes appeared before Reset. Reload details to review how to handle them."
                .into(),
        );
    }
    let stash_saved = strategy == ResetStrategy::Hard
        && mode == ResetMode::SaveStash
        && current.has_tracked_changes;
    if stash_saved {
        let prepared = crate::stash::preview_save(git, root, false)?;
        check_reset(git, root, expected)?;
        crate::stash::save(git, root, "SannyGit: automatic stash on Reset", &prepared)?;
    }
    let result = (|| {
        if stash_saved && check_reset(git, root, expected)?.has_tracked_changes {
            return Err(
                "Tracked changes appeared after saving the stash. Reload details before retrying."
                    .into(),
            );
        }
        if matches!(strategy, ResetStrategy::Keep | ResetStrategy::Merge) {
            // Refresh stat information for clean entries before Git checks for overwritten edits.
            git::run(git, root, &["update-index", "-q", "--refresh"])?;
        }
        git::run(git,
            root,
            &[
                "reset",
                strategy.flag(),
                "--no-recurse-submodules",
                &expected.target,
                "--",
            ],
        )
        .map(|_| ())
        .map_err(|error| match strategy {
            ResetStrategy::Keep => format!(
                "Keep reset did not complete. Git refuses to overwrite local changes in files it needs to update.\n{error}"
            ),
            ResetStrategy::Merge => format!(
                "Merge reset did not complete. Git refuses to overwrite unstaged changes in files it needs to update.\n{error}"
            ),
            _ => error,
        })
    })();
    if stash_saved {
        result.map_err(|error| format!("The stash was saved, but Reset did not complete.\n{error}"))
    } else {
        result
    }?;
    if strategy == ResetStrategy::Hard && advanced.is_some_and(|options| options.fix_line_endings) {
        refresh_reset_line_endings(git, root, expected).map_err(|error| {
            format!("Reset completed, but line endings could not be refreshed.\n{error}")
        })?;
    }
    Ok(())
}

fn refresh_reset_line_endings(
    git: &GitContext,
    root: &Path,
    expected: &ResetPreview,
) -> Result<(), String> {
    git::require_no_operation(git, root)?;
    let (head, branch) = reset_head(git, root)?;
    if head != expected.target
        || branch != expected.branch
        || reset_has_tracked_changes(&git::load(git, root)?)
    {
        return Err("HEAD, the current branch or tracked files changed after Reset.".into());
    }
    git::refresh_checkout_line_endings(git, root, &expected.target)
}

#[cfg(test)]
mod tests;
