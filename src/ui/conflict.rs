use super::controls::{
    dialog_style::{self, DialogResponse},
    icons::{self, Icon},
};
use crate::{
    app::{
        conflict::{Dialog, Layout, PaneCaret, WindowGeometry},
        SannyGitApp,
    },
    conflict::{Action, BlockEdit, CompareRow, Comparison, LineChange},
    theme::Palette,
};
use eframe::egui::{
    self, Button, CentralPanel, Frame, Id, Key, Margin, Modifiers, Panel, RichText, ScrollArea,
    TextEdit, Ui, ViewportCommand, ViewportId,
};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let Some(dialog) = &mut app.conflict_dialog else {
        return;
    };
    if dialog.action != Action::Edit {
        confirm_action(ctx, app);
        return;
    }

    let title = format!("{} - Conflict Solver", dialog.path);
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
                discard = ui.button("Discard Draft").clicked();
                if ui.button("Keep Editing").dialog_cancel() {
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
                        "Saving..."
                    } else {
                        "Loading conflict..."
                    }
                } else {
                    "Ready"
                };
                ui.label(state);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(90.0, rect.height()),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if let Some((line, column)) = dialog.cursor_position {
                                ui.label(format!("{line}:{column}"))
                                    .on_hover_text("Line and column");
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
                        .layout_no_wrap((*label).to_owned(), font.clone(), palette.text)
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
                    .on_hover_text("Save and stage the resolved file.")
                    .on_disabled_hover_text(if repository_active {
                        "Resolve all conflicts before saving and staging the file."
                    } else {
                        "Switch back to this file's repository to save and stage it."
                    })
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
        (label, style.font_size),
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
    .on_hover_text(
        "Compare Left and Right with their common base. Click again to return to the merge.",
    )
    .on_disabled_hover_text("The common base is unavailable or the Working Tree is being edited.")
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
                .add_enabled(clean, Button::new("Return to Merge"))
                .on_disabled_hover_text(
                    "Save the result or undo manual edits to return to block selection.",
                )
                .clicked()
            {
                dialog.draft = None;
                dialog.draft_comparison = None;
                dialog.draft_undo.clear();
                ui.close();
            }
        } else if ui.button("Edit Working Tree").clicked() {
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
        .on_hover_text(tooltip)
        .clicked()
        {
            dialog.layout = layout;
            dialog.scroll_y[1] = dialog.scroll_y[0];
        }
    }
}

fn change_view(layout: Layout) -> usize {
    match layout {
        Layout::OursResult => 0,
        Layout::ResultTheirs => 1,
        Layout::ThreeWay | Layout::ResultBelow => 2,
    }
}

fn change_navigation(ui: &mut Ui, dialog: &mut Dialog, style: ToolbarStyle, busy: bool) {
    if let Some(comparison) = &dialog.draft_comparison {
        let ranges = comparison.change_ranges[change_view(dialog.layout)].clone();
        draft_navigation(ui, dialog, style, busy, &ranges, false);
        return;
    }
    let (previous, next) = if let Some(comparison) = &dialog.comparison {
        let anchor = selected_range(dialog, comparison)
            .map(|range| range.start)
            .or(dialog.change_row);
        let ranges = &comparison.change_ranges[change_view(dialog.layout)];
        let previous = anchor.and_then(|anchor| {
            ranges
                .partition_point(|range| range.start < anchor)
                .checked_sub(1)
                .map(|index| ranges[index].start)
        });
        let next = ranges
            .get(anchor.map_or(0, |anchor| {
                ranges.partition_point(|range| range.start <= anchor)
            }))
            .map(|range| range.start);
        (previous, next)
    } else {
        (None, None)
    };
    let labels = if style.compact {
        ["Prev.", "Next"]
    } else {
        ["Prev. Change", "Next Change"]
    };
    if let Some(row) = navigation_buttons(
        ui,
        style,
        [previous, next],
        labels,
        ["Previous change", "Next change"],
        !busy && dialog.draft.is_none(),
        false,
    ) {
        dialog.change_row = Some(row);
        dialog.conflict_navigation_row = Some(row);
        dialog.jump_to_row = Some(row);
        if let Some(conflict) = dialog
            .comparison
            .as_ref()
            .and_then(|comparison| comparison.rows[row].conflict)
        {
            dialog.selected = Some(conflict);
        }
    }
}

fn conflict_navigation(ui: &mut Ui, dialog: &mut Dialog, style: ToolbarStyle, busy: bool) {
    if let Some(comparison) = &dialog.draft_comparison {
        let mut ranges = comparison.conflict_rows.clone();
        if ranges.is_empty() {
            ranges.extend(dialog.draft_recovered_spans.iter().flatten().map(|span| {
                let mut rows = comparison.rows_for_result_span(span);
                if rows.is_empty() && !comparison.rows.is_empty() {
                    rows.start = rows.start.min(comparison.rows.len() - 1);
                    rows.end = rows.start + 1;
                }
                rows
            }));
        }
        draft_navigation(ui, dialog, style, busy, &ranges, true);
        return;
    }
    let enabled = !busy && dialog.draft.is_none() && !dialog.show_base;
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
    let viewport_row = (dialog.scroll_y[0] / row_height) as usize;
    let targets = dialog.comparison.as_ref().map_or([None; 2], |comparison| {
        let row = dialog.conflict_navigation_row.unwrap_or(viewport_row);
        let has_unresolved = dialog.edits.iter().any(Option::is_none);
        let eligible =
            |index| !has_unresolved || dialog.edits.get(index).is_some_and(Option::is_none);
        [
            comparison
                .conflict_rows
                .iter()
                .enumerate()
                .rev()
                .find_map(|(index, range)| (range.end <= row && eligible(index)).then_some(index)),
            comparison
                .conflict_rows
                .iter()
                .enumerate()
                .find_map(|(index, range)| {
                    let after = if dialog.conflict_navigation_row.is_some() {
                        range.start > row
                    } else {
                        range.end > row
                    };
                    (after && eligible(index)).then_some(index)
                }),
        ]
    });
    let labels = if style.compact {
        ["Prev.", "Next"]
    } else {
        ["Prev. Conflict", "Next Conflict"]
    };
    if let Some(index) = navigation_buttons(
        ui,
        style,
        targets,
        labels,
        ["Previous conflict block", "Next conflict block"],
        enabled,
        true,
    ) {
        dialog.go_to_conflict(index);
    }
}

fn draft_caret_row(dialog: &Dialog, comparison: &Comparison) -> Option<usize> {
    let (_, line) = dialog.manual_cursor?;
    Some(
        comparison
            .result_row_boundaries
            .partition_point(|&boundary| boundary < line)
            .saturating_sub(1),
    )
}

fn draft_navigation(
    ui: &mut Ui,
    dialog: &mut Dialog,
    style: ToolbarStyle,
    busy: bool,
    ranges: &[std::ops::Range<usize>],
    conflict: bool,
) {
    let Some(comparison) = &dialog.draft_comparison else {
        return;
    };
    let anchor = dialog
        .draft_selection
        .as_ref()
        .map(|range| range.start)
        .or_else(|| draft_caret_row(dialog, comparison));
    let targets = [
        anchor.and_then(|row| {
            ranges
                .iter()
                .rposition(|range| !range.is_empty() && range.end <= row)
        }),
        ranges
            .iter()
            .position(|range| !range.is_empty() && anchor.is_none_or(|row| range.start > row)),
    ];
    let labels = if style.compact {
        ["Prev.", "Next"]
    } else if conflict {
        ["Prev. Conflict", "Next Conflict"]
    } else {
        ["Prev. Change", "Next Change"]
    };
    let tips = if conflict {
        ["Previous conflict block", "Next conflict block"]
    } else {
        ["Previous change", "Next change"]
    };
    if let Some(index) = navigation_buttons(ui, style, targets, labels, tips, !busy, conflict) {
        let range = ranges[index].clone();
        let offset = comparison.result_offset_at(range.start);
        dialog.manual_edit_start = Some(comparison.result[..offset].chars().count());
        dialog.jump_to_row = Some(range.start);
        dialog.draft_selection = Some(range);
    }
}

fn navigation_buttons(
    ui: &mut Ui,
    style: ToolbarStyle,
    targets: [Option<usize>; 2],
    labels: [&str; 2],
    tooltips: [&str; 2],
    enabled: bool,
    conflict: bool,
) -> Option<usize> {
    let order = if ui.layout().main_dir() == egui::Direction::RightToLeft {
        [1, 0]
    } else {
        [0, 1]
    };
    let mut selected = None;
    for index in order {
        let icon = ToolIcon::Navigate {
            previous: index == 0,
            conflict,
        };
        if tool(
            ui,
            style,
            icon,
            labels[index],
            enabled && targets[index].is_some(),
            false,
        )
        .on_hover_text(tooltips[index])
        .clicked()
        {
            selected = targets[index];
        }
    }
    selected
}

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
        .on_hover_text(tooltip)
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
fn documents(ui: &mut Ui, dialog: &mut Dialog, palette: Palette) {
    start_editing_on_input(ui, dialog);
    let Some(review) = &dialog.review else {
        return;
    };
    let Some(comparison) = dialog
        .draft_comparison
        .as_ref()
        .or(dialog.comparison.as_ref())
    else {
        ui.label(
            "This file has no editable text. Use Take Ours or Take Theirs from the file menu.",
        );
        return;
    };
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
    let jump = dialog.jump_to_row.take();
    let jump_range = jump
        .and_then(|_| selected_range(dialog, comparison))
        .cloned();

    let height = ui.available_height();
    let columns = match (
        dialog.edits.is_empty() && review.versions.iter().all(Option::is_none),
        dialog.layout,
    ) {
        (true, _) => &[2][..],
        (_, Layout::OursResult) => &[0, 2][..],
        (_, Layout::ThreeWay) => &[0, 2, 1][..],
        (_, Layout::ResultTheirs) => &[2, 1][..],
        (_, Layout::ResultBelow) => &[0, 1][..],
    };
    let navigated_position = jump.and_then(|index| {
        let row = comparison.rows.get(index)?;
        [2, 0, 1]
            .into_iter()
            .filter(|column| *column == 2 || columns.contains(column))
            .find_map(|column| {
                let line = row.lines[column].as_ref()?;
                Some((
                    PaneCaret {
                        column,
                        row: index,
                        character: 0,
                    },
                    (line.number, 1),
                ))
            })
    });
    if jump.is_some() {
        dialog.pane_caret = navigated_position.map(|(caret, _)| caret);
    }
    let below = columns == [0, 1];
    let top_height = if below { (height - 8.0) / 2.0 } else { height };
    let row_count = comparison.rows.len();
    let draft_jump_offset = |height, range: &std::ops::Range<usize>| {
        let boundaries = &comparison.result_row_boundaries;
        let position = centered_conflict_offset(
            boundaries[range.start]..boundaries[range.end],
            boundaries[row_count] + 1,
            row_height,
            height,
        ) / row_height;
        let aligned = boundaries
            .partition_point(|&value| value <= position as usize)
            .saturating_sub(1);
        (aligned as f32 + position.fract()) * row_height
    };
    let jump_offset = |height| {
        jump.map(|row| {
            jump_range.as_ref().map_or_else(
                || row.saturating_sub(2) as f32 * row_height,
                |range| centered_conflict_offset(range.clone(), row_count, row_height, height),
            )
        })
    };
    if let Some(offset) = jump_offset(top_height) {
        dialog.scroll_y = [offset; 2];
    }
    if let Some(range) = jump.and(dialog.draft_selection.as_ref()) {
        dialog.draft_navigation_offset = Some(draft_jump_offset(top_height, range));
    }
    let mut event = PaneEvent::default();
    let mut viewports = [None; 3];
    let spacing = ui.spacing().item_spacing.x;
    ui.spacing_mut().item_spacing.x = 30.0;
    ui.columns(columns.len(), |uis| {
        for (pane_ui, &column) in uis.iter_mut().zip(columns) {
            let outcome = pane(
                pane_ui,
                dialog,
                palette,
                column,
                top_height,
                dialog.scroll_y[0],
                row_height,
            );
            viewports[column] = outcome.viewport;
            event.merge(outcome);
        }
    });
    ui.spacing_mut().item_spacing.x = spacing;
    let mut ribbon_action = None;
    if let Some(comparison) = dialog
        .draft_comparison
        .as_ref()
        .or(dialog.comparison.as_ref())
    {
        for pair in columns.windows(2) {
            if let (Some(left), Some(right)) = (viewports[pair[0]], viewports[pair[1]]) {
                let (a, b) = (pair[0], pair[1]);
                let action = draw_connectors(
                    ui,
                    palette,
                    comparison,
                    (a, b),
                    [left, right],
                    row_height,
                    (!event.draft_changed).then_some(&*dialog),
                );
                ribbon_action = action.or(ribbon_action);
            }
        }
    }
    if let Some(offset) = event.scroll {
        if jump.is_none() {
            dialog.draft_navigation_offset = None;
        }
        dialog.scroll_y[0] = offset;
        if jump.is_none() {
            dialog.conflict_navigation_row = None;
        }
        ui.ctx().request_repaint();
    }
    if below {
        ui.add_space(8.0);
        let result_height = ui.available_height();
        if let Some(offset) = jump_offset(result_height) {
            dialog.scroll_y[1] = offset;
        }
        let result = pane(
            ui,
            dialog,
            palette,
            2,
            result_height,
            dialog.scroll_y[1],
            row_height,
        );
        if let Some(offset) = result.scroll {
            if jump.is_none() {
                dialog.draft_navigation_offset = None;
            }
            dialog.scroll_y[1] = offset;
            if jump.is_none() {
                dialog.conflict_navigation_row = None;
            }
            ui.ctx().request_repaint();
        }
        event.merge(result);
    }
    if let Some(row) = event.activated_row.filter(|_| dialog.draft.is_none()) {
        dialog.selected = dialog
            .comparison
            .as_ref()
            .and_then(|comparison| comparison.rows[row].conflict);
        dialog.change_row = Some(row);
        dialog.conflict_navigation_row = Some(row);
        dialog.pane_caret = event.caret;
        let id = working_tree_caret_id();
        ui.memory_mut(|memory| {
            if event.caret.is_some_and(|caret| caret.column == 2) && !dialog.show_base {
                memory.interested_in_focus(id, ui.layer_id());
                memory.request_focus(id);
            } else {
                memory.surrender_focus(id);
            }
        });
        ui.ctx().request_repaint();
    }
    if let Some(position) = event
        .cursor_position
        .or(navigated_position.map(|(_, position)| position))
    {
        if dialog.cursor_position != Some(position) {
            dialog.cursor_position = Some(position);
            ui.ctx().request_repaint();
        }
    }
    if event.draft_changed {
        dialog.draft_text_changed();
        ui.ctx().request_repaint();
    }
    match ribbon_action {
        Some(RibbonAction::TakeConflict(index, side)) => dialog.take_conflict(index, side),
        Some(RibbonAction::UndoConflict(index)) => dialog.undo_conflict(index),
        Some(RibbonAction::EditBlock(edit)) => {
            if dialog.draft.is_some() {
                dialog.apply_draft_block(edit);
            } else {
                dialog.apply_block(edit);
            }
        }
        Some(RibbonAction::UndoBlock(index)) => {
            if dialog.draft.is_some() {
                dialog.undo_draft_block(index);
            } else {
                dialog.undo_block(index);
            }
        }
        None => {}
    }
}

fn selected_range<'a>(
    dialog: &'a Dialog,
    comparison: &'a Comparison,
) -> Option<&'a std::ops::Range<usize>> {
    if dialog.draft.is_some() {
        dialog.draft_selection.as_ref()
    } else if let Some(row) = dialog.change_row {
        let ranges = &comparison.change_ranges[change_view(dialog.layout)];
        let index = ranges.partition_point(|range| range.end <= row);
        ranges.get(index).filter(|range| range.contains(&row))
    } else {
        dialog
            .selected
            .and_then(|index| comparison.conflict_rows.get(index))
    }
}

fn pane_scroll_height(height: f32) -> f32 {
    (height - 30.0).max(30.0)
}

fn centered_conflict_offset(
    range: std::ops::Range<usize>,
    row_count: usize,
    row_height: f32,
    pane_height: f32,
) -> f32 {
    let viewport_height = pane_scroll_height(pane_height);
    let block_center = (range.start as f32 + range.len() as f32 / 2.0) * row_height;
    let max_offset = (row_count as f32 * row_height - viewport_height).max(0.0);
    (block_center - viewport_height / 2.0).clamp(0.0, max_offset)
}

#[derive(Default)]
struct PaneEvent {
    scroll: Option<f32>,
    activated_row: Option<usize>,
    caret: Option<PaneCaret>,
    cursor_position: Option<(usize, usize)>,
    viewport: Option<PaneViewport>,
    draft_changed: bool,
}

#[derive(Clone, Copy)]
struct PaneViewport {
    rect: egui::Rect,
    offset: f32,
    draft: bool,
}

impl PaneViewport {
    fn row_y(self, row: usize, comparison: &Comparison, row_height: f32) -> f32 {
        let row = if self.draft {
            comparison.result_row_boundaries[row]
        } else {
            row
        };
        self.rect.top() - self.offset + row as f32 * row_height
    }
}

fn draft_scroll_offset(comparison: &Comparison, offset: f32, row_height: f32) -> f32 {
    let row = offset / row_height;
    let index = (row as usize).min(comparison.rows.len());
    let boundaries = &comparison.result_row_boundaries;
    let start = boundaries[index];
    let end = boundaries.get(index + 1).copied().unwrap_or(start);
    (start as f32 + row.fract() * (end - start) as f32) * row_height
}

fn pane_change(
    comparison: &Comparison,
    row: &CompareRow,
    layout: Layout,
    column: usize,
) -> Option<LineChange> {
    match (layout, column) {
        (Layout::OursResult, 2) => row.changes[0],
        (Layout::ResultTheirs, 2) => row.changes[1],
        (Layout::ResultBelow, 0 | 1) => comparison.between_sides(row),
        (_, 2) => row.changes[0].or(row.changes[1]),
        (_, side) => row.changes[side],
    }
}

fn change_style(
    palette: Palette,
    change: LineChange,
) -> (egui::Color32, egui::Color32, &'static str) {
    match change {
        LineChange::Added => (palette.added_background, palette.added_text, "+"),
        LineChange::Removed => (palette.removed_background, palette.removed_text, "−"),
        LineChange::Replaced => (palette.modified_background, palette.text, "~"),
    }
}

enum RibbonAction {
    TakeConflict(usize, usize),
    UndoConflict(usize),
    EditBlock(BlockEdit),
    UndoBlock(usize),
}

fn draw_connectors(
    ui: &Ui,
    palette: Palette,
    comparison: &Comparison,
    columns: (usize, usize),
    panes: [PaneViewport; 2],
    row_height: f32,
    dialog: Option<&Dialog>,
) -> Option<RibbonAction> {
    let [left, right] = panes;
    let (a, b) = columns;
    let clip = egui::Rect::from_min_max(
        egui::pos2(left.rect.right(), left.rect.top().max(right.rect.top())),
        egui::pos2(
            right.rect.left(),
            left.rect.bottom().min(right.rect.bottom()),
        ),
    )
    .intersect(ui.clip_rect());
    if !clip.is_positive() {
        return None;
    }
    let painter = ui.painter().with_clip_rect(clip);
    let first = panes
        .iter()
        .map(|pane| {
            let row = (pane.offset / row_height).floor() as usize;
            if pane.draft {
                comparison
                    .result_row_boundaries
                    .partition_point(|&boundary| boundary < row)
            } else {
                row
            }
        })
        .min()
        .unwrap_or(0);
    let last = panes
        .iter()
        .map(|pane| {
            let row = ((pane.offset + pane.rect.height()) / row_height).ceil() as usize;
            if pane.draft {
                comparison
                    .result_row_boundaries
                    .partition_point(|&boundary| boundary <= row)
            } else {
                row
            }
        })
        .max()
        .unwrap_or(0)
        .saturating_add(1)
        .min(comparison.rows.len());
    let side = if a == 2 || b == 2 { a.min(b) } else { 2 };
    let runs = &comparison.connectors[side];
    let controls = dialog.filter(|dialog| side < 2 && !dialog.show_base);
    let mut action = None;
    let mut shown_conflict = None;
    for run in runs
        .iter()
        .skip(runs.partition_point(|run| run.rows.end <= first))
    {
        if run.rows.start >= last {
            break;
        }
        let start = run.rows.start;
        let end = run.rows.end;
        let change = run.change;
        let occupied = [a, b].map(|column| {
            let present = if side < 2 {
                match change {
                    LineChange::Added => column == 2,
                    LineChange::Removed => column != 2,
                    LineChange::Replaced => true,
                }
            } else {
                match change {
                    LineChange::Added => column == 1,
                    LineChange::Removed => column == 0,
                    LineChange::Replaced => true,
                }
            };
            start..if present { end } else { start }
        });
        let bands: [[f32; 2]; 2] = std::array::from_fn(|side| {
            let range = &occupied[side];
            let top = panes[side].row_y(range.start, comparison, row_height);
            [
                top,
                panes[side]
                    .row_y(range.end, comparison, row_height)
                    .max(top + 1.0),
            ]
        });
        let fill = change_style(palette, change).0;
        let points = (0..=12)
            .map(|step| {
                let t = step as f32 / 12.0;
                let eased = t * t * (3.0 - 2.0 * t);
                let x = egui::lerp(left.rect.right()..=right.rect.left(), t);
                let top = egui::lerp(bands[0][0]..=bands[1][0], eased);
                let bottom = egui::lerp(bands[0][1]..=bands[1][1], eased);
                egui::epaint::BandPoint::new(x, top..=bottom)
            })
            .collect();
        painter.add(egui::epaint::BandShape::filled(points, fill));
        let Some(dialog) = controls else {
            continue;
        };
        if let Some(index) = run.conflict {
            if shown_conflict == Some(index)
                || (dialog.draft.is_none()
                    && dialog.conflict_undo[index]
                        .as_ref()
                        .is_some_and(|undo| undo.side == side || dialog.layout != Layout::ThreeWay))
            {
                continue;
            }
            shown_conflict = Some(index);
            if ribbon_button(
                ui,
                palette,
                clip,
                bands,
                ("take_conflict", side, index),
                if side == 0 {
                    "Take Ours for this conflict"
                } else {
                    "Take Theirs for this conflict"
                },
                Some(a != 2),
            ) {
                action = if let Some(draft) = &dialog.draft {
                    dialog
                        .review
                        .as_ref()
                        .and_then(|review| review.draft_conflict_edit(draft, index, side, false))
                        .map(RibbonAction::EditBlock)
                } else {
                    Some(RibbonAction::TakeConflict(index, side))
                };
            }
        } else if let Some(target) = comparison
            .block_target(run)
            .filter(|target| dialog.draft.is_some() || dialog.can_edit_block(target))
        {
            if ribbon_button(
                ui,
                palette,
                clip,
                bands,
                ("apply_block", side, start),
                if side == 0 {
                    "Apply Ours to this block"
                } else {
                    "Apply Theirs to this block"
                },
                Some(a != 2),
            ) {
                if let Some(edit) = dialog
                    .draft
                    .as_deref()
                    .or_else(|| {
                        dialog
                            .review
                            .as_ref()
                            .and_then(|review| review.text.as_deref())
                    })
                    .and_then(|text| comparison.block_edit(run, side, text, target))
                {
                    action = Some(RibbonAction::EditBlock(edit));
                }
            }
        }
    }

    if let Some(dialog) = controls {
        let undo_bands = |rows: &std::ops::Range<usize>| {
            panes.map(|pane| {
                let first = rows.start.min(comparison.rows.len().saturating_sub(1));
                let top = pane.row_y(first, comparison, row_height);
                [
                    top,
                    pane.row_y(rows.end, comparison, row_height)
                        .max(top + row_height),
                ]
            })
        };
        if dialog.draft.is_some() {
            for (index, undo) in dialog.draft_undo.iter().enumerate() {
                if (undo.side == side || dialog.layout != Layout::ThreeWay)
                    && ribbon_button(
                        ui,
                        palette,
                        clip,
                        undo_bands(&comparison.rows_for_result_span(&undo.span)),
                        ("undo_draft_block", side, index),
                        "Undo the edit to this block",
                        None,
                    )
                {
                    action = Some(RibbonAction::UndoBlock(index));
                }
            }
            return action;
        }
        for (index, undo) in dialog.conflict_undo.iter().enumerate() {
            if undo
                .as_ref()
                .is_some_and(|undo| undo.side == side || dialog.layout != Layout::ThreeWay)
                && ribbon_button(
                    ui,
                    palette,
                    clip,
                    undo_bands(&comparison.conflict_rows[index]),
                    ("undo_conflict", side, index),
                    "Undo the choice for this conflict",
                    None,
                )
            {
                action = Some(RibbonAction::UndoConflict(index));
            }
        }
        for (index, (edit, rows)) in dialog
            .block_edits
            .iter()
            .zip(&comparison.applied_rows)
            .enumerate()
        {
            if (edit.side == side || dialog.layout != Layout::ThreeWay)
                && ribbon_button(
                    ui,
                    palette,
                    clip,
                    undo_bands(rows),
                    ("undo_block", side, index),
                    "Undo the edit to this block",
                    None,
                )
            {
                action = Some(RibbonAction::UndoBlock(index));
            }
        }
    }
    action
}

fn ribbon_button(
    ui: &Ui,
    palette: Palette,
    clip: egui::Rect,
    bands: [[f32; 2]; 2],
    id: (&'static str, usize, usize),
    label: &str,
    toward_right: Option<bool>,
) -> bool {
    let middle = |edge| (bands[0][edge] + bands[1][edge]) / 2.0;
    let top = middle(0).max(clip.top());
    let bottom = middle(1).min(clip.bottom());
    let size = 17.0;
    if bottom <= top || clip.width() < size || clip.height() < size {
        return false;
    }
    let y = ((top + bottom) / 2.0).clamp(clip.top() + size / 2.0, clip.bottom() - size / 2.0);
    let rect =
        egui::Rect::from_center_size(egui::pos2(clip.center().x, y), egui::Vec2::splat(size));
    let response = ui.interact(rect, ui.make_persistent_id(id), egui::Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, response.enabled(), label));
    let fill = if response.hovered() || response.has_focus() {
        palette.selection
    } else {
        palette.panel_alt
    };
    ui.painter().rect_filled(rect, 3.0, fill);
    let stroke = if response.has_focus() {
        palette.accent
    } else {
        palette.border
    };
    ui.painter()
        .rect_stroke(rect, 3.0, (1.0, stroke), egui::StrokeKind::Inside);
    let color = if response.enabled() {
        palette.text
    } else {
        palette.muted
    };
    let image = if let Some(right) = toward_right {
        let angle = if right {
            -std::f32::consts::FRAC_PI_2
        } else {
            std::f32::consts::FRAC_PI_2
        };
        icons::image(Icon::Chevron, color, 14.0).rotate(angle, egui::vec2(0.5, 0.5))
    } else {
        icons::image(Icon::Fetch, color, 12.0)
    };
    image.paint_at(ui, rect.shrink(2.0));
    response.on_hover_text(label).clicked()
}

impl PaneEvent {
    fn merge(&mut self, other: Self) {
        self.scroll = other.scroll.or(self.scroll);
        self.activated_row = other.activated_row.or(self.activated_row);
        self.caret = other.caret.or(self.caret);
        self.cursor_position = other.cursor_position.or(self.cursor_position);
        self.draft_changed |= other.draft_changed;
    }
}

fn pane(
    ui: &mut Ui,
    dialog: &mut Dialog,
    palette: Palette,
    column: usize,
    height: f32,
    offset: f32,
    row_height: f32,
) -> PaneEvent {
    let width = ui.available_width();
    let mut event = PaneEvent::default();
    let header = Frame::new()
        .fill(palette.panel_alt)
        .inner_margin(Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.set_min_width((width - 14.0).max(0.0));
            let result = if dialog.show_base {
                "Common Base"
            } else {
                "Working Tree"
            };
            ui.strong(["Ours", "Theirs", result][column]);
        })
        .response
        .rect;
    if column == 2 && !dialog.show_base {
        working_tree_menu(ui, dialog, header);
    }
    let Some(original) = &dialog.comparison else {
        return event;
    };
    let area_height = pane_scroll_height(height);
    if column == 2 {
        if let (Some(draft), Some(comparison)) = (&mut dialog.draft, &dialog.draft_comparison) {
            let start = dialog.manual_edit_start.take();
            let draft_offset = draft_scroll_offset(
                comparison,
                dialog.draft_navigation_offset.unwrap_or(offset),
                row_height,
            );
            let scrolled = ScrollArea::both()
                .id_salt("conflict.manual")
                .auto_shrink([false; 2])
                .max_height(area_height)
                .vertical_scroll_offset(draft_offset)
                .show(ui, |ui| {
                    let background = ui.painter().add(egui::Shape::Noop);
                    let editor_id = ui.make_persistent_id("conflict.manual.editor");
                    if let Some(character) = start {
                        let mut state = if dialog.manual_cursor.is_some() {
                            egui::text_edit::TextEditState::load(ui.ctx(), editor_id)
                                .unwrap_or_default()
                        } else {
                            egui::text_edit::TextEditState::default()
                        };
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(
                                egui::text::CCursor::new(character),
                            )));
                        state.store(ui.ctx(), editor_id);
                        ui.memory_mut(|memory| memory.request_focus(editor_id));
                    }
                    let mut layout_text = |ui: &Ui, text: &dyn egui::TextBuffer, _width| {
                        let mut job = egui::text::LayoutJob::simple(
                            text.as_str().to_owned(),
                            egui::TextStyle::Monospace.resolve(ui.style()),
                            palette.text,
                            f32::INFINITY,
                        );
                        job.keep_trailing_whitespace = true;
                        for section in &mut job.sections {
                            section.format.line_height = Some(row_height);
                        }
                        ui.fonts_mut(|fonts| fonts.layout_job(job))
                    };
                    let output = TextEdit::multiline(draft)
                        .id(editor_id)
                        .background_color(egui::Color32::TRANSPARENT)
                        .margin(Margin {
                            left: 54,
                            right: 4,
                            top: 0,
                            bottom: 0,
                        })
                        .min_size(egui::vec2(width, area_height))
                        .desired_width(f32::INFINITY)
                        .code_editor()
                        .font(egui::TextStyle::Monospace)
                        .layouter(&mut layout_text)
                        .show(ui);
                    if let Some(character) = start.filter(|_| dialog.draft_selection.is_none()) {
                        let rect = output
                            .galley
                            .pos_from_cursor(egui::text::CCursor::new(character))
                            .translate(output.galley_pos.to_vec2());
                        ui.scroll_to_rect(rect, Some(egui::Align::Center));
                    }
                    let changed = output.response.changed();
                    if changed {
                        event.draft_changed = true;
                    }
                    paint_draft_changes(
                        ui,
                        palette,
                        dialog.layout,
                        comparison,
                        &output,
                        background,
                        dialog.draft_selection.as_ref(),
                    );
                    if let Some(caret) = output.cursor_range.filter(|_| output.response.has_focus())
                    {
                        let index = usize::from(caret.primary.index);
                        if changed
                            || start.is_some()
                            || dialog.manual_cursor.map(|(cursor, _)| cursor) != Some(index)
                        {
                            if start.is_none() {
                                dialog.draft_selection = None;
                            }
                            let position = text_position(draft, index);
                            dialog.manual_cursor = Some((index, position.0));
                            event.cursor_position = Some(position);
                        }
                    }
                    output.galley_pos.y
                });
            event.viewport = Some(PaneViewport {
                rect: scrolled.inner_rect,
                offset: scrolled.inner_rect.top() - scrolled.inner,
                draft: true,
            });
            let actual = scrolled.state.clamped_offset().y;
            if (actual - draft_offset).abs() > 0.5
                && (start.is_some()
                    || event.cursor_position.is_some()
                    || ui.rect_contains_pointer(scrolled.inner_rect))
            {
                let row = actual / row_height;
                let aligned = comparison
                    .result_row_boundaries
                    .partition_point(|&boundary| boundary <= row as usize)
                    .saturating_sub(1);
                event.scroll = Some((aligned as f32 + row.fract()) * row_height);
            }
            return event;
        }
    }
    let comparison = dialog.draft_comparison.as_ref().unwrap_or(original);
    let source = if column == 2 {
        &comparison.result
    } else {
        &comparison.sources[column]
    };
    let mut row_origin = None;
    let scrolled = Frame::new()
        .fill(palette.editor)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ScrollArea::both()
                .id_salt(("conflict.pane", column))
                .auto_shrink([false; 2])
                .max_height(area_height)
                .vertical_scroll_offset(offset)
                .show_rows(ui, row_height, comparison.rows.len(), |ui, rows| {
                    let selected_range = selected_range(dialog, comparison);
                    let click = ui.input(|input| {
                        input
                            .pointer
                            .interact_pos()
                            .filter(|_| input.pointer.primary_clicked())
                    });
                    row_origin = Some(ui.next_widget_position().y - rows.start as f32 * row_height);
                    for index in rows {
                        let row = &comparison.rows[index];
                        let position = ui.next_widget_position();
                        let background = egui::Rect::from_min_size(
                            position,
                            egui::vec2(width.max(ui.clip_rect().right() - position.x), row_height),
                        );
                        let change = row.lines[column]
                            .as_ref()
                            .and_then(|_| pane_change(comparison, row, dialog.layout, column));
                        let change_style = change.map(|change| change_style(palette, change));
                        let fill = if let Some((fill, _, _)) = change_style {
                            fill
                        } else if row.marker && column == 2 {
                            palette.hunk_background
                        } else {
                            palette.editor
                        };
                        if let Some(range) = selected_range.filter(|range| range.contains(&index)) {
                            let painter = ui.painter();
                            painter.rect_filled(
                                background,
                                0.0,
                                fill.lerp_to_gamma(palette.selection, 0.55),
                            );
                            painter.rect_filled(
                                egui::Rect::from_min_size(
                                    egui::pos2(ui.clip_rect().left(), position.y),
                                    egui::vec2(3.0, row_height),
                                ),
                                0.0,
                                palette.accent,
                            );
                            for (edge, y) in [
                                (range.start, background.top() + 0.5),
                                (range.end - 1, background.bottom() - 0.5),
                            ] {
                                if index == edge {
                                    painter.hline(background.x_range(), y, (1.0, palette.accent));
                                }
                            }
                        } else if change_style.is_some() || (row.marker && column == 2) {
                            ui.painter().rect_filled(background, 0.0, fill);
                        }
                        if let Some((_, mark, symbol)) = change_style {
                            ui.painter().text(
                                position + egui::vec2(7.0, row_height / 2.0),
                                egui::Align2::CENTER_CENTER,
                                symbol,
                                egui::TextStyle::Monospace.resolve(ui.style()),
                                mark,
                            );
                        }
                        let row_click = click
                            .filter(|_| ui.is_enabled() && ui.rect_contains_pointer(background));
                        if row_click.is_some() {
                            event.activated_row = Some(index);
                        }
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let line = row.lines[column].as_ref();
                                let number =
                                    line.map(|line| line.number.to_string()).unwrap_or_default();
                                ui.add_sized(
                                    [48.0, row_height],
                                    egui::Label::new(
                                        RichText::new(number).monospace().color(palette.muted),
                                    )
                                    .selectable(false),
                                );
                                if let Some(line) = line {
                                    let text = &source[line.span.clone()];
                                    let response = ui.add(
                                        egui::Label::new(RichText::new(text).monospace())
                                            .extend()
                                            .selectable(row.conflict.is_none())
                                            .sense(if row.conflict.is_some() {
                                                egui::Sense::click()
                                            } else {
                                                egui::Sense::hover()
                                            }),
                                    );
                                    let active_caret = dialog.pane_caret.filter(|caret| {
                                        caret.column == column && caret.row == index
                                    });
                                    if row_click.is_some() || active_caret.is_some() {
                                        let galley = ui.painter().layout_no_wrap(
                                            text.to_owned(),
                                            egui::TextStyle::Monospace.resolve(ui.style()),
                                            palette.text,
                                        );
                                        let character = if let Some(pointer) = row_click {
                                            let cursor = galley.cursor_from_pos(egui::vec2(
                                                pointer.x - response.rect.left(),
                                                galley.size().y / 2.0,
                                            ));
                                            let character = usize::from(cursor.index);
                                            event.caret = Some(PaneCaret {
                                                column,
                                                row: index,
                                                character,
                                            });
                                            event.cursor_position =
                                                Some((line.number, character + 1));
                                            character
                                        } else {
                                            active_caret.map_or(0, |caret| caret.character)
                                        };
                                        let caret = galley
                                            .pos_from_cursor(egui::text::CCursor::new(character))
                                            .translate(response.rect.min.to_vec2());
                                        ui.painter().rect_filled(
                                            egui::Rect::from_min_size(
                                                egui::pos2(caret.left(), position.y + 2.0),
                                                egui::vec2(1.5, row_height - 4.0),
                                            ),
                                            0.0,
                                            palette.text,
                                        );
                                    }
                                }
                            },
                        );
                    }
                })
        })
        .inner;
    let changed = (scrolled.state.clamped_offset().y - offset).abs() > 0.5;
    event.viewport = Some(PaneViewport {
        rect: scrolled.inner_rect,
        offset: row_origin.map_or(0.0, |origin| scrolled.inner_rect.top() - origin),
        draft: false,
    });
    if changed && ui.rect_contains_pointer(scrolled.inner_rect.expand(14.0)) {
        event.scroll = Some(scrolled.state.clamped_offset().y);
    }
    event
}

fn paint_draft_changes(
    ui: &Ui,
    palette: Palette,
    layout: Layout,
    comparison: &Comparison,
    editor: &egui::text_edit::TextEditOutput,
    background: egui::layers::ShapeIdx,
    selection: Option<&std::ops::Range<usize>>,
) {
    let bounds = editor.response.rect.intersect(ui.clip_rect());
    let mut shapes = vec![egui::Shape::rect_filled(bounds, 0.0, palette.editor)];
    let rows = &editor.galley.rows;
    let y = editor.galley_pos.y;
    let first = rows.partition_point(|row| y + row.rect().bottom() <= bounds.top());
    let gutter = egui::Rect::from_min_max(
        bounds.min,
        egui::pos2(bounds.left() + 48.0, bounds.bottom()),
    )
    .intersect(bounds);
    let gutter_painter = ui.painter().with_clip_rect(gutter);
    gutter_painter.rect_filled(gutter, 0.0, palette.panel_alt);
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let first_aligned = comparison
        .result_row_boundaries
        .partition_point(|&number| number < first);
    let mut lines = comparison.rows[first_aligned.min(comparison.rows.len())..]
        .iter()
        .filter(|row| row.lines[2].is_some());
    let selected_lines = selection.map(|range| {
        let boundaries = &comparison.result_row_boundaries;
        boundaries[range.start]..boundaries[range.end]
    });
    for (index, row) in rows.iter().enumerate().skip(first) {
        let rect = row.rect().translate(editor.galley_pos.to_vec2());
        if rect.top() >= bounds.bottom() {
            break;
        }
        gutter_painter.text(
            egui::pos2(gutter.right() - 7.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            (index + 1).to_string(),
            font.clone(),
            palette.muted,
        );
        let Some(line) = lines.next() else { continue };
        let change = pane_change(comparison, line, layout, 2);
        let fill = change
            .map(|change| change_style(palette, change).0)
            .or_else(|| line.marker.then_some(palette.hunk_background));
        let selected = selected_lines
            .as_ref()
            .is_some_and(|range| range.contains(&index));
        if selected || fill.is_some() {
            let fill = if selected {
                fill.unwrap_or(palette.editor)
                    .lerp_to_gamma(palette.selection, 0.55)
            } else {
                fill.unwrap_or(palette.editor)
            };
            let band = egui::Rect::from_x_y_ranges(bounds.x_range(), rect.y_range());
            shapes.push(egui::Shape::rect_filled(band.intersect(bounds), 0.0, fill));
        }
    }
    if let Some(range) = selected_lines {
        let boundary_y = |index: usize| {
            rows.get(index).map_or_else(
                || rows.last().map_or(0.0, |row| row.rect().bottom()),
                |row| row.rect().top(),
            ) + y
        };
        for edge in [range.start, range.end] {
            let edge_y = boundary_y(edge);
            if bounds.y_range().contains(edge_y) {
                shapes.push(egui::Shape::hline(
                    bounds.x_range(),
                    edge_y,
                    (1.0, palette.accent),
                ));
            }
        }
    }
    ui.painter().set(background, egui::Shape::Vec(shapes));
}

fn text_position(text: &str, index: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    let mut chars = text.chars().peekable();
    for _ in 0..index {
        match chars.next() {
            Some('\n') => {
                line += 1;
                column = 1;
            }
            Some('\r') if chars.peek() == Some(&'\n') => {}
            Some(_) => column += 1,
            None => break,
        }
    }
    (line, column)
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
            let message = match (&dialog.review, dialog.action) {
                (Some(review), Action::Ours) if !review.stages[2] => {
                    "Ours deleted this file. Confirm to remove it and stage the deletion."
                }
                (Some(review), Action::Theirs) if !review.stages[3] => {
                    "Theirs deleted this file. Confirm to remove it and stage the deletion."
                }
                _ => "This replaces the current working copy.",
            };
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
                close = ui.add_enabled(!busy, Button::new("Cancel")).dialog_cancel();
            });
        });
    });
    if submit {
        app.apply_conflict();
    } else if close {
        app.conflict_dialog = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_scroll_centers_the_block_and_clamps_to_document_bounds() {
        assert_eq!(centered_conflict_offset(40..50, 100, 20.0, 430.0), 700.0);
        assert_eq!(centered_conflict_offset(40..50, 100, 20.0, 230.0), 800.0);
        assert_eq!(centered_conflict_offset(0..4, 100, 20.0, 430.0), 0.0);
        assert_eq!(centered_conflict_offset(96..100, 100, 20.0, 430.0), 1600.0);
        assert_eq!(centered_conflict_offset(3..7, 10, 20.0, 430.0), 0.0);
    }

    #[test]
    fn cursor_position_counts_characters_and_handles_crlf() {
        let text = "é🦀\r\nβ";
        assert_eq!(text_position(text, 0), (1, 1));
        assert_eq!(text_position(text, 2), (1, 3));
        assert_eq!(text_position(text, 3), (1, 3));
        assert_eq!(text_position(text, 4), (2, 1));
        assert_eq!(text_position(text, 5), (2, 2));
    }

    #[test]
    fn clicking_the_ribbon_applies_and_undoes_only_its_block() {
        use crate::{git, test_support::TempRepo};
        use egui::{Event, Modifiers, PointerButton};

        let repo = TempRepo::new();
        repo.write("file", "start\nend\n");
        let oid = repo.git(&["hash-object", "-w", "file"]);
        let entries = format!("100644 {0} 2\tfile\n100644 {0} 3\tfile\n", oid.trim());
        git::run_with_input(
            &repo.0,
            &["update-index", "--index-info"],
            Some(entries.as_bytes()),
        )
        .unwrap();
        repo.write("file", "start\nlocal change\nend\n");
        let review = crate::conflict::prepare(&repo.0, "file", Action::Edit).unwrap();
        let mut dialog = Dialog::new(repo.0.clone(), "file".into(), Action::Edit);
        dialog.set_review(std::sync::Arc::new(review));
        let palette = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light).palette();
        let context = egui::Context::default();
        let viewport = |x| PaneViewport {
            rect: egui::Rect::from_min_size(egui::pos2(x, 30.0), egui::vec2(100.0, 180.0)),
            offset: 0.0,
            draft: false,
        };
        let render = |dialog: &Dialog, events| {
            let mut action = None;
            let mut output = context.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    action = draw_connectors(
                        ui,
                        palette,
                        dialog
                            .draft_comparison
                            .as_ref()
                            .or(dialog.comparison.as_ref())
                            .unwrap(),
                        (0, 2),
                        [
                            viewport(10.0),
                            PaneViewport {
                                draft: dialog.draft.is_some(),
                                ..viewport(140.0)
                            },
                        ],
                        18.0,
                        Some(dialog),
                    );
                },
            );
            output.textures_delta.clear();
            action
        };
        let click = |dialog: &Dialog, y| {
            let pos = egui::pos2(125.0, y);
            let event = |pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            render(dialog, vec![Event::PointerMoved(pos), event(true)]);
            render(dialog, vec![event(false)])
        };

        render(&dialog, vec![]);
        let Some(RibbonAction::EditBlock(edit)) = click(&dialog, 53.0) else {
            panic!("the insertion ribbon did not apply its edit");
        };
        dialog.apply_block(edit);
        assert_eq!(dialog.comparison.as_ref().unwrap().result, "start\nend\n");
        render(&dialog, vec![]);
        let Some(RibbonAction::UndoBlock(index)) = click(&dialog, 57.0) else {
            panic!("the undo button did not address the applied block");
        };
        dialog.undo_block(index);
        assert!(!dialog.dirty());
        assert_eq!(
            dialog.comparison.as_ref().unwrap().result,
            "start\nlocal change\nend\n"
        );

        dialog.begin_draft(0);
        dialog
            .draft
            .as_mut()
            .unwrap()
            .insert_str(0, "ручная правка\n");
        dialog.draft_text_changed();
        render(&dialog, vec![]);
        let Some(RibbonAction::EditBlock(edit)) = click(&dialog, 71.0) else {
            panic!("the insertion ribbon did not apply its edit in the draft");
        };
        dialog.apply_draft_block(edit);
        assert_eq!(dialog.draft.as_deref(), Some("ручная правка\nstart\nend\n"));
        dialog
            .draft
            .as_mut()
            .unwrap()
            .insert_str(0, "second edit\n");
        dialog.draft_text_changed();
        render(&dialog, vec![]);
        let Some(RibbonAction::UndoBlock(index)) = click(&dialog, 93.0) else {
            panic!("the undo button did not follow the edited block in the draft");
        };
        dialog.undo_draft_block(index);
        assert_eq!(
            dialog.draft.as_deref(),
            Some("second edit\nручная правка\nstart\nlocal change\nend\n")
        );
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "start\nlocal change\nend\n"
        );
    }

    #[test]
    fn links_insertions_and_removals_to_a_point_and_replacements_to_full_rows() {
        let source = "start\nremove\ncontext one\nold\ncontext two\nend\n";
        let result = "start\ncontext one\nnew\ncontext two\ninsert one\ninsert two\nend\n";
        let comparison = Comparison::new(result, &[], &[], [Some(source), Some(result)], &[]);
        let app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        let palette = app.palette();
        let viewport = |x| PaneViewport {
            rect: egui::Rect::from_min_size(egui::pos2(x, 30.0), egui::vec2(100.0, 180.0)),
            offset: 0.0,
            draft: false,
        };
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            draw_connectors(
                ui,
                palette,
                &comparison,
                (0, 2),
                [viewport(10.0), viewport(140.0)],
                18.0,
                None,
            );
        });
        output.textures_delta.clear();
        let bands: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Band(band) => Some(band),
                _ => None,
            })
            .collect();
        let extents: Vec<_> = bands
            .iter()
            .map(|band| {
                (
                    band.fill,
                    band.points.first().unwrap().y.span(),
                    band.points.last().unwrap().y.span(),
                )
            })
            .collect();
        assert_eq!(
            extents,
            [
                (palette.removed_background, 18.0, 1.0),
                (palette.modified_background, 18.0, 18.0),
                (palette.added_background, 1.0, 36.0),
            ]
        );
    }
}
