use super::{SannyGitApp, WorkerResult};
use crate::{
    conflict::{self, Action, BlockEdit, Comparison, Review},
    git,
    models::ChangeKind,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    #[default]
    OursResult,
    ThreeWay,
    ResultTheirs,
    ResultBelow,
}

pub struct WindowGeometry {
    pub inner_size: eframe::egui::Vec2,
    pub outer_position: Option<eframe::egui::Pos2>,
    pub maximized: bool,
}

pub struct Dialog {
    pub root: PathBuf,
    pub path: String,
    pub action: Action,
    pub review: Option<Arc<Review>>,
    pub selected: usize,
    pub edits: Vec<Option<String>>,
    pub block_edits: Vec<BlockEdit>,
    pub conflict_undo: Vec<Option<ConflictUndo>>,
    pub draft: Option<String>,
    pub draft_ready: bool,
    pub cursor_position: Option<(usize, usize)>,
    pub manual_cursor_index: Option<usize>,
    pub comparison: Option<Comparison>,
    pub layout: Layout,
    pub show_base: bool,
    alternate_comparison: Option<Comparison>,
    alternate_scroll: [f32; 2],
    pub change_row: Option<usize>,
    pub jump_to_row: Option<usize>,
    pub scroll_y: [f32; 2],
    pub initial_window_geometry: Option<WindowGeometry>,
    pub revealed: bool,
    pub confirm_close: bool,
}

pub struct ConflictUndo {
    pub side: usize,
    previous: Option<String>,
}

impl Dialog {
    pub fn new(root: PathBuf, path: String, action: Action) -> Self {
        Self {
            root,
            path,
            action,
            review: None,
            selected: 0,
            edits: Vec::new(),
            block_edits: Vec::new(),
            conflict_undo: Vec::new(),
            draft: None,
            draft_ready: false,
            cursor_position: None,
            manual_cursor_index: None,
            comparison: None,
            layout: Layout::default(),
            show_base: false,
            alternate_comparison: None,
            alternate_scroll: [0.0; 2],
            change_row: None,
            jump_to_row: None,
            scroll_y: [0.0; 2],
            initial_window_geometry: None,
            revealed: false,
            confirm_close: false,
        }
    }

    pub fn set_review(&mut self, review: Arc<Review>) {
        self.edits.resize(review.conflicts.len(), None);
        self.cursor_position = review.text.as_ref().map(|_| (1, 1));
        self.conflict_undo
            .resize_with(review.conflicts.len(), || None);
        self.review = Some(review);
        self.update_comparison();
        self.go_to_conflict(0);
    }

    pub fn go_to_conflict(&mut self, index: usize) {
        self.selected = index;
        self.change_row = None;
        self.jump_to_row = self
            .comparison
            .as_ref()
            .and_then(|comparison| comparison.conflict_rows.get(index))
            .map(|rows| rows.start);
    }

    pub fn update_comparison(&mut self) {
        let Some(review) = &self.review else {
            return;
        };
        if let Some(text) = &review.text {
            self.comparison = Some(Comparison::new(
                text,
                &review.conflicts,
                &self.edits,
                git_sides(review),
                &self.block_edits,
            ));
        }
    }

    pub fn toggle_base(&mut self) {
        if self.draft.is_some() {
            return;
        }
        if self.alternate_comparison.is_none() {
            let Some(review) = &self.review else {
                return;
            };
            let Some(base) = review.versions[0].as_deref() else {
                return;
            };
            self.alternate_comparison =
                Some(Comparison::new(base, &[], &[], git_sides(review), &[]));
        }
        std::mem::swap(&mut self.comparison, &mut self.alternate_comparison);
        std::mem::swap(&mut self.scroll_y, &mut self.alternate_scroll);
        self.show_base = !self.show_base;
        self.cursor_position = Some((1, 1));
        self.change_row = None;
        self.jump_to_row = None;
    }

    pub fn take_conflict(&mut self, index: usize, side: usize) {
        self.take_conflict_sides(index, side, false);
    }

    pub fn take_conflict_sides(&mut self, index: usize, side: usize, both: bool) {
        let Some(review) = &self.review else {
            return;
        };
        let Some((text, block)) = review.text.as_ref().zip(review.conflicts.get(index)) else {
            return;
        };
        let sides = [&text[block.ours.clone()], &text[block.theirs.clone()]];
        let mut replacement = sides[side].to_owned();
        if both {
            replacement.push_str(sides[1 - side]);
        }
        let previous = self.edits[index].replace(replacement);
        self.conflict_undo[index] = Some(ConflictUndo { side, previous });
        self.selected = index;
        self.change_row = None;
        self.update_comparison();
    }

    pub fn undo_conflict(&mut self, index: usize) {
        if let Some(undo) = self.conflict_undo.get_mut(index).and_then(Option::take) {
            self.edits[index] = undo.previous;
            self.selected = index;
            self.change_row = None;
            self.update_comparison();
        }
    }

    pub fn apply_block(&mut self, edit: BlockEdit) {
        if !self.can_edit_block(&edit.original) {
            return;
        }
        let index = self
            .block_edits
            .partition_point(|other| other.original.start < edit.original.start);
        self.block_edits.insert(index, edit);
        self.change_row = None;
        self.update_comparison();
    }

    pub fn can_edit_block(&self, span: &std::ops::Range<usize>) -> bool {
        !self.block_edits.iter().any(|edit| {
            let existing = &edit.original;
            if span.is_empty() || existing.is_empty() {
                existing.start <= span.end && span.start <= existing.end
            } else {
                existing.start < span.end && span.start < existing.end
            }
        })
    }

    pub fn undo_block(&mut self, index: usize) {
        if index < self.block_edits.len() {
            self.block_edits.remove(index);
            self.change_row = None;
            self.update_comparison();
        }
    }

    pub fn ready(&self) -> bool {
        !self.show_base
            && self.review.is_some()
            && (self.action != Action::Edit
                || if self.draft.is_some() {
                    self.draft_ready
                } else {
                    self.comparison.is_some() && self.edits.iter().all(Option::is_some)
                })
    }

    pub fn dirty(&self) -> bool {
        !self.block_edits.is_empty()
            || self.edits.iter().any(Option::is_some)
            || self
                .draft
                .as_ref()
                .zip(self.comparison.as_ref())
                .is_some_and(|(draft, comparison)| draft != &comparison.result)
    }
}

fn git_sides(review: &Review) -> [Option<&str>; 2] {
    [2, 3].map(|stage| {
        if review.stages[stage] {
            review.versions[stage - 1].as_deref()
        } else {
            Some("")
        }
    })
}

impl SannyGitApp {
    pub fn conflict_solver_open(&self) -> bool {
        self.conflict_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.action == Action::Edit)
    }

    pub fn can_open_conflict(&self, index: usize, action: Action) -> bool {
        !self.busy()
            && self.conflict_dialog.is_none()
            && self.state_current
            && self.selected_change_indices().len() == 1
            && self.changes.get(index).is_some_and(|change| {
                change.kind == ChangeKind::Conflicted
                    || (action == Action::Recreate && change.staged)
            })
    }

    pub fn open_conflict(&mut self, index: usize, action: Action) {
        if !self.can_open_conflict(index, action) {
            return;
        }
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let root = PathBuf::from(&repository.path);
        let path = self.changes[index].path.clone();
        let reviewed_path = path.clone();
        let reviewed_root = root.clone();
        if self.start_worker(move || {
            WorkerResult::ConflictReview(
                conflict::prepare(&reviewed_root, &reviewed_path, action).map(Box::new),
            )
        }) {
            self.conflict_dialog = Some(Dialog::new(root, path, action));
        }
    }

    pub(super) fn finish_conflict_review(&mut self, result: Result<Box<Review>, String>) {
        let Some(dialog) = &mut self.conflict_dialog else {
            return;
        };
        match result {
            Ok(review) => {
                dialog.set_review(Arc::from(review));
            }
            Err(error) => {
                self.conflict_dialog = None;
                self.error = Some(error);
            }
        }
    }

    pub fn apply_conflict(&mut self) {
        if self.busy() {
            return;
        }
        let (Some(dialog), Some(repository)) = (
            &self.conflict_dialog,
            self.repositories.get(self.selected_repository),
        ) else {
            return;
        };
        let Some(review) = dialog.review.clone() else {
            return;
        };
        if !dialog.ready() || dialog.root != std::path::Path::new(&repository.path) {
            return;
        }
        let action = dialog.action;
        let draft = if action == Action::Edit {
            dialog.draft.clone().or_else(|| {
                dialog
                    .comparison
                    .as_ref()
                    .map(|comparison| comparison.result.clone())
            })
        } else {
            None
        };
        let root = PathBuf::from(&repository.path);
        let path = dialog.path.clone();
        self.start_worker(move || {
            let outcome = conflict::apply(&root, &path, action, &review, draft);
            WorkerResult::Operation {
                committed: false,
                success: if action == Action::Recreate {
                    "Conflict recreated"
                } else {
                    "Conflict resolved"
                },
                outcome,
                snapshot: git::load(&root),
            }
        });
    }
}
