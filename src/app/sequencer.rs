use super::{SannyGitApp, WorkerResult};
use crate::{git::Sequencing, models::ChangeKind};
use std::path::Path;

impl SannyGitApp {
    pub fn sequencer_continue_allowed(&self) -> bool {
        self.merging
            && matches!(
                self.sequencing,
                Sequencing::CherryPick | Sequencing::Revert | Sequencing::Rebase
            )
            && self.state_current
            && !self.busy()
            && self.repositories.get(self.selected_repository).is_some()
            && !self
                .changes
                .iter()
                .any(|change| change.kind == ChangeKind::Conflicted)
    }

    pub fn sequencer_skip_allowed(&self) -> bool {
        self.sequencer_continue_allowed() && self.sequencing.supports_skip()
    }

    pub fn continue_sequence(&mut self) {
        if !self.sequencer_continue_allowed() {
            return;
        }
        let root = self.repositories[self.selected_repository].path.clone();
        let sequencing = self.sequencing;
        let status = match sequencing {
            Sequencing::CherryPick => "Continuing cherry-pick…",
            Sequencing::Revert => "Continuing revert…",
            Sequencing::Rebase => "Continuing rebase…",
            _ => return,
        };
        if self.start_worker(move |git| {
            let root = Path::new(&root);
            let (outcome, paused) = match sequencing {
                Sequencing::CherryPick => {
                    let result = crate::cherry_pick::continue_pick(git, root);
                    let paused = result.as_ref() == Ok(&true);
                    (result.map(|_| ()), paused)
                }
                Sequencing::Revert => {
                    let result = crate::revert::continue_revert(git, root);
                    let paused = result.as_ref() == Ok(&true);
                    (result.map(|_| ()), paused)
                }
                Sequencing::Rebase => (crate::rebase::continue_rebase(git, root), false),
                _ => (Ok(()), false),
            };
            let success = match (sequencing, paused, outcome.is_err()) {
                (_, true, false) => "Resolve conflicts in the working tree",
                (Sequencing::CherryPick, _, false) => "Cherry-pick complete",
                (Sequencing::CherryPick, _, true) => "Cherry-pick failed",
                (Sequencing::Revert, _, false) => "Revert complete",
                (Sequencing::Revert, _, true) => "Revert failed",
                (Sequencing::Rebase, _, false) => "Rebase complete",
                (Sequencing::Rebase, _, true) => "Rebase failed",
                _ => "Operation complete",
            };
            WorkerResult::Operation {
                committed: false,
                success,
                outcome,
                snapshot: crate::git::load(git, root),
            }
        }) {
            self.status = status.into();
        }
    }

    pub fn skip_sequence(&mut self) {
        if !self.sequencer_skip_allowed() {
            return;
        }
        let root = self.repositories[self.selected_repository].path.clone();
        let sequencing = self.sequencing;
        let (status, success) = match sequencing {
            Sequencing::CherryPick => ("Skipping cherry-pick commit…", "Cherry-pick skipped"),
            Sequencing::Revert => ("Skipping revert…", "Revert skipped"),
            Sequencing::Rebase => ("Skipping rebase commit…", "Rebase step skipped"),
            _ => return,
        };
        if self.start_worker(move |git| {
            let root = Path::new(&root);
            let outcome = match sequencing {
                Sequencing::CherryPick => crate::cherry_pick::skip(git, root),
                Sequencing::Revert => crate::revert::skip(git, root),
                Sequencing::Rebase => crate::rebase::skip(git, root),
                _ => Ok(()),
            };
            WorkerResult::Operation {
                committed: false,
                success,
                outcome,
                snapshot: crate::git::load(git, root),
            }
        }) {
            self.status = status.into();
        }
    }
}
