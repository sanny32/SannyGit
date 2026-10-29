use eframe::egui::{Align, Button, Context, Layout, RichText, TopBottomPanel};

use crate::{app::SannyGitApp, theme::ThemeChoice};
use super::icons::{self, Icon};

pub fn show(context: &Context, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    TopBottomPanel::top("toolbar").exact_height(58.0).show(context, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(RichText::new("Sanny").size(22.0).strong().color(palette.text));
                    ui.label(RichText::new("Git").size(22.0).strong().color(palette.orange));
                });
                ui.label(RichText::new("A simple, no-nonsense Git client").small().color(palette.muted));
            });
            ui.add_space(18.0);
            ui.separator();
            for (icon, label) in [
                (Icon::Pull, "Pull"), (Icon::Push, "Push"), (Icon::Fetch, "Fetch"),
                (Icon::Branch, "Branch"), (Icon::Merge, "Merge"), (Icon::Rebase, "Rebase"),
                (Icon::Stash, "Stash"), (Icon::Discard, "Discard"),
            ] {
                ui.add_enabled_ui(false, |ui| {
                    ui.vertical(|ui| {
                        icons::paint(ui, icon, palette.muted, 16.0);
                        ui.add(Button::new(label).frame(false));
                    });
                });
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let next_theme = match app.theme {
                    ThemeChoice::Light => ThemeChoice::Dark,
                    ThemeChoice::Dark => ThemeChoice::Light,
                };
                if ui.button(next_theme.label()).on_hover_text(format!("Switch to {} theme", next_theme.label())).clicked() {
                    app.theme = next_theme;
                }
            });
        });
    });
}
