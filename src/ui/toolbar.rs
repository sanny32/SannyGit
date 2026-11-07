use super::icons::{self, Icon};
use crate::{
    app::SannyGitApp,
    theme::{semibold_family, Palette, ThemeChoice},
};
use eframe::egui::{
    Align, Align2, Button, FontId, Frame, Layout, Margin, Panel, Popup, Rect, Response, RichText,
    Ui, Vec2,
};

fn action_button(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
    palette: Palette,
    width: f32,
) -> Response {
    let response = ui.add_enabled(
        enabled,
        Button::new("")
            .frame(false)
            .min_size(Vec2::new(width, 40.0)),
    );
    response.widget_info(|| {
        eframe::egui::WidgetInfo::labeled(eframe::egui::Role::Button, enabled, label)
    });
    let color = if enabled {
        palette.accent
    } else {
        palette.muted
    };
    icons::image(icon, color, 22.0).paint_at(
        ui,
        Rect::from_center_size(
            response.rect.center_top() + Vec2::new(0.0, 11.0),
            Vec2::splat(22.0),
        ),
    );
    ui.painter().text(
        response.rect.center_top() + Vec2::new(0.0, 24.0),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(12.0),
        if enabled { palette.text } else { palette.muted },
    );
    response
}

#[cfg(not(target_os = "macos"))]
fn menu_bar(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    Panel::top("working_tree.menu")
        .exact_size(24.0)
        .frame(
            Frame::new()
                .fill(palette.header)
                .inner_margin(Margin::symmetric(8, 0)),
        )
        .show(ui, |ui| {
            eframe::egui::MenuBar::new().ui(ui, |ui| {
                for menu in crate::menu::MENUS {
                    ui.menu_button(menu.title, |ui| {
                        for item in menu.items {
                            let enabled = item
                                .action
                                .is_some_and(|action| app.menu_action_enabled(action));
                            let response = ui.add_enabled(enabled, Button::new(item.label));
                            if let Some(action) = item.action {
                                if response.clicked() {
                                    app.handle_menu_action(action, ui.ctx());
                                    ui.close();
                                }
                            } else {
                                response.on_disabled_hover_text("Not implemented yet");
                            }
                        }
                    });
                }
            });
        });
}

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    #[cfg(not(target_os = "macos"))]
    menu_bar(ui, app);
    let palette = app.theme.palette();
    let height = 52.0;
    Panel::top("working_tree.v2.toolbar")
        .exact_size(height)
        .frame(Frame::new().fill(palette.header).inner_margin(Margin {
            left: 8,
            right: 8,
            top: 4,
            bottom: 4,
        }))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let action_width = 58.0;
                for (icon, label) in [
                    (Icon::Pull, "Pull"),
                    (Icon::Fetch, "Sync"),
                    (Icon::Push, "Push"),
                    (Icon::Check, "Stage"),
                    (Icon::File, "Unstage"),
                    (Icon::Discard, "Discard"),
                    (Icon::Stash, "Save Stash"),
                    (Icon::Branch, "Log"),
                    (Icon::Merge, "Merge"),
                    (Icon::Rebase, "Rebase"),
                ] {
                    if matches!(label, "Stage" | "Save Stash" | "Log" | "Merge") {
                        ui.separator();
                    }
                    ui.allocate_ui_with_layout(
                        Vec2::new(action_width, 40.0),
                        Layout::top_down(Align::Center),
                        |ui| {
                            ui.set_min_width(action_width);
                            if label == "Stage" || label == "Unstage" {
                                let staged = label == "Stage";
                                let enabled = app.staging_allowed(app.selected_change, staged);
                                let tooltip = if staged { "Stage all changes in the selected file" } else { "Unstage the selected file; preserve working-tree contents" };
                                if action_button(ui, icon, label, enabled, palette, action_width)
                                    .on_hover_text(tooltip)
                                    .on_disabled_hover_text("Select a suitable file after refreshing; conflicts must be resolved outside SannyGit")
                                    .clicked() {
                                    app.set_staged(app.selected_change, staged);
                                }
                            } else {
                            ui.add_enabled_ui(false, |ui| {
                                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                                    icons::paint(ui, icon, palette.accent, 22.0);
                                    ui.label(RichText::new(label).size(12.0));
                                });
                            })
                            .response
                            .on_disabled_hover_text(
                                "This operation is not implemented yet",
                            );
                            }
                        },
                    );
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let response = action_button(ui, Icon::Settings, "Settings", true, palette, action_width).on_hover_text("Settings");
                    Popup::menu(&response).show(|ui| {
                        ui.label(RichText::new("Appearance").family(semibold_family()));
                        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                            if ui
                                .selectable_label(app.theme == theme, theme.label())
                                .clicked()
                            {
                                app.theme = theme;
                                ui.close();
                            }
                        }
                    });
                });
            });
        });
}
