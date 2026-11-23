//! Shared SannyGit controls. Keep repository state and actions in the calling panels.
//! Prefer these controls for application styling; use egui directly for standard widgets.
pub(super) mod dialog_style;
pub(super) mod editable_combobox;
pub(super) mod icons;
pub(super) mod popup_menu;
pub(super) mod tooltips;

use eframe::egui::{Align, Layout, Response, Sense, Ui, UiBuilder, Vec2};

/// Column resize handles use the same neutral contrast as panel splitters.
pub fn table_resize_style(ui: &mut Ui, palette: crate::theme::Palette) {
    let visuals = &mut ui.style_mut().visuals;
    visuals.widgets.hovered.bg_stroke.color = palette.text;
    visuals.widgets.active.bg_stroke.color = palette.text;
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
    selectable_row_with_outset(ui, selected, height, 0.0, contents)
}

pub fn selectable_row_with_outset(
    ui: &mut Ui,
    selected: bool,
    height: f32,
    outset: f32,
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
