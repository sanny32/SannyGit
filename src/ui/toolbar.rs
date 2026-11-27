use super::controls::icons::{self, Icon};
#[cfg(not(target_os = "macos"))]
use super::controls::popup_menu;
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::SannyGitApp,
    theme::{semibold_family, ui_font_size, Palette, TOOLBAR_BRAND_FONT_SIZE},
};
use eframe::egui::{Align, Button, FontId, Frame, Layout, Margin, Panel, Rect, Response, Ui, Vec2};

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
        (label, ui_font_size(), Some(toolbar_caption(label))),
        enabled,
        palette,
        width,
        false,
        |ui, rect, color| {
            icons::image(icon, color, rect.width()).paint_at(ui, rect);
        },
    )
}

fn toolbar_caption(label: &str) -> String {
    toolbar_caption_key(label).map_or_else(|| crate::i18n::tr_msg(label), crate::i18n::tr_key)
}

fn toolbar_caption_key(label: &str) -> Option<&'static str> {
    Some(match label {
        "Pull" => "toolbar.pull",
        "Sync" => "toolbar.sync",
        "Push" => "toolbar.push",
        "Stage" => "toolbar.stage",
        "Unstage" => "toolbar.unstage",
        "Discard" => "toolbar.discard",
        "Save Stash" => "toolbar.stash",
        "Log" => "toolbar.log",
        "Merge" => "toolbar.merge",
        "Rebase" => "toolbar.rebase",
        "Settings" => "toolbar.settings",
        _ => return None,
    })
}

pub(super) fn toolbar_button(
    ui: &mut Ui,
    caption: (&str, f32, Option<String>),
    enabled: bool,
    palette: Palette,
    width: f32,
    selected: bool,
    paint_icon: impl FnOnce(&Ui, Rect, eframe::egui::Color32),
) -> Response {
    let (label, font_size, caption) = caption;
    let label = crate::i18n::tr_msg(label);
    let response = ui.add_enabled(
        enabled,
        Button::new("")
            .frame(false)
            .min_size(Vec2::new(width, TOOLBAR_BUTTON_HEIGHT)),
    );
    let info = label.clone();
    response.widget_info(|| {
        eframe::egui::WidgetInfo::selected(
            eframe::egui::Role::Button,
            response.enabled(),
            selected,
            info.clone(),
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
    let color = if enabled { palette.text } else { palette.muted };
    let mut job = eframe::egui::text::LayoutJob::simple_singleline(
        caption.unwrap_or_else(|| label.clone()),
        FontId::proportional(font_size),
        color,
    );
    job.wrap.max_width = (response.rect.width() - 4.0).max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.overflow_character = Some('…');
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let pos = response.rect.center_top()
        + Vec2::new(-galley.size().x / 2.0, TOOLBAR_BUTTON_PADDING_Y + 24.0)
        + press_offset;
    ui.painter().galley(pos, galley, color);
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
                        ui.menu_button(menu.title(), |ui| {
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
        item.edit_action.is_some()
            || item
                .action
                .is_some_and(|action| action.shortcut(&app.preferences.shortcuts).is_some())
    });
    if has_shortcut {
        ui.set_min_width(280.0);
    }
    for item in items {
        if item.separator {
            popup_menu::separator(ui);
        } else if !item.children.is_empty() {
            popup_menu::submenu(ui, &item.label(), |ui| {
                menu_items(ui, app, item.children);
            });
        } else {
            if let Some(edit) = item.edit_action {
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
                        Button::new(item.label())
                            .shortcut_text(ui.ctx().format_shortcut(&edit.shortcut())),
                    )
                    .disabled_help_text(crate::i18n::tr_msg(
                        "Focus a text field; Cut and Copy require selected text",
                    ))
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
            let mut button = Button::new(item.label());
            if let Some(shortcut) = item
                .action
                .and_then(|action| action.shortcut(&app.preferences.shortcuts))
            {
                button = button.shortcut_text(ui.ctx().format_shortcut(&shortcut));
            }
            let response = ui.add_enabled(enabled, button);
            if let Some(action) = item.action {
                if response.clicked() {
                    app.handle_menu_action(action, ui.ctx());
                    ui.close();
                }
            } else {
                response.disabled_help_text(crate::i18n::tr_msg("Not implemented yet"));
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
        crate::i18n::tr_msg("A simple Git client"),
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
                    (Icon::FileCheck, "Stage"),
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
                                    .disabled_help_text(crate::i18n::tr_msg("Open and refresh a repository; wait for the current operation"))
                                    .clicked()
                                {
                                    app.open_remote(operation);
                                }
                            } else if matches!(label, "Merge" | "Rebase") {
                                if action_button(ui, icon, label, app.merge_allowed(), palette, action_width)
                                    .help_text(crate::i18n::tr_msg(if label == "Rebase" { "Rebase HEAD onto a selected commit" } else { "Select a branch or commit to merge" }))
                                    .disabled_help_text(crate::i18n::tr_msg("Open and refresh a repository with commits; wait for the current operation"))
                                    .clicked() { if label == "Rebase" { app.open_rebase(); } else { app.open_merge(); } }
                            } else if label == "Discard" {
                                let blocker = app.discard_blocker();
                                if action_button(ui, icon, label, blocker.is_none(), palette, action_width)
                                    .help_text(crate::i18n::tr_msg("Review restoring selected files from the index or deleting selected untracked files"))
                                    .disabled_help_text(crate::i18n::tr_blocker(blocker))
                                    .clicked() { app.open_discard(); }
                            } else if label == "Save Stash" {
                                if action_button(ui, icon, label, app.stash_allowed(), palette, action_width)
                                    .help_text(crate::i18n::tr_msg("Save local changes for later"))
                                    .disabled_help_text(crate::i18n::tr_msg("Open and refresh a repository; wait for the current operation"))
                                    .clicked() { app.open_save_stash(); }
                            } else if label == "Stage" || label == "Unstage" {
                                let staged = label == "Stage";
                                let enabled = app.selected_staging_allowed(staged);
                                let tooltip = if staged {
                                    crate::i18n::tr_msg("Stage all changes in the selected files")
                                } else {
                                    crate::i18n::tr_msg("Unstage the selected files; preserve working-tree contents")
                                };
                                if action_button(ui, icon, label, enabled, palette, action_width)
                                    .help_text(&tooltip)
                                    .disabled_help_text(crate::i18n::tr_msg("Select a suitable file after refreshing; conflicts must be resolved outside SannyGit"))
                                    .clicked() {
                                    app.set_selected_staged(staged);
                                }
                            } else if label == "Log" {
                                let enabled = !app.blocks_reads()
                                    && app.repositories.get(app.selected_repository).is_some()
                                    && !app.history.commits.is_empty();
                                if action_button(ui, icon, label, enabled, palette, action_width)
                                    .help_text(crate::i18n::tr_msg("Open the history of the current branch"))
                                    .disabled_help_text(crate::i18n::tr_msg("Open a repository and wait until its history is loaded"))
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
                    .help_text(crate::i18n::tr_msg("Settings"));
                    if response.clicked() {
                        app.open_settings();
                    }
                });
            });
        });
}

fn remote_help(operation: crate::remotes::Operation, remote: Option<&str>) -> String {
    let repository = remote.map_or_else(
        || crate::i18n::tr_msg("remote repository"),
        |name| crate::i18n::tr_args("remote %{name} repository", &[("name", name)]),
    );
    let key = match operation {
        crate::remotes::Operation::Sync => {
            "Push local commits to the %{repository}, then pull possible remote changes (fast-forward only)."
        }
        crate::remotes::Operation::Fetch => "Fetch branches and tags from the %{repository}.",
        crate::remotes::Operation::Pull => {
            "Pull remote commits from the %{repository} into the current branch."
        }
        crate::remotes::Operation::Push => "Push local commits to the %{repository}.",
    };
    crate::i18n::tr_args(key, &[("repository", &repository)])
}

#[cfg(test)]
mod caption_tests {
    use super::*;
    use crate::theme::ThemeChoice;
    use eframe::egui::{self, Context, Pos2, RawInput};

    #[test]
    fn remote_tooltip_includes_the_selected_remote() {
        for locale in ["en", "ru"] {
            for operation in [
                crate::remotes::Operation::Pull,
                crate::remotes::Operation::Push,
                crate::remotes::Operation::Fetch,
                crate::remotes::Operation::Sync,
            ] {
                let text = remote_help(operation, Some("origin"));
                assert!(text.contains("origin") && !text.contains('{'), "{text}");
                let key = match operation {
                    crate::remotes::Operation::Pull => "Pull remote commits from the %{repository} into the current branch.",
                    crate::remotes::Operation::Push => "Push local commits to the %{repository}.",
                    crate::remotes::Operation::Fetch => "Fetch branches and tags from the %{repository}.",
                    crate::remotes::Operation::Sync => "Push local commits to the %{repository}, then pull possible remote changes (fast-forward only).",
                };
                assert!(crate::i18n::translate_for_test(locale, key).contains("%{repository}"));
            }
        }
    }

    #[test]
    fn toolbar_captions_fit_their_buttons_in_both_languages() {
        for locale in ["en", "ru"] {
            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut rects = Vec::new();
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1100.0, 700.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        ui.horizontal(|ui| {
                            for label in [
                                "Pull",
                                "Sync",
                                "Push",
                                "Stage",
                                "Unstage",
                                "Discard",
                                "Save Stash",
                                "Log",
                                "Merge",
                                "Rebase",
                                "Settings",
                            ] {
                                rects.push(
                                    toolbar_button(
                                        ui,
                                        (
                                            label,
                                            ui_font_size(),
                                            Some(crate::i18n::translate_for_test(
                                                locale,
                                                toolbar_caption_key(label).unwrap(),
                                            )),
                                        ),
                                        true,
                                        theme.palette(),
                                        58.0,
                                        false,
                                        |_, _, _| {},
                                    )
                                    .rect,
                                );
                            }
                        });
                    },
                );
                output.textures_delta.clear();
                let labels: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if !text.galley.job.text.is_empty() => Some(text),
                        _ => None,
                    })
                    .collect();
                assert_eq!(labels.len(), rects.len());
                for (text, button) in labels.iter().zip(&rects) {
                    let bounds = text.galley.rect.translate(text.pos.to_vec2());
                    assert!(
                        button.expand(0.5).contains_rect(bounds),
                        "{locale}: caption {:?} overflows {button:?}: {bounds:?}",
                        text.galley.job.text
                    );
                    assert!(
                        !text.galley.elided,
                        "{locale}: caption {:?} is truncated",
                        text.galley.job.text
                    );
                }
            }
        }
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
        for (key_id, key) in [
            ("edit.undo", Key::Z),
            ("edit.cut", Key::X),
            ("edit.copy", Key::C),
            ("edit.paste", Key::V),
            ("edit.select_all", Key::A),
        ] {
            let shortcut = EditAction::from_label_key(key_id).unwrap().shortcut();
            assert_eq!(shortcut.logical_key, key);
            assert_eq!(shortcut.modifiers, Modifiers::CTRL);
        }
        let redo = EditAction::Redo.shortcut();
        assert_eq!(
            redo.logical_key,
            if cfg!(windows) { Key::Y } else { Key::Z }
        );
        assert_eq!(redo.modifiers.shift, !cfg!(windows));
        assert!(EditAction::from_label_key("Merge…").is_none());
        let context = Context::default();
        for action in [EditAction::Cut, EditAction::Paste, EditAction::SelectAll] {
            perform_edit(&context, action);
            assert!(context.memory(|memory| memory.focused()).is_none());
            assert!(context.input(|input| input.events.is_empty()));
        }
    }
}
