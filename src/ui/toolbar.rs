use super::icons::{self, Icon};
use crate::{
    app::SannyGitApp,
    theme::{semibold_family, ThemeChoice},
};
use eframe::egui::{
    Align, Context, Frame, Layout, Margin, RichText, TextEdit, TopBottomPanel, Vec2,
};

pub fn show(context: &Context, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    let height = if context.screen_rect().height() < 850.0 {
        78.0
    } else {
        88.0
    };
    TopBottomPanel::top("working_tree.v2.toolbar")
        .exact_height(height)
        .frame(Frame::new().fill(palette.header).inner_margin(Margin {
            left: 28,
            right: 20,
            top: 8,
            bottom: 8,
        }))
        .show(context, |ui| {
            ui.horizontal_centered(|ui| {
                let brand_width = if context.screen_rect().width() >= 1400.0 {
                    336.0
                } else {
                    300.0
                };
                let action_width = if context.screen_rect().width() >= 1400.0 {
                    66.0
                } else {
                    54.0
                };
                ui.allocate_ui_with_layout(
                    Vec2::new(brand_width, height - 16.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_width(brand_width);
                        icons::paint(ui, Icon::Crab, palette.orange, 64.0);
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                for (text, color) in
                                    [("Sanny", palette.text), ("Git", palette.orange)]
                                {
                                    ui.label(
                                        RichText::new(text)
                                            .size(34.0)
                                            .family(semibold_family())
                                            .color(color),
                                    );
                                }
                            });
                            ui.label(
                                RichText::new("A simple, no-nonsense Git client.")
                                    .size(14.0)
                                    .color(palette.muted),
                            );
                        });
                    },
                );
                for (icon, label) in [
                    (Icon::Pull, "Pull"),
                    (Icon::Push, "Push"),
                    (Icon::Fetch, "Fetch"),
                    (Icon::Branch, "Branch"),
                    (Icon::Merge, "Merge"),
                    (Icon::Rebase, "Rebase"),
                    (Icon::Stash, "Stash"),
                    (Icon::Discard, "Discard"),
                ] {
                    ui.allocate_ui_with_layout(
                        Vec2::new(action_width, 58.0),
                        Layout::top_down(Align::Center),
                        |ui| {
                            ui.set_min_width(action_width);
                            ui.add_enabled_ui(false, |ui| {
                                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                                    icons::paint(ui, icon, palette.text, 23.0);
                                    ui.label(RichText::new(label).size(14.0));
                                });
                            })
                            .response
                            .on_disabled_hover_text(
                                "Repository integration is not implemented yet",
                            );
                        },
                    );
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.menu_image_button(icons::image(Icon::Settings, palette.text, 23.0), |ui| {
                        ui.label(RichText::new("Appearance").family(semibold_family()));
                        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                            if ui
                                .selectable_label(app.theme == theme, theme.label())
                                .clicked()
                            {
                                app.theme = theme;
                                ui.close_menu();
                            }
                        }
                    })
                    .response
                    .on_hover_text("Settings");
                    if ui.available_width() >= 180.0 {
                        ui.add_space(16.0);
                        let mut search = String::new();
                        let width = (ui.available_width() - 32.0).clamp(80.0, 240.0);
                        ui.add_enabled(
                            false,
                            TextEdit::singleline(&mut search)
                                .hint_text(RichText::new("Search files...").color(palette.muted))
                                .margin(Vec2::new(12.0, 8.0))
                                .desired_width(width),
                        )
                        .on_disabled_hover_text("Global repository search is not implemented yet");
                    }
                });
            });
        });
}
