use eframe::egui::{Align, Button, Context, Layout, RichText, TopBottomPanel};

use crate::{app::SannyGitApp, theme::ThemeChoice};

pub fn show(context: &Context, app: &mut SannyGitApp) {
    let palette = app.theme.palette();

    TopBottomPanel::top("toolbar")
        .exact_height(58.0)
        .show(context, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(RichText::new("Sanny").size(22.0).strong().color(palette.text));
                        ui.label(RichText::new("Git").size(22.0).strong().color(palette.orange));
                    });
                    ui.label(
                        RichText::new("A simple, no-nonsense Git client")
                            .small()
                            .color(palette.muted),
                    );
                });
                ui.add_space(18.0);
                ui.separator();

                for (icon, label) in [
                    ("↓", "Pull"),
                    ("↑", "Push"),
                    ("⇅", "Fetch"),
                    ("⑂", "Branch"),
                    ("⇄", "Merge"),
                    ("↪", "Rebase"),
                    ("▱", "Stash"),
                    ("↶", "Discard"),
                ] {
                    ui.add_enabled_ui(false, |ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(icon).size(16.0).color(palette.muted));
                            ui.add(Button::new(label).frame(false));
                        });
                    });
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let next_theme = match app.theme {
                        ThemeChoice::Light => ThemeChoice::Dark,
                        ThemeChoice::Dark => ThemeChoice::Light,
                    };
                    if ui
                        .button(next_theme.label())
                        .on_hover_text(format!("Switch to {} theme", next_theme.label()))
                        .clicked()
                    {
                        app.theme = next_theme;
                    }
                });
            });
        });
}
