//! Rebase only after checking the HEAD and branch reviewed in the dialog.
use crate::{git, merge};
use std::{collections::BTreeSet, path::Path};

fn ignored_collision(ignored: &str, target_paths: &str) -> bool {
    let normalize = |path: &str| {
        if cfg!(windows) {
            path.to_lowercase()
        } else {
            path.to_owned()
        }
    };
    let ignored: BTreeSet<_> = ignored.split_terminator('\0').map(normalize).collect();
    for path in target_paths.split_terminator('\0').map(normalize) {
        let mut ancestor = path.as_str();
        loop {
            if ignored.contains(ancestor) {
                return true;
            }
            let Some((parent, _)) = ancestor.rsplit_once('/') else {
                break;
            };
            ancestor = parent;
        }
        let prefix = format!("{path}/");
        if ignored
            .range(prefix.clone()..)
            .next()
            .is_some_and(|entry| entry.starts_with(&prefix))
        {
            return true;
        }
    }
    false
}

pub fn execute(
    root: &Path,
    expected_head: &str,
    expected_branch: &str,
    target: &str,
) -> Result<(), String> {
    if !matches!(target.len(), 40 | 64) || !target.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Select a valid commit before rebasing.".into());
    }
    git::require_no_operation(root)?;
    if merge::head(root)? != expected_head {
        return Err("HEAD changed. Close and reopen Rebase before retrying.".into());
    }
    let snapshot = git::load(root)?;
    if snapshot.branch != expected_branch {
        return Err("The current branch changed. Close and reopen Rebase before retrying.".into());
    }
    if !snapshot.changes.is_empty() {
        return Err("Commit or stash local changes before rebasing.".into());
    }
    // Rebase resets to the target internally, potentially overwriting ignored
    // files, which are absent from porcelain status. Compare literal NUL paths.
    let ignored = git::text(git::run(
        root,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
        ],
    )?)?;
    if !ignored.is_empty() {
        let paths = git::text(git::run(
            root,
            &["ls-tree", "-r", "--name-only", "-z", target],
        )?)?;
        if ignored_collision(&ignored, &paths) {
            return Err("Ignored files would be overwritten by the rebase target. Move them out of the way before retrying.".into());
        }
    }
    // Pin the chosen commit and keep configuration from enabling interactive
    // steps, autostashing, or rewriting other local branches.
    git::run(root, &[
        "rebase", "--no-autostash", "--no-autosquash",
        "--no-rebase-merges", "--no-update-refs", "--no-fork-point", target,
    ])
    .map(|_| ())
    .map_err(|error| format!("{error}\nIf conflicts occurred, resolve them outside SannyGit, then use git rebase --continue or git rebase --abort before retrying."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempRepo;

    #[test]
    fn ignored_collisions_compare_literal_paths_and_directory_boundaries() {
        assert!(ignored_collision("a b\n.txt\0", "a b\n.txt\0"));
        assert!(ignored_collision("dir/file\0", "dir\0"));
        assert!(ignored_collision("dir\0", "dir/file\0"));
        assert!(!ignored_collision("directory/file\0", "dir\0"));
        assert!(!ignored_collision("dir/file\0", "dir/other\0"));
        assert!(!ignored_collision("", "dir\0"));
        assert_eq!(ignored_collision("NAME\0", "name\0"), cfg!(windows));
    }

    fn fixture() -> (TempRepo, String, String) {
        let repo = TempRepo::new();
        repo.write("base", "base\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "base"]);
        repo.git(&["checkout", "-b", "upstream"]);
        repo.write("upstream", "upstream\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "upstream"]);
        let target = merge::head(&repo.0).unwrap();
        repo.git(&["checkout", "main"]);
        repo.write("local", "local\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "local"]);
        let before = merge::head(&repo.0).unwrap();
        (repo, before, target)
    }

    #[test]
    fn replays_commits_without_updating_other_branches() {
        let (repo, before, target) = fixture();
        repo.git(&["branch", "keep"]);
        repo.git(&["config", "rebase.updateRefs", "true"]);
        repo.git(&["config", "rebase.autoStash", "true"]);
        execute(&repo.0, &before, "main", &target).unwrap();
        assert_ne!(merge::head(&repo.0).unwrap(), before);
        assert_eq!(
            repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(),
            target
        );
        assert_eq!(repo.git(&["rev-parse", "keep"]).trim(), before);
        assert_eq!(repo.git(&["rev-parse", "upstream"]).trim(), target);
        assert_eq!(
            std::fs::read_to_string(repo.0.join("local")).unwrap(),
            "local\n"
        );
        assert!(git::load(&repo.0).unwrap().changes.is_empty());
    }

    #[test]
    fn rejects_stale_head_changed_branch_invalid_target_and_local_changes() {
        let (repo, before, target) = fixture();
        assert!(execute(&repo.0, &target, "main", &target).is_err());
        assert!(execute(&repo.0, &before, "main", "--all").is_err());
        assert!(execute(&repo.0, &before, "other", &target).is_err());
        repo.write("local", "keep\n");
        assert!(execute(&repo.0, &before, "main", &target).is_err());
        repo.git(&["add", "local"]);
        assert!(execute(&repo.0, &before, "main", &target).is_err());
        assert_eq!(merge::head(&repo.0).unwrap(), before);
        assert_eq!(
            std::fs::read_to_string(repo.0.join("local")).unwrap(),
            "keep\n"
        );
    }

    #[test]
    fn reports_conflicts_blocks_retries_and_allows_external_abort() {
        let (repo, before, target) = fixture();
        repo.git(&["checkout", "upstream"]);
        repo.write("local", "conflicting\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "conflict"]);
        let conflicting = merge::head(&repo.0).unwrap();
        repo.git(&["checkout", "main"]);
        let error = execute(&repo.0, &before, "main", &conflicting).unwrap_err();
        assert!(error.contains("git rebase --abort"));
        assert!(git::load(&repo.0)
            .unwrap()
            .changes
            .iter()
            .any(|file| file.kind == crate::models::ChangeKind::Conflicted));
        assert!(execute(&repo.0, &before, "main", &target)
            .unwrap_err()
            .contains("in-progress"));
        repo.git(&["rebase", "--abort"]);
        assert_eq!(merge::head(&repo.0).unwrap(), before);
    }

    #[test]
    fn preserves_ignored_files_that_collide_with_the_target() {
        let (repo, before, target) = fixture();
        repo.write(".git/info/exclude", "upstream\n");
        repo.write("upstream", "keep ignored bytes\n");
        assert!(git::load(&repo.0).unwrap().changes.is_empty());
        assert!(execute(&repo.0, &before, "main", &target).is_err());
        assert_eq!(merge::head(&repo.0).unwrap(), before);
        assert_eq!(
            std::fs::read_to_string(repo.0.join("upstream")).unwrap(),
            "keep ignored bytes\n"
        );
        assert!(git::load(&repo.0).unwrap().changes.is_empty());
    }
}
