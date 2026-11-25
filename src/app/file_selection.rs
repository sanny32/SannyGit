use super::{
    working_tree::{FileDiff, Update},
    SannyGitApp, WorkerResult,
};
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
        let active_path = changes.get(self.active).map(|change| change.path.clone());
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
            self.active = active_path
                .as_ref()
                .filter(|path| self.paths.contains(*path))
                .and_then(|path| changes.iter().position(|change| &change.path == path))
                .or_else(|| {
                    changes
                        .iter()
                        .position(|change| self.paths.contains(&change.path))
                })
                .unwrap_or(usize::MAX);
        }
    }
}

struct IndexRow {
    path: String,
    kind: ChangeKind,
    original_path: Option<String>,
}

fn index_rows(changes: &[crate::models::ChangeFile], paths: &[String]) -> Vec<IndexRow> {
    changes
        .iter()
        .filter(|row| paths.iter().any(|path| path == &row.path))
        .map(|row| IndexRow {
            path: row.path.clone(),
            kind: row.kind,
            original_path: row.original_path.clone(),
        })
        .collect()
}

/// Rows the file list can show after one unstage command, before the next status read.
fn preview_unstage(rows: &[IndexRow], paths: &[String]) -> Vec<crate::models::ChangeFile> {
    let mut changes = Vec::new();
    for path in paths {
        let Some(row) = rows.iter().find(|row| row.path == *path) else {
            continue;
        };
        let mut push = |path: String, kind: ChangeKind| {
            changes.push(crate::models::ChangeFile {
                path,
                original_path: None,
                kind,
                conflict_status: None,
                staged: false,
                unstaged: true,
                diff: String::new(),
            });
        };
        match row.kind {
            ChangeKind::Renamed => {
                if let Some(source) = &row.original_path {
                    push(source.clone(), ChangeKind::Deleted);
                }
                push(path.clone(), ChangeKind::Untracked);
            }
            ChangeKind::Added => push(path.clone(), ChangeKind::Untracked),
            ChangeKind::Deleted => push(path.clone(), ChangeKind::Deleted),
            _ => push(path.clone(), ChangeKind::Modified),
        }
    }
    changes
}

impl SannyGitApp {
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
        if self.start_background_operation(move |git| WorkerResult::Operation {
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
        self.update_index(paths, staged);
    }

    pub(super) fn update_index(&mut self, paths: Vec<String>, staged: bool) {
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let mut refresh_paths = paths.clone();
        if !staged {
            // Unstaging a rename can recreate a separate row for its source.
            refresh_paths.extend(
                self.changes
                    .iter()
                    .filter(|row| row.kind == ChangeKind::Renamed && paths.contains(&row.path))
                    .filter_map(|row| row.original_path.clone()),
            );
            refresh_paths.sort_unstable();
            refresh_paths.dedup();
        }
        self.error = None;
        self.status = if staged { "Staging?" } else { "Unstaging?" }.into();
        let selected = self
            .changes
            .get(self.selection.active)
            .map(|c| c.path.clone());
        let ignore_whitespace = self.preferences.diff_ignore_whitespace;
        let show_ignored = self.preferences.show_ignored_files;
        // The file list already shows this HEAD. Unstage does not ask Git again.
        let has_head = !self.branches.head_commit.is_empty();
        let rows = index_rows(&self.changes, &paths);
        let (sender, receiver) = std::sync::mpsc::channel();
        let git = GitContext::new(self.preferences.git_executable.clone());
        let repaint = self.index_repaint.clone();
        let worker = std::thread::Builder::new()
            .name("sannygit-index".into())
            .spawn(move || {
                let publish = |result| {
                    if sender.send(result).is_err() {
                        return false;
                    }
                    if let Some(context) = &repaint {
                        context.request_repaint();
                    }
                    true
                };
                let outcome = if staged {
                    git::set_staged_paths(&git, &root, &paths, true)
                } else {
                    git::unstage_paths(&git, &root, &refresh_paths, has_head)
                };
                if !staged {
                    if outcome.is_ok() {
                        let changes = preview_unstage(&rows, &paths);
                        let selected_change = changes
                            .iter()
                            .find(|change| Some(&change.path) == selected.as_ref())
                            .map(|change| (change.kind, change.original_path.clone()));
                        if !publish(WorkerResult::IndexPathsUpdated {
                            staged,
                            paths: refresh_paths,
                            changes,
                        }) {
                            return;
                        }
                        if let (Some(path), Some((kind, original_path))) =
                            (&selected, selected_change)
                        {
                            let diff = FileDiff::read(
                                &git,
                                &root,
                                path,
                                kind,
                                original_path.as_deref(),
                                ignore_whitespace,
                            );
                            if diff.succeeded()
                                && !publish(WorkerResult::IndexDiffUpdated {
                                    diff,
                                    ignore_whitespace,
                                })
                            {
                                return;
                            }
                        }
                    }
                    publish(WorkerResult::IndexFinished { staged, outcome });
                    return;
                }
                let mut early_diff = None;
                let scoped_changes = outcome
                    .is_ok()
                    .then(|| git::load_path_changes(&git, &root, &refresh_paths).ok())
                    .flatten();
                if let Some(changes) = scoped_changes {
                    let selected_change = changes
                        .iter()
                        .find(|change| Some(&change.path) == selected.as_ref())
                        .map(|change| (change.kind, change.original_path.clone()));
                    if !publish(WorkerResult::IndexPathsUpdated {
                        staged,
                        paths: refresh_paths,
                        changes,
                    }) {
                        return;
                    }
                    if let (Some(path), Some((kind, original_path))) = (&selected, selected_change)
                    {
                        let diff = FileDiff::read(
                            &git,
                            &root,
                            path,
                            kind,
                            original_path.as_deref(),
                            ignore_whitespace,
                        );
                        if diff.succeeded() {
                            if !publish(WorkerResult::IndexDiffUpdated {
                                diff,
                                ignore_whitespace,
                            }) {
                                return;
                            }
                            early_diff = Some((kind, original_path));
                        }
                    }
                }
                let mut update = Update::read_status(
                    &git,
                    root.to_string_lossy().into_owned(),
                    show_ignored,
                    false,
                );
                // Full status can discover a rename outside the selected paths.
                update.read_diff(
                    &git,
                    selected.as_deref(),
                    ignore_whitespace,
                    early_diff
                        .as_ref()
                        .map(|(kind, path)| (*kind, path.as_deref())),
                );
                publish(WorkerResult::Index {
                    staged,
                    outcome,
                    update,
                });
            });
        match worker {
            Ok(_) => {
                // Keep immutable previews and history reads while replacing stale working-tree reads.
                self.working_tree.invalidate();
                self.diff_worker = None;
                self.state_current = false;
                self.worker = Some(receiver);
            }
            Err(error) => {
                self.error = Some(format!("Cannot start index worker: {error}"));
                self.status = "Operation failed".into();
            }
        }
    }

    pub(super) fn show_updated_index_paths(
        &mut self,
        staged: bool,
        paths: Vec<String>,
        changes: Vec<crate::models::ChangeFile>,
    ) {
        let paths: std::collections::HashSet<_> = paths.iter().map(String::as_str).collect();
        let mut changed: std::collections::HashMap<_, _> = changes
            .into_iter()
            .map(|change| (change.path.clone(), change))
            .collect();
        for row in &mut self.changes {
            if let Some(change) = changed.remove(&row.path) {
                row.staged = change.staged;
                row.unstaged = change.unstaged;
                row.kind = change.kind;
                row.original_path = change.original_path;
                row.conflict_status = change.conflict_status;
            } else if paths.contains(row.path.as_str()) {
                // A path may become clean when its index entry matches HEAD.
                row.staged = false;
                row.unstaged = false;
            }
        }
        let mut new_rows: Vec<_> = changed.into_values().collect();
        new_rows.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        self.changes.extend(new_rows);
        self.status = if staged {
            "File staged"
        } else {
            "Index updated"
        }
        .into();
        if staged
            && self.preferences.staged_only_after_staging
            && self.changes.iter().any(|row| row.staged)
        {
            self.commit.contents = git::CommitContents::Staged;
        }
        self.update_commit_contents_after_refresh(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::SYSTEM_GIT;
    use crate::{test_support::TempRepo, theme::ThemeChoice};
    use std::path::PathBuf;

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
    fn confirmed_stage_updates_rows_before_full_refresh() {
        let repo = TempRepo::new();
        repo.write("reverted", "base\n");
        git::commit(&SYSTEM_GIT, &repo.0, "base", git::CommitContents::All).unwrap();
        repo.write("reverted", "staged\n");
        repo.git(&["add", "reverted"]);
        repo.write("reverted", "base\n");
        repo.write("new", "new\n");

        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(git::load(&SYSTEM_GIT, &repo.0).unwrap());
        let visible = (0..app.changes.len()).collect::<Vec<_>>();
        app.select_change(visible[0], false, false, &visible);
        app.select_change(visible[1], true, false, &visible);
        app.set_selected_staged(true);
        assert_eq!(app.status, "Staging?");
        assert!(app
            .changes
            .iter()
            .any(|row| row.path == "new" && !row.staged));

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while app.status == "Staging?" {
            app.poll_worker();
            assert!(
                std::time::Instant::now() < deadline,
                "Stage confirmation timed out"
            );
            std::thread::yield_now();
        }
        assert_eq!(app.status, "File staged");
        assert!(app.busy(), "Full refresh is still pending");
        assert!(!app.state_current);
        assert!(app
            .changes
            .iter()
            .any(|row| row.path == "new" && row.staged && !row.unstaged));
        assert!(app
            .changes
            .iter()
            .any(|row| row.path == "reverted" && !row.staged));
        while app.busy() {
            app.poll_worker();
            assert!(
                std::time::Instant::now() < deadline,
                "Stage refresh timed out"
            );
            std::thread::yield_now();
        }
        assert!(app.state_current);
        assert!(app.error.is_none());
        assert_eq!(app.changes.len(), 1);
        assert_eq!(app.changes[0].path, "new");
    }

    #[test]
    fn confirmed_unstage_splits_a_rename_before_full_refresh() {
        let repo = TempRepo::new();
        repo.write("old", "base\n");
        git::commit(&SYSTEM_GIT, &repo.0, "base", git::CommitContents::All).unwrap();
        repo.git(&["mv", "old", "new"]);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(git::load(&SYSTEM_GIT, &repo.0).unwrap());
        assert_eq!(app.changes[0].kind, ChangeKind::Renamed);

        app.set_staged(0, false);
        assert_eq!(app.status, "Unstaging?");
        assert!(app.changes[0].staged);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while app.status == "Unstaging?" {
            app.poll_worker();
            assert!(
                std::time::Instant::now() < deadline,
                "Unstage confirmation timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.status, "Index updated");
        assert!(app.busy(), "Full refresh is still pending");
        assert!(app.changes.iter().all(|row| !row.staged && row.unstaged));
        assert!(app
            .changes
            .iter()
            .any(|row| row.path == "old" && row.kind == ChangeKind::Deleted));
        assert!(app
            .changes
            .iter()
            .any(|row| row.path == "new" && row.kind == ChangeKind::Untracked));
        assert!(repo.git(&["diff", "--cached", "--name-only"]).is_empty());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("new")).unwrap(),
            "base\n"
        );
    }

    #[test]
    fn unstage_before_first_commit_keeps_the_worktree_file() {
        let repo = TempRepo::new();
        repo.write("new", "contents\n");
        repo.git(&["add", "--", "new"]);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(git::load(&SYSTEM_GIT, &repo.0).unwrap());
        assert!(app.branches.head_commit.is_empty());
        app.set_staged(0, false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while app.status == "Unstaging?" {
            app.poll_worker();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.status, "Index updated");
        assert!(app.changes.iter().any(|row| {
            row.path == "new"
                && row.kind == ChangeKind::Untracked
                && !row.staged
                && row.unstaged
        }));
        assert_eq!(
            std::fs::read_to_string(repo.0.join("new")).unwrap(),
            "contents\n"
        );
        assert!(repo.git(&["diff", "--cached", "--name-only"]).is_empty());
    }

    #[test]
    fn unstage_checkbox_does_not_wait_for_status() {
        let repo = TempRepo::new();
        repo.write("selected", "base\n");
        repo.write("other", "base\n");
        git::commit(&SYSTEM_GIT, &repo.0, "base", git::CommitContents::All).unwrap();
        repo.write("selected", "edited\n");
        repo.write("other", "edited\n");
        repo.git(&["add", "--", "selected", "other"]);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(git::load(&SYSTEM_GIT, &repo.0).unwrap());
        let root = PathBuf::from(&app.repositories[app.selected_repository].path);
        let counter = crate::test_support::GitCommandCount::new(&root);
        let selected = app
            .changes
            .iter()
            .position(|row| row.path == "selected")
            .unwrap();
        let other = app
            .changes
            .iter()
            .position(|row| row.path == "other")
            .unwrap();
        app.selection.active = selected;
        app.selection.paths.clear();
        app.selection.paths.insert("selected".into());
        app.set_staged(other, false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while app.busy() {
            assert!(std::time::Instant::now() < deadline);
            app.poll_worker();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.state_current);
        assert!(app.changes.iter().any(|row| row.path == "other" && !row.staged));
        assert_eq!(counter.get(), 1, "an unselected unstage is one Git command");

        let before = counter.get();
        app.set_staged(selected, false);
        while app.busy() {
            assert!(std::time::Instant::now() < deadline);
            app.poll_worker();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(
            counter.get() - before,
            3,
            "a selected modified unstage is reset plus two diffs"
        );
        assert!(app
            .changes
            .iter()
            .any(|row| row.path == "selected" && !row.staged && row.unstaged));
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
