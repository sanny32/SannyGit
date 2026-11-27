//! Shared SannyGit controls. Keep repository state and actions in the calling panels.
//! Prefer these controls for application styling; use egui directly for standard widgets.
pub(super) mod dialog_style;
pub(super) mod editable_combobox;
pub(super) mod icons;
pub(super) mod popup_menu;
pub(super) mod tooltips;

use eframe::egui::{Align, Layout, Response, Sense, Ui, UiBuilder, Vec2};

/// Shared left inset for table header labels and the cells below them.
pub const TABLE_CELL_PADDING: f32 = 6.0;

/// Hide the table's own column separators. Dividers are drawn only in the
/// header row with [`table_header_separator`], and hovering a resize handle
/// must not paint a full-height line through the rows.
pub fn table_resize_style(ui: &mut Ui) {
    let visuals = &mut ui.style_mut().visuals;
    visuals.widgets.noninteractive.bg_stroke = eframe::egui::Stroke::NONE;
    visuals.widgets.hovered.bg_stroke = eframe::egui::Stroke::NONE;
    visuals.widgets.active.bg_stroke = eframe::egui::Stroke::NONE;
}

/// Draw the column divider at the right edge of a header cell. Resizing is
/// indicated by the cursor only, so the divider does not change on hover.
pub fn table_header_separator(ui: &mut Ui, palette: crate::theme::Palette) {
    let rect = ui.max_rect();
    let x = rect.right() + ui.spacing().item_spacing.x * 0.5;
    table_header_divider(ui, x, rect, palette);
}

/// Shared vertical divider between table header cells. Callers place it at the
/// column boundary; the line is always the neutral border color and keeps a
/// small inset from the header row edges.
pub fn table_header_divider(
    ui: &Ui,
    x: f32,
    rect: eframe::egui::Rect,
    palette: crate::theme::Palette,
) {
    ui.painter().line_segment(
        [
            eframe::egui::pos2(x, rect.top() + 3.0),
            eframe::egui::pos2(x, rect.bottom() - 3.0),
        ],
        (1.0, palette.border),
    );
}

/// Draggable header divider at an explicit `x`. Draws the shared divider and
/// returns the horizontal drag delta while the user drags it. Resizing is
/// indicated by the cursor only; the line itself never highlights.
pub fn table_header_splitter(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    x: f32,
    rect: eframe::egui::Rect,
    palette: crate::theme::Palette,
) -> Option<f32> {
    let hit = eframe::egui::Rect::from_min_max(
        eframe::egui::pos2(x - 5.0, rect.top()),
        eframe::egui::pos2(x + 5.0, rect.bottom()),
    );
    let response = ui
        .interact(hit, ui.make_persistent_id(id), Sense::drag())
        .on_hover_cursor(eframe::egui::CursorIcon::ResizeColumn);
    table_header_divider(ui, x, rect, palette);
    response.dragged().then(|| response.drag_delta().x)
}

/// Draggable divider at the right edge of a table header cell.
pub fn table_header_cell_splitter(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    palette: crate::theme::Palette,
) -> Option<f32> {
    let rect = ui.max_rect();
    // Stay inside the cell so the interaction survives the cell's clip rect.
    let x = rect.right() - 0.5;
    table_header_splitter(ui, id, x, rect, palette)
}

/// Single-line fields share the same height, padding, and vertical text alignment.
pub fn singleline(text: &mut dyn eframe::egui::TextBuffer) -> eframe::egui::TextEdit<'_> {
    eframe::egui::TextEdit::singleline(text)
        .margin(eframe::egui::Margin::symmetric(5, 2))
        .min_size(Vec2::new(0.0, crate::theme::CONTROL_HEIGHT))
        .align(eframe::egui::Align2::LEFT_CENTER)
}

pub fn selectable_row(
    ui: &mut Ui,
    selected: bool,
    height: f32,
    contents: impl FnOnce(&mut Ui),
) -> Response {
    selectable_row_with_outset(ui, selected, height, 0.0, Sense::click(), contents)
}

pub fn selectable_row_with_outset(
    ui: &mut Ui,
    selected: bool,
    height: f32,
    outset: f32,
    sense: Sense,
    contents: impl FnOnce(&mut Ui),
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), sense);
    let fill = if selected {
        ui.visuals().selection.bg_fill
    } else if response.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        eframe::egui::Color32::TRANSPARENT
    };
    let paint_rect = rect.expand2(Vec2::new(outset, 0.0));
    ui.painter()
        .with_clip_rect(ui.clip_rect().expand2(Vec2::new(outset, 0.0)))
        .rect_filled(paint_rect, 0.0, fill);
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            0.0,
            ui.visuals().selection.stroke,
            eframe::egui::StrokeKind::Inside,
        );
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(6.0, 1.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    // Labels inside a clickable row must leave pointer input to the row.
    child.style_mut().interaction.selectable_labels = false;
    contents(&mut child);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{install_fonts, ThemeChoice, CONTROL_HEIGHT};
    use eframe::egui::{self, Context, Pos2, RawInput, Rect, Shape, Vec2};

    fn field_text_center(output: &egui::FullOutput, value: &str) -> (Rect, Pos2) {
        let field = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                Shape::Rect(rect) if (rect.rect.height() - CONTROL_HEIGHT).abs() < 0.6 => {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("singleline field missing");
        let text = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                Shape::Text(text) if text.galley.job.text == value => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()).center())
                }
                _ => None,
            })
            .expect("field text missing");
        (field, text)
    }

    #[test]
    fn singleline_text_is_vertically_centered() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for scale in [1.0, 2.0] {
                let ctx = Context::default();
                ctx.set_pixels_per_point(scale);
                install_fonts(&ctx);
                theme.apply(&ctx);
                let mut pattern = "*.mdc".to_owned();
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 200.0))),
                        ..Default::default()
                    },
                    |ui| {
                        super::dialog_style::setup(ui);
                        ui.add(singleline(&mut pattern).desired_width(220.0));
                    },
                );
                output.textures_delta.set.clear();
                output.textures_delta.free.clear();
                let (field, text) = field_text_center(&output, &pattern);
                assert!(
                    (text.y - field.center().y).abs() <= 0.5,
                    "Text is off-center at {scale}x {theme:?}: field={field:?}, text_y={}",
                    text.y
                );
            }
        }
    }
}
