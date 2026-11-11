//! Periodic, read-only working-tree checks, independent of interactive operations.
use super::SannyGitApp;
use crate::{git, models::DiffPreview};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

const CHECK_INTERVAL: Duration = Duration::from_secs(2);

struct Update {
    path: String,
    snapshot: Result<git::Snapshot, String>,
    diff: Option<FileDiff>,
}

struct FileDiff {
    path: String,
    result: Result<(String, DiffPreview), String>,
}

impl Update {
    fn read(path: String, selected: Option<String>) -> Self {
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
    error: Option<String>,
}

impl Default for Monitor {
    fn default() -> Self {
        Self {
            receiver: None,
            next_read: Instant::now() + CHECK_INTERVAL,
            error: None,
        }
    }
}

impl Monitor {
    pub(super) fn invalidate(&mut self) {
        self.receiver = None;
        self.next_read = Instant::now() + CHECK_INTERVAL;
    }
}

impl SannyGitApp {
    pub(super) fn check_working_tree(&mut self, context: &eframe::egui::Context) {
        if self.busy()
            || !self.state_current
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
            Err(error) => self.working_tree_error(format!("Cannot check working tree: {error}")),
        }
    }

    fn working_tree_error(&mut self, error: String) {
        let error = format!("Automatic refresh failed; previous view retained: {error}");
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
                self.working_tree.invalidate();
                self.working_tree_error("Background worker stopped unexpectedly".into());
                return;
            }
        };
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
                self.working_tree_error(error);
                return;
            }
        };
        if self.working_tree.error.is_some() && self.error == self.working_tree.error {
            self.error = None;
        }
        self.working_tree.error = None;
        if snapshot.branch != self.current_branch {
            self.apply_snapshot(snapshot);
            return;
        }
        let selected = self
            .changes
            .get(self.selected_change)
            .map(|c| c.path.clone());
        let previous_diff = self.loaded_diff == Some(self.selected_change);
        let retained_diff = self
            .changes
            .get_mut(self.selected_change)
            .map(|change| std::mem::take(&mut change.diff))
            .unwrap_or_default();
        let index = selected
            .as_ref()
            .and_then(|path| snapshot.changes.iter().position(|c| &c.path == path));
        self.changes = snapshot.changes;
        self.selected_change = index
            .or_else(|| (!self.changes.is_empty()).then_some(0))
            .unwrap_or(usize::MAX);
        if index.is_some() && previous_diff {
            self.loaded_diff = Some(self.selected_change);
            self.changes[self.selected_change].diff = retained_diff;
        } else {
            self.loaded_diff = None;
            self.diff_preview = None;
        }
        if let Some(diff) = update.diff {
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
                    }
                    Err(error) => self.working_tree_error(error),
                }
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
