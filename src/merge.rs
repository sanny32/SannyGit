//! Explicit merge operations; the dialog pins the target commit and current HEAD.
use crate::git;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Commit,
    WorkingTree,
}

pub fn head(root: &Path) -> Result<String, String> {
    git::text(git::run(root, &["rev-parse", "--verify", "HEAD^{commit}"])?)
        .map(|value| value.trim().to_owned())
}

pub fn execute(
    root: &Path,
    expected_head: &str,
    expected_branch: &str,
    target: &str,
    mode: Mode,
) -> Result<(), String> {
    if !matches!(target.len(), 40 | 64) || !target.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Select a valid commit before merging.".into());
    }
    git::require_no_operation(root)?;
    if head(root)? != expected_head {
        return Err("HEAD changed. Close and reopen Merge before retrying.".into());
    }
    let snapshot = git::load(root)?;
    if snapshot.branch != expected_branch {
        return Err("The current branch changed. Close and reopen Merge before retrying.".into());
    }
    if !snapshot.changes.is_empty() {
        return Err("Commit or stash local changes before merging.".into());
    }
    let args = match mode {
        Mode::Commit => vec!["merge", "--no-ff", "--no-edit", "--no-autostash", target],
        Mode::WorkingTree => vec!["merge", "--squash", "--no-commit", "--no-autostash", target],
    };
    git::run(root, &args).map(|_| ()).map_err(|error| {
        format!("{error}\nIf conflicts occurred, resolve them outside SannyGit; finish or abort an unfinished merge with Git before retrying.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempRepo;

    fn fixture() -> (TempRepo, String, String) {
        let repo = TempRepo::new();
        repo.write("base", "base\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "base"]);
        let before = head(&repo.0).unwrap();
        repo.git(&["checkout", "-b", "feature"]);
        repo.write("feature", "feature\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "feature"]);
        let target = head(&repo.0).unwrap();
        repo.git(&["checkout", "main"]);
        (repo, before, target)
    }

    #[test]
    fn commit_creates_two_parents_even_for_fast_forward() {
        let (repo, before, target) = fixture();
        execute(&repo.0, &before, "main", &target, Mode::Commit).unwrap();
        assert_eq!(
            repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(),
            format!("{before} {target}")
        );
        assert!(git::load(&repo.0).unwrap().changes.is_empty());
    }

    #[test]
    fn working_tree_squashes_without_moving_head_and_can_be_committed() {
        let (repo, before, target) = fixture();
        execute(&repo.0, &before, "main", &target, Mode::WorkingTree).unwrap();
        assert_eq!(head(&repo.0).unwrap(), before);
        assert!(git::load(&repo.0)
            .unwrap()
            .changes
            .iter()
            .any(|file| file.path == "feature" && file.staged));
        git::commit(&repo.0, "squashed", git::CommitContents::Staged).unwrap();
        assert_eq!(
            repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(),
            before
        );
    }

    #[test]
    fn rejects_stale_head_dirty_tree_invalid_target_and_unfinished_merge() {
        let (repo, before, target) = fixture();
        assert!(execute(&repo.0, &target, "main", &target, Mode::Commit).is_err());
        assert!(execute(&repo.0, &before, "main", "--all", Mode::Commit).is_err());
        repo.write("local", "keep");
        assert!(execute(&repo.0, &before, "main", &target, Mode::WorkingTree).is_err());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("local")).unwrap(),
            "keep"
        );
        assert_eq!(head(&repo.0).unwrap(), before);
        assert!(repo.git(&["diff", "--cached"]).is_empty());
        let (repo, before, target) = fixture();
        std::fs::write(repo.0.join(".git/MERGE_HEAD"), format!("{target}\n")).unwrap();
        assert!(execute(&repo.0, &before, "main", &target, Mode::Commit).is_err());
        assert_eq!(head(&repo.0).unwrap(), before);
    }

    #[test]
    fn rejects_branch_switch_even_when_commit_is_unchanged() {
        let (repo, before, target) = fixture();
        repo.git(&["checkout", "-b", "other"]);
        assert!(execute(&repo.0, &before, "main", &target, Mode::Commit).is_err());
        assert_eq!(head(&repo.0).unwrap(), before);
        assert!(git::load(&repo.0).unwrap().changes.is_empty());
    }

    #[test]
    fn conflicts_are_reported_and_working_tree_can_be_refreshed() {
        let (repo, _, target) = fixture();
        repo.write("feature", "different\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "diverge"]);
        let before = head(&repo.0).unwrap();
        assert!(execute(&repo.0, &before, "main", &target, Mode::Commit).is_err());
        assert_eq!(head(&repo.0).unwrap(), before);
        assert!(git::load(&repo.0)
            .unwrap()
            .changes
            .iter()
            .any(|file| file.kind == crate::models::ChangeKind::Conflicted));
    }
}
