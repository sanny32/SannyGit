//! Cherry-pick a reviewed commit onto the current branch.
use crate::git::GitContext;
use crate::{git, models::ChangeKind};
use std::path::Path;

fn stopped_on_conflicts(snapshot: &git::Snapshot) -> bool {
    snapshot.sequencing == git::Sequencing::CherryPick
        && snapshot
            .changes
            .iter()
            .any(|change| change.kind == ChangeKind::Conflicted)
}

pub fn execute(
    git: &GitContext,
    root: &Path,
    expected_head: &str,
    expected_branch: &str,
    target: &str,
) -> Result<bool, String> {
    git::preflight_apply_commit(git, root, expected_head, expected_branch, target)?;
    match git::run(git, root, &["cherry-pick", target]) {
        Ok(_) => Ok(false),
        Err(error) => {
            let snapshot = git::load(git, root)?;
            if stopped_on_conflicts(&snapshot) {
                Ok(true)
            } else {
                Err(error)
            }
        }
    }
}

pub fn continue_pick(git: &GitContext, root: &Path) -> Result<bool, String> {
    match git::run(git, root, &["cherry-pick", "--continue"]) {
        Ok(_) => Ok(false),
        Err(error) => {
            let snapshot = git::load(git, root)?;
            if stopped_on_conflicts(&snapshot) {
                Ok(true)
            } else {
                Err(error)
            }
        }
    }
}

pub fn skip(git: &GitContext, root: &Path) -> Result<(), String> {
    git::run(git, root, &["cherry-pick", "--skip"]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge;
    use crate::test_support::{TempRepo, SYSTEM_GIT};

    fn fixture() -> (TempRepo, String, String) {
        let repo = TempRepo::new();
        repo.write("base", "base\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "base"]);
        let before = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["checkout", "-b", "feature"]);
        repo.write("feature", "feature\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "feature"]);
        let picked = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["checkout", "main"]);
        (repo, before, picked)
    }

    #[test]
    fn applies_the_selected_commit() {
        let (repo, before, picked) = fixture();
        execute(&SYSTEM_GIT, &repo.0, &before, "main", &picked).unwrap();
        assert_ne!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);
        assert!(repo.0.join("feature").exists());
    }
}
