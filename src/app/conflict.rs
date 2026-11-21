use super::{SannyGitApp, WorkerResult};
use crate::{
    conflict::{self, Action, BlockEdit, Comparison, Review},
    git,
    models::ChangeKind,
};
use std::{ops::Range, path::PathBuf, sync::Arc};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneCaret {
    pub column: usize,
    pub row: usize,
    pub character: usize,
}

pub struct Dialog {
    pub root: PathBuf,
    pub path: String,
    pub action: Action,
    pub review: Option<Arc<Review>>,
    pub selected: Option<usize>,
    pub edits: Vec<Option<String>>,
    pub block_edits: Vec<BlockEdit>,
    pub conflict_undo: Vec<Option<ConflictUndo>>,
    pub draft: Option<String>,
    pub draft_comparison: Option<Comparison>,
    pub draft_undo: Vec<DraftUndo>,
    pub draft_recovered_spans: Vec<Option<Range<usize>>>,
    pub draft_selection: Option<Range<usize>>,
    pub draft_navigation_offset: Option<f32>,
    pub draft_ready: bool,
    pub cursor_position: Option<(usize, usize)>,
    pub pane_caret: Option<PaneCaret>,
    pub manual_edit_start: Option<usize>,
    pub manual_cursor: Option<(usize, usize)>,
    pub comparison: Option<Comparison>,
    pub layout: Layout,
    pub show_base: bool,
    alternate_comparison: Option<Comparison>,
    alternate_scroll: [f32; 2],
    pub change_row: Option<usize>,
    pub conflict_navigation_row: Option<usize>,
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

pub struct DraftUndo {
    pub span: Range<usize>,
    pub side: usize,
    previous: String,
}

impl Dialog {
    pub fn new(root: PathBuf, path: String, action: Action) -> Self {
        Self {
            root,
            path,
            action,
            review: None,
            selected: None,
            edits: Vec::new(),
            block_edits: Vec::new(),
            conflict_undo: Vec::new(),
            draft: None,
            draft_comparison: None,
            draft_undo: Vec::new(),
            draft_recovered_spans: Vec::new(),
            draft_selection: None,
            draft_navigation_offset: None,
            draft_ready: false,
            cursor_position: None,
            pane_caret: None,
            manual_edit_start: None,
            manual_cursor: None,
            comparison: None,
            layout: Layout::default(),
            show_base: false,
            alternate_comparison: None,
            alternate_scroll: [0.0; 2],
            change_row: None,
            conflict_navigation_row: None,
            jump_to_row: None,
            scroll_y: [0.0; 2],
            initial_window_geometry: None,
            revealed: false,
            confirm_close: false,
        }
    }

    pub fn set_review(&mut self, review: Arc<Review>) {
        self.edits = review
            .blocks()
            .iter()
            .map(|block| {
                review
                    .text
                    .as_deref()
                    .filter(|_| review.has_recovered_conflicts())
                    .map(|text| text[block.span.clone()].to_owned())
            })
            .collect();
        self.cursor_position = review.text.as_ref().map(|_| (1, 1));
        self.conflict_undo
            .resize_with(review.blocks().len(), || None);
        self.review = Some(review);
        self.update_comparison();
    }

    pub fn go_to_conflict(&mut self, index: usize) {
        self.selected = Some(index);
        self.change_row = None;
        self.jump_to_row = self
            .comparison
            .as_ref()
            .and_then(|comparison| comparison.conflict_rows.get(index))
            .map(|rows| rows.start);
        self.conflict_navigation_row = self.jump_to_row;
    }

    pub fn update_comparison(&mut self) {
        self.conflict_navigation_row = None;
        let Some(review) = &self.review else {
            return;
        };
        if let Some(text) = &review.text {
            let mut sources = git_sides(review);
            if review.has_recovered_conflicts() {
                // The side ranges for these blocks refer to the merge snapshot.
                sources = sources.map(|source| source.or(Some(text)));
            }
            self.comparison = Some(Comparison::new(
                text,
                review.blocks(),
                &self.edits,
                sources,
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
        self.pane_caret = None;
        self.change_row = None;
        self.conflict_navigation_row = None;
        self.jump_to_row = None;
    }

    pub fn take_conflict(&mut self, index: usize, side: usize) {
        self.take_conflict_sides(index, side, false);
    }

    pub fn take_conflict_sides(&mut self, index: usize, side: usize, both: bool) {
        let Some(review) = &self.review else {
            return;
        };
        let Some(sides) = review.conflict_sides(index) else {
            return;
        };
        let recovered = review.has_recovered_conflicts();
        let replacement = conflict::take_sides(sides, side, both);
        if self.edits[index].as_deref() == Some(&replacement) {
            return;
        }
        let previous = self.edits[index].replace(replacement);
        self.conflict_undo[index] = Some(ConflictUndo { side, previous });
        self.selected = Some(index);
        self.change_row = None;
        self.update_comparison();
        if recovered {
            self.go_to_conflict(index);
        }
    }

    pub fn undo_conflict(&mut self, index: usize) {
        if let Some(undo) = self.conflict_undo.get_mut(index).and_then(Option::take) {
            self.edits[index] = undo.previous;
            self.selected = Some(index);
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

    pub fn begin_draft(&mut self, character: usize) {
        let (Some(review), Some(comparison)) = (&self.review, &self.comparison) else {
            return;
        };
        let Some(text) = review.text.as_deref() else {
            return;
        };
        self.draft_undo.clear();
        self.draft_selection = None;
        self.draft_navigation_offset = None;
        self.draft_recovered_spans = if review.has_recovered_conflicts() {
            comparison
                .conflict_spans
                .iter()
                .cloned()
                .map(Some)
                .collect()
        } else {
            Vec::new()
        };
        for (edit, rows) in self.block_edits.iter().zip(&comparison.applied_rows) {
            let start = comparison.result_offset_at(rows.start);
            let span = start..start + edit.replacement.len();
            if comparison.result.get(span.clone()) != Some(&edit.replacement) {
                continue;
            }
            self.draft_undo.push(DraftUndo {
                span,
                side: edit.side,
                previous: text[edit.original.clone()].to_owned(),
            });
        }
        for (index, undo) in self.conflict_undo.iter().enumerate() {
            if let (Some(undo), Some(replacement)) = (undo, &self.edits[index]) {
                let start = comparison.result_offset_at(comparison.conflict_rows[index].start);
                let span = start..start + replacement.len();
                if comparison.result.get(span.clone()) != Some(replacement) {
                    continue;
                }
                let previous = undo
                    .previous
                    .as_deref()
                    .unwrap_or_else(|| &text[review.blocks()[index].span.clone()]);
                self.draft_undo.push(DraftUndo {
                    span,
                    side: undo.side,
                    previous: previous.to_owned(),
                });
            }
        }
        self.draft = Some(comparison.result.clone());
        self.manual_edit_start = Some(character);
        self.manual_cursor = None;
        self.rebuild_draft_comparison();
    }

    pub fn draft_text_changed(&mut self) {
        if let (Some(draft), Some(comparison)) = (&self.draft, &self.draft_comparison) {
            let old = &comparison.result;
            if old == draft {
                return;
            }
            let prefix = old
                .bytes()
                .zip(draft.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            let start = old.floor_char_boundary(prefix);
            let suffix: usize = old[start..]
                .chars()
                .rev()
                .zip(draft[start..].chars().rev())
                .take_while(|(a, b)| a == b)
                .map(|(character, _)| character.len_utf8())
                .sum();
            self.rebase_draft_ranges(start..old.len() - suffix, draft.len() - suffix - start);
        }
        self.rebuild_draft_comparison();
    }

    pub fn apply_draft_block(&mut self, edit: BlockEdit) {
        let Some(previous) = self
            .draft
            .as_deref()
            .and_then(|draft| draft.get(edit.original.clone()))
        else {
            return;
        };
        if previous == edit.replacement {
            return;
        }
        let undo = DraftUndo {
            span: edit.original.start..edit.original.start + edit.replacement.len(),
            side: edit.side,
            previous: previous.to_owned(),
        };
        self.replace_draft_range(edit.original, &edit.replacement);
        self.draft_undo.push(undo);
    }

    pub fn undo_draft_block(&mut self, index: usize) {
        if index < self.draft_undo.len() {
            let undo = self.draft_undo.remove(index);
            self.replace_draft_range(undo.span, &undo.previous);
        }
    }

    fn replace_draft_range(&mut self, span: Range<usize>, replacement: &str) {
        if let Some(draft) = &mut self.draft {
            if let Some((cursor, _)) = self.manual_cursor {
                let start = draft[..span.start].chars().count();
                let end = start + draft[span.clone()].chars().count();
                let inserted = replacement.chars().count();
                self.manual_edit_start = Some(if cursor <= start {
                    cursor
                } else if cursor < end {
                    start + (cursor - start).min(inserted)
                } else {
                    cursor - (end - start) + inserted
                });
            }
            draft.replace_range(span.clone(), replacement);
            self.rebase_draft_ranges(span, replacement.len());
            self.rebuild_draft_comparison();
        }
    }

    fn rebase_draft_ranges(&mut self, changed: Range<usize>, replacement_len: usize) {
        let shift = |offset| offset - changed.len() + replacement_len;
        self.draft_undo.retain_mut(|undo| {
            if undo.span.is_empty() && undo.span.start == changed.start {
                return false;
            }
            if undo.span.end <= changed.start {
                return true;
            }
            if undo.span.start >= changed.end {
                undo.span = shift(undo.span.start)..shift(undo.span.end);
                return true;
            }
            false
        });
        for block in &mut self.draft_recovered_spans {
            let Some(span) = block else { continue };
            if *span == changed {
                span.end = span.start + replacement_len;
            } else if span.end <= changed.start {
                continue;
            } else if span.start >= changed.end {
                *span = shift(span.start)..shift(span.end);
            } else if span.start <= changed.start && changed.end <= span.end {
                span.end = shift(span.end);
            } else {
                *block = None;
            }
        }
    }

    fn rebuild_draft_comparison(&mut self) {
        self.draft_selection = None;
        if let (Some(review), Some(comparison), Some(draft)) =
            (&self.review, &self.comparison, &self.draft)
        {
            self.draft_ready = review.can_stage(draft);
            self.draft_comparison = Some(
                review.compare_draft(draft, comparison.sources.each_ref().map(String::as_str)),
            );
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
        let changed_conflicts = if let Some(review) = self
            .review
            .as_ref()
            .filter(|review| review.has_recovered_conflicts())
        {
            review.text.as_deref().is_some_and(|text| {
                review
                    .blocks()
                    .iter()
                    .zip(&self.edits)
                    .any(|(block, edit)| edit.as_deref() != Some(&text[block.span.clone()]))
            })
        } else {
            self.edits.iter().any(Option::is_some)
        };
        !self.block_edits.is_empty()
            || changed_conflicts
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
        if self.start_worker(move |git| {
            WorkerResult::ConflictReview(
                conflict::prepare(git, &reviewed_root, &reviewed_path, action).map(Box::new),
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
        self.start_worker(move |git| {
            let outcome = conflict::apply(git, &root, &path, action, &review, draft);
            WorkerResult::Operation {
                committed: false,
                success: if action == Action::Recreate {
                    "Conflict recreated"
                } else {
                    "Conflict resolved"
                },
                outcome,
                snapshot: git::load(git, &root),
            }
        });
    }
}
