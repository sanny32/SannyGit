use super::{SannyGitApp, WorkerResult};
use crate::branches;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
enum PickOrRevert {
    CherryPick,
    Revert,
}

impl PickOrRevert {
    fn progress(self) -> &'static str {
        match self {
            Self::CherryPick => "Cherry-pick…",
            Self::Revert => "Revert…",
        }
    }

    fn success(self, conflicts: bool, failed: bool) -> &'static str {
        match (self, conflicts, failed) {
            (_, _, true) => match self {
                Self::CherryPick => "Cherry-pick failed",
                Self::Revert => "Revert failed",
            },
            (Self::CherryPick, true, false) => {
                "Cherry-pick started. Resolve conflicts in the working tree"
            }
            (Self::CherryPick, false, false) => "Cherry-pick complete",
            (Self::Revert, true, false) => "Revert started. Resolve conflicts in the working tree",
            (Self::Revert, false, false) => "Revert complete",
        }
    }

    fn run(
        self,
        git: &crate::git::GitContext,
        root: &Path,
        head: &str,
        branch: &str,
        target: &str,
        mainline: Option<u8>,
    ) -> Result<bool, String> {
        match self {
            Self::CherryPick => crate::cherry_pick::execute(git, root, head, branch, target),
            Self::Revert => crate::revert::execute(git, root, head, branch, target, mainline),
        }
    }
}

#[derive(Clone)]
pub struct RevertMergeDialog {
    pub root: String,
    pub commit: String,
    pub parents: Vec<String>,
    pub expected_head: String,
    pub expected_branch: String,
}

impl SannyGitApp {
    pub fn cherry_pick_commit(&mut self, commit: String) {
        self.start_pick_or_revert(PickOrRevert::CherryPick, commit);
    }

    pub fn revert_commit(&mut self, commit: String) {
        if !self.merge_allowed() || !branches::is_commit_id(&commit) {
            return;
        }
        let root = self.repositories[self.selected_repository].path.clone();
        let expected_head = self.branches.head_commit.clone();
        let expected_branch = self.branches.current_branch.clone();
        if self.start_worker(move |git| {
            let path = Path::new(&root);
            match crate::revert::parent_ids(git, path, &commit) {
                Ok(parents) => WorkerResult::RevertMergeParents(RevertMergeDialog {
                    root,
                    commit,
                    parents,
                    expected_head,
                    expected_branch,
                }),
                Err(error) => WorkerResult::Repository(Err(error)),
            }
        }) {
            self.status = "Checking revert…".into();
        }
    }

    pub(super) fn apply_revert_merge_parents(&mut self, dialog: RevertMergeDialog) {
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        if repository.path != dialog.root {
            return;
        }
        if dialog.parents.len() > 1 {
            self.revert_merge_dialog = Some(dialog);
            self.status = "Ready".into();
            return;
        }
        self.run_revert(
            dialog.commit,
            dialog.expected_head,
            dialog.expected_branch,
            None,
        );
    }

    pub fn confirm_revert_merge_parent(&mut self, parent: u8) {
        let Some(dialog) = self.revert_merge_dialog.take() else {
            return;
        };
        if parent == 0 || parent as usize > dialog.parents.len() {
            return;
        }
        self.run_revert(
            dialog.commit,
            dialog.expected_head,
            dialog.expected_branch,
            Some(parent),
        );
    }

    fn run_revert(
        &mut self,
        commit: String,
        expected_head: String,
        expected_branch: String,
        mainline: Option<u8>,
    ) {
        let root = self.repositories[self.selected_repository].path.clone();
        let target = commit.clone();
        if self.start_background_operation(move |git| {
            let root = Path::new(&root);
            let result = PickOrRevert::Revert.run(
                git,
                root,
                &expected_head,
                &expected_branch,
                &target,
                mainline,
            );
            let snapshot = crate::git::load(git, root);
            let success =
                PickOrRevert::Revert.success(result.as_ref() == Ok(&true), result.is_err());
            WorkerResult::Operation {
                committed: false,
                success,
                outcome: result.map(|_| ()),
                snapshot,
            }
        }) {
            self.status = "Revert…".into();
        }
    }

    pub fn checkout_commit(&mut self, commit: String) {
        if self.busy() || self.branch_base_blocker().is_some() || !branches::is_commit_id(&commit) {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        self.state_current = false;
        self.branches.current = false;
        self.error = None;
        self.status = "Checking out commit…".into();
        self.start_background_operation(move |git| {
            let outcome = crate::branches::checkout_commit(git, &root, &commit);
            WorkerResult::Operation {
                committed: false,
                success: "Checkout complete",
                outcome,
                snapshot: crate::git::load(git, &root),
            }
        });
    }

    pub fn open_branch_at_commit(&mut self, commit: String) {
        if self.busy() || self.branch_base_blocker().is_some() || !branches::is_commit_id(&commit) {
            return;
        }
        self.branches.create_from_commit = Some(commit);
        self.branches.branch_dialog_open = true;
        self.branches.new_branch_name.clear();
    }

    fn start_pick_or_revert(&mut self, operation: PickOrRevert, commit: String) {
        if !self.merge_allowed() || !branches::is_commit_id(&commit) {
            return;
        }
        let root = self.repositories[self.selected_repository].path.clone();
        let expected_head = self.branches.head_commit.clone();
        let expected_branch = self.branches.current_branch.clone();
        let target = commit.clone();
        if self.start_background_operation(move |git| {
            let root = Path::new(&root);
            let result = operation.run(git, root, &expected_head, &expected_branch, &target, None);
            let snapshot = crate::git::load(git, root);
            let success = operation.success(result.as_ref() == Ok(&true), result.is_err());
            WorkerResult::Operation {
                committed: false,
                success,
                outcome: result.map(|_| ()),
                snapshot,
            }
        }) {
            self.status = operation.progress().into();
        }
    }
}