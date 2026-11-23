use super::{SannyGitApp, WorkerResult};
use crate::{history as backend, models::DiffPreview};
use std::path::PathBuf;

impl SannyGitApp {
    pub fn open_file_log(&mut self, file: String) {
        if self.busy()
            || self.repositories.get(self.selected_repository).is_none()
            || file.is_empty()
        {
            return;
        }
        let tag = format!("file:{file}");
        if self.tag_log.as_ref().and_then(|view| view.tag.as_ref()) == Some(&tag) {
            self.with_history_window(|app| {
                if let Some(hash) = app.history.commits.first().map(|c| c.hash.clone()) {
                    app.select_commit(hash);
                }
            });
            return;
        }
        self.close_tag_log();
        self.history.commit_details_open = false;
        self.tag_log = Some(super::history::View {
            tag: Some(tag),
            commit_details_open: true,
            ..Default::default()
        });
        self.with_history_window(|app| app.queue_history_reload());
    }

    pub fn set_comparison_commit(&mut self, commit: String) {
        if crate::branches::is_commit_id(&commit) {
            self.history.comparison_commit = Some(commit);
            self.status = "Comparison commit set".into();
        }
    }

    pub fn clear_commit_comparison(&mut self) {
        self.history.comparison_commit = None;
        if self.history.comparing_with.take().is_some() {
            self.history.commit_preview = None;
            self.history.commit_preview_error = None;
            self.history.commit_preview_generation =
                self.history.commit_preview_generation.wrapping_add(1);
            if self.history.commit_details_open {
                self.load_commit_preview();
            }
        }
    }

    pub fn compare_selected_with_head(&mut self) {
        let Some(to) = self.history.selected_commit.clone() else {
            return;
        };
        let from = self.branches.head_commit.clone();
        if from.is_empty() || to.eq_ignore_ascii_case(&from) {
            return;
        }
        self.start_commit_compare(from, to);
    }

    pub fn compare_selected_with_marker(&mut self) {
        let Some(from) = self.history.comparison_commit.clone() else {
            return;
        };
        let Some(to) = self.history.selected_commit.clone() else {
            return;
        };
        if to.eq_ignore_ascii_case(&from) {
            return;
        }
        self.start_commit_compare(from, to);
    }

    pub(super) fn start_commit_compare(&mut self, from: String, to: String) {
        self.history.commit_preview_worker = None;
        self.history.commit_preview = None;
        self.history.commit_preview_error = None;
        self.history.commit_files = None;
        self.history.selected_commit_file = None;
        self.history.loaded_commit = None;
        self.history.loaded_commit_file = None;
        self.history.comparing_with = Some(from.clone());
        self.history.selected_commit = Some(to.clone());
        self.history.commit_details_open = true;
        self.history.commit_preview_generation =
            self.history.commit_preview_generation.wrapping_add(1);
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let path = repository.path.clone();
        let root = PathBuf::from(&path);
        let generation = self.history.commit_preview_generation;
        let from_for_diff = from.clone();
        let to_for_diff = to.clone();
        match self.spawn_reader(None, move |git| {
            let patch = backend::compare_commits(git, &root, &from_for_diff, &to_for_diff)
                .map(|text| DiffPreview::new(&text));
            WorkerResult::CommitCompare {
                path,
                from,
                to,
                generation,
                result: patch,
            }
        }) {
            Ok(receiver) => self.history.commit_preview_worker = Some(receiver),
            Err(error) => self.history.commit_preview_error = Some(error),
        }
    }

    pub(super) fn apply_commit_compare(
        &mut self,
        path: String,
        from: String,
        to: String,
        generation: u64,
        result: Result<DiffPreview, String>,
    ) {
        if !self
            .repositories
            .get(self.selected_repository)
            .is_some_and(|repo| repo.path == path)
            || self.history.commit_preview_generation != generation
            || self.history.comparing_with.as_deref() != Some(from.as_str())
            || self.history.selected_commit.as_deref() != Some(to.as_str())
        {
            return;
        }
        match result {
            Ok(preview) => {
                self.history.commit_preview = Some(preview);
                self.history.commit_preview_error = None;
            }
            Err(error) => self.history.commit_preview_error = Some(error),
        }
    }
}
