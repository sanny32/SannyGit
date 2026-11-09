use crate::theme::Palette;
use eframe::egui::{
    self, Align2, Frame, Id, Popup, Rect, Response, Sense, StrokeKind, TextEdit, Ui,
};

/// Editable text with a full-width dropdown. Use a distinct id for each control.
pub fn show(
    ui: &mut Ui,
    id: Id,
    value: &mut String,
    options: &[String],
    width: f32,
    palette: Palette,
    empty_text: &str,
) -> Response {
    let edit_id = id.with("editor");
    let focused = ui.memory(|memory| memory.has_focus(edit_id));
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 26.0), Sense::hover());
    ui.painter().rect(
        rect,
        4,
        palette.editor,
        (
            1.0,
            if focused {
                palette.accent
            } else {
                palette.border
            },
        ),
        StrokeKind::Inside,
    );
    let icon_width = ui.spacing().icon_width;
    let right_padding = ui.spacing().button_padding.x;
    let button_rect = Rect::from_min_max(
        egui::pos2(rect.right() - icon_width - 2.0 * right_padding, rect.top()),
        rect.max,
    );
    // Match the theme's 26-point editor minimum: a shorter inset rect expands
    // downward and shifts the text below the center, especially at Retina scale.
    let editor_rect = Rect::from_min_max(
        rect.min + egui::vec2(6.0, 0.0),
        egui::pos2(button_rect.left() - 4.0, rect.bottom()),
    );
    let editor = ui.put(
        editor_rect,
        TextEdit::singleline(value)
            .id(edit_id)
            .frame(Frame::NONE)
            .margin(0)
            .min_size(editor_rect.size())
            .align(Align2::LEFT_CENTER)
            .text_color(palette.text),
    );
    let button = ui.interact(button_rect, id.with("arrow"), Sense::click());
    // Match egui's standard ComboBox icon geometry and interaction color.
    let visuals = ui.style().interact(&button);
    let icon_rect = Rect::from_center_size(
        egui::pos2(
            rect.right() - right_padding - icon_width / 2.0,
            rect.center().y,
        ),
        egui::Vec2::splat(icon_width),
    )
    .expand(visuals.expansion);
    let triangle_rect = Rect::from_center_size(
        icon_rect.center(),
        egui::vec2(icon_rect.width() * 0.7, icon_rect.height() * 0.45),
    );
    ui.painter().add(egui::Shape::rotated_triangle(
        triangle_rect,
        0.0,
        visuals.fg_stroke.color,
    ));
    let mut response = response.union(button);
    if editor.changed() {
        response.mark_changed();
    }
    Popup::menu(&response)
        .id(id.with("popup"))
        .width(width)
        .show(|ui| {
            ui.set_min_width((width - 16.0).max(120.0));
            if options.is_empty() {
                ui.add_enabled(false, egui::Label::new(empty_text));
            }
            for option in options {
                if ui.selectable_label(*value == *option, option).clicked() {
                    if *value != *option {
                        value.clone_from(option);
                        response.mark_changed();
                    }
                    ui.close();
                }
            }
        });
    response.union(editor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, Pos2, RawInput, Vec2};

    #[test]
    fn text_is_vertically_centered() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1536.0, 1024.0)] {
                for scale in [1.0, 2.0] {
                    let ctx = Context::default();
                    ctx.set_pixels_per_point(scale);
                    crate::theme::install_fonts(&ctx);
                    egui_extras::install_image_loaders(&ctx);
                    theme.apply(&ctx);
                    let mut value = "v1.0.0-b1".to_owned();
                    for frame in 0..3 {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| {
                                super::super::dialog_style::setup(ui);
                                show(
                                    ui,
                                    Id::unique("test.combo"),
                                    &mut value,
                                    &[],
                                    400.0,
                                    theme.palette(),
                                    "No options",
                                );
                            },
                        );
                        output.textures_delta.clear();
                        if frame == 2 {
                            let field = output
                                .shapes
                                .iter()
                                .find_map(|shape| match &shape.shape {
                                    eframe::egui::Shape::Rect(rect)
                                        if (rect.rect.height() - 26.0).abs() < 0.1
                                            && rect.rect.width() > 300.0 =>
                                    {
                                        Some(rect.rect)
                                    }
                                    _ => None,
                                })
                                .expect("Combo box field missing");
                            let text = output
                                .shapes
                                .iter()
                                .find_map(|shape| match &shape.shape {
                                    eframe::egui::Shape::Text(text)
                                        if text.galley.job.text == "v1.0.0-b1" =>
                                    {
                                        Some(text.galley.mesh_bounds.translate(text.pos.to_vec2()))
                                    }
                                    _ => None,
                                })
                                .expect("Combo box text missing");
                            assert!(
                                (text.center().y - field.center().y).abs() <= 0.5,
                                "Combo box text is off-center: field={field:?}, text={text:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
