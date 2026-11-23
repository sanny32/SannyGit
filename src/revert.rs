//! Revert a reviewed commit on the current branch.
use crate::git::GitContext;
use crate::{git, models::ChangeKind};
use std::path::Path;

fn stopped_on_conflicts(snapshot: &git::Snapshot) -> bool {
    snapshot.sequencing == git::Sequencing::Revert
        && snapshot
            .changes
            .iter()
            .any(|change| change.kind == ChangeKind::Conflicted)
}

pub fn parent_ids(git: &GitContext, root: &Path, target: &str) -> Result<Vec<String>, String> {
    let parents = git::text(git::run(
        git,
        root,
        &[
            "rev-list",
            "--parents",
            "-n",
            "1",
            "--end-of-options",
            target,
        ],
    )?)?;
    Ok(parents
        .split_whitespace()
        .skip(1)
        .map(str::to_owned)
        .collect())
}

pub fn execute(
    git: &GitContext,
    root: &Path,
    expected_head: &str,
    expected_branch: &str,
    target: &str,
    mainline: Option<u8>,
) -> Result<bool, String> {
    git::preflight_apply_commit(git, root, expected_head, expected_branch, target)?;
    let run = match mainline {
        None => git::run(git, root, &["revert", "--no-edit", target]),
        Some(1) => git::run(git, root, &["revert", "--no-edit", "-m", "1", target]),
        Some(2) => git::run(git, root, &["revert", "--no-edit", "-m", "2", target]),
        Some(_) => return Err("Select parent 1 or parent 2 for a merge commit.".into()),
    };
    match run {
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

pub fn continue_revert(git: &GitContext, root: &Path) -> Result<bool, String> {
    match git::run(git, root, &["revert", "--continue"]) {
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
    git::run(git, root, &["revert", "--skip"]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge;
    use crate::test_support::{TempRepo, SYSTEM_GIT};

    #[test]
    fn creates_a_revert_commit() {
        let repo = TempRepo::new();
        repo.write("file", "one\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "first"]);
        let first = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.write("file", "two\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "second"]);
        let head = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        execute(&SYSTEM_GIT, &repo.0, &head, "main", &first, None).unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "two\n"
        );
    }
}
