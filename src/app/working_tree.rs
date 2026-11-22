//! Periodic, read-only working-tree checks, independent of interactive operations.
use super::SannyGitApp;
use crate::git::GitContext;
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
    ignored: Vec<crate::models::ChangeFile>,
    /// Full ref identity when this read sampled refs. `None` skips that comparison.
    refs: Option<Vec<u8>>,
}

struct FileDiff {
    path: String,
    result: Result<(String, DiffPreview), String>,
}

impl Update {
    pub(super) fn read(
        git: &GitContext,
        path: String,
        selected: Option<String>,
        ignore_whitespace: bool,
        show_ignored: bool,
        sample_refs: bool,
    ) -> Self {
        let snapshot = git::load(git, Path::new(&path));
        let ignored = if show_ignored && snapshot.is_ok() {
            git::ignored_changes(git, Path::new(&path))
        } else {
            Ok(Vec::new())
        };
        let (snapshot, ignored) = match (snapshot, ignored) {
            (Ok(snapshot), Ok(ignored)) => (Ok(snapshot), ignored),
            (Err(error), _) | (Ok(_), Err(error)) => (Err(error), Vec::new()),
        };
        let diff = snapshot.as_ref().ok().and_then(|snapshot| {
            let change = snapshot
                .changes
                .iter()
                .find(|change| Some(&change.path) == selected.as_ref())
                .or_else(|| {
                    ignored
                        .iter()
                        .find(|change| Some(&change.path) == selected.as_ref())
                })?;
            Some(FileDiff {
                path: change.path.clone(),
                result: git::file_diff(
                    git,
                    Path::new(&path),
                    &change.path,
                    change.kind,
                    change.original_path.as_deref(),
                    ignore_whitespace,
                )
                .map(|patch| {
                    let preview = DiffPreview::new(&patch);
                    (patch, preview)
                }),
            })
        });
        let refs = match &snapshot {
            Ok(_) if sample_refs => crate::history::ref_identity(git, Path::new(&path)).ok(),
            _ => None,
        };
        Self {
            path,
            snapshot,
            diff,
            ignored,
            refs,
        }
    }
}

pub(super) struct Monitor {
    receiver: Option<Receiver<Update>>,
    next_read: Instant,
    refresh_requested: bool,
    /// Last full ref identity. Not cleared when a read is cancelled.
    pub(super) refs_token: Option<Vec<u8>>,
    pub(super) error: Option<String>,
}

impl Default for Monitor {
    fn default() -> Self {
        Self {
            receiver: None,
            next_read: Instant::now() + CHECK_INTERVAL,
            refresh_requested: false,
            refs_token: None,
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

    pub(super) fn read_soon(&mut self) {
        self.next_read = Instant::now();
    }
}

impl SannyGitApp {
    pub(super) fn apply_selected_diff(
        &mut self,
        index: usize,
        result: Result<(String, DiffPreview), String>,
    ) {
        if self.selection.active == index {
            self.loaded_diff = Some(index);
            match result {
                Ok((diff, preview)) => {
                    let changed = self
                        .changes
                        .get(index)
                        .is_some_and(|change| change.diff != diff);
                    if changed {
                        self.diff_line_selection.clear();
                    }
                    self.diff_preview = Some(preview);
                    if let Some(change) = self.changes.get_mut(index) {
                        change.diff = diff;
                    }
                    if self.status == "Loading diff…" {
                        self.status = "Ready".into();
                    }
                }
                Err(error) => {
                    self.error = Some(error);
                    self.status = "Could not load diff".into();
                }
            }
        }
    }

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
            .get(self.selection.active)
            .map(|c| c.path.clone());
        let context = context.clone();
        let (sender, receiver) = mpsc::channel();
        self.working_tree.next_read = Instant::now() + CHECK_INTERVAL;
        let git_context = crate::git::GitContext::new(self.preferences.git_executable.clone());
        let ignore_whitespace = self.preferences.diff_ignore_whitespace;
        let show_ignored = self.preferences.show_ignored_files;
        match std::thread::Builder::new()
            .name("sannygit-working-tree".into())
            .spawn(move || {
                let git = &git_context;
                let _ = sender.send(Update::read(
                    git,
                    path,
                    selected,
                    ignore_whitespace,
                    show_ignored,
                    true,
                ));
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
        let show_ignored = self.preferences.show_ignored_files;
        let Update {
            snapshot,
            diff,
            ignored,
            refs,
            ..
        } = update;
        let snapshot = match snapshot {
            Ok(mut snapshot) => {
                attach_ignored(&mut snapshot, ignored, show_ignored);
                snapshot
            }
            Err(error) => {
                self.working_tree_error(error, refresh_requested);
                return;
            }
        };
        if self.working_tree.error.is_some() && self.error == self.working_tree.error {
            self.error = None;
        }
        self.working_tree.error = None;
        let head_moved = self.apply_working_tree_snapshot(snapshot);
        if let Some(diff) = diff {
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
        self.sync_external_history(head_moved, refresh_requested, refs);
    }

    pub(super) fn apply_index_update(
        &mut self,
        update: Update,
        staged: bool,
        outcome: Result<(), String>,
    ) {
        let show_ignored = self.preferences.show_ignored_files;
        let Update {
            path,
            snapshot,
            diff,
            ignored,
            refs,
        } = update;
        if !self
            .repositories
            .get(self.selected_repository)
            .is_some_and(|r| crate::repository::same_path(&r.path, &path))
        {
            return;
        }
        self.working_tree.invalidate();
        let snapshot = snapshot.map(|mut snapshot| {
            attach_ignored(&mut snapshot, ignored, show_ignored);
            snapshot
        });
        let success = if staged {
            "File staged"
        } else {
            "Index updated"
        };
        if outcome.is_ok() && staged && self.preferences.staged_only_after_staging {
            self.commit.contents = git::CommitContents::Staged;
        }
        let refresh_error = match snapshot {
            Ok(snapshot) => {
                let head_moved = self.apply_working_tree_snapshot(snapshot);
                self.sync_external_history(head_moved, false, refs);
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
        } else if let Some(diff) = diff {
            if let Err(error) = self.apply_working_tree_diff(diff) {
                let refresh = format!("Could not refresh diff: {error}");
                self.error = Some(match self.error.take() {
                    Some(error) => format!("{error}\n{refresh}"),
                    None => refresh,
                });
            }
        }
    }

    /// Returns whether HEAD moved while the branch name stayed the same.
    /// A branch or repository change already reloads history.
    fn apply_working_tree_snapshot(&mut self, snapshot: git::Snapshot) -> bool {
        if snapshot.branch != self.branches.current_branch
            || !self
                .repositories
                .get(self.selected_repository)
                .is_some_and(|r| crate::repository::same_path(&r.path, &snapshot.repository.path))
        {
            self.apply_snapshot(snapshot);
            return false;
        }
        let head_moved = snapshot.head != self.branches.head_commit;
        self.branches.head_commit.clone_from(&snapshot.head);
        self.state_current = true;
        let selected = self
            .changes
            .get(self.selection.active)
            .map(|c| c.path.clone());
        let previous_diff =
            self.loaded_diff == Some(self.selection.active) && self.diff_preview.is_some();
        let retained_diff = self
            .changes
            .get_mut(self.selection.active)
            .map(|change| std::mem::take(&mut change.diff))
            .unwrap_or_default();
        let index = selected
            .as_ref()
            .and_then(|path| snapshot.changes.iter().position(|c| &c.path == path));
        self.update_merge_message(snapshot.merging, snapshot.merge_message, true);
        self.changes = snapshot.changes;
        self.update_commit_contents_after_refresh(false);
        // Row indices and file contents may have changed during the read.
        self.diff_refresh_pending |= self.diff_worker.take().is_some() && previous_diff;
        self.selection.active = index
            .or_else(|| (!self.changes.is_empty()).then_some(0))
            .unwrap_or(usize::MAX);
        self.retain_change_selection();
        if index.is_some()
            && previous_diff
            && self
                .changes
                .get(self.selection.active)
                .is_some_and(|c| Some(&c.path) == selected.as_ref())
        {
            self.loaded_diff = Some(self.selection.active);
            self.changes[self.selection.active].diff = retained_diff;
        } else {
            self.loaded_diff = None;
            self.diff_preview = None;
            self.diff_refresh_pending = false;
        }
        head_moved
    }

    /// Reload the journal when another program moves HEAD or any ref.
    /// A matching branch name is not enough: `git commit` keeps `refs/heads/main`.
    fn sync_external_history(&mut self, head_moved: bool, explicit: bool, refs: Option<Vec<u8>>) {
        if head_moved {
            self.tracking.invalidate();
        }
        let mut reload_history = explicit || head_moved;
        let mut reload_branches = explicit || head_moved;
        let refs = if self.history.history_loading || self.history.history_pending {
            None
        } else {
            refs
        };
        if let Some(refs) = refs {
            if self.working_tree.refs_token.as_deref() != Some(refs.as_slice()) {
                // The first sample after opening a repository matches the history
                // already loaded. Later changes are commits made outside this window.
                let adopt = !explicit
                    && !head_moved
                    && self.working_tree.refs_token.is_none()
                    && (!self.history.history_all_branches || self.history_refs_match(&refs));
                self.working_tree.refs_token = Some(refs);
                if !adopt {
                    reload_branches = true;
                    if head_moved || self.history.history_all_branches {
                        reload_history = true;
                    }
                }
            }
        }
        if reload_branches {
            self.branches.pending = true;
            self.branches.loading = true;
            self.branches.error = None;
        }
        if reload_history {
            self.queue_history_reload();
        }
    }

    fn apply_working_tree_diff(&mut self, diff: FileDiff) -> Result<(), String> {
        let active = self.selection.active;
        if !self
            .changes
            .get(active)
            .is_some_and(|change| change.path == diff.path)
        {
            return Ok(());
        }
        match diff.result {
            Ok((patch, preview)) => {
                if self.changes[active].diff != patch {
                    self.diff_line_selection.clear();
                }
                if self.changes[active].diff != patch || self.diff_preview.is_none() {
                    self.diff_preview = Some(preview);
                }
                self.changes[active].diff = patch;
                self.loaded_diff = Some(active);
                self.diff_refresh_pending = false;
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }

    pub fn set_show_ignored_files(&mut self, show: bool) {
        if self.preferences.show_ignored_files == show {
            return;
        }
        self.preferences.show_ignored_files = show;
        if show {
            self.working_tree.read_soon();
            return;
        }
        let selected = self
            .changes
            .get(self.selection.active)
            .map(|change| change.path.clone());
        self.changes
            .retain(|change| change.kind != crate::models::ChangeKind::Ignored);
        self.selection.active = selected
            .as_ref()
            .and_then(|path| self.changes.iter().position(|change| &change.path == path))
            .or_else(|| (!self.changes.is_empty()).then_some(0))
            .unwrap_or(usize::MAX);
        self.retain_change_selection();
        if self
            .changes
            .get(self.selection.active)
            .map(|change| &change.path)
            != selected.as_ref()
        {
            self.loaded_diff = None;
            self.diff_preview = None;
            self.diff_line_selection.clear();
        }
    }
}

fn attach_ignored(
    snapshot: &mut git::Snapshot,
    ignored: Vec<crate::models::ChangeFile>,
    show: bool,
) {
    if !show {
        return;
    }
    let known: std::collections::BTreeSet<_> = snapshot
        .changes
        .iter()
        .map(|change| change.path.clone())
        .collect();
    for change in ignored {
        if !known.contains(&change.path) {
            snapshot.changes.push(change);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::SYSTEM_GIT;
    use crate::{models::ChangeKind, test_support::TempRepo, theme::ThemeChoice};

    fn opened(repo: &TempRepo) -> SannyGitApp {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(git::load(&SYSTEM_GIT, &repo.0).unwrap());
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
        app.commit.message = "Keep draft".into();
        app.change_filter = super::super::ChangeFilter::Modified;
        app.branches.current = true;
        app.branches.pending = false;
        app.branches.loading = false;
        app.history.history_pending = false;
        app.history.history_loading = false;
        app.history.loaded_commit = Some("commit-to-keep".into());
        let generation = app.history.history_session.generation;
        let (history_sender, history_receiver) = mpsc::channel();
        app.history.history_worker = Some(history_receiver);
        let (refs_sender, refs_receiver) = mpsc::channel();
        app.branches.worker = Some(refs_receiver);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        app.history.history_session.cancel = Some(cancel.clone());

        // Editing an unselected file must retain the parsed selected preview.
        // Group edits must refresh its staged/unstaged sections without clearing it.
        for (group, staged) in [(false, true), (false, false), (true, true), (true, false)] {
            let preview = app.diff_preview.as_ref().unwrap().lines.as_ptr();
            let patch = app.changes[app.selection.active].diff.clone();
            if group {
                app.selection.paths = ["a".into(), "b".into()].into_iter().collect();
                app.set_selected_staged(staged);
            } else {
                app.set_staged(1, staged);
            }
            assert!(app.busy());
            assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);
            assert_eq!(app.changes[app.selection.active].diff, patch);
            let deadline = Instant::now() + Duration::from_secs(10);
            while app.busy() {
                assert!(Instant::now() < deadline);
                app.poll_worker();
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(app.error.is_none(), "{:?}", app.error);
            assert_eq!(app.changes[app.selection.active].path, "a");
            assert_eq!(app.changes[app.selection.active].staged, group && staged);
            assert_eq!(app.loaded_diff, Some(app.selection.active));
            if !group {
                assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);
            } else {
                assert_eq!(app.selection.paths.len(), 2);
                assert_eq!(
                    app.changes[app.selection.active]
                        .diff
                        .contains("Staged changes"),
                    staged
                );
            }
            assert_eq!(app.commit.message, "Keep draft");
            assert_eq!(app.change_filter, super::super::ChangeFilter::Modified);
            assert!(app.branches.current);
            assert!(!app.branches.pending && !app.branches.loading);
            assert!(!app.history.history_pending && !app.history.history_loading);
            assert_eq!(app.history.history_session.generation, generation);
            assert_eq!(app.history.loaded_commit.as_deref(), Some("commit-to-keep"));
            assert!(app.history.history_worker.is_some() && app.branches.worker.is_some());
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
        app.commit.message = "Keep draft".into();
        let preview = app.diff_preview.as_ref().unwrap().lines.as_ptr();
        let path = app.repositories[app.selected_repository].path.clone();
        app.apply_index_update(
            Update {
                path: path.clone(),
                snapshot: Err("Status failed".into()),
                diff: None,
                ignored: Vec::new(),
                refs: None,
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
                snapshot: git::load(&SYSTEM_GIT, &repo.0),
                diff: Some(FileDiff {
                    path: "file".into(),
                    result: Err("Diff failed".into()),
                }),
                ignored: Vec::new(),
                refs: None,
            },
            true,
            Ok(()),
        );
        assert!(app.state_current);
        assert!(app.error.as_ref().unwrap().contains("Diff failed"));
        assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);
        assert_eq!(app.commit.message, "Keep draft");
    }

    #[test]
    fn index_update_falls_back_to_full_refresh_after_external_branch_change() {
        let repo = TempRepo::new();
        let mut app = opened(&repo);
        app.branches.pending = false;
        app.history.history_pending = false;
        repo.git(&["symbolic-ref", "HEAD", "refs/heads/other"]);
        app.apply_index_update(
            Update::read(
                &SYSTEM_GIT,
                app.repositories[0].path.clone(),
                None,
                false,
                false,
                false,
            ),
            true,
            Ok(()),
        );
        assert_eq!(app.branches.current_branch, "other");
        assert!(app.branches.pending && app.history.history_pending);
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
            app.commit.contents = git::CommitContents::Staged;
            app.commit.message = "Keep draft".into();
            repo.git(&["commit", "-m", "external commit"]);
            refresh(&mut app);
            assert!(app.changes.is_empty());
            assert_eq!(
                app.commit.contents,
                if enabled {
                    git::CommitContents::All
                } else {
                    git::CommitContents::Staged
                }
            );
            assert_eq!(app.commit.message, "Keep draft");
        }
    }
    #[test]
    fn explicit_refresh_preserves_preview_and_draft_and_reloads_other_views() {
        let repo = TempRepo::new();
        repo.write("file", "contents");
        let mut app = opened(&repo);
        refresh(&mut app);
        app.commit.message = "Keep draft".into();
        let preview = app.diff_preview.as_ref().unwrap().lines.as_ptr();
        app.branches.pending = false;
        app.history.history_pending = false;
        app.refresh();
        assert!(!app.busy());
        assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);
        refresh(&mut app);
        assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);
        assert_eq!(app.commit.message, "Keep draft");
        assert!(app.branches.pending && app.history.history_pending);

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
        app.commit.message = "Draft to preserve".into();
        app.change_filter = super::super::ChangeFilter::Modified;
        refresh(&mut app);
        assert!(app.changes[app.selection.active].diff.contains("+first"));
        let preview = app.diff_preview.as_ref().unwrap().lines.as_ptr();
        refresh(&mut app);
        assert_eq!(app.diff_preview.as_ref().unwrap().lines.as_ptr(), preview);

        // Same porcelain status, different contents: the preview must still refresh.
        repo.write("tracked", "second\n");
        repo.write("aaa", "new file\n");
        refresh(&mut app);
        assert_eq!(app.changes.len(), 2);
        assert_eq!(app.changes[app.selection.active].path, "tracked");
        assert!(app.changes[app.selection.active].diff.contains("+second"));
        assert_eq!(app.commit.message, "Draft to preserve");
        assert_eq!(app.change_filter, super::super::ChangeFilter::Modified);

        repo.git(&["add", "tracked"]);
        refresh(&mut app);
        assert!(app.changes[app.selection.active].staged);
        assert!(!app.changes[app.selection.active].unstaged);
        std::fs::remove_file(repo.0.join("tracked")).unwrap();
        refresh(&mut app);
        assert_eq!(app.changes[app.selection.active].kind, ChangeKind::Deleted);
        repo.git(&["restore", "--staged", "tracked"]);
        repo.git(&["restore", "tracked"]);
        std::fs::remove_file(repo.0.join("aaa")).unwrap();
        refresh(&mut app);
        assert!(app.changes.is_empty());
        assert_eq!(app.selection.active, usize::MAX);
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
                ignored: Vec::new(),
                refs: None,
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
                ignored: Vec::new(),
                refs: None,
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
        assert!(sender
            .send(Update::read(
                &SYSTEM_GIT,
                path.clone(),
                None,
                false,
                false,
                false,
            ))
            .is_err());
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.busy() {
            assert!(Instant::now() < deadline);
            app.poll_worker();
            std::thread::sleep(Duration::from_millis(10));
        }
        app.selected_repository = usize::MAX;
        deliver(
            &mut app,
            Update::read(&SYSTEM_GIT, path, None, false, false, false),
        );
        assert_eq!(app.selected_repository, usize::MAX);
        assert_eq!(app.changes.len(), 1);
    }

    fn idle_history_and_refs(app: &mut SannyGitApp) {
        app.history.history_pending = false;
        app.history.history_loading = false;
        app.branches.pending = false;
        app.branches.loading = false;
    }

    #[test]
    fn external_commit_on_the_same_branch_reloads_history() {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        repo.git(&["add", "file"]);
        repo.git(&["commit", "-m", "base"]);
        let mut app = opened(&repo);
        app.commit.message = "Keep draft".into();
        let head = app.branches.head_commit.clone();
        assert!(!head.is_empty());
        idle_history_and_refs(&mut app);
        repo.git(&["commit", "--allow-empty", "-m", "external"]);
        refresh(&mut app);
        assert_ne!(app.branches.head_commit, head);
        assert_eq!(app.branches.current_branch, "main");
        assert!(app.changes.is_empty());
        assert_eq!(app.commit.message, "Keep draft");
        assert!(app.history.history_pending && app.history.history_loading);
        assert!(app.branches.pending && app.branches.loading);
    }

    #[test]
    fn external_file_edits_do_not_reload_history() {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        repo.git(&["add", "file"]);
        repo.git(&["commit", "-m", "base"]);
        let mut app = opened(&repo);
        idle_history_and_refs(&mut app);
        let generation = app.history.history_session.generation;
        repo.write("file", "edited\n");
        refresh(&mut app);
        assert_eq!(app.changes.len(), 1);
        assert!(!app.history.history_pending && !app.history.history_loading);
        assert!(!app.branches.pending && !app.branches.loading);
        assert_eq!(app.history.history_session.generation, generation);
    }

    #[test]
    fn external_commit_on_another_branch_reloads_all_branch_history() {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        repo.git(&["add", "file"]);
        repo.git(&["commit", "-m", "base"]);
        let mut app = opened(&repo);
        idle_history_and_refs(&mut app);
        refresh(&mut app);
        assert!(!app.history.history_pending && !app.branches.pending);
        let head = app.branches.head_commit.clone();
        let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
        let parent = repo.git(&["rev-parse", "HEAD"]);
        let side = repo.git(&[
            "commit-tree",
            tree.trim(),
            "-p",
            parent.trim(),
            "-m",
            "side",
        ]);
        repo.git(&["update-ref", "refs/heads/side", side.trim()]);
        refresh(&mut app);
        assert_eq!(app.branches.head_commit, head);
        assert_eq!(app.branches.current_branch, "main");
        assert!(!app.history.history_pending && !app.history.history_loading);
        assert!(app.branches.pending && app.branches.loading);

        idle_history_and_refs(&mut app);
        app.history.history_all_branches = true;
        let again = repo.git(&[
            "commit-tree",
            tree.trim(),
            "-p",
            side.trim(),
            "-m",
            "side again",
        ]);
        repo.git(&["update-ref", "refs/heads/side", again.trim()]);
        refresh(&mut app);
        assert_eq!(app.branches.head_commit, head);
        assert!(app.history.history_pending && app.history.history_loading);
        assert!(app.branches.pending && app.branches.loading);
    }
}
