use super::{SannyGitApp, WorkerResult};
use crate::git::GitContext;
use crate::{git, models::ChangeKind};
use std::path::{Path, PathBuf};

pub struct IgnoreDialog {
    pub root: String,
    pub paths: Vec<String>,
    pub parent: Option<PathBuf>,
    pub use_parent: bool,
    pub use_pattern: bool,
    pub pattern: String,
}

pub struct FileSelection {
    pub active: usize,
    pub paths: std::collections::BTreeSet<String>,
    pub anchor: Option<String>,
}

impl Default for FileSelection {
    fn default() -> Self {
        Self {
            active: usize::MAX,
            paths: Default::default(),
            anchor: None,
        }
    }
}

impl FileSelection {
    fn select(
        &mut self,
        changes: &[crate::models::ChangeFile],
        index: usize,
        toggle: bool,
        extend: bool,
        visible: &[usize],
    ) {
        let Some(change) = changes.get(index) else {
            return;
        };
        let path = change.path.clone();
        if self.paths.is_empty() {
            if let Some(active) = changes.get(self.active) {
                self.paths.insert(active.path.clone());
            }
        }
        if extend {
            if self
                .anchor
                .as_ref()
                .is_none_or(|path| !visible.iter().any(|&i| changes[i].path == *path))
            {
                self.anchor = changes.get(self.active).map(|change| change.path.clone());
            }
            let anchor = self
                .anchor
                .as_ref()
                .and_then(|path| visible.iter().position(|&i| changes[i].path == *path))
                .or_else(|| visible.iter().position(|&i| i == self.active));
            if !toggle {
                self.paths.clear();
            }
            if let (Some(start), Some(end)) = (anchor, visible.iter().position(|&i| i == index)) {
                for &i in &visible[start.min(end)..=start.max(end)] {
                    self.paths.insert(changes[i].path.clone());
                }
            } else {
                self.paths.insert(path.clone());
            }
        } else if toggle {
            if !self.paths.remove(&path) {
                self.paths.insert(path.clone());
            }
            self.anchor = Some(path.clone());
        } else {
            self.paths.clear();
            self.paths.insert(path.clone());
            self.anchor = Some(path.clone());
        }
        self.active = if self.paths.contains(&path) {
            index
        } else {
            changes
                .iter()
                .position(|change| self.paths.contains(&change.path))
                .unwrap_or(usize::MAX)
        };
    }
    fn retain(&mut self, changes: &[crate::models::ChangeFile]) {
        self.paths
            .retain(|path| changes.iter().any(|change| &change.path == path));
        if self
            .anchor
            .as_ref()
            .is_some_and(|path| !changes.iter().any(|change| &change.path == path))
        {
            self.anchor = None;
        }
        if !self.paths.is_empty()
            && !changes
                .get(self.active)
                .is_some_and(|change| self.paths.contains(&change.path))
        {
            self.active = changes
                .iter()
                .position(|change| self.paths.contains(&change.path))
                .unwrap_or(usize::MAX);
        }
    }
}

impl SannyGitApp {
    pub(super) fn start_index_worker(
        &mut self,
        job: impl FnOnce(&GitContext) -> WorkerResult + Send + 'static,
    ) {
        // Index edits supersede working-tree reads but leave immutable commit
        // previews, history and refs (including their in-flight reads) intact.
        self.working_tree.invalidate();
        self.diff_worker = None;
        match self.spawn_reader(None, job) {
            Ok(receiver) => self.worker = Some(receiver),
            Err(error) => {
                self.error = Some(error);
                self.status = "Operation failed".into();
            }
        }
    }

    pub fn change_is_selected(&self, index: usize) -> bool {
        self.changes.get(index).is_some_and(|change| {
            if self.selection.paths.is_empty() {
                self.selection.active == index
            } else {
                self.selection.paths.contains(&change.path)
            }
        })
    }

    pub fn selected_change_indices(&self) -> Vec<usize> {
        (0..self.changes.len())
            .filter(|&index| self.change_is_selected(index))
            .collect()
    }

    pub fn select_change(&mut self, index: usize, toggle: bool, extend: bool, visible: &[usize]) {
        let previous = self.selection.active;
        self.selection
            .select(&self.changes, index, toggle, extend, visible);
        if self.selection.active != previous {
            self.diff_line_selection.clear();
        }
    }

    pub(super) fn retain_change_selection(&mut self) {
        self.selection.retain(&self.changes);
    }

    pub fn can_ignore_selected_changes(&self) -> bool {
        let selected = self.selected_change_indices();
        !self.busy()
            && self.state_current
            && self.repositories.get(self.selected_repository).is_some()
            && !selected.is_empty()
            && selected.iter().all(|&index| {
                let change = &self.changes[index];
                change.kind == ChangeKind::Untracked
                    && crate::git::ignore_pattern(&change.path).is_ok()
            })
    }

    pub fn open_ignore_dialog(&mut self) {
        if !self.can_ignore_selected_changes() {
            return;
        }
        let paths: Vec<_> = self
            .selected_change_indices()
            .into_iter()
            .map(|index| self.changes[index].path.clone())
            .collect();
        let first = Path::new(&paths[0]);
        let parent = first.parent().filter(|parent| {
            paths
                .iter()
                .all(|path| Path::new(path).parent() == Some(parent))
        });
        let pattern = first
            .extension()
            .and_then(|extension| extension.to_str())
            .filter(|extension| !extension.is_empty())
            .map(|extension| format!("*.{extension}"))
            .unwrap_or_default();
        self.ignore_dialog = Some(IgnoreDialog {
            root: self.repositories[self.selected_repository].path.clone(),
            parent: parent.map(Path::to_path_buf),
            use_parent: parent.is_some_and(|path| !path.as_os_str().is_empty()),
            paths,
            use_pattern: false,
            pattern,
        });
    }

    pub fn confirm_ignore(&mut self) {
        let Some(dialog) = &self.ignore_dialog else {
            return;
        };
        if self.busy()
            || !self.state_current
            || self
                .repositories
                .get(self.selected_repository)
                .map(|repo| &repo.path)
                != Some(&dialog.root)
            || (dialog.use_pattern
                && (dialog.parent.is_none() || !git::valid_ignore_glob(&dialog.pattern)))
        {
            return;
        }
        let root = PathBuf::from(&dialog.root);
        let paths = dialog.paths.clone();
        let target_dir = if dialog.use_parent {
            dialog.parent.clone().unwrap_or_default()
        } else {
            PathBuf::new()
        };
        let pattern = dialog.use_pattern.then(|| dialog.pattern.clone());
        self.error = None;
        self.status = "Updating .gitignore…".into();
        if self.start_worker(move |git| WorkerResult::Operation {
            committed: false,
            success: ".gitignore updated",
            outcome: git::ignore_paths(git, &root, &paths, &target_dir, pattern.as_deref()),
            snapshot: git::load(git, &root),
        }) {
            self.state_current = false;
        }
    }

    pub fn selected_staging_allowed(&self, staged: bool) -> bool {
        let selected = self.selected_change_indices();
        !selected.is_empty()
            && !self.busy()
            && self.state_current
            && self.repositories.get(self.selected_repository).is_some()
            && selected.iter().all(|&i| {
                self.changes[i].kind != ChangeKind::Conflicted || (self.merging && staged)
            })
            && selected.iter().any(|&i| self.staging_allowed(i, staged))
    }

    pub fn set_selected_staged(&mut self, staged: bool) {
        if !self.selected_staging_allowed(staged) {
            return;
        }
        let paths: Vec<_> = self
            .selected_change_indices()
            .into_iter()
            .filter(|&i| self.staging_allowed(i, staged))
            .map(|i| self.changes[i].path.clone())
            .collect();
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        self.state_current = false;
        self.error = None;
        self.status = if staged {
            "Staging selected files…"
        } else {
            "Unstaging selected files…"
        }
        .into();
        let selected = self
            .changes
            .get(self.selection.active)
            .map(|c| c.path.clone());
        let ignore_whitespace = self.preferences.diff_ignore_whitespace;
        let show_ignored = self.preferences.show_ignored_files;
        self.start_index_worker(move |git| WorkerResult::Index {
            staged,
            outcome: git::set_staged_paths(git, &root, &paths, staged),
            update: super::working_tree::Update::read(
                git,
                root.to_string_lossy().into_owned(),
                selected,
                ignore_whitespace,
                show_ignored,
                false,
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::SYSTEM_GIT;
    use crate::{test_support::TempRepo, theme::ThemeChoice};

    #[test]
    fn toggles_ranges_and_refresh_keep_selection_by_path() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
        app.changes = crate::mock_data::changes();
        let visible = (0..app.changes.len()).collect::<Vec<_>>();
        app.select_change(1, false, false, &visible);
        app.select_change(3, true, false, &visible);
        assert_eq!(app.selected_change_indices(), vec![1, 3]);
        app.select_change(1, true, false, &visible);
        assert_eq!(app.selected_change_indices(), vec![3]);
        app.select_change(3, true, false, &visible);
        assert!(app.selected_change_indices().is_empty());
        let visible = vec![0, 2, 4];
        app.select_change(0, false, false, &visible);
        app.select_change(4, false, true, &visible);
        assert_eq!(app.selected_change_indices(), visible);
        let paths = app.selection.paths.clone();
        app.changes.reverse();
        app.retain_change_selection();
        assert_eq!(app.selection.paths, paths);
        assert!(app.change_is_selected(app.selection.active));
        app.changes.clear();
        app.retain_change_selection();
        assert!(app.selected_change_indices().is_empty());
    }

    #[test]
    fn group_staging_preserves_other_files_and_working_tree() {
        let repo = TempRepo::new();
        for path in ["a", "b", "other"] {
            repo.write(path, "base\n");
        }
        git::commit(&SYSTEM_GIT, &repo.0, "base", git::CommitContents::All).unwrap();
        for path in ["a", "b", "other"] {
            repo.write(path, "edited\n");
        }
        git::set_staged_paths(&SYSTEM_GIT, &repo.0, &["a".into(), "b".into()], true).unwrap();
        assert_eq!(repo.git(&["diff", "--cached", "--name-only"]), "a\nb\n");
        git::set_staged_paths(&SYSTEM_GIT, &repo.0, &["a".into(), "b".into()], false).unwrap();
        assert!(repo.git(&["diff", "--cached"]).is_empty());
        for path in ["a", "b", "other"] {
            assert_eq!(
                std::fs::read_to_string(repo.0.join(path)).unwrap(),
                "edited\n"
            );
        }
        assert!(
            git::set_staged_paths(&SYSTEM_GIT, &repo.0, &["a".into(), "missing".into()], true)
                .is_err()
        );
        assert!(repo.git(&["diff", "--cached"]).is_empty());
    }
}
