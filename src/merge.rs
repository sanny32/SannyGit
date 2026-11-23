//! Explicit merge operations; the dialog pins the target commit and current HEAD.
use crate::git::GitContext;
use crate::{branches, git, models::RefKind};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Commit,
    WorkingTree,
    FastForward,
}

pub fn head(git: &GitContext, root: &Path) -> Result<String, String> {
    git::text(git::run(
        git,
        root,
        &["rev-parse", "--verify", "HEAD^{commit}"],
    )?)
    .map(|value| value.trim().to_owned())
}

pub fn abort(git: &GitContext, root: &Path) -> Result<(), String> {
    if !git::check_operation(git, root, true)? {
        return Err("There is no merge in progress. Refresh the working tree.".into());
    }
    git::run(git, root, &["merge", "--abort"]).map(|_| ())
}

pub fn execute(
    git: &GitContext,
    root: &Path,
    expected_head: &str,
    expected_branch: &str,
    target: &str,
    preferred_ref: &str,
    mode: Mode,
) -> Result<bool, String> {
    if !matches!(target.len(), 40 | 64) || !target.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Select a valid commit before merging.".into());
    }
    git::require_no_operation(git, root)?;
    if head(git, root)? != expected_head {
        return Err("HEAD changed. Close and reopen Merge before retrying.".into());
    }
    let snapshot = git::load(git, root)?;
    if snapshot.branch != expected_branch {
        return Err("The current branch changed. Close and reopen Merge before retrying.".into());
    }
    if !snapshot.changes.is_empty() {
        let untracked = snapshot
            .changes
            .iter()
            .filter(|change| change.kind == crate::models::ChangeKind::Untracked)
            .map(|change| change.path.as_str())
            .collect::<Vec<_>>()
            .join("\0");
        git::require_no_path_collisions(git, root, target, &untracked, "Untracked files")?;
        if mode != Mode::FastForward
            || snapshot
                .changes
                .iter()
                .any(|change| change.kind != crate::models::ChangeKind::Untracked)
        {
            return Err("Commit or stash local changes before merging.".into());
        }
    }
    // --no-overwrite-ignore alone does not protect every merge strategy.
    git::require_no_ignored_collisions(git, root, target)?;
    if mode == Mode::FastForward {
        return git::run(
            git,
            root,
            &[
                "merge",
                "--ff-only",
                "--no-autostash",
                "--no-overwrite-ignore",
                "--",
                target,
            ],
        )
        .map(|_| false);
    }
    let mut args = vec![
        "merge",
        "--no-ff",
        if mode == Mode::Commit {
            "--no-edit"
        } else {
            "--no-commit"
        },
        "--no-autostash",
        "--no-overwrite-ignore",
    ];
    // Use the branch name for the message while merging the reviewed object ID.
    let refs = branches::load(git, root)?;
    let mut matching = refs
        .refs
        .iter()
        .filter(|entry| entry.hash == target && entry.kind != RefKind::Tag);
    let selected = matching
        .clone()
        .find(|entry| entry.full_name == preferred_ref)
        .or_else(|| match (matching.next(), matching.next()) {
            (Some(entry), None) => Some(entry),
            _ => None,
        });
    let message = selected.and_then(|entry| match entry.kind {
        RefKind::Local => Some(format!("Merge branch '{}'", entry.name)),
        RefKind::Remote => Some(format!("Merge remote-tracking branch '{}'", entry.name)),
        RefKind::Tag => None,
    });
    if let Some(message) = &message {
        args.extend(["-m", message]);
    }
    args.push(target);
    match git::run(git, root, &args) {
        Ok(_) => Ok(false),
        Err(error) => {
            let snapshot = git::load(git, root)?;
            if snapshot.merging
                && snapshot
                    .changes
                    .iter()
                    .any(|change| change.kind == crate::models::ChangeKind::Conflicted)
            {
                Ok(true)
            } else {
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempRepo;
    use crate::test_support::SYSTEM_GIT;

    fn fixture() -> (TempRepo, String, String) {
        let repo = TempRepo::new();
        repo.write("base", "base\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "base"]);
        let before = head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["checkout", "-b", "feature"]);
        repo.write("feature", "feature\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "feature"]);
        let target = head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["checkout", "main"]);
        (repo, before, target)
    }

    #[test]
    fn fast_forward_moves_head_without_creating_a_merge_commit() {
        let (repo, before, target) = fixture();
        execute(
            &SYSTEM_GIT,
            &repo.0,
            &before,
            "main",
            &target,
            "",
            Mode::FastForward,
        )
        .unwrap();
        assert_eq!(head(&SYSTEM_GIT, &repo.0).unwrap(), target);
        assert_eq!(
            repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(),
            before
        );
    }

    #[test]
    fn ignored_files_are_preserved_in_both_merge_modes() {
        for mode in [Mode::Commit, Mode::WorkingTree] {
            let (repo, before, target) = fixture();
            repo.write(".git/info/exclude", "feature\n");
            repo.write("feature", "private local contents\n");
            let index = std::fs::read(repo.0.join(".git/index")).unwrap();
            assert!(git::load(&SYSTEM_GIT, &repo.0).unwrap().changes.is_empty());

            let error =
                execute(&SYSTEM_GIT, &repo.0, &before, "main", &target, "", mode).unwrap_err();
            assert!(error.contains("\"feature\""), "{error}");

            assert_eq!(
                std::fs::read(repo.0.join("feature")).unwrap(),
                b"private local contents\n"
            );
            assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
            assert_eq!(head(&SYSTEM_GIT, &repo.0).unwrap(), before);
            git::require_no_operation(&SYSTEM_GIT, &repo.0).unwrap();
        }
    }

    #[test]
    fn reports_untracked_paths_that_block_the_merge() {
        let (repo, before, target) = fixture();
        repo.write("feature", "local contents\n");
        repo.write("unrelated", "keep\n");
        let error = execute(
            &SYSTEM_GIT,
            &repo.0,
            &before,
            "main",
            &target,
            "",
            Mode::WorkingTree,
        )
        .unwrap_err();
        assert!(error.starts_with("Untracked files overlap"));
        assert_eq!(
            error.split_once("\n\nGit diagnostic:\n").unwrap().1.trim(),
            "\"feature\""
        );
        assert_eq!(
            std::fs::read_to_string(repo.0.join("feature")).unwrap(),
            "local contents\n"
        );
        assert_eq!(head(&SYSTEM_GIT, &repo.0).unwrap(), before);
    }

    #[test]
    fn commit_creates_two_parents_even_for_fast_forward() {
        let (repo, before, target) = fixture();
        execute(
            &SYSTEM_GIT,
            &repo.0,
            &before,
            "main",
            &target,
            "",
            Mode::Commit,
        )
        .unwrap();
        assert_eq!(
            repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(),
            format!("{before} {target}")
        );
        assert!(git::load(&SYSTEM_GIT, &repo.0).unwrap().changes.is_empty());
    }

    #[test]
    fn working_tree_leaves_a_merge_to_commit_without_moving_head() {
        let (repo, before, target) = fixture();
        execute(
            &SYSTEM_GIT,
            &repo.0,
            &before,
            "main",
            &target,
            "",
            Mode::WorkingTree,
        )
        .unwrap();
        assert_eq!(head(&SYSTEM_GIT, &repo.0).unwrap(), before);
        assert!(git::load(&SYSTEM_GIT, &repo.0).unwrap().merging);
        assert!(git::load(&SYSTEM_GIT, &repo.0)
            .unwrap()
            .changes
            .iter()
            .any(|file| file.path == "feature" && file.staged));
        git::commit(&SYSTEM_GIT, &repo.0, "merged", git::CommitContents::Staged).unwrap();
        assert_eq!(
            repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(),
            format!("{before} {target}")
        );
    }

    #[test]
    fn rejects_stale_head_dirty_tree_invalid_target_and_unfinished_merge() {
        let (repo, before, target) = fixture();
        assert!(execute(
            &SYSTEM_GIT,
            &repo.0,
            &target,
            "main",
            &target,
            "",
            Mode::Commit
        )
        .is_err());
        assert!(execute(
            &SYSTEM_GIT,
            &repo.0,
            &before,
            "main",
            "--all",
            "",
            Mode::Commit
        )
        .is_err());
        repo.write("local", "keep");
        assert!(execute(
            &SYSTEM_GIT,
            &repo.0,
            &before,
            "main",
            &target,
            "",
            Mode::WorkingTree
        )
        .is_err());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("local")).unwrap(),
            "keep"
        );
        assert_eq!(head(&SYSTEM_GIT, &repo.0).unwrap(), before);
        assert!(repo.git(&["diff", "--cached"]).is_empty());
        let (repo, before, target) = fixture();
        std::fs::write(repo.0.join(".git/MERGE_HEAD"), format!("{target}\n")).unwrap();
        assert!(execute(
            &SYSTEM_GIT,
            &repo.0,
            &before,
            "main",
            &target,
            "",
            Mode::Commit
        )
        .is_err());
        assert_eq!(head(&SYSTEM_GIT, &repo.0).unwrap(), before);
    }

    #[test]
    fn rejects_branch_switch_even_when_commit_is_unchanged() {
        let (repo, before, target) = fixture();
        repo.git(&["checkout", "-b", "other"]);
        assert!(execute(
            &SYSTEM_GIT,
            &repo.0,
            &before,
            "main",
            &target,
            "",
            Mode::Commit
        )
        .is_err());
        assert_eq!(head(&SYSTEM_GIT, &repo.0).unwrap(), before);
        assert!(git::load(&SYSTEM_GIT, &repo.0).unwrap().changes.is_empty());
    }

    #[test]
    fn conflicts_are_reported_and_working_tree_can_be_refreshed() {
        let (repo, _, target) = fixture();
        repo.write("feature", "different\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "diverge"]);
        let before = head(&SYSTEM_GIT, &repo.0).unwrap();
        assert!(execute(
            &SYSTEM_GIT,
            &repo.0,
            &before,
            "main",
            &target,
            "",
            Mode::Commit
        )
        .unwrap());
        assert_eq!(head(&SYSTEM_GIT, &repo.0).unwrap(), before);
        assert!(git::load(&SYSTEM_GIT, &repo.0)
            .unwrap()
            .changes
            .iter()
            .any(|file| file.kind == crate::models::ChangeKind::Conflicted));
    }
}
