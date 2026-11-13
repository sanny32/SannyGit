use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use crate::{app::SannyGitApp, theme::Palette};
use eframe::egui::{
    Align, Align2, Button, FontId, Frame, Layout, Margin, Panel, Rect, Response, Ui, Vec2,
};

const TOOLBAR_BUTTON_HEIGHT: f32 = 48.0;
const TOOLBAR_BUTTON_PADDING_Y: f32 = 4.0;
const TOOLBAR_ICON_SIZE: f32 = 22.0;
const HOVER_ANIMATION_TIME: f32 = 0.12;
const PRESS_ANIMATION_TIME: f32 = 0.07;

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
            .min_size(Vec2::new(width, TOOLBAR_BUTTON_HEIGHT)),
    );
    response.widget_info(|| {
        eframe::egui::WidgetInfo::labeled(eframe::egui::Role::Button, enabled, label)
    });

    let hover_progress = ui.ctx().animate_bool_with_time_and_easing(
        response.id.with("hover"),
        enabled && response.hovered(),
        HOVER_ANIMATION_TIME,
        eframe::egui::emath::easing::quadratic_out,
    );
    let press_progress = ui.ctx().animate_bool_with_time_and_easing(
        response.id.with("press"),
        enabled && response.is_pointer_button_down_on(),
        PRESS_ANIMATION_TIME,
        eframe::egui::emath::easing::quadratic_out,
    );

    if enabled && (hover_progress > 0.0 || press_progress > 0.0) {
        let fill = palette
            .header
            .lerp_to_gamma(palette.hover, hover_progress)
            .lerp_to_gamma(palette.selection, press_progress);
        ui.painter()
            .rect_filled(response.rect.shrink(1.0), 3.0, fill);
    }

    let color = if enabled && palette.panel_radius > 0 {
        palette.text
    } else if enabled {
        palette.accent
    } else {
        palette.muted
    };
    let press_offset = Vec2::new(0.0, press_progress);
    let icon_size = TOOLBAR_ICON_SIZE * (1.0 + 0.05 * hover_progress - 0.07 * press_progress);
    icons::image(icon, color, icon_size).paint_at(
        ui,
        Rect::from_center_size(
            response.rect.center_top()
                + Vec2::new(0.0, TOOLBAR_BUTTON_PADDING_Y + 11.0)
                + press_offset,
            Vec2::splat(icon_size),
        ),
    );
    ui.painter().text(
        response.rect.center_top() + Vec2::new(0.0, TOOLBAR_BUTTON_PADDING_Y + 24.0) + press_offset,
        Align2::CENTER_TOP,
        label,
        FontId::proportional(12.0),
        if enabled { palette.text } else { palette.muted },
    );
    response
}

#[cfg(not(target_os = "macos"))]
fn menu_bar(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    Panel::top("working_tree.menu")
        .exact_size(24.0)
        .frame(
            Frame::new()
                .fill(palette.header)
                .inner_margin(Margin::symmetric(8, 0)),
        )
        .show(ui, |ui| {
            eframe::egui::MenuBar::new()
                .style(|style: &mut eframe::egui::Style| {
                    eframe::egui::containers::menu::menu_style(style);
                    style.spacing.item_spacing.x = 4.0;
                    style.spacing.button_padding.x = 8.0;
                    style.spacing.interact_size.y = 24.0;
                    style.override_font_id = Some(FontId::proportional(12.0));
                })
                .ui(ui, |ui| {
                    for menu in crate::menu::MENUS {
                        ui.menu_button(menu.title, |ui| {
                            menu_items(ui, app, menu.items);
                        });
                    }
                });
        });
}

#[cfg(not(target_os = "macos"))]
fn menu_items(ui: &mut Ui, app: &mut SannyGitApp, items: &[crate::menu::MenuItem]) {
    for item in items {
        if item.separator {
            ui.separator();
        } else if !item.children.is_empty() {
            ui.menu_button(item.label, |ui| menu_items(ui, app, item.children));
        } else {
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
                response.disabled_help_text("Not implemented yet");
            }
        }
    }
}

fn brand(ui: &mut Ui, palette: Palette) {
    use eframe::egui::{text::LayoutJob, TextFormat};

    let mut title = LayoutJob::default();
    for (text, color) in [("Sanny", palette.text), ("Git", palette.brand)] {
        title.append(
            text,
            0.0,
            TextFormat {
                font_id: FontId::new(24.0, crate::theme::semibold_family()),
                color,
                ..Default::default()
            },
        );
    }
    let title = ui.fonts_mut(|fonts| fonts.layout_job(title));
    let subtitle = ui.painter().layout_no_wrap(
        "A simple Git client.".into(),
        FontId::proportional(11.0),
        palette.muted,
    );
    let icon_size = 44.0;
    let gap = 8.0;
    // Align visible glyphs instead of the font's extra space above and below each line.
    let title_bounds = title.mesh_bounds;
    let subtitle_bounds = subtitle.mesh_bounds;
    let line_gap = 3.0;
    let text_height = title_bounds.height() + line_gap + subtitle_bounds.height();
    let width = icon_size + gap + title_bounds.width().max(subtitle_bounds.width());
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(width, icon_size.max(text_height)),
        eframe::egui::Sense::hover(),
    );
    eframe::egui::Image::from_bytes(
        "bytes://app/sannygit.svg",
        include_bytes!("../../assets/app/sannygit.svg"),
    )
    .paint_at(
        ui,
        Rect::from_center_size(
            rect.left_center() + Vec2::new(icon_size / 2.0, 0.0),
            Vec2::splat(icon_size),
        ),
    );
    let text_origin = rect.left_center() + Vec2::new(icon_size + gap, -text_height / 2.0);
    let title_origin = text_origin - title_bounds.min.to_vec2();
    let subtitle_origin = text_origin + Vec2::new(0.0, title_bounds.height() + line_gap)
        - subtitle_bounds.min.to_vec2();
    ui.painter().galley(title_origin, title, palette.text);
    ui.painter()
        .galley(subtitle_origin, subtitle, palette.muted);
}

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    #[cfg(not(target_os = "macos"))]
    menu_bar(ui, app);
    let palette = app.palette();
    let modern = app.interface_style == crate::theme::InterfaceStyle::Modern;
    let height = if modern { 68.0 } else { 56.0 };
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
                if modern {
                    brand(ui, palette);
                    ui.add_space(12.0);
                }
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
                        Vec2::new(action_width, TOOLBAR_BUTTON_HEIGHT),
                        Layout::top_down(Align::Center),
                        |ui| {
                            ui.set_min_width(action_width);
                            if matches!(label, "Sync" | "Pull" | "Push") {
                                let operation = match label {
                                    "Sync" => crate::remotes::Operation::Sync,
                                    "Pull" => crate::remotes::Operation::Pull,
                                    _ => crate::remotes::Operation::Push,
                                };
                                if action_button(ui, icon, label, app.remote_allowed(), palette, action_width)
                                    .help_text(remote_help(operation, app.tracking.status.as_ref().and_then(|status| status.remote.as_deref())))
                                    .disabled_help_text("Open and refresh a repository; wait for the current operation")
                                    .clicked()
                                {
                                    app.open_remote(operation);
                                }
                            } else if label == "Discard" {
                                let blocker = app.discard_blocker();
                                if action_button(ui, icon, label, blocker.is_none(), palette, action_width)
                                    .help_text("Review restoring the selected file from the index, or deleting one untracked file")
                                    .disabled_help_text(blocker.unwrap_or(""))
                                    .clicked() { app.open_discard(); }
                            } else if label == "Save Stash" {
                                if action_button(ui, icon, label, app.stash_allowed(), palette, action_width)
                                    .help_text("Preview and save local changes, or manage saved stashes")
                                    .disabled_help_text("Open and refresh a repository; wait for the current operation")
                                    .clicked() { app.open_stashes(); }
                            } else if label == "Stage" || label == "Unstage" {
                                let staged = label == "Stage";
                                let enabled = app.staging_allowed(app.selected_change, staged);
                                let tooltip = if staged { "Stage all changes in the selected file" } else { "Unstage the selected file; preserve working-tree contents" };
                                if action_button(ui, icon, label, enabled, palette, action_width)
                                    .help_text(tooltip)
                                    .disabled_help_text("Select a suitable file after refreshing; conflicts must be resolved outside SannyGit")
                                    .clicked() {
                                    app.set_staged(app.selected_change, staged);
                                }
                            } else {
                                action_button(ui, icon, label, false, palette, action_width)
                                    .disabled_help_text("This operation is not implemented yet");
                            }
                        },
                    );
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let response = action_button(
                        ui,
                        Icon::Settings,
                        "Settings",
                        true,
                        palette,
                        action_width,
                    )
                    .help_text("Settings");
                    if response.clicked() {
                        app.open_settings();
                    }
                });
            });
        });
}

fn remote_help(operation: crate::remotes::Operation, remote: Option<&str>) -> String {
    let repository = remote.map_or_else(
        || "remote repository".to_owned(),
        |name| format!("remote {name} repository"),
    );
    match operation {
        crate::remotes::Operation::Sync => format!(
            "Push local commits to the {repository}, then pull possible remote changes (fast-forward only)."
        ),
        crate::remotes::Operation::Fetch => {
            format!("Fetch branches and tags from the {repository}.")
        }
        crate::remotes::Operation::Pull => {
            format!("Pull remote commits from the {repository} into the current branch.")
        }
        crate::remotes::Operation::Push => format!("Push local commits to the {repository}."),
    }
}
