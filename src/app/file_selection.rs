use super::{SannyGitApp, WorkerResult};
use crate::{git, models::ChangeKind};
use std::path::PathBuf;

impl SannyGitApp {
    pub fn change_is_selected(&self, index: usize) -> bool {
        self.changes.get(index).is_some_and(|change| {
            if self.selected_paths.is_empty() {
                self.selected_change == index
            } else {
                self.selected_paths.contains(&change.path)
            }
        })
    }

    pub fn selected_change_indices(&self) -> Vec<usize> {
        (0..self.changes.len())
            .filter(|&index| self.change_is_selected(index))
            .collect()
    }

    pub fn select_change(&mut self, index: usize, toggle: bool, extend: bool, visible: &[usize]) {
        let Some(change) = self.changes.get(index) else {
            return;
        };
        let path = change.path.clone();
        if self.selected_paths.is_empty() {
            if let Some(active) = self.changes.get(self.selected_change) {
                self.selected_paths.insert(active.path.clone());
            }
        }
        if extend {
            if self
                .selection_anchor
                .as_ref()
                .is_none_or(|path| !visible.iter().any(|&i| self.changes[i].path == *path))
            {
                self.selection_anchor = self
                    .changes
                    .get(self.selected_change)
                    .map(|change| change.path.clone());
            }
            let anchor = self
                .selection_anchor
                .as_ref()
                .and_then(|path| visible.iter().position(|&i| self.changes[i].path == *path))
                .or_else(|| visible.iter().position(|&i| i == self.selected_change));
            if !toggle {
                self.selected_paths.clear();
            }
            if let (Some(start), Some(end)) = (anchor, visible.iter().position(|&i| i == index)) {
                for &i in &visible[start.min(end)..=start.max(end)] {
                    self.selected_paths.insert(self.changes[i].path.clone());
                }
            } else {
                self.selected_paths.insert(path.clone());
            }
        } else if toggle {
            if !self.selected_paths.remove(&path) {
                self.selected_paths.insert(path.clone());
            }
            self.selection_anchor = Some(path.clone());
        } else {
            self.selected_paths.clear();
            self.selected_paths.insert(path.clone());
            self.selection_anchor = Some(path.clone());
        }
        self.selected_change = if self.selected_paths.contains(&path) {
            index
        } else {
            self.changes
                .iter()
                .position(|change| self.selected_paths.contains(&change.path))
                .unwrap_or(usize::MAX)
        };
    }

    pub(super) fn retain_change_selection(&mut self) {
        self.selected_paths
            .retain(|path| self.changes.iter().any(|change| &change.path == path));
        if self
            .selection_anchor
            .as_ref()
            .is_some_and(|path| !self.changes.iter().any(|change| &change.path == path))
        {
            self.selection_anchor = None;
        }
        if !self.selected_paths.is_empty()
            && !self
                .changes
                .get(self.selected_change)
                .is_some_and(|change| self.selected_paths.contains(&change.path))
        {
            self.selected_change = self
                .changes
                .iter()
                .position(|change| self.selected_paths.contains(&change.path))
                .unwrap_or(usize::MAX);
        }
    }

    pub fn selected_staging_allowed(&self, staged: bool) -> bool {
        let selected = self.selected_change_indices();
        !selected.is_empty()
            && !self.busy()
            && self.state_current
            && self.repositories.get(self.selected_repository).is_some()
            && selected
                .iter()
                .all(|&i| self.changes[i].kind != ChangeKind::Conflicted)
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
        self.start_worker(move || WorkerResult::Operation {
            committed: false,
            success: if staged {
                "File staged"
            } else {
                "Index updated"
            },
            outcome: git::set_staged_paths(&root, &paths, staged),
            snapshot: git::load(&root),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let paths = app.selected_paths.clone();
        app.changes.reverse();
        app.retain_change_selection();
        assert_eq!(app.selected_paths, paths);
        assert!(app.change_is_selected(app.selected_change));
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
        git::commit(&repo.0, "base", git::CommitContents::All).unwrap();
        for path in ["a", "b", "other"] {
            repo.write(path, "edited\n");
        }
        git::set_staged_paths(&repo.0, &["a".into(), "b".into()], true).unwrap();
        assert_eq!(repo.git(&["diff", "--cached", "--name-only"]), "a\nb\n");
        git::set_staged_paths(&repo.0, &["a".into(), "b".into()], false).unwrap();
        assert!(repo.git(&["diff", "--cached"]).is_empty());
        for path in ["a", "b", "other"] {
            assert_eq!(
                std::fs::read_to_string(repo.0.join(path)).unwrap(),
                "edited\n"
            );
        }
        assert!(git::set_staged_paths(&repo.0, &["a".into(), "missing".into()], true).is_err());
        assert!(repo.git(&["diff", "--cached"]).is_empty());
    }
}
