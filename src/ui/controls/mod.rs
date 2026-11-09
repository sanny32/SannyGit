//! Shared SannyGit controls. Keep repository state and actions in the calling panels.
//! Prefer these controls for application styling; use egui directly for standard widgets.
pub(super) mod dialog_style;
pub(super) mod editable_combo_box;
pub(super) mod icons;
pub(super) mod tooltips;

use eframe::egui::{Align, Layout, Response, Sense, Ui, UiBuilder, Vec2};

pub(crate) trait EditableText {
    fn editable(self) -> Self;
}

impl EditableText for eframe::egui::TextEdit<'_> {
    fn editable(self) -> Self {
        self.background_color(eframe::egui::Color32::WHITE)
            .text_color(eframe::egui::Color32::from_rgb(24, 24, 24))
    }
}

pub fn selectable_row(
    ui: &mut Ui,
    selected: bool,
    height: f32,
    contents: impl FnOnce(&mut Ui),
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    let fill = if selected {
        ui.visuals().selection.bg_fill
    } else if response.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        eframe::egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 0.0, fill);
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
