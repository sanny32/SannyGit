use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

enum WorkerResult {
    Repository(Result<crate::git::Snapshot, String>),
    Operation {
        committed: bool,
        outcome: Result<(), String>,
        snapshot: Result<crate::git::Snapshot, String>,
    },
    Diff {
        index: usize,
        result: Result<(String, crate::models::DiffPreview), String>,
    },
}

use crate::{
    git,
    models::{ChangeFile, Commit, Repository},
    theme::{ThemeChoice, THEME_STORAGE_KEY},
    ui,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeFilter {
    All,
    Modified,
    Added,
    Deleted,
}

pub struct SannyGitApp {
    pub theme: ThemeChoice,
    applied_theme: ThemeChoice,
    pub repositories: Vec<Repository>,
    pub local_branches: Vec<&'static str>,
    pub remote_branches: Vec<&'static str>,
    pub tags: Vec<&'static str>,
    pub changes: Vec<ChangeFile>,
    pub commits: Vec<Commit>,
    pub selected_repository: usize,
    pub selected_change: usize,
    pub selected_branch: String,
    pub branch_filter: String,
    pub change_filter: ChangeFilter,
    pub journal_filter: String,
    pub current_branch: String,
    pub commit_message: String,
    pub amend: bool,
    pub repository_path: String,
    pub open_dialog: bool,
    pub error: Option<String>,
    pub status: String,
    worker: Option<Receiver<WorkerResult>>,
    loaded_diff: Option<usize>,
    pub diff_preview: Option<crate::models::DiffPreview>,
    pub state_current: bool,
}

impl SannyGitApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install_fonts(&creation_context.egui_ctx);
        egui_extras::install_image_loaders(&creation_context.egui_ctx);
        let system_theme = if creation_context.egui_ctx.global_style().visuals.dark_mode {
            ThemeChoice::Dark
        } else {
            ThemeChoice::Light
        };
        let theme = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, THEME_STORAGE_KEY))
            .unwrap_or(system_theme);
        theme.apply(&creation_context.egui_ctx);

        let mut app = Self::with_theme(theme);
        if let Some(paths) = creation_context
            .storage
            .and_then(|storage| eframe::get_value::<Vec<String>>(storage, "sannygit.repositories"))
        {
            app.repositories = paths
                .into_iter()
                .map(|path| Repository {
                    name: PathBuf::from(&path)
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.clone()),
                    path,
                })
                .collect();
        }
        app
    }

    fn with_theme(theme: ThemeChoice) -> Self {
        Self {
            theme,
            applied_theme: theme,
            repositories: Vec::new(),
            local_branches: Vec::new(),
            remote_branches: Vec::new(),
            tags: Vec::new(),
            changes: Vec::new(),
            commits: Vec::new(),
            selected_repository: usize::MAX,
            selected_change: usize::MAX,
            selected_branch: String::new(),
            branch_filter: String::new(),
            change_filter: ChangeFilter::All,
            journal_filter: String::new(),
            current_branch: String::new(),
            commit_message: String::new(),
            amend: false,
            repository_path: String::new(),
            open_dialog: false,
            error: None,
            status: "Open a repository to get started".into(),
            worker: None,
            loaded_diff: None,
            diff_preview: None,
            state_current: false,
        }
    }
    pub fn selected_diff(&self) -> Option<&crate::models::DiffPreview> {
        (self.loaded_diff == Some(self.selected_change))
            .then_some(self.diff_preview.as_ref())
            .flatten()
    }

    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }

    fn start_worker(&mut self, job: impl FnOnce() -> WorkerResult + Send + 'static) {
        let (sender, receiver) = mpsc::channel();
        match std::thread::Builder::new()
            .name("sannygit-reader".into())
            .spawn(move || {
                let _ = sender.send(job());
            }) {
            Ok(_) => {
                self.worker = Some(receiver);
            }
            Err(e) => {
                self.error = Some(format!("Cannot start background worker: {e}"));
                self.status = "Operation failed".into();
            }
        }
    }

    pub fn open_repository(&mut self, path: String) {
        if self.busy() {
            return;
        }
        self.error = None;
        self.state_current = false;
        self.status = "Loading repository…".into();
        self.start_worker(move || WorkerResult::Repository(git::load(&PathBuf::from(path))));
    }

    pub fn refresh(&mut self) {
        if let Some(repository) = self.repositories.get(self.selected_repository) {
            self.open_repository(repository.path.clone());
        }
    }

    fn apply_snapshot(&mut self, snapshot: git::Snapshot) {
        self.state_current = true;
        let selected = self
            .changes
            .get(self.selected_change)
            .map(|c| c.path.clone());
        let index = self
            .repositories
            .iter()
            .position(|r| r.path == snapshot.repository.path)
            .unwrap_or_else(|| {
                self.repositories.push(snapshot.repository);
                self.repositories.len() - 1
            });
        let same_repository = self.selected_repository == index;
        self.selected_repository = index;
        self.current_branch = snapshot.branch;
        self.changes = snapshot.changes;
        self.selected_change = if same_repository {
            selected.and_then(|path| self.changes.iter().position(|c| c.path == path))
        } else {
            None
        }
        .or_else(|| (!self.changes.is_empty()).then_some(0))
        .unwrap_or(usize::MAX);
        self.loaded_diff = None;
        self.diff_preview = None;
        self.open_dialog = false;
        self.local_branches.clear();
        self.remote_branches.clear();
        self.tags.clear();
        self.commits.clear();
        self.selected_branch.clear();
        if !same_repository {
            self.commit_message.clear();
            self.branch_filter.clear();
            self.journal_filter.clear();
        }
        self.status = if self.changes.is_empty() {
            "Working tree clean"
        } else {
            "Ready"
        }
        .into();
    }

    pub fn staging_allowed(&self, index: usize, staged: bool) -> bool {
        !self.busy()
            && self.state_current
            && self.repositories.get(self.selected_repository).is_some()
            && self.changes.get(index).is_some_and(|c| {
                c.kind != crate::models::ChangeKind::Conflicted
                    && if staged { c.unstaged } else { c.staged }
            })
    }

    pub fn set_staged(&mut self, index: usize, staged: bool) {
        if !self.staging_allowed(index, staged) {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let path = self.changes[index].path.clone();
        self.state_current = false;
        self.error = None;
        self.status = if staged {
            "Staging file…"
        } else {
            "Unstaging file…"
        }
        .into();
        self.start_worker(move || {
            let outcome = git::set_staged(&root, &path, staged);
            let snapshot = git::load(&root);
            WorkerResult::Operation {
                committed: false,
                outcome,
                snapshot,
            }
        });
    }

    pub fn commit_blocker(&self) -> Option<&'static str> {
        if self.busy() {
            Some("Wait for the current operation to finish")
        } else if self.repositories.get(self.selected_repository).is_none() {
            Some("Open a repository first")
        } else if !self.state_current {
            Some("Refresh the repository before committing")
        } else if self
            .changes
            .iter()
            .any(|c| c.kind == crate::models::ChangeKind::Conflicted)
        {
            Some("Resolve all conflicts before committing")
        } else if !self.changes.iter().any(|c| c.staged) {
            Some("Stage at least one change")
        } else if self.commit_message.trim().is_empty() {
            Some("Enter a commit message")
        } else if self.commit_message.contains('\0') || self.commit_message.len() > 65_536 {
            Some("Use a message without NUL characters and no larger than 64 KiB")
        } else {
            None
        }
    }

    pub fn create_commit(&mut self) {
        if self.commit_blocker().is_some() {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let message = self.commit_message.clone();
        self.state_current = false;
        self.error = None;
        self.status = "Creating commit…".into();
        self.start_worker(move || {
            let outcome = git::commit(&root, &message);
            let committed = outcome.is_ok();
            let snapshot = git::load(&root);
            WorkerResult::Operation {
                committed,
                outcome,
                snapshot,
            }
        });
    }

    fn poll_worker(&mut self) {
        let Some(receiver) = &self.worker else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.worker = None;
                self.error = Some("Background worker stopped unexpectedly".into());
                self.status = "Operation failed".into();
                return;
            }
        };
        self.worker = None;
        match result {
            WorkerResult::Repository(Ok(snapshot)) => {
                self.apply_snapshot(snapshot);
            }
            WorkerResult::Repository(Err(error)) => {
                self.error = Some(error);
                self.status = "Could not load repository; previous view retained".into();
            }
            WorkerResult::Operation {
                committed,
                outcome,
                snapshot,
            } => {
                if committed {
                    self.commit_message.clear();
                }
                let refreshed = snapshot.is_ok();
                match snapshot {
                    Ok(snapshot) => self.apply_snapshot(snapshot),
                    Err(error) => {
                        self.state_current = false;
                        self.error =
                            Some(format!("Could not refresh after the operation: {error}"));
                        self.status = "Refresh required before further operations".into();
                    }
                }
                match outcome {
                    Ok(()) => {
                        if refreshed {
                            self.status = if committed {
                                "Commit created"
                            } else {
                                "Index updated"
                            }
                            .into();
                        }
                    }
                    Err(error) => {
                        self.error = Some(error);
                        self.status =
                            "Operation failed; check the refreshed repository state".into();
                    }
                }
            }
            WorkerResult::Diff { index, result } => {
                if self.selected_change == index {
                    self.loaded_diff = Some(index);
                    match result {
                        Ok((diff, preview)) => {
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
        }
    }

    fn load_selected_diff(&mut self) {
        if self.busy() || self.loaded_diff == Some(self.selected_change) {
            return;
        }
        let Some(change) = self.changes.get(self.selected_change) else {
            return;
        };
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let root = PathBuf::from(&repository.path);
        let path = change.path.clone();
        let kind = change.kind;
        let original_path = change.original_path.clone();
        let index = self.selected_change;
        self.diff_preview = None;
        if let Some(previous) = self.loaded_diff.and_then(|i| self.changes.get_mut(i)) {
            previous.diff = String::new();
        }
        self.loaded_diff = Some(index);
        if self.error.is_none()
            && !matches!(self.status.as_str(), "Commit created" | "Index updated")
        {
            self.status = "Loading diff…".into();
        }
        self.start_worker(move || WorkerResult::Diff {
            index,
            result: git::diff(&root, &path, kind, original_path.as_deref()).map(|patch| {
                let preview = crate::models::DiffPreview::new(&patch);
                (patch, preview)
            }),
        });
    }
}

impl eframe::App for SannyGitApp {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        if self.applied_theme != self.theme {
            self.theme.apply(ui.ctx());
            self.applied_theme = self.theme;
        }
        self.poll_worker();
        ui::show(ui, self);
        self.load_selected_diff();
        if self.busy() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, THEME_STORAGE_KEY, &self.theme);
        let paths: Vec<_> = self.repositories.iter().map(|r| &r.path).collect();
        eframe::set_value(storage, "sannygit.repositories", &paths);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{self, epaint::ClippedShape, Rect, Vec2};

    fn deliver(app: &mut SannyGitApp, result: WorkerResult) {
        let (sender, receiver) = mpsc::channel();
        app.worker = Some(receiver);
        sender.send(result).ok().unwrap();
        app.poll_worker();
    }

    #[test]
    fn repository_error_retains_view_and_refresh_preserves_selection() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories = crate::mock_data::repositories();
        app.changes = crate::mock_data::changes();
        app.selected_repository = 0;
        app.selected_change = 2;
        let selected = app.changes[2].path.clone();
        deliver(
            &mut app,
            WorkerResult::Repository(Err("Cannot read repository".into())),
        );
        assert_eq!(app.changes[app.selected_change].path, selected);
        assert!(app.error.is_some());
        let mut changes = crate::mock_data::changes();
        changes.reverse();
        let snapshot = git::Snapshot {
            repository: Repository {
                name: app.repositories[0].name.clone(),
                path: app.repositories[0].path.clone(),
            },
            branch: "real-branch".into(),
            changes,
        };
        deliver(&mut app, WorkerResult::Repository(Ok(snapshot)));
        assert_eq!(app.changes[app.selected_change].path, selected);
        assert_eq!(app.current_branch, "real-branch");
        assert!(app.commits.is_empty());
    }

    #[test]
    fn changing_selection_hides_previous_diff_and_discards_late_result() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
        app.changes = crate::mock_data::changes();
        app.selected_change = 0;
        app.loaded_diff = Some(0);
        app.diff_preview = Some(crate::models::DiffPreview::new("+old"));
        assert!(app.selected_diff().is_some());
        app.selected_change = 1;
        assert!(app.selected_diff().is_none());
        deliver(
            &mut app,
            WorkerResult::Diff {
                index: 0,
                result: Err("stale error".into()),
            },
        );
        assert!(app.error.is_none());
        assert!(app.selected_diff().is_none());
    }

    #[test]
    fn commit_requires_fresh_state_message_staged_files_and_no_conflicts() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        assert!(app.commit_blocker().is_some());
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        app.changes = crate::mock_data::changes();
        app.commit_message = "Message".into();
        assert!(app.commit_blocker().unwrap().contains("Refresh"));
        app.state_current = true;
        for change in &mut app.changes {
            change.staged = false;
        }
        assert!(app.commit_blocker().unwrap().contains("Stage"));
        app.changes[0].staged = true;
        app.commit_message = " \n\t".into();
        assert!(app.commit_blocker().unwrap().contains("message"));
        app.create_commit();
        assert!(!app.busy());
        app.commit_message = "Valid message".into();
        assert!(app.commit_blocker().is_none());
        assert!(app.staging_allowed(0, false));
        app.changes[0].kind = crate::models::ChangeKind::Conflicted;
        assert!(app.commit_blocker().unwrap().contains("conflicts"));
        assert!(!app.staging_allowed(0, true));
        assert!(!app.staging_allowed(0, false));
    }

    #[test]
    fn operation_errors_keep_draft_and_success_clears_it_even_if_refresh_fails() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.commit_message = "Keep draft".into();
        app.state_current = false;
        deliver(
            &mut app,
            WorkerResult::Operation {
                committed: false,
                outcome: Err("Hook rejected commit".into()),
                snapshot: Err("Refresh failed".into()),
            },
        );
        assert_eq!(app.commit_message, "Keep draft");
        assert!(app.error.as_deref().unwrap().contains("Hook"));
        assert!(!app.state_current);
        deliver(
            &mut app,
            WorkerResult::Operation {
                committed: true,
                outcome: Ok(()),
                snapshot: Err("Refresh failed".into()),
            },
        );
        assert!(app.commit_message.is_empty());
        assert!(app.error.as_deref().unwrap().contains("refresh"));
        assert!(!app.state_current);
        assert!(app.commit_blocker().is_some());
    }

    fn visible_text_count(shapes: &[ClippedShape], expected: &str) -> usize {
        shapes
            .iter()
            .filter(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    text.galley.job.text == expected
                        && shape
                            .clip_rect
                            .intersect(text.galley.rect.translate(text.pos.to_vec2()))
                            .height()
                            > 8.0
                } else {
                    false
                }
            })
            .count()
    }

    fn visible_text(shapes: &[ClippedShape], expected: &str) -> bool {
        visible_text_count(shapes, expected) > 0
    }

    #[test]
    fn empty_state_and_open_dialog_fit_in_both_themes() {
        for size in [Vec2::new(1100.0, 700.0), Vec2::new(1536.0, 1024.0)] {
            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                let context = egui::Context::default();
                crate::theme::install_fonts(&context);
                egui_extras::install_image_loaders(&context);
                theme.apply(&context);
                let mut app = SannyGitApp::with_theme(theme);
                app.open_dialog = true;
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                let mut output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                output.textures_delta.clear();
                for _ in 0..2 {
                    output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                    output.textures_delta.clear();
                }
                for text in [
                    "Open",
                    "Refresh",
                    "Settings",
                    "Open repository",
                    "Use + to open a local repository",
                ] {
                    assert!(
                        visible_text(&output.shapes, text),
                        "{text} clipped at {size:?} in {theme:?}"
                    );
                }
                assert!(app.repositories.is_empty());
                assert!(app.worker.is_none());
            }
        }
    }

    #[test]
    fn all_changed_files_fit_at_reference_and_small_window_sizes() {
        for size in [
            Vec2::new(1536.0, 1024.0),
            Vec2::new(1244.0, 720.0),
            Vec2::new(1100.0, 700.0),
        ] {
            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                let context = egui::Context::default();
                crate::theme::install_fonts(&context);
                egui_extras::install_image_loaders(&context);
                theme.apply(&context);
                let mut app = SannyGitApp::with_theme(theme);
                app.repositories = crate::mock_data::repositories();
                app.changes = crate::mock_data::changes();
                app.commits = crate::mock_data::commits();
                app.local_branches = crate::mock_data::local_branches();
                app.remote_branches = crate::mock_data::remote_branches();
                app.tags = crate::mock_data::tags();
                app.selected_repository = 0;
                app.state_current = true;
                app.commit_message = "Layout check".into();
                app.selected_change = 2;
                app.loaded_diff = Some(2);
                app.diff_preview = Some(crate::models::DiffPreview::new(&app.changes[2].diff));
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                let mut output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                // Headless layout checks do not upload textures to a renderer.
                output.textures_delta.clear();
                for _ in 0..2 {
                    output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                    output.textures_delta.clear();
                }
                for file in &app.changes {
                    assert!(
                        visible_text(&output.shapes, &file.path),
                        "File {} is clipped at {size:?} in {theme:?}",
                        file.path
                    );
                }
                assert!(
                    visible_text_count(&output.shapes, "Commit") >= 2,
                    "Commit control is clipped at {size:?}"
                );
                assert!(
                    visible_text(&output.shapes, "Initial commit"),
                    "History is clipped at {size:?}"
                );
            }
        }
    }
}
