use super::{SannyGitApp, WorkerResult};
use crate::history as backend;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone)]
struct Anchor {
    hash: String,
    offset: f32,
    filter: String,
}

#[derive(Default)]
pub(super) struct Session {
    cursor: Option<backend::Cursor>,
    pub(super) cancel: Option<Arc<AtomicBool>>,
    pub(super) generation: u64,
    append: bool,
    notice: Option<String>,
    anchor: Option<Anchor>,
    restore: Option<Anchor>,
}

impl Session {
    pub(super) fn clear_view(&mut self) {
        let generation = self.generation.wrapping_add(1);
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        *self = Self {
            generation,
            ..Default::default()
        };
    }
}

impl SannyGitApp {
    pub(super) fn queue_history_reload(&mut self) {
        if let Some(cancel) = self.history_session.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
            self.history_worker = None;
        }
        self.history_session.generation = self.history_session.generation.wrapping_add(1);
        self.history_session.append = false;
        self.history_session.notice = None;
        self.history_pending = true;
        self.history_loading = true;
        self.history_error = None;
    }

    pub fn reload_history(&mut self) {
        if self.busy() || self.repositories.get(self.selected_repository).is_none() {
            return;
        }
        self.queue_history_reload();
    }

    pub fn history_can_load_more(&self) -> bool {
        !self.busy()
            && !self.history_loading
            && !self.history_pending
            && self.repositories.get(self.selected_repository).is_some()
            && self.history_session.cursor.as_ref().is_some_and(|cursor| {
                cursor.has_more()
                    && cursor.all_branches() == self.history_all_branches
                    && cursor.loaded() == self.commits.len()
            })
    }

    pub fn history_at_limit(&self) -> bool {
        self.history_session
            .cursor
            .as_ref()
            .is_some_and(backend::Cursor::at_limit)
    }

    pub fn history_cancellable(&self) -> bool {
        self.history_loading && (self.history_pending || self.history_session.cancel.is_some())
    }

    pub fn history_notice(&self) -> Option<&str> {
        self.history_session.notice.as_deref()
    }

    pub fn load_more_history(&mut self) {
        if !self.history_can_load_more() {
            return;
        }
        self.history_session.generation = self.history_session.generation.wrapping_add(1);
        self.history_session.append = true;
        self.history_session.notice = None;
        self.history_pending = true;
        self.history_loading = true;
        self.history_error = None;
    }

    pub fn cancel_history(&mut self) {
        if !self.history_cancellable() {
            return;
        }
        if let Some(cancel) = self.history_session.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
            self.history_worker = None;
        }
        self.history_error = None;
        self.history_session.generation = self.history_session.generation.wrapping_add(1);
        self.history_loading = false;
        self.history_pending = false;
        self.history_session.notice = Some("History loading cancelled; previous rows and selection are retained. Use More or Reload to retry.".into());
    }

    pub(super) fn load_history(&mut self) {
        if self.busy() || self.history_worker.is_some() || !self.history_pending {
            return;
        }
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let path = repository.path.clone();
        let all = self.history_all_branches;
        let generation = self.history_session.generation;
        let append = self.history_session.append;
        let cursor = self.history_session.cursor.clone();
        let previous = if append {
            self.commits.clone()
        } else {
            Vec::new()
        };
        let requested = self.commits.len().max(backend::PAGE_SIZE);
        let keep = self
            .selected_commit
            .iter()
            .cloned()
            .chain(
                self.history_session
                    .anchor
                    .iter()
                    .map(|anchor| anchor.hash.clone()),
            )
            .chain(self.commits.last().map(|commit| commit.hash.clone()))
            .collect::<Vec<_>>();
        let cancel = Arc::new(AtomicBool::new(false));
        self.history_session.cancel = Some(cancel.clone());
        self.history_pending = false;
        match self.spawn_reader(move || {
            let root = PathBuf::from(&path);
            let result = if append {
                cursor
                    .as_ref()
                    .ok_or_else(|| "Reload history before loading more.".to_owned())
                    .and_then(|cursor| backend::more(&root, cursor, previous, &cancel))
            } else {
                backend::reload(&root, all, requested, &keep, &cancel)
            };
            WorkerResult::History {
                path,
                all,
                generation,
                append,
                result,
            }
        }) {
            Ok(receiver) => self.history_worker = Some(receiver),
            Err(error) => {
                self.history_session.cancel = None;
                self.history_loading = false;
                self.history_error = Some(error);
            }
        }
    }

    pub(super) fn apply_history(
        &mut self,
        path: String,
        all: bool,
        generation: u64,
        append: bool,
        result: Result<backend::Page, String>,
    ) {
        if !self
            .repositories
            .get(self.selected_repository)
            .is_some_and(|repository| repository.path == path)
            || self.history_all_branches != all
            || self.history_session.generation != generation
        {
            return;
        }
        self.history_loading = false;
        self.history_session.cancel = None;
        match result {
            Ok(page) => {
                if !append {
                    self.history_session.restore = self.history_session.anchor.clone();
                }
                self.commits = page.history.commits;
                self.history_edges = page.history.edges;
                self.history_max_lane = page.history.max_lane;
                self.history_truncated = page.history.truncated;
                self.history_session.cursor = Some(page.cursor);
                self.history_error = None;
                self.history_session.notice = None;
                if !self
                    .commits
                    .iter()
                    .any(|commit| Some(&commit.hash) == self.selected_commit.as_ref())
                {
                    self.selected_commit = None;
                    self.commit_details_open = false;
                    self.clear_commit_preview();
                }
            }
            Err(error) => self.history_error = Some(error),
        }
    }

    pub(crate) fn remember_history_scroll(&mut self, hash: String, offset: f32) {
        self.history_session.anchor = Some(Anchor {
            hash,
            offset,
            filter: self.journal_filter.clone(),
        });
    }

    pub(crate) fn take_history_scroll(&mut self) -> Option<(String, f32)> {
        self.history_session
            .restore
            .take()
            .filter(|anchor| anchor.filter == self.journal_filter)
            .map(|anchor| (anchor.hash, anchor.offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{models::DiffPreview, test_support::linear_repo, theme::ThemeChoice};
    use std::time::{Duration, Instant};

    fn wait(app: &mut SannyGitApp) {
        app.load_history();
        let deadline = Instant::now() + Duration::from_secs(15);
        while app.history_worker.is_some() {
            assert!(!app.busy(), "History reads must not disable other controls");
            assert!(Instant::now() < deadline);
            app.poll_view_reads();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn opened() -> (crate::test_support::TempRepo, SannyGitApp) {
        let repo = linear_repo(620);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&repo.0).unwrap());
        wait(&mut app);
        assert_eq!(app.commits.len(), backend::PAGE_SIZE);
        assert!(app.history_can_load_more());
        (repo, app)
    }

    #[test]
    fn appending_preserves_loaded_rows_selection_preview_draft_and_scroll_anchor() {
        let (_repo, mut app) = opened();
        let hashes = app
            .commits
            .iter()
            .map(|commit| commit.hash.clone())
            .collect::<Vec<_>>();
        app.select_commit(hashes[450].clone());
        app.commit_message = "Keep draft".into();
        app.commit_preview = Some(DiffPreview::new("+selected commit diff\n"));
        let preview = app.commit_preview.as_ref().unwrap().lines.as_ptr();
        app.remember_history_scroll(hashes[400].clone(), 7.0);
        app.load_more_history();
        wait(&mut app);
        assert_eq!(app.commits.len(), 620);
        assert!(!app.history_can_load_more());
        assert_eq!(
            app.commits
                .iter()
                .take(backend::PAGE_SIZE)
                .map(|commit| commit.hash.clone())
                .collect::<Vec<_>>(),
            hashes
        );
        assert_eq!(app.selected_commit.as_deref(), Some(hashes[450].as_str()));
        assert_eq!(app.commit_preview.as_ref().unwrap().lines.as_ptr(), preview);
        assert_eq!(app.commit_message, "Keep draft");
        assert!(app.take_history_scroll().is_none());
        assert!(app
            .history_edges
            .iter()
            .any(|edge| edge.child == 499 && edge.parent == 500));
    }

    #[test]
    fn cancellation_discards_late_pages_and_retry_does_not_duplicate_rows() {
        let (repo, mut app) = opened();
        app.selected_commit = Some(app.commits[400].hash.clone());
        let selected = app.selected_commit.clone();
        app.load_more_history();
        app.load_history();
        let generation = app.history_session.generation;
        let flag = app.history_session.cancel.clone().unwrap();
        app.cancel_history();
        assert!(flag.load(Ordering::Relaxed));
        assert!(!app.busy());
        assert_eq!(app.commits.len(), 500);
        assert_eq!(app.selected_commit, selected);
        assert!(app.history_notice().is_some());
        let page = backend::more(
            &repo.0,
            app.history_session.cursor.as_ref().unwrap(),
            app.commits.clone(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let path = app.repositories[0].path.clone();
        app.apply_history(path, false, generation, true, Ok(page));
        assert_eq!(app.commits.len(), 500);
        app.load_more_history();
        wait(&mut app);
        assert_eq!(app.commits.len(), 620);
        assert_eq!(app.selected_commit, selected);
        assert!(app.history_notice().is_none());
    }

    #[test]
    fn cancelling_queued_history_does_not_detach_another_worker() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.queue_history_reload();
        let (sender, receiver) = std::sync::mpsc::channel();
        app.worker = Some(receiver);
        app.cancel_history();
        assert!(app.busy());
        assert!(!app.history_pending && !app.history_loading);
        sender
            .send(WorkerResult::AmendMessage(Ok((
                "a".repeat(40),
                "message".into(),
            ))))
            .unwrap();
        app.poll_worker();
        assert!(!app.busy());
    }

    #[test]
    fn stale_head_keeps_rows_until_reload_which_retains_deep_selection_and_anchor() {
        let (repo, mut app) = opened();
        let selected = app.commits[499].hash.clone();
        let anchor = app.commits[490].hash.clone();
        app.selected_commit = Some(selected.clone());
        app.remember_history_scroll(anchor.clone(), 7.0);
        repo.git(&["commit", "--allow-empty", "-m", "External HEAD"]);
        app.load_more_history();
        wait(&mut app);
        assert!(app
            .history_error
            .as_deref()
            .unwrap()
            .contains("History changed"));
        assert_eq!(app.commits.len(), 500);
        assert_eq!(app.selected_commit.as_deref(), Some(selected.as_str()));
        app.reload_history();
        wait(&mut app);
        assert!(app.history_error.is_none());
        assert_eq!(app.commits.len(), 501);
        assert_eq!(app.commits[500].hash, selected);
        assert_eq!(app.take_history_scroll(), Some((anchor, 7.0)));
    }

    #[test]
    fn scope_generation_and_repository_changes_reject_late_valid_pages() {
        let (repo, mut app) = opened();
        let path = app.repositories[0].path.clone();
        let generation = app.history_session.generation;
        let page = backend::reload(&repo.0, false, 500, &[], &AtomicBool::new(false)).unwrap();
        app.history_all_branches = true;
        app.queue_history_reload();
        app.apply_history(path.clone(), false, generation, false, Ok(page));
        assert!(app.history_pending);
        app.close_repository(true);
        assert!(app.commits.is_empty());
        let page = backend::reload(&repo.0, true, 500, &[], &AtomicBool::new(false)).unwrap();
        app.apply_history(path, true, app.history_session.generation, false, Ok(page));
        assert!(app.commits.is_empty());
    }

    #[test]
    fn reload_after_new_head_keeps_the_same_visible_rows_at_the_bottom() {
        use crate::theme::InterfaceStyle;
        use eframe::egui::{self, Context, Event, Modifiers, Pos2, RawInput, Rect};
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                    let (repo, mut app) = opened();
                    app.theme = theme;
                    app.interface_style = style;
                    app.selected_commit = Some(app.commits[0].hash.clone());
                    let selected = app.selected_commit.clone();
                    let ctx = Context::default();
                    crate::theme::install_fonts(&ctx);
                    egui_extras::install_image_loaders(&ctx);
                    theme.apply(&ctx);
                    let mut time = 0.0;
                    let mut render = |app: &mut SannyGitApp, events| {
                        time += 0.1;
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                time: Some(time),
                                events,
                                ..Default::default()
                            },
                            |ui| crate::ui::show(ui, app),
                        );
                        output.textures_delta.clear();
                        output
                    };
                    let rows = |output: &egui::FullOutput| {
                        output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Text(text)
                                    if text.galley.job.text.starts_with("History commit ") =>
                                {
                                    Some((text.galley.job.text.clone(), text.pos))
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    };
                    for _ in 0..4 {
                        render(&mut app, vec![]);
                    }
                    let initial = render(&mut app, vec![]);
                    let pointer = rows(&initial)[0].1;
                    render(
                        &mut app,
                        vec![
                            Event::PointerMoved(pointer),
                            Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0.0, -100000.0),
                                phase: egui::TouchPhase::Move,
                                source: egui::MouseWheelSource::Wheel,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                    );
                    render(&mut app, vec![Event::PointerMoved(Pos2::ZERO)]);
                    for _ in 0..12 {
                        render(&mut app, vec![]);
                    }
                    let before = rows(&render(&mut app, vec![]));
                    assert_ne!(before, rows(&initial));
                    repo.git(&["commit", "--allow-empty", "-m", "New HEAD"]);
                    app.reload_history();
                    wait(&mut app);
                    assert!(app.history_error.is_none());
                    assert_eq!(app.commits.len(), 501);
                    for _ in 0..3 {
                        render(&mut app, vec![]);
                    }
                    let after = rows(&render(&mut app, vec![]));
                    assert_eq!(
                        before, after,
                        "Reload moved old rows: {theme:?} {style:?} {size:?}"
                    );
                    assert_eq!(app.selected_commit, selected);
                }
            }
        }
    }

    #[test]
    fn more_click_is_not_reinterpreted_as_cancel_in_a_second_layout_pass() {
        use eframe::egui::{self, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};
        let (_repo, mut app) = opened();
        let ctx = Context::default();
        crate::theme::install_fonts(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let render = |app: &mut SannyGitApp, events| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1100.0, 700.0))),
                    events,
                    ..Default::default()
                },
                |ui| crate::ui::show(ui, app),
            );
            output.textures_delta.clear();
            output
        };
        for _ in 0..4 {
            render(&mut app, vec![]);
        }
        let output = render(&mut app, vec![]);
        let pointer = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "More" => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .unwrap();
        render(
            &mut app,
            vec![
                Event::PointerMoved(pointer),
                Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        app.history_session.restore = app.history_session.anchor.clone();
        let passes = ctx.cumulative_pass_nr();
        render(
            &mut app,
            vec![
                Event::PointerMoved(pointer),
                Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        assert!(ctx.cumulative_pass_nr() >= passes + 2);
        assert!(app.history_pending && app.history_loading);
        assert!(app.history_notice().is_none());
        assert_eq!(app.commits.len(), 500);
    }
}
