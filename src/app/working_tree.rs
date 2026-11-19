//! Periodic, read-only working-tree checks, independent of interactive operations.
use super::SannyGitApp;
use crate::{git, models::DiffPreview};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

const CHECK_INTERVAL: Duration = Duration::from_secs(2);

#[cfg(test)]
#[path = "working_tree_performance.rs"]
mod performance;

pub(super) struct Update {
    path: String,
    snapshot: Result<git::Snapshot, String>,
    diff: Option<FileDiff>,
}

struct FileDiff {
    path: String,
    result: Result<(String, DiffPreview), String>,
}

impl Update {
    pub(super) fn read(path: String, selected: Option<String>) -> Self {
        let snapshot = git::load(Path::new(&path));
        let diff = snapshot.as_ref().ok().and_then(|snapshot| {
            let change = snapshot
                .changes
                .iter()
                .find(|change| Some(&change.path) == selected.as_ref())?;
            Some(FileDiff {
                path: change.path.clone(),
                result: git::diff(
                    Path::new(&path),
                    &change.path,
                    change.kind,
                    change.original_path.as_deref(),
                )
                .map(|patch| {
                    let preview = DiffPreview::new(&patch);
                    (patch, preview)
                }),
            })
        });
        Self {
            path,
            snapshot,
            diff,
        }
    }
}

pub(super) struct Monitor {
    receiver: Option<Receiver<Update>>,
    next_read: Instant,
    refresh_requested: bool,
    pub(super) error: Option<String>,
}

impl Default for Monitor {
    fn default() -> Self {
        Self {
            receiver: None,
            next_read: Instant::now() + CHECK_INTERVAL,
            refresh_requested: false,
            error: None,
        }
    }
}

impl Monitor {
    pub(super) fn invalidate(&mut self) {
        self.receiver = None;
        self.next_read = Instant::now() + CHECK_INTERVAL;
        self.refresh_requested = false;
    }
}

impl SannyGitApp {
    pub(super) fn refresh_working_tree(&mut self) {
        if self.busy() || self.repositories.get(self.selected_repository).is_none() {
            return;
        }
        self.working_tree.invalidate();
        self.working_tree.refresh_requested = true;
        self.working_tree.next_read = Instant::now();
        self.error = None;
    }

    pub(super) fn check_working_tree(&mut self, context: &eframe::egui::Context) {
        if self.busy()
            || (!self.state_current && !self.working_tree.refresh_requested)
            || self.working_tree.receiver.is_some()
            || Instant::now() < self.working_tree.next_read
        {
            return;
        }
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let path = repository.path.clone();
        let selected = self
            .changes
            .get(self.selected_change)
            .map(|c| c.path.clone());
        let context = context.clone();
        let (sender, receiver) = mpsc::channel();
        self.working_tree.next_read = Instant::now() + CHECK_INTERVAL;
        let git_executable = self.preferences.git_executable.clone();
        match std::thread::Builder::new()
            .name("sannygit-working-tree".into())
            .spawn(move || {
                git::set_executable(git_executable);
                let _ = sender.send(Update::read(path, selected));
                context.request_repaint();
            }) {
            Ok(_) => self.working_tree.receiver = Some(receiver),
            Err(error) => self.working_tree_error(
                format!("Cannot check working tree: {error}"),
                self.working_tree.refresh_requested,
            ),
        }
    }

    fn working_tree_error(&mut self, error: String, explicit: bool) {
        let operation = if explicit {
            "Refresh"
        } else {
            "Automatic refresh"
        };
        let error = format!("{operation} failed; previous view retained: {error}");
        // Do not replace an error from an explicit user operation.
        if self.error.is_none() || self.error == self.working_tree.error {
            self.error = Some(error.clone());
        }
        self.working_tree.error = Some(error);
    }

    pub(super) fn poll_working_tree(&mut self) {
        if self.busy() {
            return;
        }
        let Some(receiver) = &self.working_tree.receiver else {
            return;
        };
        let update = match receiver.try_recv() {
            Ok(update) => update,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                let explicit = self.working_tree.refresh_requested;
                self.working_tree.invalidate();
                self.working_tree_error("Background worker stopped unexpectedly".into(), explicit);
                return;
            }
        };
        let refresh_requested = self.working_tree.refresh_requested;
        self.working_tree.invalidate();
        if !self
            .repositories
            .get(self.selected_repository)
            .is_some_and(|r| r.path == update.path)
        {
            return;
        }
        let snapshot = match update.snapshot {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.working_tree_error(error, refresh_requested);
                return;
            }
        };
        if self.working_tree.error.is_some() && self.error == self.working_tree.error {
            self.error = None;
        }
        self.working_tree.error = None;
        self.apply_working_tree_snapshot(snapshot);
        if let Some(diff) = update.diff {
            if let Err(error) = self.apply_working_tree_diff(diff) {
                self.working_tree_error(error, refresh_requested);
            }
        }
        if matches!(self.status.as_str(), "Ready" | "Working tree clean") {
            self.status = if self.changes.is_empty() {
                "Working tree clean"
            } else {
                "Ready"
            }
            .into();
        }
        if refresh_requested {
            self.refs_pending = true;
            self.refs_loading = true;
            self.refs_error = None;
            self.queue_history_reload();
        }
    }

    pub(super) fn apply_index_update(
        &mut self,
        update: Update,
        staged: bool,
        outcome: Result<(), String>,
    ) {
        if !self
            .repositories
            .get(self.selected_repository)
            .is_some_and(|r| crate::repository::same_path(&r.path, &update.path))
        {
            return;
        }
        self.working_tree.invalidate();
        let success = if staged {
            "File staged"
        } else {
            "Index updated"
        };
        if outcome.is_ok() && staged && self.preferences.staged_only_after_staging {
            self.commit_contents = git::CommitContents::Staged;
        }
        let refresh_error = match update.snapshot {
            Ok(snapshot) => {
                self.apply_working_tree_snapshot(snapshot);
                None
            }
            Err(error) => {
                self.state_current = false;
                Some(format!(
                    "Could not refresh repository: {error}. Refresh before further operations."
                ))
            }
        };
        self.status = if outcome.is_ok() {
            success
        } else {
            "Operation failed"
        }
        .into();
        self.error = match (outcome, refresh_error) {
            (Ok(()), error) => error.map(|error| format!("{success}. {error}")),
            (Err(error), Some(refresh)) => Some(format!("{error}\n{refresh}")),
            (Err(error), None) => Some(error),
        };
        if !self.state_current {
            self.status
                .push_str("; refresh required before further operations");
        } else if let Some(diff) = update.diff {
            if let Err(error) = self.apply_working_tree_diff(diff) {
                let refresh = format!("Could not refresh diff: {error}");
                self.error = Some(match self.error.take() {
                    Some(error) => format!("{error}\n{refresh}"),
                    None => refresh,
                });
            }
        }
    }

    fn apply_working_tree_snapshot(&mut self, snapshot: git::Snapshot) {
        if snapshot.branch != self.current_branch
            || !self
                .repositories
                .get(self.selected_repository)
                .is_some_and(|r| crate::repository::same_path(&r.path, &snapshot.repository.path))
        {
            self.apply_snapshot(snapshot);
            return;
        }
        self.state_current = true;
        let selected = self
            .changes
            .get(self.selected_change)
            .map(|c| c.path.clone());
        let previous_diff =
            self.loaded_diff == Some(self.selected_change) && self.diff_preview.is_some();
        let retained_diff = self
            .changes
            .get_mut(self.selected_change)
            .map(|change| std::mem::take(&mut change.diff))
            .unwrap_or_default();
        let index = selected
            .as_ref()
            .and_then(|path| snapshot.changes.iter().position(|c| &c.path == path));
        self.changes = snapshot.changes;
        self.update_commit_contents_after_refresh(false);
        // Row indices and file contents may have changed during the read.
        self.diff_refresh_pending |= self.diff_worker.take().is_some() && previous_diff;
        self.selected_change = index
            .or_else(|| (!self.changes.is_empty()).then_some(0))
            .unwrap_or(usize::MAX);
        self.retain_change_selection();
        if index.is_some()
            && previous_diff
            && self
                .changes
                .get(self.selected_change)
                .is_some_and(|c| Some(&c.path) == selected.as_ref())
        {
            self.loaded_diff = Some(self.selected_change);
            self.changes[self.selected_change].diff = retained_diff;
        } else {
            self.loaded_diff = None;
            self.diff_preview = None;
            self.diff_refresh_pending = false;
        }
    }

    fn apply_working_tree_diff(&mut self, diff: FileDiff) -> Result<(), String> {
        if let Some(change) = self
            .changes
            .get_mut(self.selected_change)
            .filter(|c| c.path == diff.path)
        {
            match diff.result {
                Ok((patch, preview)) => {
                    if change.diff != patch || self.diff_preview.is_none() {
                        self.diff_preview = Some(preview);
                    }
                    change.diff = patch;
                    self.loaded_diff = Some(self.selected_change);
                    self.diff_refresh_pending = false;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{models::ChangeKind, test_support::TempRepo, theme::ThemeChoice};

    fn opened(repo: &TempRepo) -> SannyGitApp {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(git::load(&repo.0).unwrap());
        app
    }

    #[test]
    fn index_updates_preserve_views_and_in_flight_history_and_ref_reads() {
        let repo = TempRepo::new();
        for path in ["a", "b"] {
            repo.write(path, "base\n");
        }
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "base"]);
        for path in ["a", "b"] {
            repo.write(path, "edited\n");
        }
        let mut app = opened(&repo);
        refresh(&mut app);
        app.commit_message = "Keep draft".into();
        app.change_filter = super::super::ChangeFilter::Modified;
        app.refs_current = true;
        app.refs_pending = false;
        app.refs_loading = false;
        app.history_pending = false;
        app.history_loading = false;
        app.loaded_commit = Some("commit-to-keep".into());
        let generation = app.history_session.generation;
        let (history_sender, history_receiver) = mpsc::channel();
        app.history_worker = Some(history_receiver);
        let (refs_sender, refs_receiver) = mpsc::channel();
        app.refs_worker = Some(refs_receiver);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        app.history_session.cancel = Some(cancel.clone());

        // Editing an unselected file must retain the parsed selected preview.
        // Group edits must refresh its staged/unstaged sections without clearing it.
        for (group, staged) in [(false, true), (false, false), (true, true), (true, false)] {
            let preview = app.diff_preview.as_ref().unwrap().lines.as_ptr();
            let patch = app.changes[app.selected_change].diff.clone();
            if group {
                app.selected_paths = ["a".into(), "b".into()].into_iter().collect();
                app.set_selected_staged(staged);
            } else {
                app.set_staged(1, staged);
            }
            assert!(app.busy());
            assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);
            assert_eq!(app.changes[app.selected_change].diff, patch);
            let deadline = Instant::now() + Duration::from_secs(10);
            while app.busy() {
                assert!(Instant::now() < deadline);
                app.poll_worker();
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(app.error.is_none(), "{:?}", app.error);
            assert_eq!(app.changes[app.selected_change].path, "a");
            assert_eq!(app.changes[app.selected_change].staged, group && staged);
            assert_eq!(app.loaded_diff, Some(app.selected_change));
            if !group {
                assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);
            } else {
                assert_eq!(app.selected_paths.len(), 2);
                assert_eq!(
                    app.changes[app.selected_change]
                        .diff
                        .contains("Staged changes"),
                    staged
                );
            }
            assert_eq!(app.commit_message, "Keep draft");
            assert_eq!(app.change_filter, super::super::ChangeFilter::Modified);
            assert!(app.refs_current);
            assert!(!app.refs_pending && !app.refs_loading);
            assert!(!app.history_pending && !app.history_loading);
            assert_eq!(app.history_session.generation, generation);
            assert_eq!(app.loaded_commit.as_deref(), Some("commit-to-keep"));
            assert!(app.history_worker.is_some() && app.refs_worker.is_some());
            assert!(!cancel.load(std::sync::atomic::Ordering::Relaxed));
        }
        drop((history_sender, refs_sender));
    }

    #[test]
    fn index_refresh_errors_preserve_preview_draft_and_retry_state() {
        let repo = TempRepo::new();
        repo.write("file", "contents\n");
        let mut app = opened(&repo);
        refresh(&mut app);
        app.commit_message = "Keep draft".into();
        let preview = app.diff_preview.as_ref().unwrap().lines.as_ptr();
        let path = app.repositories[app.selected_repository].path.clone();
        app.apply_index_update(
            Update {
                path: path.clone(),
                snapshot: Err("Status failed".into()),
                diff: None,
            },
            true,
            Err("Index locked".into()),
        );
        assert!(!app.state_current);
        assert_eq!(app.changes.len(), 1);
        assert!(app.error.as_ref().unwrap().contains("Index locked"));
        assert!(app.error.as_ref().unwrap().contains("Status failed"));
        assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);
        app.apply_index_update(
            Update {
                path,
                snapshot: git::load(&repo.0),
                diff: Some(FileDiff {
                    path: "file".into(),
                    result: Err("Diff failed".into()),
                }),
            },
            true,
            Ok(()),
        );
        assert!(app.state_current);
        assert!(app.error.as_ref().unwrap().contains("Diff failed"));
        assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);
        assert_eq!(app.commit_message, "Keep draft");
    }

    #[test]
    fn index_update_falls_back_to_full_refresh_after_external_branch_change() {
        let repo = TempRepo::new();
        let mut app = opened(&repo);
        app.refs_pending = false;
        app.history_pending = false;
        repo.git(&["symbolic-ref", "HEAD", "refs/heads/other"]);
        app.apply_index_update(
            Update::read(app.repositories[0].path.clone(), None),
            true,
            Ok(()),
        );
        assert_eq!(app.current_branch, "other");
        assert!(app.refs_pending && app.history_pending);
    }

    fn refresh(app: &mut SannyGitApp) {
        app.working_tree.next_read = Instant::now();
        app.check_working_tree(&eframe::egui::Context::default());
        assert!(!app.busy(), "Automatic reads must not disable controls");
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.working_tree.receiver.is_some() {
            assert!(Instant::now() < deadline, "Automatic refresh timed out");
            app.poll_working_tree();
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn empty_index_restores_all_changes_after_external_commit_when_enabled() {
        for enabled in [false, true] {
            let repo = TempRepo::new();
            repo.write("file", "staged contents\n");
            repo.git(&["add", "--", "file"]);
            let mut app = opened(&repo);
            app.preferences.staged_only_after_staging = enabled;
            app.commit_contents = git::CommitContents::Staged;
            app.commit_message = "Keep draft".into();
            repo.git(&["commit", "-m", "external commit"]);
            refresh(&mut app);
            assert!(app.changes.is_empty());
            assert_eq!(
                app.commit_contents,
                if enabled {
                    git::CommitContents::All
                } else {
                    git::CommitContents::Staged
                }
            );
            assert_eq!(app.commit_message, "Keep draft");
        }
    }
    #[test]
    fn explicit_refresh_preserves_preview_and_draft_and_reloads_other_views() {
        let repo = TempRepo::new();
        repo.write("file", "contents");
        let mut app = opened(&repo);
        refresh(&mut app);
        app.commit_message = "Keep draft".into();
        let preview = app.diff_preview.as_ref().unwrap().lines.as_ptr();
        app.refs_pending = false;
        app.history_pending = false;
        app.refresh();
        assert!(!app.busy());
        assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);
        refresh(&mut app);
        assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);
        assert_eq!(app.commit_message, "Keep draft");
        assert!(app.refs_pending && app.history_pending);

        app.state_current = false;
        app.refresh();
        refresh(&mut app);
        assert!(
            app.state_current,
            "Explicit refresh must recover stale repository state"
        );
    }

    #[test]
    fn disk_changes_update_rows_index_and_selected_diff_without_resetting_ui() {
        let repo = TempRepo::new();
        repo.write("tracked", "base\n");
        repo.git(&["add", "tracked"]);
        repo.git(&["commit", "-m", "base"]);
        repo.write("tracked", "first\n");
        let mut app = opened(&repo);
        app.commit_message = "Draft to preserve".into();
        app.change_filter = super::super::ChangeFilter::Modified;
        refresh(&mut app);
        assert!(app.changes[app.selected_change].diff.contains("+first"));
        let preview = app.diff_preview.as_ref().unwrap().lines.as_ptr();
        refresh(&mut app);
        assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);

        // Same porcelain status, different contents: the preview must still refresh.
        repo.write("tracked", "second\n");
        repo.write("aaa", "new file\n");
        refresh(&mut app);
        assert_eq!(app.changes.len(), 2);
        assert_eq!(app.changes[app.selected_change].path, "tracked");
        assert!(app.changes[app.selected_change].diff.contains("+second"));
        assert_eq!(app.commit_message, "Draft to preserve");
        assert_eq!(app.change_filter, super::super::ChangeFilter::Modified);

        repo.git(&["add", "tracked"]);
        refresh(&mut app);
        assert!(app.changes[app.selected_change].staged);
        assert!(!app.changes[app.selected_change].unstaged);
        std::fs::remove_file(repo.0.join("tracked")).unwrap();
        refresh(&mut app);
        assert_eq!(app.changes[app.selected_change].kind, ChangeKind::Deleted);
        repo.git(&["restore", "--staged", "tracked"]);
        repo.git(&["restore", "tracked"]);
        std::fs::remove_file(repo.0.join("aaa")).unwrap();
        refresh(&mut app);
        assert!(app.changes.is_empty());
        assert_eq!(app.selected_change, usize::MAX);
        assert!(app.selected_diff().is_none());
        assert_eq!(app.status, "Working tree clean");
        // The next frame must not start another Git check immediately.
        app.check_working_tree(&eframe::egui::Context::default());
        assert!(app.working_tree.receiver.is_none());
    }

    fn deliver(app: &mut SannyGitApp, update: Update) {
        let (sender, receiver) = mpsc::channel();
        sender.send(update).unwrap();
        app.working_tree.receiver = Some(receiver);
        app.poll_working_tree();
    }

    #[test]
    fn automatic_errors_keep_rows_and_recover_without_clearing_other_errors() {
        let repo = TempRepo::new();
        repo.write("file", "contents");
        let mut app = opened(&repo);
        let path = app.repositories[0].path.clone();
        deliver(
            &mut app,
            Update {
                path: path.clone(),
                snapshot: Err("Cannot read repository".into()),
                diff: None,
            },
        );
        assert_eq!(app.changes.len(), 1);
        assert!(app
            .error
            .as_deref()
            .unwrap()
            .contains("Automatic refresh failed"));
        refresh(&mut app);
        assert!(app.error.is_none());
        app.error = Some("Explicit operation failed".into());
        deliver(
            &mut app,
            Update {
                path,
                snapshot: Err("Read failed".into()),
                diff: None,
            },
        );
        refresh(&mut app);
        assert_eq!(app.error.as_deref(), Some("Explicit operation failed"));
    }

    #[test]
    fn operations_and_repository_switches_discard_stale_background_results() {
        let repo = TempRepo::new();
        repo.write("file", "contents");
        let mut app = opened(&repo);
        let path = app.repositories[0].path.clone();
        let (sender, receiver) = mpsc::channel();
        app.working_tree.receiver = Some(receiver);
        app.open_repository(path.clone());
        assert!(sender.send(Update::read(path.clone(), None)).is_err());
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.busy() {
            assert!(Instant::now() < deadline);
            app.poll_worker();
            std::thread::sleep(Duration::from_millis(10));
        }
        app.selected_repository = usize::MAX;
        deliver(&mut app, Update::read(path, None));
        assert_eq!(app.selected_repository, usize::MAX);
        assert_eq!(app.changes.len(), 1);
    }
}
