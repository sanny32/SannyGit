use crate::conflict::Comparison;
use eframe::egui;

pub(super) fn pane_scroll_height(height: f32) -> f32 {
    (height - 30.0).max(30.0)
}

pub(super) fn centered_conflict_offset(
    range: std::ops::Range<usize>,
    row_count: usize,
    row_height: f32,
    pane_height: f32,
) -> f32 {
    let viewport_height = pane_scroll_height(pane_height);
    let block_center = (range.start as f32 + range.len() as f32 / 2.0) * row_height;
    let max_offset = (row_count as f32 * row_height - viewport_height).max(0.0);
    (block_center - viewport_height / 2.0).clamp(0.0, max_offset)
}

#[derive(Clone, Copy)]
pub(super) struct PaneViewport {
    pub(super) rect: egui::Rect,
    pub(super) offset: f32,
    pub(super) draft: bool,
}

impl PaneViewport {
    pub(super) fn row_y(self, row: usize, comparison: &Comparison, row_height: f32) -> f32 {
        let row = if self.draft {
            comparison.result_row_boundaries[row]
        } else {
            row
        };
        self.rect.top() - self.offset + row as f32 * row_height
    }
}

pub(super) fn draft_scroll_offset(comparison: &Comparison, offset: f32, row_height: f32) -> f32 {
    let row = offset / row_height;
    let index = (row as usize).min(comparison.rows.len());
    let boundaries = &comparison.result_row_boundaries;
    let start = boundaries[index];
    let end = boundaries.get(index + 1).copied().unwrap_or(start);
    (start as f32 + row.fract() * (end - start) as f32) * row_height
}
