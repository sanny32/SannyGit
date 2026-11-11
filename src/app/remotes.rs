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
    pub dont_show_again: bool,
    pub pull: PullDialog,
}

#[derive(Default)]
pub struct PullDialog {
    pub more_options: bool,
    pub update_tags: bool,
    pub remember: bool,
}

impl RemoteDialog {
    pub fn can_skip_confirmation(&self) -> bool {
        matches!(self.operation, Operation::Push | Operation::Sync)
            && self.state.as_ref().is_some_and(|state| {
                state.branch.is_some()
                    && state.remotes.contains(&self.remote)
                    && state.upstream_remote.as_deref() == Some(self.remote.as_str())
                    && state.upstream_ref.as_deref() == Some(self.branch.as_str())
                    && crate::branches::name_hint(&self.branch).is_none()
            })
    }
}

impl SannyGitApp {
    pub fn remote_confirmation_hidden(&self) -> bool {
        self.network_dialog.as_ref().is_some_and(|dialog| {
            self.skip_remote_confirmation(dialog.operation)
                && (dialog.state.is_none() || dialog.can_skip_confirmation())
        })
    }

    fn skip_remote_confirmation(&self, operation: Operation) -> bool {
        match operation {
            Operation::Push => self.skip_push_confirmation,
            Operation::Sync => self.preferences.skip_sync_confirmation,
            _ => false,
        }
    }

    pub(super) fn finish_hidden_remote(&mut self) {
        self.network_dialog = None;
    }

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
        let defaults = self
            .manager
            .preferences
            .pull_defaults
            .get(&self.repositories[self.selected_repository].path);
        self.network_dialog = Some(RemoteDialog {
            operation,
            state: None,
            remote: String::new(),
            branch: String::new(),
            set_upstream: false,
            dont_show_again: false,
            pull: PullDialog {
                update_tags: defaults.is_some_and(|defaults| defaults.update_tags),
                remember: defaults.is_some(),
                ..Default::default()
            },
        });
        self.error = None;
        self.status = "Loading remotes…".into();
        if !self.start_worker(move || WorkerResult::RemoteInfo {
            operation,
            result: remotes::load(&root),
        }) && self.remote_confirmation_hidden()
        {
            self.finish_hidden_remote();
        }
    }

    pub(super) fn apply_remote_info(
        &mut self,
        operation: Operation,
        result: Result<State, String>,
    ) {
        self.status = "Ready".into();
        let skip = self.skip_remote_confirmation(operation);
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
                    if operation == Operation::Pull {
                        if let Some(defaults) = self
                            .repositories
                            .get(self.selected_repository)
                            .and_then(|repository| {
                                self.manager.preferences.pull_defaults.get(&repository.path)
                            })
                            .filter(|defaults| state.remotes.contains(&defaults.remote))
                        {
                            dialog.remote = defaults.remote.clone();
                        }
                    }
                    dialog.branch = state
                        .upstream_ref
                        .clone()
                        .or_else(|| state.branch.clone())
                        .unwrap_or_default();
                    dialog.set_upstream =
                        operation == Operation::Push && state.upstream_remote.is_none();
                    dialog.state = Some(state);
                    if skip && dialog.can_skip_confirmation() {
                        self.run_remote();
                    }
                }
            }
            Err(error) => {
                self.error = Some(error);
                if self.remote_confirmation_hidden() {
                    self.finish_hidden_remote();
                }
            }
        }
    }

    pub fn run_remote(&mut self) {
        self.run_remote_action(false);
    }

    pub fn run_pull_fetch_only(&mut self) {
        if self
            .network_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.operation == Operation::Pull)
        {
            self.run_remote_action(true);
        }
    }

    fn run_remote_action(&mut self, fetch_only: bool) {
        if !self.remote_allowed() {
            return;
        }
        let Some(dialog) = &self.network_dialog else {
            return;
        };
        let Some(state) = &dialog.state else {
            return;
        };
        let skip_confirmation = dialog.dont_show_again && dialog.can_skip_confirmation();
        let operation = if fetch_only {
            Operation::Fetch
        } else {
            dialog.operation
        };
        let update_tags = dialog.operation == Operation::Pull && dialog.pull.update_tags;
        let tag_policy = (dialog.operation == Operation::Pull).then_some(dialog.pull.update_tags);
        let remote = dialog.remote.clone();
        let target = dialog.branch.clone();
        let expected_branch = state.branch.clone();
        let set_upstream = operation == Operation::Push && dialog.set_upstream;
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        if dialog.operation == Operation::Pull {
            let path = self.repositories[self.selected_repository].path.clone();
            if dialog.pull.remember {
                self.manager.preferences.pull_defaults.insert(
                    path,
                    crate::repository::PullDefaults {
                        remote: remote.clone(),
                        update_tags,
                    },
                );
            } else {
                self.manager.preferences.pull_defaults.remove(&path);
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        if skip_confirmation {
            match operation {
                Operation::Push => self.skip_push_confirmation = true,
                Operation::Sync => self.preferences.skip_sync_confirmation = true,
                _ => {}
            }
        }
        self.manager.cancel = Some(cancel.clone());
        self.error = None;
        self.status = format!("{} in progress…", operation.label());
        self.state_current = false;
        self.refs_current = false;
        if !self.start_worker(move || {
            let outcome = remotes::run_with_tags(
                &root,
                operation,
                &remote,
                &target,
                expected_branch.as_deref(),
                &cancel,
                tag_policy,
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
            if self.remote_confirmation_hidden() {
                self.finish_hidden_remote();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeChoice;

    #[test]
    fn pull_defaults_survive_restart_and_restore_the_remote_without_starting_an_operation() {
        let repo = crate::test_support::TempRepo::new();
        repo.git(&["remote", "add", "origin", "https://example.test/repo"]);
        repo.git(&["remote", "add", "team", "https://example.test/team"]);
        let path = repo.0.to_string_lossy().into_owned();
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories.push(crate::models::Repository {
            name: "Test".into(),
            path: path.clone(),
        });
        app.selected_repository = 0;
        app.state_current = true;
        app.manager.preferences.pull_defaults.insert(
            path.clone(),
            crate::repository::PullDefaults {
                remote: "team".into(),
                update_tags: true,
            },
        );
        let saved = serde_json::to_string(&app.manager.preferences).unwrap();
        app.manager.preferences = serde_json::from_str(&saved).unwrap();
        app.open_remote(Operation::Pull);
        assert!(app.network_dialog.as_ref().unwrap().pull.update_tags);
        assert!(app.network_dialog.as_ref().unwrap().pull.remember);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.busy() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
            app.poll_worker();
        }
        let dialog = app.network_dialog.as_ref().unwrap();
        assert_eq!(dialog.remote, "team");
        assert!(!dialog.pull.more_options);
        assert!(app.state_current);
        let legacy: crate::repository::Preferences =
            serde_json::from_str(r#"{"groups":[],"selected_group":null}"#).unwrap();
        assert!(legacy.pull_defaults.is_empty());
    }

    #[test]
    fn skip_confirmation_requires_the_current_branch_upstream() {
        let mut dialog = RemoteDialog {
            operation: Operation::Push,
            state: Some(State {
                remotes: vec!["origin".into()],
                branch: Some("main".into()),
                upstream_remote: Some("origin".into()),
                upstream_ref: Some("published".into()),
                ..Default::default()
            }),
            remote: "origin".into(),
            branch: "published".into(),
            set_upstream: false,
            dont_show_again: false,
            pull: Default::default(),
        };
        assert!(dialog.can_skip_confirmation());
        dialog.branch = "other".into();
        assert!(!dialog.can_skip_confirmation());
        dialog.branch = "published".into();
        dialog.remote = "other".into();
        assert!(!dialog.can_skip_confirmation());
        dialog.remote = "origin".into();
        dialog.state.as_mut().unwrap().upstream_remote = None;
        assert!(!dialog.can_skip_confirmation());
        dialog.state.as_mut().unwrap().upstream_remote = Some("origin".into());
        dialog.state.as_mut().unwrap().branch = None;
        assert!(!dialog.can_skip_confirmation());
        dialog.state = None;
        assert!(!dialog.can_skip_confirmation());
    }

    #[test]
    fn skipped_push_hides_loading_and_preserves_configuration_error_for_modal() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.skip_push_confirmation = true;
        app.network_dialog = Some(RemoteDialog {
            operation: Operation::Push,
            state: None,
            remote: String::new(),
            branch: String::new(),
            set_upstream: false,
            dont_show_again: false,
            pull: Default::default(),
        });
        assert!(app.remote_confirmation_hidden());
        app.apply_remote_info(Operation::Push, Err("Cannot read configuration".into()));
        assert!(app.network_dialog.is_none());
        assert_eq!(app.error.as_deref(), Some("Cannot read configuration"));
    }

    #[test]
    fn sync_skip_is_independent_and_requires_upstream() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
        app.skip_push_confirmation = true;
        app.network_dialog = Some(RemoteDialog {
            operation: Operation::Sync,
            state: None,
            remote: String::new(),
            branch: String::new(),
            set_upstream: false,
            dont_show_again: false,
            pull: Default::default(),
        });
        assert!(!app.remote_confirmation_hidden());
        app.preferences.skip_sync_confirmation = true;
        assert!(app.remote_confirmation_hidden());
        app.apply_remote_info(
            Operation::Sync,
            Ok(State {
                remotes: vec!["origin".into()],
                branch: Some("main".into()),
                ..Default::default()
            }),
        );
        assert!(!app.remote_confirmation_hidden());
        assert!(!app.network_dialog.as_ref().unwrap().can_skip_confirmation());
        app.network_dialog.as_mut().unwrap().state = None;
        app.apply_remote_info(Operation::Sync, Err("Cannot read configuration".into()));
        assert!(app.network_dialog.is_none());
        assert_eq!(app.error.as_deref(), Some("Cannot read configuration"));
    }

    #[test]
    fn remote_dialog_defaults_to_upstream_and_ignores_results_after_cancel() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.network_dialog = Some(RemoteDialog {
            operation: Operation::Push,
            state: None,
            remote: String::new(),
            branch: String::new(),
            set_upstream: false,
            dont_show_again: false,
            pull: Default::default(),
        });
        app.apply_remote_info(
            Operation::Push,
            Ok(State {
                remotes: vec!["origin".into(), "team".into()],
                branch: Some("local-name".into()),
                upstream_remote: Some("team".into()),
                upstream_ref: Some("remote-name".into()),
                ..Default::default()
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
