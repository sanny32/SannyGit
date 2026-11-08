//! Compact desktop dialog chrome shared by repository dialogs.
use super::icons::{self, Icon};
use crate::theme::{semibold_family, Palette};
use eframe::egui::{self, Button, Frame, Id, Margin, Modal, RichText, Ui};

pub fn frame(ctx: &egui::Context, palette: Palette) -> Frame {
    Frame::window(&ctx.global_style())
        .fill(palette.header)
        .stroke((1.0, palette.border))
        .corner_radius(8)
        .inner_margin(1)
}

pub fn modal(ctx: &egui::Context, palette: Palette, id: &'static str) -> Modal {
    Modal::new(Id::unique(id))
        .frame(frame(ctx, palette))
        .backdrop_color(egui::Color32::TRANSPARENT)
}

pub fn setup(ui: &mut Ui) {
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
    ui.spacing_mut().button_padding = egui::vec2(12.0, 4.0);
    ui.spacing_mut().interact_size.y = 26.0;
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
}

pub fn section(ui: &mut Ui, fill: egui::Color32, margin: Margin, contents: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    Frame::new()
        .fill(fill)
        .corner_radius(7)
        .inner_margin(margin)
        .show(ui, |ui| {
            ui.set_width(width - margin.sum().x);
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), contents);
        });
}

pub fn header(ui: &mut Ui, palette: Palette, title: &str, description: &str) -> egui::Rect {
    let width = ui.available_width();
    Frame::new()
        .fill(palette.panel)
        .corner_radius(egui::CornerRadius {
            nw: 7,
            ne: 7,
            sw: 0,
            se: 0,
        })
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_width(width - 36.0);
            ui.spacing_mut().item_spacing = egui::vec2(16.0, 6.0);
            let origin = ui.next_widget_position();
            let content_width = ui.available_width();
            let mut text = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_min_size(
                        origin,
                        egui::vec2(content_width - 64.0, 0.0),
                    ))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            text.label(RichText::new(title).size(16.0).family(semibold_family()));
            text.add(egui::Label::new(description).wrap());
            let height = text.min_rect().height().max(48.0);
            let (content_rect, _) =
                ui.allocate_exact_size(egui::vec2(content_width, height), egui::Sense::hover());
            let icon_rect = egui::Rect::from_center_size(
                egui::pos2(content_rect.right() - 24.0, content_rect.center().y),
                egui::Vec2::splat(48.0),
            );
            icons::image(Icon::Repository, palette.accent, 48.0).paint_at(ui, icon_rect);
        })
        .response
        .rect
}

pub fn button(label: &str) -> Button<'_> {
    Button::new(label).min_size(egui::vec2(82.0, 28.0))
}

pub fn primary(label: &str, palette: Palette) -> Button<'_> {
    Button::new(RichText::new(label).color(palette.on_accent))
        .min_size(egui::vec2(82.0, 28.0))
        .fill(palette.accent)
        .stroke((1.0, palette.accent))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{install_fonts, ThemeChoice};

    #[test]
    fn header_height_depends_on_content_not_available_space() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let ctx = egui::Context::default();
            install_fonts(&ctx);
            theme.apply(&ctx);
            let mut heights = Vec::new();
            for height in [300.0, 700.0, 1024.0] {
                let mut rect = egui::Rect::NOTHING;
                for _ in 0..3 {
                    let mut output = ctx.run_ui(egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, height))),
                        ..Default::default()
                    }, |ui| {
                        ui.set_width(560.0);
                        setup(ui);
                        rect = header(ui, theme.palette(), "Add existing or create new repository",
                            "Specify the local Git repository to open. To create a new repository, specify an empty directory.");
                    });
                    output.textures_delta.clear();
                }
                assert!(
                    rect.height() <= 112.0,
                    "Header has excess padding: {rect:?}"
                );
                heights.push(rect.height());
            }
            assert!(
                heights.windows(2).all(|h| (h[0] - h[1]).abs() < 1.0),
                "Header height changed: {heights:?}"
            );
        }
    }
}
