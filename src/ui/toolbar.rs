use super::controls::icons::{self, Icon};
#[cfg(not(target_os = "macos"))]
use super::controls::popup_menu;
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::SannyGitApp,
    theme::{semibold_family, ui_font_size, Palette, TOOLBAR_BRAND_FONT_SIZE},
};
use eframe::egui::{
    Align, Align2, Button, FontId, Frame, Label, Layout, Margin, Panel, Rect, Response, RichText,
    Ui, Vec2,
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
    toolbar_button(
        ui,
        (label, ui_font_size()),
        enabled,
        palette,
        width,
        false,
        |ui, rect, color| {
            icons::image(icon, color, rect.width()).paint_at(ui, rect);
        },
    )
}

pub(super) fn toolbar_button(
    ui: &mut Ui,
    caption: (&str, f32),
    enabled: bool,
    palette: Palette,
    width: f32,
    selected: bool,
    paint_icon: impl FnOnce(&Ui, Rect, eframe::egui::Color32),
) -> Response {
    let (label, font_size) = caption;
    let response = ui.add_enabled(
        enabled,
        Button::new("")
            .frame(false)
            .min_size(Vec2::new(width, TOOLBAR_BUTTON_HEIGHT)),
    );
    response.widget_info(|| {
        eframe::egui::WidgetInfo::selected(
            eframe::egui::Role::Button,
            response.enabled(),
            selected,
            label,
        )
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

    if selected || hover_progress > 0.0 || press_progress > 0.0 || response.has_focus() {
        let rect = response.rect.shrink(1.0);
        let fill = if selected {
            palette.selection
        } else {
            palette
                .header
                .lerp_to_gamma(palette.hover, hover_progress)
                .lerp_to_gamma(palette.selection, press_progress)
        };
        ui.painter().rect_filled(rect, 3.0, fill);
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                3.0,
                (1.0, palette.accent),
                eframe::egui::StrokeKind::Inside,
            );
        }
    }

    let color = if enabled && palette.panel_radius > 0 {
        palette.text
    } else if enabled {
        palette.accent
    } else {
        palette.muted
    };
    let press_offset = Vec2::new(0.0, press_progress);
    let icon_size = TOOLBAR_ICON_SIZE * (1.0 - 0.07 * press_progress);
    paint_icon(
        ui,
        Rect::from_center_size(
            response.rect.center_top()
                + Vec2::new(0.0, TOOLBAR_BUTTON_PADDING_Y + 11.0)
                + press_offset,
            Vec2::splat(icon_size),
        ),
        color,
    );
    ui.painter().text(
        response.rect.center_top() + Vec2::new(0.0, TOOLBAR_BUTTON_PADDING_Y + 24.0) + press_offset,
        Align2::CENTER_TOP,
        label,
        FontId::proportional(font_size),
        if enabled { palette.text } else { palette.muted },
    );
    response
}

#[cfg(not(target_os = "macos"))]
fn menu_bar(ui: &mut Ui, app: &mut SannyGitApp) {
    remember_edit_target(ui.ctx());
    let palette = app.palette();
    Panel::top("working_tree.menu")
        .exact_size(30.0)
        .frame(Frame::new().fill(palette.header).inner_margin(Margin {
            left: 8,
            right: 8,
            top: 3,
            bottom: 3,
        }))
        .show(ui, |ui| {
            // MenuBar creates its horizontal layout before applying its own style.
            ui.spacing_mut().interact_size.y = 24.0;
            eframe::egui::MenuBar::new()
                .config(popup_menu::config(palette))
                .style(|style: &mut eframe::egui::Style| {
                    eframe::egui::containers::menu::menu_style(style);
                    style.spacing.item_spacing.x = 0.0;
                    style.spacing.button_padding.x = 8.0;
                    style.spacing.interact_size.y = 24.0;
                    style.override_font_id = Some(FontId::proportional(ui_font_size()));
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
fn remember_edit_target(context: &eframe::egui::Context) {
    let key = eframe::egui::Id::unique("menu.edit.target");
    if context.text_edit_focused() {
        if let Some(target) = context.memory(|memory| memory.focused()) {
            context.data_mut(|data| data.insert_temp(key, target));
        }
    } else if !eframe::egui::Popup::is_any_open(context) {
        context.data_mut(|data| data.remove::<eframe::egui::Id>(key));
    }
}

#[cfg(not(target_os = "macos"))]
fn edit_target(context: &eframe::egui::Context) -> Option<eframe::egui::Id> {
    context.data(|data| data.get_temp(eframe::egui::Id::unique("menu.edit.target")))
}

#[cfg(not(target_os = "macos"))]
fn perform_edit(context: &eframe::egui::Context, action: crate::menu::EditAction) {
    let Some(target) = edit_target(context) else {
        return;
    };
    // Clicking a menu item must not redirect the edit away from the original field.
    context.memory_mut(|memory| memory.request_focus(target));
    if let Some(event) = action.event() {
        context.input_mut(|input| input.events.push(event));
    } else {
        context.send_viewport_cmd(eframe::egui::ViewportCommand::RequestPaste);
    }
    context.request_repaint();
}

#[cfg(not(target_os = "macos"))]
fn menu_items(ui: &mut Ui, app: &mut SannyGitApp, items: &[crate::menu::MenuItem]) {
    // Reserve the shortcut column only when the menu actually shows shortcuts;
    // menus without any shortcut stay as narrow as their labels (like SmartGit).
    let has_shortcut = items.iter().any(|item| {
        crate::menu::EditAction::from_label(item.label).is_some()
            || item
                .action
                .is_some_and(|action| action.shortcut().is_some())
    });
    if has_shortcut {
        ui.set_min_width(280.0);
    }
    for item in items {
        if item.separator {
            popup_menu::separator(ui);
        } else if !item.children.is_empty() {
            popup_menu::submenu(ui, item.label, |ui| {
                menu_items(ui, app, item.children);
            });
        } else {
            if let Some(edit) = crate::menu::EditAction::from_label(item.label) {
                let target = edit_target(ui.ctx());
                let has_selection = target
                    .and_then(|id| eframe::egui::text_edit::TextEditState::load(ui.ctx(), id))
                    .and_then(|state| state.cursor.char_range())
                    .is_some_and(|range| !range.is_empty());
                let enabled = target.is_some()
                    && (!matches!(
                        edit,
                        crate::menu::EditAction::Cut | crate::menu::EditAction::Copy
                    ) || has_selection);
                if ui
                    .add_enabled(
                        enabled,
                        Button::new(item.label)
                            .shortcut_text(ui.ctx().format_shortcut(&edit.shortcut())),
                    )
                    .disabled_help_text("Focus a text field; Cut and Copy require selected text")
                    .clicked()
                {
                    perform_edit(ui.ctx(), edit);
                    ui.close();
                }
                continue;
            }
            let enabled = item
                .action
                .is_some_and(|action| app.menu_action_enabled(action));
            let mut button = Button::new(item.label);
            if let Some(shortcut) = item.action.and_then(|action| action.shortcut()) {
                button = button.shortcut_text(ui.ctx().format_shortcut(&shortcut));
            }
            let response = ui.add_enabled(enabled, button);
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
                font_id: FontId::new(TOOLBAR_BRAND_FONT_SIZE, semibold_family()),
                color,
                ..Default::default()
            },
        );
    }
    let title = ui.fonts_mut(|fonts| fonts.layout_job(title));
    let subtitle = ui.painter().layout_no_wrap(
        "A simple Git client".into(),
        FontId::proportional(ui_font_size()),
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
                            } else if matches!(label, "Merge" | "Rebase") {
                                if action_button(ui, icon, label, app.merge_allowed(), palette, action_width)
                                    .help_text(if label == "Rebase" { "Rebase HEAD onto a selected commit" } else { "Select a branch or commit to merge" })
                                    .disabled_help_text("Open and refresh a repository with commits; wait for the current operation")
                                    .clicked() { if label == "Rebase" { app.open_rebase(); } else { app.open_merge(); } }
                            } else if label == "Discard" {
                                let blocker = app.discard_blocker();
                                if action_button(ui, icon, label, blocker.is_none(), palette, action_width)
                                    .help_text("Review restoring selected files from the index or deleting selected untracked files")
                                    .disabled_help_text(blocker.unwrap_or(""))
                                    .clicked() { app.open_discard(); }
                            } else if label == "Save Stash" {
                                if action_button(ui, icon, label, app.stash_allowed(), palette, action_width)
                                    .help_text("Save local changes for later")
                                    .disabled_help_text("Open and refresh a repository; wait for the current operation")
                                    .clicked() { app.open_save_stash(); }
                            } else if label == "Stage" || label == "Unstage" {
                                let staged = label == "Stage";
                                let enabled = app.selected_staging_allowed(staged);
                                let tooltip = if staged { "Stage all changes in the selected files" } else { "Unstage the selected files; preserve working-tree contents" };
                                if action_button(ui, icon, label, enabled, palette, action_width)
                                    .help_text(tooltip)
                                    .disabled_help_text("Select a suitable file after refreshing; conflicts must be resolved outside SannyGit")
                                    .clicked() {
                                    app.set_selected_staged(staged);
                                }
                            } else if label == "Log" {
                                let enabled = !app.busy()
                                    && app.repositories.get(app.selected_repository).is_some()
                                    && !app.history.commits.is_empty();
                                if action_button(ui, icon, label, enabled, palette, action_width)
                                    .help_text("Open the history of the current branch")
                                    .disabled_help_text("Open a repository and wait until its history is loaded")
                                    .clicked()
                                {
                                    app.open_current_branch_log();
                                    super::journal::focus_history(ui.ctx());
                                }
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
                    show_operation_cancel(ui, app, palette);
                });
            });
        });
}

fn operation_cancel_id() -> eframe::egui::Id {
    eframe::egui::Id::unique("toolbar.operation.cancel")
}

fn show_operation_cancel(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette) {
    if !(app.background_progress_only && app.manager.cancel.is_some()) {
        return;
    }
    let status = app.status.clone();
    let font = eframe::egui::TextStyle::Body.resolve(ui.style());
    let measured = ui
        .painter()
        .layout_no_wrap(status.clone(), font, palette.muted);
    let button = icons::BUTTON_SIZE;
    let spacing = ui.spacing().item_spacing.x;
    let available = ui.available_width().max(button);
    let width = (measured.size().x + spacing + button).min(available);
    let mut requested = false;
    ui.allocate_ui_with_layout(
        Vec2::new(width, button),
        Layout::left_to_right(Align::Center),
        |ui| {
            let text_width = (width - button - spacing).max(0.0);
            ui.scope(|ui| {
                ui.set_max_width(text_width);
                ui.add(
                    Label::new(RichText::new(&status).color(palette.muted))
                        .truncate()
                        .show_tooltip_when_elided(false),
                )
                .help_text(&status);
            });
            let response = icons::button(ui, Icon::Close, "Cancel the background operation");
            ui.ctx()
                .data_mut(|data| data.insert_temp(operation_cancel_id(), response.rect));
            requested = response.clicked();
        },
    );
    if requested {
        app.cancel_repository_job();
    }
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

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;
    use crate::{menu::EditAction, theme::ThemeChoice};
    use eframe::egui::{self, Context, Event, Id, OutputCommand, RawInput};

    #[test]
    fn edit_menu_preserves_target_selection_and_uses_text_edit_undo_history() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let context = Context::default();
            theme.apply(&context);
            let id = Id::unique("edit.menu.test.field");
            let mut text = "Selected text".to_owned();
            let mut frame = |action: Option<EditAction>, events: Vec<Event>| {
                let mut output = context.run_ui(
                    RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        if action.is_none() {
                            ui.memory_mut(|memory| memory.request_focus(id));
                        }
                        remember_edit_target(ui.ctx());
                        if let Some(action) = action {
                            // Opening a menu can move focus to its buttons.
                            ui.memory_mut(|memory| memory.request_focus(Id::unique("menu.button")));
                            perform_edit(ui.ctx(), action);
                        }
                        ui.add(crate::ui::controls::singleline(&mut text).id(id));
                    },
                );
                output.textures_delta.clear();
                (text.clone(), output)
            };
            frame(None, vec![]);
            frame(Some(EditAction::SelectAll), vec![]);
            let state = egui::text_edit::TextEditState::load(&context, id).unwrap();
            assert_eq!(
                state
                    .cursor
                    .char_range()
                    .unwrap()
                    .slice_str("Selected text"),
                "Selected text"
            );
            let (value, output) = frame(Some(EditAction::Copy), vec![]);
            assert_eq!(value, "Selected text");
            assert!(output.platform_output.commands.iter().any(|command| {
                matches!(command, OutputCommand::CopyText(text) if text == "Selected text")
            }));
            assert_eq!(frame(Some(EditAction::Cut), vec![]).0, "");
            assert_eq!(frame(Some(EditAction::Undo), vec![]).0, "Selected text");
            assert_eq!(frame(Some(EditAction::Redo), vec![]).0, "");
            let (_, output) = frame(Some(EditAction::Paste), vec![]);
            assert!(output.viewport_output.values().any(|viewport| {
                viewport
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::RequestPaste))
            }));
            assert_eq!(
                frame(None, vec![Event::Paste("Pasted text".into())]).0,
                "Pasted text"
            );
        }
    }

    #[test]
    fn edit_shortcuts_use_os_conventions_and_leave_unimplemented_items_alone() {
        use egui::{Key, Modifiers};
        for (label, key) in [
            ("Undo", Key::Z),
            ("Cut", Key::X),
            ("Copy", Key::C),
            ("Paste", Key::V),
            ("Select All", Key::A),
        ] {
            let shortcut = EditAction::from_label(label).unwrap().shortcut();
            assert_eq!(shortcut.logical_key, key);
            assert_eq!(shortcut.modifiers, Modifiers::CTRL);
        }
        let redo = EditAction::Redo.shortcut();
        assert_eq!(
            redo.logical_key,
            if cfg!(windows) { Key::Y } else { Key::Z }
        );
        assert_eq!(redo.modifiers.shift, !cfg!(windows));
        assert!(EditAction::from_label("Merge…").is_none());
        let context = Context::default();
        for action in [EditAction::Cut, EditAction::Paste, EditAction::SelectAll] {
            perform_edit(&context, action);
            assert!(context.memory(|memory| memory.focused()).is_none());
            assert!(context.input(|input| input.events.is_empty()));
        }
    }
}

#[cfg(test)]
mod operation_cancel {
    use super::*;
    use crate::theme::{InterfaceStyle, ThemeChoice};
    use eframe::egui::{
        self, epaint::ClippedShape, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Vec2,
    };
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    fn text_rects(shapes: &[ClippedShape], expected: &str) -> Vec<Rect> {
        shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == expected => {
                    Some(Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn operation_cancel_follows_the_message_on_the_toolbar() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for size in [Vec2::new(1100.0, 700.0), Vec2::new(1400.0, 900.0)] {
                    let ctx = Context::default();
                    theme.apply(&ctx);
                    theme.apply_style(&ctx, style);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.interface_style = style;
                    app.repositories.push(crate::models::Repository {
                        name: "Test".into(),
                        path: "D:/repositories/example".into(),
                    });
                    app.selected_repository = 0;
                    app.branches.current_branch = "main".into();
                    app.status = "Pushing…".into();
                    app.background_progress_only = true;
                    app.manager.cancel = Some(Arc::new(AtomicBool::new(false)));

                    let render = |app: &mut SannyGitApp, events: Vec<Event>| {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                events,
                                ..Default::default()
                            },
                            |ui| crate::ui::show(ui, app),
                        );
                        output.textures_delta.clear();
                        output
                    };

                    let output = render(&mut app, vec![]);
                    let messages = text_rects(&output.shapes, "Pushing…");
                    assert!(
                        messages.len() >= 2,
                        "Missing toolbar or status message in {theme:?} {style:?} at {size:?}: {}",
                        messages.len()
                    );
                    let toolbar_message = messages
                        .iter()
                        .min_by(|left, right| left.top().total_cmp(&right.top()))
                        .copied()
                        .expect("toolbar message");
                    let status_message = messages
                        .iter()
                        .max_by(|left, right| left.top().total_cmp(&right.top()))
                        .copied()
                        .expect("status message");
                    let cancel = ctx
                        .data(|data| data.get_temp::<Rect>(operation_cancel_id()))
                        .unwrap_or_else(|| {
                            panic!("Cancel cross missing in {theme:?} {style:?} at {size:?}")
                        });
                    assert!(
                        Rect::from_min_size(Pos2::ZERO, size).contains_rect(cancel),
                        "Cancel cross {cancel:?} leaves the {size:?} window"
                    );
                    assert!(
                        toolbar_message.right() <= cancel.left() + 1.0
                            && cancel.left() - toolbar_message.right() < 16.0,
                        "Cross is not immediately after the message in {theme:?} {style:?} at {size:?}: message {toolbar_message:?} cancel {cancel:?}"
                    );
                    assert!(
                        (cancel.center().y - toolbar_message.center().y).abs() < 20.0,
                        "Cross left the message row"
                    );
                    assert!(cancel.bottom() < 140.0, "Cross is outside the toolbar");
                    assert!(status_message.top() > size.y - 40.0);
                    assert!(
                        text_rects(&output.shapes, "Cancel")
                            .iter()
                            .all(|rect| (rect.center().y - cancel.center().y).abs() >= 24.0),
                        "Cancel is still a text button on the operation row"
                    );
                    let settings = text_rects(&output.shapes, "Settings")
                        .into_iter()
                        .find(|rect| (rect.center().y - cancel.center().y).abs() < 40.0)
                        .unwrap_or_else(|| panic!("Settings missing beside the cross"));
                    assert!(settings.center().x > cancel.right());

                    let pointer = cancel.center();
                    for pressed in [true, false] {
                        render(
                            &mut app,
                            vec![
                                Event::PointerMoved(pointer),
                                Event::PointerButton {
                                    pos: pointer,
                                    button: PointerButton::Primary,
                                    pressed,
                                    modifiers: Modifiers::NONE,
                                },
                            ],
                        );
                    }
                    assert!(
                        app.manager
                            .cancel
                            .as_ref()
                            .is_some_and(|cancel| cancel.load(Ordering::Relaxed)),
                        "Clicking the cross did not cancel"
                    );
                    assert_eq!(app.status, "Cancelling operation…");

                    app.manager.cancel = None;
                    app.background_progress_only = false;
                    app.status = "Idle".into();
                    let output = render(&mut app, vec![]);
                    assert!(text_rects(&output.shapes, "Pushing…").is_empty());
                    assert_eq!(text_rects(&output.shapes, "Idle").len(), 1);
                }
            }
        }
    }
}
