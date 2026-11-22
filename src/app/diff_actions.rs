//! Stage, unstage, and discard reviewed diff lines, and rename one file.
use super::{SannyGitApp, WorkerResult};
use crate::diff_edit::{self, DiffEdit};
use crate::models::{ChangeKind, DiffLineKind};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

#[derive(Clone)]
pub struct RenamePathDialog {
    pub root: String,
    pub from: String,
    pub to: String,
    pub error: Option<String>,
}

pub fn destination_error(to: &str) -> Option<&'static str> {
    if to.is_empty() || to.trim().is_empty() {
        return Some("Enter a destination path.");
    }
    if to.contains('\0') || to.len() > 4_096 {
        return Some("Use a path without NUL characters and no longer than 4,096 bytes.");
    }
    // `Path::components` drops `.`, so a raw segment check keeps `a/./b` out.
    let dot_or_empty = to
        .split(['/', '\\'])
        .any(|segment| segment.is_empty() || segment == "." || segment == "..");
    if dot_or_empty
        || Path::new(to)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Some("Enter a relative path inside the repository.");
    }
    None
}

fn edit_success(edit: DiffEdit, hunk: bool) -> &'static str {
    match (edit, hunk) {
        (DiffEdit::Stage, true) => "Hunk staged",
        (DiffEdit::Stage, false) => "Lines staged",
        (DiffEdit::Unstage, true) => "Hunk unstaged",
        (DiffEdit::Unstage, false) => "Lines unstaged",
        (DiffEdit::Discard, true) => "Hunk discarded",
        (DiffEdit::Discard, false) => "Lines discarded",
    }
}

impl SannyGitApp {
    pub fn diff_selection_is_hunk(&self) -> bool {
        let Some(preview) = self.selected_diff() else {
            return false;
        };
        selection_is_hunk(&preview.lines, &self.diff_line_selection)
    }

    pub fn diff_edit_blocker(&self, edit: DiffEdit) -> Option<String> {
        if self.busy() {
            return Some("Wait for the current operation to finish.".into());
        }
        if !self.state_current || self.repositories.get(self.selected_repository).is_none() {
            return Some("Refresh the working tree before editing lines.".into());
        }
        if self.preferences.diff_ignore_whitespace {
            return Some("Turn off Ignore Whitespace to stage or discard individual lines.".into());
        }
        if self.merging && edit != DiffEdit::Stage {
            return Some(
                "During a merge, stage individual lines or discard the whole file from the review dialog.".into(),
            );
        }
        let Some(change) = self.changes.get(self.selection.active) else {
            return Some("Select a file.".into());
        };
        if change.kind == ChangeKind::Conflicted || change.original_path.is_some() {
            return Some(
                "Stage, unstage, or discard this file as a whole. Line edits do not apply to conflicts or renames.".into(),
            );
        }
        let Some(preview) = self.selected_diff() else {
            return Some("Wait for the diff to finish loading.".into());
        };
        if self.diff_line_selection.is_empty() {
            return Some("Select one or more changed lines.".into());
        }
        diff_edit::partial_patch(&preview.lines, &self.diff_line_selection, edit).err()
    }

    pub fn open_diff_edit(&mut self, edit: DiffEdit) {
        let request = match self.prepare_diff_request(edit) {
            Ok(request) => request,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        if edit == DiffEdit::Discard {
            self.diff_discard = Some(request);
            return;
        }
        self.run_diff_edit(request);
    }

    pub fn reload_selected_diff(&mut self) {
        self.diff_line_selection.clear();
        self.diff_refresh_pending = true;
    }

    pub fn confirm_diff_discard(&mut self) {
        let Some(request) = self.diff_discard.take() else {
            return;
        };
        self.run_diff_edit(request);
    }

    fn prepare_diff_request(&self, edit: DiffEdit) -> Result<crate::hunk::Request, String> {
        if let Some(reason) = self.diff_edit_blocker(edit) {
            return Err(reason);
        }
        let change = &self.changes[self.selection.active];
        let selected: Vec<_> = self.diff_line_selection.iter().copied().collect();
        crate::hunk::prepare(
            &change.path,
            change.kind,
            change.original_path.clone(),
            &change.diff,
            &selected,
            edit,
        )
    }

    fn run_diff_edit(&mut self, request: crate::hunk::Request) {
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let root = PathBuf::from(&repository.path);
        let success = edit_success(request.edit, self.request_is_hunk(&request));
        self.state_current = false;
        self.error = None;
        self.status = "Editing lines…".into();
        self.start_worker(move |git| {
            let outcome = crate::hunk::apply(git, &root, &request);
            WorkerResult::Operation {
                committed: false,
                success,
                outcome,
                snapshot: crate::git::load(git, &root),
            }
        });
    }

    fn request_is_hunk(&self, request: &crate::hunk::Request) -> bool {
        let Some(preview) = self.selected_diff() else {
            return false;
        };
        let selected: BTreeSet<_> = request.selected.iter().copied().collect();
        selection_is_hunk(&preview.lines, &selected)
    }

    pub fn rename_path_allowed(&self) -> bool {
        let selected = self.selected_change_indices();
        if selected.len() != 1
            || self.busy()
            || !self.state_current
            || self.repositories.get(self.selected_repository).is_none()
        {
            return false;
        }
        self.changes.get(selected[0]).is_some_and(|change| {
            !matches!(change.kind, ChangeKind::Deleted | ChangeKind::Conflicted)
        })
    }

    pub fn open_rename_dialog(&mut self) {
        if !self.rename_path_allowed() {
            return;
        }
        let index = self.selected_change_indices()[0];
        let from = self.changes[index].path.clone();
        let root = self.repositories[self.selected_repository].path.clone();
        self.rename_dialog = Some(RenamePathDialog {
            root,
            from: from.clone(),
            to: from,
            error: None,
        });
    }

    pub fn confirm_rename_path(&mut self) {
        let Some(dialog) = self.rename_dialog.clone() else {
            return;
        };
        if let Some(error) = destination_error(&dialog.to) {
            if let Some(dialog) = &mut self.rename_dialog {
                dialog.error = Some(error.to_owned());
            }
            return;
        }
        if dialog.to == dialog.from {
            if let Some(dialog) = &mut self.rename_dialog {
                dialog.error = Some("Choose a different path.".into());
            }
            return;
        }
        let root = PathBuf::from(dialog.root);
        let from = dialog.from;
        let to = dialog.to;
        self.rename_dialog = None;
        self.state_current = false;
        self.error = None;
        self.status = "Renaming file…".into();
        self.start_worker(move |git| {
            let outcome = crate::git::move_path(git, &root, &from, &to);
            WorkerResult::Operation {
                committed: false,
                success: "File renamed",
                outcome,
                snapshot: crate::git::load(git, &root),
            }
        });
    }
}

fn selection_is_hunk(lines: &[crate::models::DiffLine], selected: &BTreeSet<usize>) -> bool {
    !selected.is_empty()
        && lines.iter().enumerate().any(|(index, line)| {
            line.kind == DiffLineKind::Hunk && diff_edit::hunk_changes(lines, index) == *selected
        })
}

#[cfg(test)]
mod tests {
    use super::destination_error;

    #[test]
    fn destination_rejects_empty_parent_and_absolute_paths() {
        assert!(destination_error("").is_some());
        assert!(destination_error("   ").is_some());
        assert!(destination_error("a/../b").is_some());
        assert!(destination_error("/tmp/file").is_some());
        assert!(destination_error("a/./b").is_some());
        assert!(destination_error(&"a".repeat(4_097)).is_some());
        assert!(destination_error("notes/new file.txt").is_none());
    }
}
