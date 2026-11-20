use super::SannyGitApp;
use crate::logging::{self, Level};
use std::collections::{BTreeMap, VecDeque};

#[derive(Default)]
pub struct ErrorDialogs {
    pub pending: VecDeque<String>,
    observed: BTreeMap<String, String>,
}

impl SannyGitApp {
    /// Retain diagnostics used for retry state, but present each new error once.
    pub fn collect_errors(&mut self) {
        if let Some(error) = logging::take_error() {
            if !self.error_dialogs.pending.contains(&error) {
                self.error_dialogs.pending.push_back(error);
            }
        }
        let mut current = BTreeMap::new();
        for (source, error) in [
            ("application", self.error.as_deref()),
            ("branches", self.refs_error.as_deref()),
            ("history", self.history.history_error.as_deref()),
            ("commit diff", self.history.commit_preview_error.as_deref()),
            (
                "tag history",
                self.tag_log
                    .as_ref()
                    .and_then(|view| view.history_error.as_deref()),
            ),
            (
                "tag commit diff",
                self.tag_log
                    .as_ref()
                    .and_then(|view| view.commit_preview_error.as_deref()),
            ),
            // Periodic background fetch failures stay in the tracking monitor:
            // they surface as the "?" badge and its tooltip, not as a modal.
            ("working tree", self.working_tree.error.as_deref()),
            (
                "discard",
                self.discard_dialog
                    .as_ref()
                    .and_then(|d| d.error.as_deref()),
            ),
            (
                "stash",
                self.stash_dialog.as_ref().and_then(|d| d.error.as_deref()),
            ),
            (
                "refs",
                self.ref_dialog.as_ref().and_then(|d| d.error.as_deref()),
            ),
            ("settings", self.settings_window.llm.secret_error.as_deref()),
        ] {
            if let Some(error) = error {
                current.insert(source.to_owned(), error.to_owned());
            }
        }
        for (path, draft) in &self.settings_window.llm_repositories {
            if let Some(error) = &draft.secret_error {
                current.insert(format!("settings:{path}"), error.clone());
            }
        }
        for (source, error) in &current {
            if self.error_dialogs.observed.get(source) != Some(error)
                && !self.error_dialogs.pending.contains(error)
            {
                let source = if source.starts_with("settings:") {
                    "repository settings"
                } else {
                    source
                };
                logging::record(Level::Error, format_args!("Error reported in {source}"));
                self.error_dialogs.pending.push_back(error.clone());
            }
        }
        self.error_dialogs.observed = current;
    }

    pub fn dismiss_error(&mut self) {
        if let Some(error) = self.error_dialogs.pending.pop_front() {
            if self.error.as_ref() == Some(&error) {
                self.error = None;
                self.error_dialogs.observed.remove("application");
            }
            // Credential diagnostics are transient; clearing them lets a failed
            // synchronous retry report the same message again.
            if self.settings_window.llm.secret_error.as_ref() == Some(&error) {
                self.settings_window.llm.secret_error = None;
            }
            for draft in self.settings_window.llm_repositories.values_mut() {
                if draft.secret_error.as_ref() == Some(&error) {
                    draft.secret_error = None;
                }
            }
            self.error_dialogs
                .observed
                .retain(|source, message| !(source.starts_with("settings") && message == &error));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeChoice;

    #[test]
    fn simultaneous_errors_are_queued_deduplicated_and_retained_until_dismissed() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.error = Some("Operation failed".into());
        app.history.history_error = app.error.clone();
        app.refs_error = Some("Cannot load branches".into());
        app.collect_errors();
        assert_eq!(app.error_dialogs.pending.len(), 2);
        app.error = None;
        app.refs_error = None;
        app.collect_errors();
        assert_eq!(app.error_dialogs.pending.len(), 2);
        app.dismiss_error();
        app.dismiss_error();
        app.collect_errors();
        assert!(app.error_dialogs.pending.is_empty());
        assert!(app.history.history_error.is_some());
        app.history.history_error = None;
        app.collect_errors();
        app.history.history_error = Some("Operation failed".into());
        app.collect_errors();
        assert_eq!(app.error_dialogs.pending.len(), 1);
    }

    #[test]
    fn settings_and_preview_errors_reach_the_queue_without_model_list_errors() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
        app.settings_window.llm.secret_error = Some("Credential access failed".into());
        app.history.commit_preview_error = Some("Commit diff failed".into());
        app.llm_models.entry(&app.llm_config, None).error = Some("Model list failed".into());
        app.collect_errors();
        assert_eq!(app.error_dialogs.pending.len(), 2);
        app.collect_errors();
        assert_eq!(app.error_dialogs.pending.len(), 2);
        app.settings_window.llm.secret_error = Some("Another credential error".into());
        app.collect_errors();
        assert_eq!(app.error_dialogs.pending.len(), 3);
    }

    #[test]
    fn periodic_background_fetch_errors_stay_in_the_monitor_not_in_dialogs() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
        app.tracking.error = Some("Git fetch failed".into());
        app.collect_errors();
        assert!(app.error_dialogs.pending.is_empty());
        assert!(app.tracking.error.is_some(), "retry state must be retained");
    }

    #[test]
    fn dismissing_a_credential_error_allows_an_identical_synchronous_retry() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.settings_window.llm.secret_error = Some("Cannot save key".into());
        app.collect_errors();
        app.dismiss_error();
        app.settings_window.llm.secret_error = Some("Cannot save key".into());
        app.collect_errors();
        assert_eq!(app.error_dialogs.pending.len(), 1);
    }
}
