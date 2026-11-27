use super::controls::dialog_style::{self, DialogResponse};
use crate::{
    app::{
        conflict::{Dialog, Layout, WindowGeometry},
        SannyGitApp,
    },
    conflict::{Action, BlockEdit, Comparison, LineChange},
    theme::Palette,
};
use eframe::egui::{
    self, Button, CentralPanel, Frame, Id, Key, Margin, Modifiers, Panel, RichText, ScrollArea, Ui,
    ViewportCommand, ViewportId,
};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let Some(dialog) = &mut app.conflict_dialog else {
        return;
    };
    if dialog.action != Action::Edit {
        confirm_action(ctx, app);
        return;
    }

    let title = crate::i18n::tr_args("%{file} - Conflict Solver", &[("file", &dialog.path)]);
    if ctx.embed_viewports() {
        let size = ctx.content_rect().size() - egui::vec2(36.0, 55.0);
        let mut open = true;
        egui::Window::new(title)
            .id(Id::unique("conflict_solver.embedded"))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .open(&mut open)
            .fixed_size(size.max(egui::vec2(360.0, 240.0)))
            .show(ctx, |ui| {
                ui.take_available_space();
                workspace(ui, app);
            });
        if !open {
            request_close(ctx, app);
        }
        return;
    }

    let geometry = dialog.initial_window_geometry.get_or_insert_with(|| {
        let fallback_size = ctx.viewport_rect().size();
        ctx.input(|input| {
            let viewport = input.viewport();
            WindowGeometry {
                inner_size: viewport
                    .inner_rect
                    .map_or(fallback_size, |rect| rect.size()),
                outer_position: viewport.outer_rect.map(|rect| rect.min),
                maximized: viewport.maximized.unwrap_or(false),
            }
        })
    });
    let mut builder = crate::theme::native_viewport()
        .with_title(&title)
        .with_inner_size(geometry.inner_size)
        .with_maximized(geometry.maximized)
        .with_min_inner_size([850.0, 500.0])
        .with_clamp_size_to_monitor_size(true)
        .with_decorations(true);
    if let Some(position) = geometry.outer_position {
        builder = builder.with_position(position);
    }
    ctx.show_viewport_immediate(
        ViewportId::from_hash_of("sannygit.conflict_solver"),
        builder,
        |ui, _| {
            #[cfg(target_os = "macos")]
            ui.input_mut(|input| input.events.append(&mut app.pending_native_edit_events));
            let palette = app.palette();
            crate::theme::paint_native_background(ui, palette);
            #[cfg(target_os = "windows")]
            if let Err(error) = crate::native_window::finish_first_presentation(ui.ctx()) {
                app.error = Some(error.to_string());
            }

            let requested = ui.input(|input| input.viewport().close_requested());
            if requested {
                ui.ctx().send_viewport_cmd(ViewportCommand::CancelClose);
                request_close(ui.ctx(), app);
            }
            workspace(ui, app);
            if let Some(dialog) = &mut app.conflict_dialog {
                if !dialog.revealed {
                    dialog.revealed = true;
                    #[cfg(target_os = "windows")]
                    if let Err(error) =
                        crate::native_window::prepare_child_presentation(ui.ctx(), &title)
                    {
                        app.error = Some(error.to_string());
                    }
                    ui.ctx().send_viewport_cmd(ViewportCommand::Visible(true));
                }
                super::show_errors(ui, app);
            }
        },
    );
}

fn request_close(ctx: &egui::Context, app: &mut SannyGitApp) {
    if app.busy() || app.error.is_some() || !app.error_dialogs.pending.is_empty() {
        return;
    }
    if let Some(dialog) = &mut app.conflict_dialog {
        if dialog.dirty() {
            dialog.confirm_close = true;
        } else {
            close(ctx, app);
        }
    }
}

fn close(ctx: &egui::Context, app: &mut SannyGitApp) {
    app.conflict_dialog = None;
    if !ctx.embed_viewports() {
        #[cfg(target_os = "windows")]
        crate::native_window::cancel_first_presentation(ctx);
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}

fn workspace(ui: &mut Ui, app: &mut SannyGitApp) {
    let busy = app.busy();
    let palette = app.palette();
    let repository = app.repositories.get(app.selected_repository);
    let Some(dialog) = &mut app.conflict_dialog else {
        return;
    };
    let repository_active =
        repository.is_some_and(|repository| dialog.root == std::path::Path::new(&repository.path));
    let mut save = false;
    let mut discard = false;
    let blocked =
        dialog.confirm_close || app.error.is_some() || !app.error_dialogs.pending.is_empty();

    if !busy && !blocked && !egui::Popup::is_any_open(ui.ctx()) {
        let shortcut = egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
        save = repository_active
            && dialog.ready()
            && ui.input_mut(|input| input.consume_shortcut(&shortcut));
    }
    ui.add_enabled_ui(!blocked, |ui| {
        Panel::top("conflict.toolbar")
            .frame(
                Frame::new()
                    .fill(palette.header)
                    .inner_margin(Margin::symmetric(8, 4)),
            )
            .show(ui, |ui| {
                save |= toolbar(ui, dialog, palette, busy, repository_active);
            });
        status_bar(ui, dialog, palette, busy);
        CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(palette.background)
                    .inner_margin(Margin::same(8)),
            )
            .show(ui, |ui| {
                ui.add_enabled_ui(!busy, |ui| documents(ui, dialog, palette));
            });
    });

    if dialog.confirm_close {
        dialog_style::modal(ui.ctx(), palette, "conflict.discard_draft").show(ui.ctx(), |ui| {
            ui.set_width(420.0);
            dialog_style::header(
                ui,
                palette,
                "Discard resolution draft?",
                "Your changes in Conflict Solver will be lost.",
            );
            dialog_style::button_row(ui, |ui| {
                discard = ui.button(crate::i18n::tr_msg("Discard Draft")).clicked();
                if ui
                    .button(crate::i18n::tr_msg("Keep Editing"))
                    .dialog_cancel()
                {
                    dialog.confirm_close = false;
                }
            });
        });
    }

    if discard {
        close(ui.ctx(), app);
    } else if save {
        app.apply_conflict();
    }
}

fn status_bar(ui: &mut Ui, dialog: &Dialog, palette: Palette, busy: bool) {
    Panel::bottom("conflict.status")
        .exact_size(22.0)
        .frame(
            Frame::new()
                .fill(palette.panel_alt)
                .inner_margin(Margin::symmetric(8, 0)),
        )
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter()
                .hline(ui.clip_rect().x_range(), rect.top(), (1.0, palette.border));
            ui.horizontal_centered(|ui| {
                let state = if busy {
                    if dialog.review.is_some() {
                        crate::i18n::tr_msg("Saving...")
                    } else {
                        crate::i18n::tr_msg("Loading conflict...")
                    }
                } else {
                    crate::i18n::tr_msg("Ready")
                };
                ui.label(state);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(90.0, rect.height()),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if let Some((line, column)) = dialog.cursor_position {
                                ui.label(format!("{line}:{column}"))
                                    .on_hover_text(crate::i18n::tr_msg("Line and column"));
                            }
                        },
                    );
                    ui.separator();
                });
            });
        });
}

#[derive(Clone, Copy)]
struct ToolbarStyle {
    palette: Palette,
    width: f32,
    compact: bool,
    font_size: f32,
}

fn toolbar(
    ui: &mut Ui,
    dialog: &mut Dialog,
    palette: Palette,
    busy: bool,
    repository_active: bool,
) -> bool {
    const BUTTONS: f32 = 14.0;
    const GAP: f32 = 2.0;
    const DIVIDER: f32 = 6.0;
    // Three dividers, the spacing within both groups and the gap before the right group.
    let space = 3.0 * DIVIDER + 16.0 * GAP + 4.0;
    let available = ((ui.available_width() - space) / BUTTONS).floor().max(38.0);
    let font = egui::FontId::proportional(crate::theme::ui_font_size());
    let (full_width, compact_width) = ui.fonts_mut(|fonts| {
        let mut caption_width = |labels: &[&str]| {
            labels
                .iter()
                .map(|label| {
                    fonts
                        .layout_no_wrap(crate::i18n::tr_msg(label), font.clone(), palette.text)
                        .size()
                        .x
                })
                .fold(0.0_f32, f32::max)
                .ceil()
                + 2.0
        };
        let full = caption_width(&[
            "Base Changes",
            "Take Left, Right",
            "Take Right, Left",
            "Prev. Conflict",
            "Next Conflict",
            "Merge + Right",
        ]);
        (
            full,
            caption_width(&["L + Merge", "Merge + R", "L + Base", "Base + R"]),
        )
    });
    let compact = available < full_width * 0.92;
    let width = available.min(full_width);
    let caption_width = if compact { compact_width } else { full_width };
    let style = ToolbarStyle {
        palette,
        width,
        compact,
        font_size: font.size * (width / caption_width).min(1.0),
    };
    let right_width = 2.0 * style.width + GAP;
    let left_width = (ui.available_width() - right_width - 2.0 * GAP).max(0.0);
    ui.spacing_mut().item_spacing.x = GAP;
    ui.horizontal_centered(|ui| {
        let save = ScrollArea::horizontal()
            .id_salt("conflict.toolbar.scroll")
            .max_width(left_width)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    base_control(ui, dialog, style, busy);
                    let save = tool(
                        ui,
                        style,
                        ToolIcon::Save,
                        "Save",
                        !busy && repository_active && dialog.ready(),
                        false,
                    )
                    .on_hover_text(crate::i18n::tr_msg("Save and stage the resolved file."))
                    .on_disabled_hover_text(crate::i18n::tr_msg(if repository_active {
                        "Resolve all conflicts before saving and staging the file."
                    } else {
                        "Switch back to this file's repository to save and stage it."
                    }))
                    .clicked();
                    ui.add(egui::Separator::default().spacing(DIVIDER));
                    change_navigation(ui, dialog, style, busy);
                    ui.add(egui::Separator::default().spacing(DIVIDER));
                    resolution_controls(ui, dialog, style, busy);
                    ui.add(egui::Separator::default().spacing(DIVIDER));
                    layout_controls(ui, dialog, style, busy);
                    save
                })
                .inner
            })
            .inner;

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            conflict_navigation(ui, dialog, style, busy);
        });
        save
    })
    .inner
}

#[derive(Clone, Copy)]
enum ToolIcon {
    Base,
    Save,
    Navigate { previous: bool, conflict: bool },
    Take { first: usize, both: bool },
    Panes(Layout),
}

fn tool(
    ui: &mut Ui,
    style: ToolbarStyle,
    icon: ToolIcon,
    label: &str,
    enabled: bool,
    selected: bool,
) -> egui::Response {
    super::toolbar::toolbar_button(
        ui,
        (label, style.font_size, None),
        enabled,
        style.palette,
        style.width,
        selected,
        |ui, rect, color| {
            paint_tool_icon(
                ui,
                rect,
                color,
                style.palette,
                enabled && ui.is_enabled(),
                icon,
            )
        },
    )
}

fn paint_tool_icon(
    ui: &Ui,
    rect: egui::Rect,
    color: egui::Color32,
    palette: Palette,
    enabled: bool,
    icon: ToolIcon,
) {
    let painter = ui.painter();
    let point = |x, y| rect.lerp_inside(egui::vec2(x, y));
    let panel = |x, y, right, bottom| egui::Rect::from_min_max(point(x, y), point(right, bottom));
    match icon {
        ToolIcon::Base => {
            let outline = if enabled { palette.text } else { color };
            let root = if enabled { palette.accent } else { color };
            let stroke = egui::Stroke::new(1.65, outline);
            painter.line_segment([point(0.5, 0.29), point(0.5, 0.46)], stroke);
            painter.add(egui::Shape::line(
                vec![
                    point(0.24, 0.60),
                    point(0.24, 0.46),
                    point(0.76, 0.46),
                    point(0.76, 0.60),
                ],
                stroke,
            ));
            painter.circle_filled(point(0.5, 0.19), rect.width() * 0.11, root);
            for x in [0.24, 0.76] {
                painter.rect_stroke(
                    panel(x - 0.17, 0.60, x + 0.17, 0.94),
                    1.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
                painter.line_segment([point(x - 0.06, 0.77), point(x + 0.06, 0.77)], stroke);
            }
        }
        ToolIcon::Save => {
            let outline = if enabled { palette.text } else { color };
            let shutter = if enabled { palette.accent } else { color };
            let stroke = egui::Stroke::new(1.65, outline);
            painter.add(egui::Shape::closed_line(
                vec![
                    point(0.10, 0.09),
                    point(0.72, 0.09),
                    point(0.90, 0.27),
                    point(0.90, 0.91),
                    point(0.10, 0.91),
                ],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![
                    point(0.27, 0.14),
                    point(0.27, 0.35),
                    point(0.65, 0.35),
                    point(0.65, 0.14),
                ],
                (1.65, shutter),
            ));
            painter.add(egui::Shape::line(
                vec![
                    point(0.28, 0.91),
                    point(0.28, 0.57),
                    point(0.72, 0.57),
                    point(0.72, 0.91),
                ],
                stroke,
            ));
        }
        ToolIcon::Navigate { previous, conflict } => {
            let color = if enabled && conflict {
                palette.warning
            } else {
                color
            };
            let (tip, tail, shoulder) = if previous {
                (0.14, 0.87, 0.44)
            } else {
                (0.87, 0.14, 0.57)
            };
            painter.line_segment([point(0.5, tail), point(0.5, shoulder)], (2.1, color));
            painter.add(egui::Shape::convex_polygon(
                vec![
                    point(0.5, tip),
                    point(0.24, shoulder),
                    point(0.76, shoulder),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        ToolIcon::Take { first, both } => {
            let accent = if enabled { palette.incoming } else { color };
            let outline = if enabled { palette.text } else { color };
            painter.line_segment([point(0.50, 0.10), point(0.50, 0.90)], (1.5, outline));
            for (order, (side, y)) in [(first, if both { 0.26 } else { 0.50 }), (1 - first, 0.74)]
                .into_iter()
                .take(if both { 2 } else { 1 })
                .enumerate()
            {
                let x = |left| if side == 0 { left } else { 1.0 - left };
                let start = x(if both { 0.21 } else { 0.06 });
                let shoulder = x(0.32);
                painter.line_segment([point(start, y), point(shoulder, y)], (1.7, accent));
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        point(x(0.45), y),
                        point(shoulder, y - 0.13),
                        point(shoulder, y + 0.13),
                    ],
                    accent,
                    egui::Stroke::NONE,
                ));
                if both {
                    painter.text(
                        point(x(0.05), y),
                        egui::Align2::CENTER_CENTER,
                        if order == 0 { "1" } else { "2" },
                        egui::FontId::monospace(8.5),
                        outline,
                    );
                }
            }
        }
        ToolIcon::Panes(layout) => {
            let accent = if enabled { palette.accent } else { color };
            paint_layout_icon(ui, rect, color, accent, layout);
        }
    }
}

fn document_icon(ui: &Ui, rect: egui::Rect, border: egui::Color32, lines: egui::Color32) {
    ui.painter()
        .rect_stroke(rect, 0.5, (1.0, border), egui::StrokeKind::Inside);
    let point = |x, y| rect.lerp_inside(egui::vec2(x, y));
    for (y, right) in [(0.30, 0.75), (0.52, 0.75), (0.74, 0.58)] {
        ui.painter()
            .line_segment([point(0.24, y), point(right, y)], (0.85, lines));
    }
}
fn paint_layout_icon(
    ui: &Ui,
    rect: egui::Rect,
    color: egui::Color32,
    accent: egui::Color32,
    layout: Layout,
) {
    let rect = rect.shrink2(egui::vec2(0.5, 3.0));
    let (columns, result) = match layout {
        Layout::OursResult => (2, 1),
        Layout::ThreeWay => (3, 1),
        Layout::ResultTheirs => (2, 0),
        Layout::ResultBelow => (2, 2),
    };
    let bottom = layout == Layout::ResultBelow;
    let pane_bottom = if bottom {
        rect.center().y
    } else {
        rect.bottom()
    };
    let width = rect.width() / columns as f32;
    for column in 0..columns {
        let left = rect.left() + column as f32 * width;
        let pane = egui::Rect::from_min_max(
            egui::pos2(left, rect.top()),
            egui::pos2(left + width, pane_bottom),
        );
        document_icon(
            ui,
            pane.shrink2(egui::vec2(0.5, 0.0)),
            color,
            if column == result { accent } else { color },
        );
    }
    if bottom {
        let pane = egui::Rect::from_min_max(
            egui::pos2(rect.left(), pane_bottom + 1.0),
            rect.right_bottom(),
        );
        document_icon(ui, pane, color, accent);
    }
}
fn base_control(ui: &mut Ui, dialog: &mut Dialog, style: ToolbarStyle, busy: bool) {
    let available = dialog
        .review
        .as_ref()
        .is_some_and(|review| review.versions[0].is_some());
    if tool(
        ui,
        style,
        ToolIcon::Base,
        if style.compact {
            "Base"
        } else {
            "Base Changes"
        },
        !busy && dialog.draft.is_none() && available,
        dialog.show_base,
    )
    .on_hover_text(crate::i18n::tr_msg(
        "Compare Left and Right with their common base. Click again to return to the merge.",
    ))
    .on_disabled_hover_text(crate::i18n::tr_msg(
        "The common base is unavailable or the Working Tree is being edited.",
    ))
    .clicked()
    {
        dialog.toggle_base();
    }
}

fn working_tree_menu(ui: &Ui, dialog: &mut Dialog, header: egui::Rect) {
    let response = ui.interact(
        header,
        ui.make_persistent_id("conflict.working_tree.menu"),
        egui::Sense::click(),
    );
    response.context_menu(|ui| {
        if let Some(draft) = &dialog.draft {
            let clean = dialog
                .comparison
                .as_ref()
                .is_some_and(|comparison| *draft == comparison.result);
            if ui
                .add_enabled(clean, Button::new(crate::i18n::tr_msg("Return to Merge")))
                .on_disabled_hover_text(crate::i18n::tr_msg(
                    "Save the result or undo manual edits to return to block selection.",
                ))
                .clicked()
            {
                dialog.draft = None;
                dialog.draft_comparison = None;
                dialog.draft_undo.clear();
                ui.close();
            }
        } else if ui
            .button(crate::i18n::tr_msg("Edit Working Tree"))
            .clicked()
        {
            dialog.begin_draft(0);
            ui.close();
        }
    });
}
fn working_tree_caret_id() -> Id {
    Id::unique("conflict.working_tree.caret")
}

fn start_editing_on_input(ui: &Ui, dialog: &mut Dialog) {
    let id = working_tree_caret_id();
    if !ui.is_enabled()
        || dialog.show_base
        || dialog.draft.is_some()
        || egui::Popup::is_any_open(ui.ctx())
        || !ui.memory(|memory| memory.has_focus(id))
    {
        return;
    }
    ui.memory_mut(|memory| memory.interested_in_focus(id, ui.layer_id()));
    if !ui.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Text(_)
                    | egui::Event::Paste(_)
                    | egui::Event::Ime(_)
                    | egui::Event::Key {
                        key: Key::Backspace | Key::Delete | Key::Enter,
                        pressed: true,
                        ..
                    }
            )
        })
    }) {
        return;
    }
    let Some(caret) = dialog.pane_caret.filter(|caret| caret.column == 2) else {
        return;
    };
    let Some(comparison) = dialog
        .draft_comparison
        .as_ref()
        .or(dialog.comparison.as_ref())
    else {
        return;
    };
    if let Some(line) = comparison
        .rows
        .get(caret.row)
        .and_then(|row| row.lines[2].as_ref())
    {
        let character = comparison.result[..line.span.start].chars().count() + caret.character;
        dialog.begin_draft(character);
    }
}

fn layout_controls(ui: &mut Ui, dialog: &mut Dialog, style: ToolbarStyle, busy: bool) {
    let enabled = !busy
        && dialog.comparison.is_some()
        && (!dialog.edits.is_empty()
            || dialog
                .review
                .as_ref()
                .is_some_and(|review| review.versions.iter().any(Option::is_some)));
    for (layout, label, tooltip) in [
        (
            Layout::OursResult,
            if style.compact {
                if dialog.show_base {
                    "L + Base"
                } else {
                    "L + Merge"
                }
            } else if dialog.show_base {
                "Left + Base"
            } else {
                "Left + Merge"
            },
            "Show Ours and Working Tree side by side.",
        ),
        (
            Layout::ThreeWay,
            "All",
            "Show Ours, Working Tree and Theirs side by side.",
        ),
        (
            Layout::ResultTheirs,
            if style.compact {
                if dialog.show_base {
                    "Base + R"
                } else {
                    "Merge + R"
                }
            } else if dialog.show_base {
                "Base + Right"
            } else {
                "Merge + Right"
            },
            "Show Working Tree and Theirs side by side.",
        ),
        (
            Layout::ResultBelow,
            if style.compact {
                "Below"
            } else if dialog.show_base {
                "Base Below"
            } else {
                "Merge Below"
            },
            "Show Working Tree below Ours and Theirs.",
        ),
    ] {
        if tool(
            ui,
            style,
            ToolIcon::Panes(layout),
            label,
            enabled,
            dialog.layout == layout,
        )
        .on_hover_text(crate::i18n::tr_msg(tooltip))
        .clicked()
        {
            dialog.layout = layout;
            dialog.scroll_y[1] = dialog.scroll_y[0];
        }
    }
}

mod comparison;
use comparison::documents;
mod navigation;
use navigation::{change_navigation, conflict_navigation, draft_caret_row};

fn resolution_controls(ui: &mut Ui, dialog: &mut Dialog, style: ToolbarStyle, busy: bool) {
    let enabled = !busy && !dialog.show_base;
    let draft = dialog.draft.as_deref();
    let comparison = dialog
        .draft_comparison
        .as_ref()
        .or(dialog.comparison.as_ref());
    let draft_row = draft.and_then(|_| {
        dialog
            .draft_selection
            .as_ref()
            .map(|range| range.start)
            .or_else(|| draft_caret_row(dialog, comparison?))
    });
    let row = if draft.is_some() {
        draft_row
    } else {
        dialog.change_row
    };
    let conflict = if draft.is_none() && row.is_none() {
        dialog.selected.filter(|&index| index < dialog.edits.len())
    } else {
        row.and_then(|row| comparison?.rows.get(row)?.conflict)
    };
    let recovered = draft_row.and_then(|row| {
        let offset = comparison?.result_offset_at(row);
        dialog
            .draft_recovered_spans
            .iter()
            .enumerate()
            .find_map(|(index, span)| {
                let span = span.as_ref()?;
                (span.contains(&offset) || (span.is_empty() && span.start == offset))
                    .then_some((index, span))
            })
    });
    let selected_block = |side: usize| {
        if matches!(
            (dialog.layout, side),
            (Layout::OursResult, 1) | (Layout::ResultTheirs, 0)
        ) {
            return None;
        }
        let row = row?;
        let comparison = comparison?;
        let connectors = &comparison.connectors[side];
        let index = connectors.partition_point(|run| run.rows.end <= row);
        let run = connectors
            .get(index)
            .filter(|run| run.rows.contains(&row))
            .or_else(|| {
                let previous = connectors.get(index.checked_sub(1)?)?;
                let boundaries = &comparison.result_row_boundaries;
                (draft.is_some()
                    && previous.change == LineChange::Removed
                    && boundaries[previous.rows.start] == boundaries[row])
                    .then_some(previous)
            })?;
        let target = comparison.block_target(run)?;
        (draft.is_some() || dialog.can_edit_block(&target)).then_some((run, target))
    };
    let available = [selected_block(0).is_some(), selected_block(1).is_some()];
    let mut choice = None;
    for (side, both, icon, label, short, tooltip) in [
        (
            0,
            true,
            ToolIcon::Take {
                first: 0,
                both: true,
            },
            "Take Left, Right",
            "L → R",
            "Take both sides for this conflict, Left first.",
        ),
        (
            0,
            false,
            ToolIcon::Take {
                first: 0,
                both: false,
            },
            "Take Left",
            "Left",
            "Take the Left (Ours) version of this block.",
        ),
        (
            1,
            false,
            ToolIcon::Take {
                first: 1,
                both: false,
            },
            "Take Right",
            "Right",
            "Take the Right (Theirs) version of this block.",
        ),
        (
            1,
            true,
            ToolIcon::Take {
                first: 1,
                both: true,
            },
            "Take Right, Left",
            "R → L",
            "Take both sides for this conflict, Right first.",
        ),
    ] {
        let can_take = conflict.is_some() || recovered.is_some() || (!both && available[side]);
        if tool(
            ui,
            style,
            icon,
            if style.compact { short } else { label },
            enabled && can_take,
            false,
        )
        .on_hover_text(crate::i18n::tr_msg(tooltip))
        .clicked()
        {
            choice = Some((side, both));
        }
    }
    if let Some((side, both)) = choice {
        if let Some(draft) = draft {
            let edit = if let Some(index) = conflict {
                dialog
                    .review
                    .as_ref()
                    .and_then(|review| review.draft_conflict_edit(draft, index, side, both))
            } else if let Some((index, span)) = recovered {
                dialog
                    .review
                    .as_ref()
                    .and_then(|review| review.conflict_sides(index))
                    .map(|sides| BlockEdit {
                        original: span.clone(),
                        replacement: crate::conflict::take_sides(sides, side, both),
                        side,
                    })
            } else {
                selected_block(side)
                    .and_then(|(run, target)| comparison?.block_edit(run, side, draft, target))
            };
            if let Some(edit) = edit {
                dialog.apply_draft_block(edit);
            }
        } else if let Some(index) = conflict {
            dialog.take_conflict_sides(index, side, both);
        } else if let Some((run, target)) = selected_block(side) {
            if let (Some(comparison), Some(original)) = (
                &dialog.comparison,
                dialog
                    .review
                    .as_ref()
                    .and_then(|review| review.text.as_deref()),
            ) {
                if let Some(edit) = comparison.block_edit(run, side, original, target) {
                    dialog.apply_block(edit);
                }
            }
        }
    }
}
fn confirm_action(ctx: &egui::Context, app: &mut SannyGitApp) {
    let busy = app.busy();
    let palette = app.palette();
    let Some(dialog) = &app.conflict_dialog else {
        return;
    };
    let mut submit = false;
    let mut close = false;
    dialog_style::modal(ctx, palette, "working_tree.conflict_action").show(ctx, |ui| {
        ui.set_width(460.0);
        dialog_style::header(
            ui,
            palette,
            dialog.action.label(),
            dialog.action.description(),
        );
        dialog_style::section(ui, palette.panel, Margin::same(10), |ui| {
            ui.add(egui::Label::new(RichText::new(&dialog.path).strong()).truncate())
                .on_hover_text(&dialog.path);
            let message = crate::i18n::tr_msg(match (&dialog.review, dialog.action) {
                (Some(review), Action::Ours) if !review.stages[2] => {
                    "Ours deleted this file. Confirm to remove it and stage the deletion."
                }
                (Some(review), Action::Theirs) if !review.stages[3] => {
                    "Theirs deleted this file. Confirm to remove it and stage the deletion."
                }
                _ => "This replaces the current working copy.",
            });
            ui.allocate_ui(egui::vec2(ui.available_width(), 46.0), |ui| {
                ui.set_min_height(46.0);
                ui.label(message);
            });
            dialog_style::button_row(ui, |ui| {
                submit = ui
                    .add_enabled(
                        !busy && dialog.ready(),
                        dialog_style::primary(dialog.action.label(), palette),
                    )
                    .clicked();
                close = ui
                    .add_enabled(!busy, Button::new(crate::i18n::tr_msg("Cancel")))
                    .dialog_cancel();
            });
        });
    });
    if submit {
        app.apply_conflict();
    } else if close {
        app.conflict_dialog = None;
    }
}
