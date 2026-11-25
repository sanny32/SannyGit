use super::{SannyGitApp, WorkerResult};
use crate::{
    history as backend,
    models::{Commit, DiffPreview, HistoryEdge},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::AtomicBool,
        mpsc::Receiver,
        Arc,
    },
};

#[derive(Default)]
pub struct View {
    pub commits: Vec<Commit>,
    pub history_edges: Vec<HistoryEdge>,
    pub history_max_lane: usize,
    pub history_truncated: bool,
    pub history_loading: bool,
    pub history_error: Option<String>,
    pub history_all_branches: bool,
    pub journal_filter: String,
    pub selected_commit: Option<String>,
    pub commit_details_open: bool,
    pub commit_preview: Option<DiffPreview>,
    pub commit_preview_error: Option<String>,
    pub commit_files: Option<Vec<backend::CommitFile>>,
    pub selected_commit_file: Option<String>,
    pub tag: Option<String>,
    pub comparison_commit: Option<String>,
    pub comparing_with: Option<String>,
    pub reveal_commit: Option<String>,
    pub(super) history_worker: Option<Receiver<WorkerResult>>,
    pub(super) history_pending: bool,
    pub(super) history_session: Session,
    pub(super) commit_preview_worker: Option<Receiver<WorkerResult>>,
    pub(super) loaded_commit: Option<String>,
    pub(super) loaded_commit_file: Option<String>,
    pub(super) commit_preview_generation: u64,
}

impl View {
    pub(super) fn reveal_head_on_reload(&mut self) {
        self.history_session.reveal_head = true;
    }

    pub(super) fn reveal_head(&mut self) {
        if self.history_loading {
            self.reveal_head_on_reload();
            return;
        }
        let Some(cursor) = &self.history_session.cursor else {
            return;
        };
        let Some(head) = self
            .commits
            .iter()
            .find(|commit| commit.hash == cursor.head())
        else {
            return;
        };
        if !head.matches(&self.journal_filter.trim().to_lowercase()) {
            self.journal_filter.clear();
        }
        self.history_session.restore = Some(Anchor {
            hash: head.hash.clone(),
            offset: 0.0,
            filter: self.journal_filter.clone(),
        });
    }

    pub fn ref_name(&self) -> Option<&str> {
        let reference = self.tag.as_deref()?;
        if let Some(path) = reference.strip_prefix("file:") {
            return Some(path);
        }
        reference
            .strip_prefix("refs/tags/")
            .or_else(|| reference.strip_prefix("refs/remotes/"))
            .or_else(|| reference.strip_prefix("refs/heads/"))
    }

    pub fn tag_commit(&self) -> Option<&str> {
        self.history_session.cursor.as_ref()?.tag_commit()
    }

    pub(super) fn invalidate_reads(&mut self) {
        if self.history_worker.take().is_some() {
            if let Some(cancel) = self.history_session.cancel.take() {
                crate::git::signal_cancel(&cancel);
            }
            self.history_session.generation = self.history_session.generation.wrapping_add(1);
            self.history_pending = true;
        }
        if self.commit_preview_worker.take().is_some() {
            if self.commit_files.is_none() {
                self.loaded_commit = None;
            }
            self.loaded_commit_file = None;
            self.commit_preview_generation = self.commit_preview_generation.wrapping_add(1);
        }
    }
}

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
    reveal_head: bool,
}

impl Session {
    pub(super) fn clear_view(&mut self) {
        let generation = self.generation.wrapping_add(1);
        if let Some(cancel) = &self.cancel {
            crate::git::signal_cancel(&cancel);
        }
        *self = Self {
            generation,
            ..Default::default()
        };
    }
}

impl SannyGitApp {
    fn commit_preview_matches(&self, path: &str, hash: &str, generation: u64) -> bool {
        self.repositories
            .get(self.selected_repository)
            .is_some_and(|repo| repo.path == path)
            && self.history.selected_commit.as_deref() == Some(hash)
            && self.history.commit_preview_generation == generation
    }

    pub(super) fn apply_commit_patch(
        &mut self,
        path: String,
        file: String,
        hash: String,
        generation: u64,
        result: Result<DiffPreview, String>,
    ) {
        if self.commit_preview_matches(&path, &hash, generation)
            && self.history.selected_commit_file.as_ref() == Some(&file)
        {
            match result {
                Ok(preview) => {
                    self.history.commit_preview = Some(preview);
                    self.history.commit_preview_error = None;
                }
                Err(error) => self.history.commit_preview_error = Some(error),
            }
        }
    }

    pub(super) fn apply_commit_files(
        &mut self,
        path: String,
        hash: String,
        generation: u64,
        result: Result<Vec<backend::CommitFile>, String>,
    ) {
        if self.commit_preview_matches(&path, &hash, generation) {
            match result {
                Ok(files) => {
                    self.history.selected_commit_file = files.first().map(|file| file.path.clone());
                    self.history.commit_files = Some(files);
                    self.history.commit_preview_error = None;
                }
                Err(error) => self.history.commit_preview_error = Some(error),
            }
        }
    }

    /// Same entry point as branch Log: opens the history window on the current branch.
    pub fn open_current_branch_log(&mut self) {
        if self.busy() || self.repositories.get(self.selected_repository).is_none() {
            return;
        }
        let Some(current) = self
            .branches
            .entries
            .iter()
            .find(|entry| entry.current && entry.kind == crate::models::RefKind::Local)
        else {
            self.open_head_log();
            return;
        };
        self.branches.selected_branch = current.full_name.clone();
        self.open_ref_log(current.full_name.clone());
    }

    pub fn open_head_log(&mut self) {
        if self.busy()
            || self.repositories.get(self.selected_repository).is_none()
            || self.history.commits.is_empty()
        {
            return;
        }
        self.close_tag_log();
        let hash = self
            .branches
            .entries
            .iter()
            .find(|entry| entry.current)
            .map(|entry| entry.hash.clone())
            .filter(|hash| {
                self.history
                    .commits
                    .iter()
                    .any(|commit| &commit.hash == hash)
            })
            .unwrap_or_else(|| self.history.commits[0].hash.clone());
        self.history.journal_filter.clear();
        self.select_commit(hash.clone());
        self.history.reveal_commit = Some(hash);
    }

    /// Open current-branch history on a journal commit, replacing a ref-scoped log.
    pub fn open_commit_history(&mut self, hash: &str) -> bool {
        if self.busy()
            || self.repositories.get(self.selected_repository).is_none()
            || !self
                .history
                .commits
                .iter()
                .any(|commit| commit.hash == hash)
        {
            return false;
        }
        self.close_tag_log();
        let hash = hash.to_owned();
        self.select_commit(hash.clone());
        self.history.commit_details_open = true;
        self.history.reveal_commit = Some(hash);
        if self.history.history_all_branches {
            self.history.history_all_branches = false;
            self.queue_history_reload();
        }
        true
    }

    pub fn open_ref_log(&mut self, tag: String) {
        if self.busy()
            || self.repositories.get(self.selected_repository).is_none()
            || !self
                .branches
                .entries
                .iter()
                .any(|entry| entry.full_name == tag)
        {
            return;
        }
        if self.tag_log.as_ref().and_then(|view| view.tag.as_ref()) == Some(&tag) {
            self.with_history_window(|app| {
                if let Some(hash) = app.history.tag_commit().map(str::to_owned) {
                    app.history.journal_filter.clear();
                    app.select_commit(hash.clone());
                    app.history.reveal_commit = Some(hash);
                }
            });
            return;
        }
        self.close_tag_log();
        self.history.commit_details_open = false;
        self.tag_log = Some(View {
            tag: Some(tag),
            commit_details_open: true,
            ..View::default()
        });
        self.with_history_window(|app| app.queue_history_reload());
    }

    pub fn close_tag_log(&mut self) {
        if let Some(mut view) = self.tag_log.take() {
            view.history_session.clear_view();
        }
    }

    pub fn history_window(&self) -> &View {
        self.tag_log.as_ref().unwrap_or(&self.history)
    }

    pub fn with_history_window(&mut self, f: impl FnOnce(&mut Self)) {
        if let Some(mut view) = self.tag_log.take() {
            std::mem::swap(&mut self.history, &mut view);
            f(self);
            std::mem::swap(&mut self.history, &mut view);
            self.tag_log = Some(view);
        } else {
            f(self);
        }
    }

    pub fn close_history_window(&mut self) {
        self.close_tag_log();
        self.history.commit_details_open = false;
    }

    pub(super) fn queue_history_reload(&mut self) {
        if let Some(cancel) = self.history.history_session.cancel.take() {
            crate::git::signal_cancel(&cancel);
            self.history.history_worker = None;
        }
        self.history.history_session.generation =
            self.history.history_session.generation.wrapping_add(1);
        self.history.history_session.append = false;
        self.history.history_session.notice = None;
        self.history.history_pending = true;
        self.history.history_loading = true;
        self.history.history_error = None;
    }

    pub fn reload_history(&mut self) {
        if self.busy() || self.repositories.get(self.selected_repository).is_none() {
            return;
        }
        self.queue_history_reload();
        if self.history.tag.is_none() {
            self.history.reveal_head_on_reload();
        }
    }

    pub(crate) fn history_reload_pending(&self) -> bool {
        self.history.history_pending || self.history.history_loading
    }

    pub fn history_can_load_more(&self) -> bool {
        !self.busy()
            && !self.history.history_loading
            && !self.history.history_pending
            && self.repositories.get(self.selected_repository).is_some()
            && self
                .history
                .history_session
                .cursor
                .as_ref()
                .is_some_and(|cursor| {
                    cursor.has_more()
                        && cursor.all_branches() == self.history.history_all_branches
                        && cursor.loaded() == self.history.commits.len()
                })
    }

    pub fn history_at_limit(&self) -> bool {
        self.history
            .history_session
            .cursor
            .as_ref()
            .is_some_and(backend::Cursor::at_limit)
    }

    pub fn history_cancellable(&self) -> bool {
        self.history.history_loading
            && (self.history.history_pending || self.history.history_session.cancel.is_some())
    }

    pub fn history_notice(&self) -> Option<&str> {
        self.history.history_session.notice.as_deref()
    }

    pub fn load_more_history(&mut self) {
        if !self.history_can_load_more() {
            return;
        }
        self.history.history_session.generation =
            self.history.history_session.generation.wrapping_add(1);
        self.history.history_session.append = true;
        self.history.history_session.notice = None;
        self.history.history_pending = true;
        self.history.history_loading = true;
        self.history.history_error = None;
    }

    pub fn cancel_history(&mut self) {
        if !self.history_cancellable() {
            return;
        }
        if let Some(cancel) = self.history.history_session.cancel.take() {
            crate::git::signal_cancel(&cancel);
            self.history.history_worker = None;
        }
        self.history.history_error = None;
        self.history.history_session.generation =
            self.history.history_session.generation.wrapping_add(1);
        self.history.history_loading = false;
        self.history.history_pending = false;
        self.history.history_session.reveal_head = false;
        self.history.history_session.notice = Some("History loading cancelled; previous rows and selection are retained. Use More or Reload to retry.".into());
    }

    pub(super) fn load_history(&mut self) {
        if self.busy() || self.history.history_worker.is_some() || !self.history.history_pending {
            return;
        }
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let path = repository.path.clone();
        let all = self.history.history_all_branches;
        let tag = self.history.tag.clone();
        let generation = self.history.history_session.generation;
        let append = self.history.history_session.append;
        let cursor = self.history.history_session.cursor.clone();
        let previous = if append {
            self.history.commits.clone()
        } else {
            Vec::new()
        };
        let requested = self.history.commits.len().max(backend::PAGE_SIZE);
        let keep = self
            .history
            .selected_commit
            .iter()
            .cloned()
            .chain(
                self.history
                    .history_session
                    .anchor
                    .iter()
                    .map(|anchor| anchor.hash.clone()),
            )
            .chain(
                self.history
                    .commits
                    .last()
                    .map(|commit| commit.hash.clone()),
            )
            .collect::<Vec<_>>();
        let cancel = Arc::new(AtomicBool::new(false));
        self.history.history_session.cancel = Some(cancel.clone());
        self.history.history_pending = false;
        match self.spawn_reader(None, move |git| {
            let root = PathBuf::from(&path);
            let result = if append {
                cursor
                    .as_ref()
                    .ok_or_else(|| "Reload history before loading more.".to_owned())
                    .and_then(|cursor| backend::more(git, &root, cursor, previous, &cancel))
            } else if let Some(tag) = tag.as_deref() {
                if let Some(file) = tag.strip_prefix("file:") {
                    backend::reload_file(git, &root, file, requested, &keep, &cancel)
                } else {
                    backend::reload_tag(git, &root, tag, requested, &keep, &cancel)
                }
            } else {
                backend::reload(git, &root, all, requested, &keep, &cancel)
            };
            WorkerResult::History {
                path,
                all,
                generation,
                append,
                result,
            }
        }) {
            Ok(receiver) => self.history.history_worker = Some(receiver),
            Err(error) => {
                self.history.history_session.cancel = None;
                self.history.history_loading = false;
                self.history.history_error = Some(error);
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
            || self.history.history_all_branches != all
            || self.history.history_session.generation != generation
        {
            return;
        }
        self.history.history_loading = false;
        self.history.history_session.cancel = None;
        match result {
            Ok(page) => {
                if all && self.history.tag.is_none() {
                    self.working_tree.refs_token = Some(page.cursor.listed_refs().to_vec());
                }
                if !append {
                    self.history.history_session.restore =
                        self.history.history_session.anchor.clone();
                }
                self.history.commits = page.history.commits;
                self.history.history_edges = page.history.edges;
                self.history.history_max_lane = page.history.max_lane;
                self.history.history_truncated = page.history.truncated;
                let tag_commit = page.cursor.tag_commit().map(str::to_owned);
                self.history.history_session.cursor = Some(page.cursor);
                if std::mem::take(&mut self.history.history_session.reveal_head) {
                    self.history.reveal_head();
                }
                self.history.history_error = None;
                self.history.history_session.notice = None;
                if !self
                    .history
                    .commits
                    .iter()
                    .any(|commit| Some(&commit.hash) == self.history.selected_commit.as_ref())
                {
                    self.clear_commit_preview();
                    self.history.selected_commit = tag_commit.clone();
                    self.history.reveal_commit = tag_commit;
                    self.history.commit_details_open = self.history.selected_commit.is_some();
                }
            }
            Err(error) => self.history.history_error = Some(error),
        }
    }

    pub(super) fn history_refs_match(&self, refs: &[u8]) -> bool {
        self.history
            .history_session
            .cursor
            .as_ref()
            .is_some_and(|cursor| cursor.all_branches() && cursor.listed_refs() == refs)
    }

    pub(crate) fn remember_history_scroll(&mut self, hash: String, offset: f32) {
        self.history.history_session.anchor = Some(Anchor {
            hash,
            offset,
            filter: self.history.journal_filter.clone(),
        });
    }

    pub(crate) fn take_history_scroll(&mut self) -> Option<(String, f32)> {
        self.history
            .history_session
            .restore
            .take()
            .filter(|anchor| anchor.filter == self.history.journal_filter)
            .map(|anchor| (anchor.hash, anchor.offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::SYSTEM_GIT;
    use crate::{models::DiffPreview, test_support::linear_repo, theme::ThemeChoice};
    use std::time::{Duration, Instant};

    fn wait(app: &mut SannyGitApp) {
        app.load_history();
        let deadline = Instant::now() + Duration::from_secs(15);
        while app.history.history_worker.is_some() {
            assert!(!app.busy(), "History reads must not disable other controls");
            assert!(Instant::now() < deadline);
            app.poll_view_reads();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn journal_reveals_head_after_open_and_successful_operations() {
        let repo = crate::test_support::TempRepo::new();
        repo.git(&["commit", "--allow-empty", "-m", "Current commit"]);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        let snapshot = || crate::git::load(&SYSTEM_GIT, &repo.0);
        app.apply_worker_result(WorkerResult::Repository(snapshot()));
        wait(&mut app);
        let head = app.history.commits[0].hash.clone();
        assert_eq!(app.take_history_scroll(), Some((head.clone(), 0.0)));

        for (success, committed, succeeded) in [
            ("Commit created", true, true),
            ("Pull complete", false, true),
            ("Push complete", false, true),
            ("Push complete", false, false),
            ("Fetch complete", false, true),
            ("Checkout complete", false, true),
            ("Stash applied", false, true),
        ] {
            app.history.journal_filter = "hidden".into();
            app.remember_history_scroll(head.clone(), 8.0);
            app.apply_worker_result(WorkerResult::Operation {
                committed,
                success,
                outcome: if succeeded {
                    Ok(())
                } else {
                    Err("Failed".into())
                },
                snapshot: snapshot(),
            });
            assert_eq!(app.take_history_scroll(), None);
            wait(&mut app);
            let reveal = succeeded;
            assert_eq!(
                app.take_history_scroll(),
                Some((head.clone(), if reveal { 0.0 } else { 8.0 }))
            );
            assert_eq!(
                app.history.journal_filter,
                if reveal { "" } else { "hidden" }
            );
        }
    }

    #[test]
    fn head_log_opens_the_current_commit_and_closes_a_ref_log() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        let head = "a".repeat(40);
        let other = "b".repeat(40);
        app.repositories.push(crate::models::Repository {
            name: "Repo".into(),
            path: "/repo".into(),
        });
        app.selected_repository = 0;
        app.history.commits = vec![
            crate::models::Commit {
                message: "Head".into(),
                author: String::new(),
                email: String::new(),
                date: String::new(),
                hash: head.clone(),
                parents: Vec::new(),
                lane: 0,
            },
            crate::models::Commit {
                message: "Older".into(),
                author: String::new(),
                email: String::new(),
                date: String::new(),
                hash: other,
                parents: Vec::new(),
                lane: 0,
            },
        ];
        app.branches.entries.push(crate::models::BranchRef {
            name: "main".into(),
            full_name: "refs/heads/main".into(),
            hash: head.clone(),
            upstream: None,
            tag_subject: None,
            current: true,
            kind: crate::models::RefKind::Local,
        });
        app.history.journal_filter = "noise".into();
        app.tag_log = Some(View {
            tag: Some("refs/tags/v1".into()),
            commit_details_open: true,
            ..View::default()
        });
        app.open_head_log();
        assert!(app.tag_log.is_none());
        assert!(app.history.commit_details_open);
        assert_eq!(app.history.selected_commit.as_deref(), Some(head.as_str()));
        assert!(app.history.journal_filter.is_empty());
        assert_eq!(app.history.reveal_commit.as_deref(), Some(head.as_str()));
    }

    #[test]
    fn ref_log_selects_the_target_commit_and_preserves_the_journal() {
        let repo = crate::test_support::TempRepo::new();
        repo.write("base.txt", "Base\n");
        repo.git(&["add", "base.txt"]);
        repo.git(&["commit", "-m", "Base"]);
        repo.git(&["switch", "-c", "release"]);
        repo.write("release.txt", "Release content\n");
        repo.git(&["add", "release.txt"]);
        repo.git(&["commit", "-m", "Release"]);
        let tag_commit = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
        repo.git(&["tag", "-a", "-m", "Release notes", "v1"]);
        repo.git(&["update-ref", "refs/remotes/origin/release", &tag_commit]);
        repo.git(&["switch", "main"]);
        repo.git(&["commit", "--allow-empty", "-m", "Main only"]);
        let head = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
        repo.git(&["switch", "-c", "unrelated", "HEAD~"]);
        repo.git(&["commit", "--allow-empty", "-m", "Unrelated only"]);
        repo.git(&["switch", "main"]);

        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&SYSTEM_GIT, &repo.0).unwrap());
        app.branches.entries = crate::branches::load(&SYSTEM_GIT, &repo.0).unwrap().refs;
        wait(&mut app);
        app.history.selected_commit = Some(head.clone());
        app.history.journal_filter = "Main only".into();
        for (reference, name) in [
            ("refs/tags/v1", "v1"),
            ("refs/remotes/origin/release", "origin/release"),
        ] {
            app.open_ref_log(reference.into());
            assert_eq!(app.history_window().ref_name(), Some(name));
            assert!(app.history_window().commit_details_open);
            let deadline = Instant::now() + Duration::from_secs(30);
            while app.history_window().commit_preview.is_none() {
                app.with_history_window(|app| {
                    app.load_history();
                    app.load_commit_preview();
                });
                app.poll_view_reads();
                let view = app.history_window();
                assert!(view.history_error.is_none(), "{:?}", view.history_error);
                assert!(
                    view.commit_preview_error.is_none(),
                    "{:?}",
                    view.commit_preview_error
                );
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            let view = app.history_window();
            assert_eq!(view.selected_commit.as_ref(), Some(&tag_commit));
            assert_eq!(view.reveal_commit.as_ref(), Some(&tag_commit));
            assert_eq!(view.selected_commit_file.as_deref(), Some("release.txt"));
            assert_eq!(view.commit_preview.as_ref().unwrap().added, 1);
            assert!(view.commits.iter().any(|commit| commit.hash == head));
            assert!(!view
                .commits
                .iter()
                .any(|commit| commit.subject() == "Unrelated only"));
            assert_eq!(app.history.selected_commit.as_ref(), Some(&head));
            assert_eq!(app.history.journal_filter, "Main only");
            assert!(!app
                .history
                .commits
                .iter()
                .any(|commit| commit.hash == tag_commit));
            app.close_history_window();
        }
        assert!(!app.history_window().commit_details_open);
        assert!(!app.history_window().history_loading);
        assert_eq!(app.history_window().selected_commit.as_ref(), Some(&head));
    }

    fn opened() -> (crate::test_support::TempRepo, SannyGitApp) {
        let repo = linear_repo(620);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&SYSTEM_GIT, &repo.0).unwrap());
        wait(&mut app);
        assert_eq!(app.history.commits.len(), backend::PAGE_SIZE);
        assert!(app.history_can_load_more());
        assert!(app.take_history_scroll().is_some());
        (repo, app)
    }

    #[test]
    fn appending_preserves_loaded_rows_selection_preview_draft_and_scroll_anchor() {
        let (_repo, mut app) = opened();
        let hashes = app
            .history
            .commits
            .iter()
            .map(|commit| commit.hash.clone())
            .collect::<Vec<_>>();
        app.select_commit(hashes[450].clone());
        app.commit.message = "Keep draft".into();
        app.history.commit_preview = Some(DiffPreview::new("+selected commit diff\n"));
        let preview = app.history.commit_preview.as_ref().unwrap().lines.as_ptr();
        app.remember_history_scroll(hashes[400].clone(), 7.0);
        app.load_more_history();
        wait(&mut app);
        assert_eq!(app.history.commits.len(), 620);
        assert!(!app.history_can_load_more());
        assert_eq!(
            app.history
                .commits
                .iter()
                .take(backend::PAGE_SIZE)
                .map(|commit| commit.hash.clone())
                .collect::<Vec<_>>(),
            hashes
        );
        assert_eq!(
            app.history.selected_commit.as_deref(),
            Some(hashes[450].as_str())
        );
        assert_eq!(
            app.history.commit_preview.as_ref().unwrap().lines.as_ptr(),
            preview
        );
        assert_eq!(app.commit.message, "Keep draft");
        assert!(app.take_history_scroll().is_none());
        assert!(app
            .history
            .history_edges
            .iter()
            .any(|edge| edge.child == 499 && edge.parent == 500));
    }

    #[test]
    fn cancellation_discards_late_pages_and_retry_does_not_duplicate_rows() {
        let (repo, mut app) = opened();
        app.history.selected_commit = Some(app.history.commits[400].hash.clone());
        let selected = app.history.selected_commit.clone();
        app.load_more_history();
        app.load_history();
        let generation = app.history.history_session.generation;
        let flag = app.history.history_session.cancel.clone().unwrap();
        app.cancel_history();
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
        assert!(!app.busy());
        assert_eq!(app.history.commits.len(), 500);
        assert_eq!(app.history.selected_commit, selected);
        assert!(app.history_notice().is_some());
        let page = backend::more(
            &SYSTEM_GIT,
            &repo.0,
            app.history.history_session.cursor.as_ref().unwrap(),
            app.history.commits.clone(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let path = app.repositories[0].path.clone();
        app.apply_history(path, false, generation, true, Ok(page));
        assert_eq!(app.history.commits.len(), 500);
        app.load_more_history();
        wait(&mut app);
        assert_eq!(app.history.commits.len(), 620);
        assert_eq!(app.history.selected_commit, selected);
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
        assert!(!app.history.history_pending && !app.history.history_loading);
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
    fn stale_head_keeps_rows_until_reload_which_retains_deep_selection() {
        let (repo, mut app) = opened();
        let selected = app.history.commits[499].hash.clone();
        let anchor = app.history.commits[490].hash.clone();
        app.history.selected_commit = Some(selected.clone());
        app.remember_history_scroll(anchor.clone(), 7.0);
        repo.git(&["commit", "--allow-empty", "-m", "External HEAD"]);
        app.load_more_history();
        wait(&mut app);
        assert!(app
            .history
            .history_error
            .as_deref()
            .unwrap()
            .contains("History changed"));
        assert_eq!(app.history.commits.len(), 500);
        assert_eq!(
            app.history.selected_commit.as_deref(),
            Some(selected.as_str())
        );
        app.reload_history();
        wait(&mut app);
        assert!(app.history.history_error.is_none());
        assert_eq!(app.history.commits.len(), 501);
        assert_eq!(app.history.commits[500].hash, selected);
        let head = app.history.commits[0].hash.clone();
        assert_eq!(app.take_history_scroll(), Some((head, 0.0)));
    }

    #[test]
    fn scope_generation_and_repository_changes_reject_late_valid_pages() {
        let (repo, mut app) = opened();
        let path = app.repositories[0].path.clone();
        let generation = app.history.history_session.generation;
        let page = backend::reload(
            &SYSTEM_GIT,
            &repo.0,
            false,
            500,
            &[],
            &AtomicBool::new(false),
        )
        .unwrap();
        app.history.history_all_branches = true;
        app.queue_history_reload();
        app.apply_history(path.clone(), false, generation, false, Ok(page));
        assert!(app.history.history_pending);
        app.close_repository(true);
        assert!(app.history.commits.is_empty());
        let page = backend::reload(
            &SYSTEM_GIT,
            &repo.0,
            true,
            500,
            &[],
            &AtomicBool::new(false),
        )
        .unwrap();
        app.apply_history(
            path,
            true,
            app.history.history_session.generation,
            false,
            Ok(page),
        );
        assert!(app.history.commits.is_empty());
    }

    #[test]
    fn journal_keeps_rows_visible_until_reload_reveals_head() {
        use crate::theme::InterfaceStyle;
        use eframe::egui::{self, Context, Event, Modifiers, Pos2, RawInput, Rect};
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                    let (repo, mut app) = opened();
                    app.theme = theme;
                    app.interface_style = style;
                    app.history.selected_commit = Some(app.history.commits[0].hash.clone());
                    let selected = app.history.selected_commit.clone();
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
                                    if text.galley.job.text.starts_with("History commit ")
                                        || text.galley.job.text == "New HEAD" =>
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
                    assert_eq!(before, rows(&render(&mut app, vec![])));
                    wait(&mut app);
                    assert!(app.history.history_error.is_none());
                    assert_eq!(app.history.commits.len(), 501);
                    assert_eq!(app.history.selected_commit, selected);
                    for _ in 0..3 {
                        render(&mut app, vec![]);
                    }
                    assert_eq!(
                        rows(&render(&mut app, vec![]))[0].0,
                        "New HEAD",
                        "HEAD was not visible: {theme:?} {style:?} {size:?}"
                    );
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
        app.history.history_session.restore = app.history.history_session.anchor.clone();
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
        assert!(app.history.history_pending && app.history.history_loading);
        assert!(app.history_notice().is_none());
        assert_eq!(app.history.commits.len(), 500);
    }

    #[test]
    fn journal_commit_log_replaces_a_ref_log_and_reveals_that_commit() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        let hash = "c".repeat(40);
        app.repositories.push(crate::models::Repository {
            name: "Repo".into(),
            path: "/repo".into(),
        });
        app.selected_repository = 0;
        app.history.commits = vec![crate::models::Commit {
            message: "Chosen".into(),
            author: String::new(),
            email: String::new(),
            date: String::new(),
            hash: hash.clone(),
            parents: Vec::new(),
            lane: 0,
        }];
        app.tag_log = Some(View {
            tag: Some("refs/tags/v1".into()),
            commit_details_open: true,
            ..View::default()
        });
        assert!(app.open_commit_history(&hash));
        assert!(app.tag_log.is_none());
        assert!(app.history.commit_details_open);
        assert_eq!(app.history.selected_commit.as_deref(), Some(hash.as_str()));
        assert_eq!(app.history.reveal_commit.as_deref(), Some(hash.as_str()));
        assert!(!app.history.history_all_branches);
        assert!(!app.history.history_pending);
        assert!(!app.open_commit_history("missing"));
        assert_eq!(app.history.selected_commit.as_deref(), Some(hash.as_str()));
        app.history.history_all_branches = true;
        assert!(app.open_commit_history(&hash));
        assert!(!app.history.history_all_branches);
        assert!(app.history.history_pending && app.history.history_loading);
        assert_eq!(app.history.reveal_commit.as_deref(), Some(hash.as_str()));
    }
}
