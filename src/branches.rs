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
    if !git::load(root)?.changes.is_empty() {
        return Err("Commit or move aside staged, unstaged and untracked changes before switching branches.".into());
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::TempRepo;

    fn committed_repo() -> TempRepo {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "fixture"]);
        repo
    }

    #[test]
    fn rename_preserves_dirty_files_index_head_and_tracking_configuration() {
        let repo = committed_repo();
        repo.git(&["config", "branch.main.remote", "origin"]);
        repo.git(&["config", "branch.main.merge", "refs/heads/main"]);
        let head = repo.git(&["rev-parse", "HEAD"]);
        repo.write("file", "staged\n");
        repo.git(&["add", "file"]);
        repo.write("file", "unstaged\n");
        repo.write("new", "untracked\n");
        let index = std::fs::read(repo.0.join(".git/index")).unwrap();
        rename(&repo.0, "refs/heads/main", "feature/новая").unwrap();
        assert_eq!(git::load(&repo.0).unwrap().branch, "feature/новая");
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "unstaged\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.0.join("new")).unwrap(),
            "untracked\n"
        );
        assert_eq!(
            repo.git(&["config", "branch.feature/новая.remote"]).trim(),
            "origin"
        );
        assert_eq!(
            repo.git(&["config", "branch.feature/новая.merge"]).trim(),
            "refs/heads/main"
        );
        assert!(!load(&repo.0)
            .unwrap()
            .refs
            .iter()
            .any(|r| r.full_name == "refs/heads/main"));
        create(&repo.0, "other", false).unwrap();
        rename(&repo.0, "refs/heads/other", "renamed-other").unwrap();
        assert_eq!(git::load(&repo.0).unwrap().branch, "feature/новая");
        assert_eq!(repo.git(&["rev-parse", "renamed-other"]), head);
    }

    #[test]
    fn rename_rejects_invalid_missing_nonlocal_and_existing_targets_without_writes() {
        let repo = committed_repo();
        create(&repo.0, "existing", false).unwrap();
        repo.git(&["tag", "release"]);
        repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        let before = repo.git(&["show-ref"]);
        let index = std::fs::read(repo.0.join(".git/index")).unwrap();
        for name in [
            "",
            "HEAD",
            "-bad",
            "refs/heads/bad",
            "@{-1}",
            "a..b",
            "a.lock",
            "space name",
            "bad\0name",
            "existing",
            "main",
        ] {
            assert!(
                rename(&repo.0, "refs/heads/main", name).is_err(),
                "{name:?}"
            );
        }
        for source in [
            "main",
            "refs/heads/missing",
            "refs/tags/release",
            "refs/remotes/origin/main",
            "--all",
        ] {
            assert!(rename(&repo.0, source, "new-name").is_err());
        }
        assert_eq!(repo.git(&["show-ref"]), before);
        assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
        assert_eq!(git::load(&repo.0).unwrap().branch, "main");
    }

    #[test]
    fn lists_current_local_remote_upstream_and_annotated_tags_without_writes() {
        let repo = committed_repo();
        repo.git(&["branch", "feature/ветка"]);
        repo.git(&[
            "config",
            "--local",
            "remote.origin.url",
            "https://example.invalid/fixture.git",
        ]);
        repo.git(&[
            "config",
            "--local",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ]);
        repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        repo.git(&[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ]);
        repo.git(&["branch", "--set-upstream-to=origin/main", "main"]);
        repo.git(&["tag", "lightweight"]);
        repo.git(&["tag", "-a", "release", "-m", "Release"]);
        let index = std::fs::read(repo.0.join(".git/index")).unwrap();
        let snapshot = load(&repo.0).unwrap();
        assert!(snapshot.has_head);
        assert_eq!(snapshot.refs.len(), 5);
        let current = snapshot.refs.iter().find(|r| r.current).unwrap();
        assert_eq!(current.full_name, "refs/heads/main");
        assert_eq!(current.upstream.as_deref(), Some("origin/main"));
        assert!(snapshot.refs.iter().any(|r| r.name == "feature/ветка"));
        assert!(snapshot
            .refs
            .iter()
            .any(|r| r.kind == RefKind::Remote && r.name == "origin/main"));
        assert_eq!(
            snapshot
                .refs
                .iter()
                .filter(|r| r.kind == RefKind::Tag)
                .count(),
            2
        );
        assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
    }

    #[test]
    fn creates_branches_with_and_without_switching_and_handles_detached_head() {
        let repo = committed_repo();
        let head = repo.git(&["rev-parse", "HEAD"]);
        create(&repo.0, "feature/новая", false).unwrap();
        assert_eq!(git::load(&repo.0).unwrap().branch, "main");
        assert_eq!(repo.git(&["rev-parse", "feature/новая"]), head);
        create(&repo.0, "another", true).unwrap();
        repo.git(&["tag", "main"]);
        assert_eq!(git::load(&repo.0).unwrap().branch, "another");
        switch(&repo.0, "refs/heads/main").unwrap();
        assert_eq!(git::load(&repo.0).unwrap().branch, "main");
        assert!(switch(&repo.0, "refs/heads/main").is_err());
        repo.git(&["checkout", "--detach"]);
        assert!(load(&repo.0).unwrap().refs.iter().all(|r| !r.current));
        create(&repo.0, "from-detached", true).unwrap();
        assert_eq!(git::load(&repo.0).unwrap().branch, "from-detached");
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    }

    #[test]
    fn refuses_dirty_switches_and_preserves_index_and_worktree() {
        let repo = committed_repo();
        create(&repo.0, "other", false).unwrap();
        for staged in [false, true] {
            repo.write("file", "unsaved\n");
            if staged {
                repo.git(&["add", "file"]);
            }
            let index = std::fs::read(repo.0.join(".git/index")).unwrap();
            assert!(switch(&repo.0, "refs/heads/other").is_err());
            assert_eq!(git::load(&repo.0).unwrap().branch, "main");
            assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
            assert_eq!(
                std::fs::read_to_string(repo.0.join("file")).unwrap(),
                "unsaved\n"
            );
            repo.git(&["restore", "--staged", "--worktree", "--", "file"]);
        }
        repo.write("untracked", "keep\n");
        assert!(switch(&repo.0, "refs/heads/other").is_err());
        assert!(create(&repo.0, "must-not-exist", true).is_err());
        assert!(!load(&repo.0)
            .unwrap()
            .refs
            .iter()
            .any(|r| r.name == "must-not-exist"));
        create(&repo.0, "without-switch", false).unwrap();
        assert_eq!(git::load(&repo.0).unwrap().branch, "main");
        assert_eq!(
            std::fs::read_to_string(repo.0.join("untracked")).unwrap(),
            "keep\n"
        );
    }

    #[test]
    fn never_overwrites_ignored_files_when_switching() {
        let repo = committed_repo();
        repo.write(".gitignore", "ignored.data\n");
        repo.git(&["add", ".gitignore"]);
        repo.git(&["commit", "-m", "ignore"]);
        create(&repo.0, "target", true).unwrap();
        repo.write("ignored.data", "tracked target\n");
        repo.git(&["add", "-f", "ignored.data"]);
        repo.git(&["commit", "-m", "target file"]);
        switch(&repo.0, "refs/heads/main").unwrap();
        repo.write("ignored.data", "private local contents\n");
        assert!(git::load(&repo.0).unwrap().changes.is_empty());
        assert!(switch(&repo.0, "refs/heads/target").is_err());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("ignored.data")).unwrap(),
            "private local contents\n"
        );
        assert_eq!(git::load(&repo.0).unwrap().branch, "main");
    }

    #[test]
    fn refuses_branch_in_another_worktree_and_missing_local_targets() {
        let repo = committed_repo();
        create(&repo.0, "occupied", false).unwrap();
        repo.git(&[
            "worktree",
            "add",
            repo.0.join("linked").to_str().unwrap(),
            "occupied",
        ]);
        repo.write(".git/info/exclude", "linked/\n");
        assert!(git::load(&repo.0).unwrap().changes.is_empty());
        assert!(switch(&repo.0, "refs/heads/occupied").is_err());
        repo.git(&["tag", "main"]);
        repo.git(&["update-ref", "refs/remotes/origin/remote-only", "HEAD"]);
        assert!(switch(&repo.0, "refs/heads/remote-only").is_err());
        assert_eq!(git::load(&repo.0).unwrap().branch, "main");
    }

    #[test]
    fn switches_tags_and_remote_refs_to_detached_commits_without_creating_branches() {
        let repo = committed_repo();
        let original = git::run(&repo.0, &["rev-parse", "HEAD"]).unwrap();
        repo.git(&["tag", "lightweight"]);
        repo.git(&["tag", "-a", "release", "-m", "Release"]);
        repo.git(&["update-ref", "refs/remotes/origin/remote-only", "HEAD"]);
        repo.write("file", "next\n");
        repo.git(&["commit", "-am", "next"]);
        for target in [
            "refs/tags/lightweight",
            "refs/tags/release",
            "refs/remotes/origin/remote-only",
        ] {
            switch(&repo.0, target).unwrap();
            assert_eq!(git::run(&repo.0, &["rev-parse", "HEAD"]).unwrap(), original);
            assert!(git::run(&repo.0, &["symbolic-ref", "-q", "HEAD"]).is_err());
            assert_eq!(
                std::fs::read_to_string(repo.0.join("file")).unwrap(),
                "base\n"
            );
            switch(&repo.0, "refs/heads/main").unwrap();
        }
        assert_eq!(
            load(&repo.0)
                .unwrap()
                .refs
                .iter()
                .filter(|r| r.kind == RefKind::Local)
                .count(),
            1
        );
        repo.write("file", "local edits\n");
        assert!(switch(&repo.0, "refs/tags/release").is_err());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "local edits\n"
        );
        assert_eq!(git::load(&repo.0).unwrap().branch, "main");
    }

    #[test]
    fn remote_checkout_creates_tracking_branch_at_remote_commit_and_preserves_existing() {
        let repo = committed_repo();
        repo.git(&[
            "config",
            "remote.origin.url",
            "https://example.invalid/repo",
        ]);
        repo.git(&[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ]);
        repo.git(&["update-ref", "refs/remotes/origin/topic", "HEAD"]);
        let remote = "refs/remotes/origin/topic";
        let original = git::run(&repo.0, &["rev-parse", "HEAD"]).unwrap();
        repo.write("file", "next\n");
        repo.git(&["commit", "-am", "next"]);
        checkout_remote(&repo.0, remote, &RemoteChoice::Create("topic".into())).unwrap();
        assert_eq!(git::run(&repo.0, &["rev-parse", "HEAD"]).unwrap(), original);
        let refs = load(&repo.0).unwrap().refs;
        let target = refs.iter().find(|r| r.full_name == remote).unwrap();
        let local = local_for_remote(&refs, target).unwrap();
        assert_eq!(local.name, "topic");
        assert_eq!(local.upstream.as_deref(), Some("origin/topic"));
        checkout_remote(
            &repo.0,
            remote,
            &RemoteChoice::Local("refs/heads/main".into()),
        )
        .unwrap();
        assert_eq!(git::load(&repo.0).unwrap().branch, "main");
        assert!(checkout_remote(&repo.0, remote, &RemoteChoice::Create("main".into())).is_err());
        checkout_remote(&repo.0, remote, &RemoteChoice::Detached).unwrap();
        assert!(git::run(&repo.0, &["symbolic-ref", "-q", "HEAD"]).is_err());
        repo.write("file", "local\n");
        assert!(checkout_remote(&repo.0, remote, &RemoteChoice::Create("safe".into())).is_err());
        assert!(!load(&repo.0).unwrap().refs.iter().any(|r| r.name == "safe"));
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "local\n"
        );
    }

    #[test]
    fn remote_local_matching_avoids_other_remotes_and_ambiguous_tracking() {
        let repo = committed_repo();
        repo.git(&["branch", "topic"]);
        repo.git(&[
            "config",
            "remote.origin.url",
            "https://example.invalid/repo",
        ]);
        repo.git(&[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ]);
        repo.git(&["update-ref", "refs/remotes/origin/topic", "HEAD"]);
        let mut refs = load(&repo.0).unwrap().refs;
        let remote_index = refs.iter().position(|r| r.kind == RefKind::Remote).unwrap();
        assert_eq!(
            local_for_remote(&refs, &refs[remote_index]).unwrap().name,
            "topic"
        );
        let topic = refs.iter_mut().find(|r| r.name == "topic").unwrap();
        topic.upstream = Some("other/topic".into());
        assert!(local_for_remote(&refs, &refs[remote_index]).is_none());
        for local in refs.iter_mut().filter(|r| r.kind == RefKind::Local) {
            local.upstream = Some("origin/topic".into());
        }
        assert!(local_for_remote(&refs, &refs[remote_index]).is_none());
    }

    #[test]
    fn validates_names_rejects_duplicates_and_handles_unborn_head() {
        let repo = TempRepo::new();
        let snapshot = load(&repo.0).unwrap();
        assert!(!snapshot.has_head);
        assert!(snapshot.refs.is_empty());
        assert!(create(&repo.0, "new", false)
            .unwrap_err()
            .contains("first commit"));
        repo.write("file", "fixture\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "fixture"]);
        for name in [
            "",
            "HEAD",
            "-option",
            "refs/heads/nested",
            "@{-1}",
            "a b",
            "a..b",
            ".hidden",
            "a.lock",
            "a\0b",
        ] {
            assert!(create(&repo.0, name, false).is_err(), "accepted {name:?}");
        }
        assert!(create(&repo.0, "main", false)
            .unwrap_err()
            .contains("already exists"));
        assert_eq!(load(&repo.0).unwrap().refs.len(), 1);
    }

    #[test]
    fn rejects_malformed_ref_records_and_bounds_ref_count() {
        let record = format!("refs/heads/main\0{}\0\0*\0\0\n", "a".repeat(40));
        assert_eq!(parse(record.as_bytes()).unwrap()[0].name, "main");
        assert!(parse(&record.as_bytes()[..record.len() - 1]).is_err());
        assert!(parse(b"short\0\n").is_err());
        assert!(parse(format!("{record}{record}").as_bytes()).is_err());
        assert!(parse(record.replace(&"a".repeat(40), "bad").as_bytes()).is_err());
        let mut bytes = record.into_bytes();
        bytes[12] = 255;
        assert!(parse(&bytes).is_err());
        let many: String = (0..=REF_LIMIT)
            .map(|i| format!("refs/heads/b{i}\0{}\0\0 \0\0\n", "a".repeat(40)))
            .collect();
        assert!(parse(many.as_bytes()).is_err_and(|error| error.contains("5,000")));
    }
}
