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

pub fn header(ui: &mut Ui, palette: Palette, title: &str, description: &str) {
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
            ui.horizontal(|ui| {
                let text_width = ui.available_width() - 64.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(text_width, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(RichText::new(title).size(16.0).family(semibold_family()));
                        ui.add(egui::Label::new(description).wrap());
                    },
                );
                icons::paint(ui, Icon::Repository, palette.accent, 48.0);
            });
        });
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
