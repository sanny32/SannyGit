use super::{SannyGitApp, WorkerResult};
use crate::remotes::{self, Operation, State};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

pub struct RemoteDialog {
    pub operation: Operation,
    pub state: Option<State>,
    pub remote: String,
    pub branch: String,
    pub set_upstream: bool,
}

impl SannyGitApp {
    pub fn remote_allowed(&self) -> bool {
        !self.busy()
            && self.state_current
            && self.repositories.get(self.selected_repository).is_some()
    }

    pub fn open_remote(&mut self, operation: Operation) {
        if !self.remote_allowed() {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        self.network_dialog = Some(RemoteDialog {
            operation,
            state: None,
            remote: String::new(),
            branch: String::new(),
            set_upstream: false,
        });
        self.error = None;
        self.status = "Loading remotes…".into();
        self.start_worker(move || WorkerResult::RemoteInfo {
            operation,
            result: remotes::load(&root),
        });
    }

    pub(super) fn apply_remote_info(
        &mut self,
        operation: Operation,
        result: Result<State, String>,
    ) {
        self.status = "Ready".into();
        match result {
            Ok(state) => {
                if let Some(dialog) = self
                    .network_dialog
                    .as_mut()
                    .filter(|d| d.operation == operation)
                {
                    dialog.remote = state
                        .upstream_remote
                        .as_ref()
                        .filter(|remote| state.remotes.contains(remote))
                        .cloned()
                        .or_else(|| {
                            state
                                .remotes
                                .iter()
                                .find(|r| r.as_str() == "origin")
                                .cloned()
                        })
                        .or_else(|| state.remotes.first().cloned())
                        .unwrap_or_default();
                    dialog.branch = state
                        .upstream_ref
                        .clone()
                        .or_else(|| state.branch.clone())
                        .unwrap_or_default();
                    dialog.set_upstream =
                        operation == Operation::Push && state.upstream_remote.is_none();
                    dialog.state = Some(state);
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn run_remote(&mut self) {
        if !self.remote_allowed() {
            return;
        }
        let Some(dialog) = &self.network_dialog else {
            return;
        };
        let Some(state) = &dialog.state else {
            return;
        };
        let operation = dialog.operation;
        let remote = dialog.remote.clone();
        let target = dialog.branch.clone();
        let expected_branch = state.branch.clone();
        let set_upstream = operation == Operation::Push && dialog.set_upstream;
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let cancel = Arc::new(AtomicBool::new(false));
        self.manager.cancel = Some(cancel.clone());
        self.error = None;
        self.status = format!("{} in progress…", operation.label());
        self.state_current = false;
        self.refs_current = false;
        if !self.start_worker(move || {
            let outcome = remotes::run(
                &root,
                operation,
                &remote,
                &target,
                expected_branch.as_deref(),
                &cancel,
            )
            .and_then(|()| {
                if set_upstream {
                    remotes::set_upstream(
                        &root,
                        expected_branch.as_deref().unwrap_or(""),
                        &remote,
                        &target,
                    )
                    .map_err(|error| {
                        format!("Push completed, but upstream could not be set: {error}")
                    })
                } else {
                    Ok(())
                }
            });
            WorkerResult::Operation {
                committed: false,
                success: operation.success(),
                outcome,
                snapshot: crate::git::load(&root),
            }
        }) {
            self.manager.cancel = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeChoice;

    #[test]
    fn remote_dialog_defaults_to_upstream_and_ignores_results_after_cancel() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.network_dialog = Some(RemoteDialog {
            operation: Operation::Push,
            state: None,
            remote: String::new(),
            branch: String::new(),
            set_upstream: false,
        });
        app.apply_remote_info(
            Operation::Push,
            Ok(State {
                remotes: vec!["origin".into(), "team".into()],
                branch: Some("local-name".into()),
                upstream_remote: Some("team".into()),
                upstream_ref: Some("remote-name".into()),
            }),
        );
        let dialog = app.network_dialog.as_ref().unwrap();
        assert_eq!(dialog.remote, "team");
        assert_eq!(dialog.branch, "remote-name");
        app.network_dialog = None;
        app.apply_remote_info(Operation::Push, Ok(State::default()));
        assert!(app.network_dialog.is_none());
        app.apply_remote_info(Operation::Fetch, Err("Cannot read configuration".into()));
        assert!(app.error.is_some());
        assert!(!app.busy());
    }
}
