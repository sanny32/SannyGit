use super::controls::tooltips::TooltipResponse;
use eframe::egui::{
    epaint::text::ByteRangeExt, Align, Button, Frame, Label, Layout, Margin, RichText, ScrollArea,
    TextWrapMode, Ui, Vec2,
};
use egui_extras::syntax_highlighting::{self, CodeTheme};

use crate::{
    app::SannyGitApp,
    conflict::Action,
    diff_edit::DiffEdit,
    models::{ChangeKind, DiffLine, DiffLineKind},
};
use std::collections::BTreeSet;

use super::controls::icons::{self, Icon};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    ui.spacing_mut().item_spacing.y = 0.0;
    let index = app.selection.active;
    let Some(change) = app.changes.get(index) else {
        super::header(ui, super::WORKSPACE_HEADER_HEIGHT, palette, |ui| {
            ui.label(RichText::new("Changes").family(crate::theme::semibold_family()));
        });
        return;
    };
    if file_header(ui, &change.path, palette) {
        app.selection.active = usize::MAX;
        return;
    };
    if change.kind == ChangeKind::Conflicted {
        conflict_bar(ui, app, index);
    }
    if app.selected_diff().is_none() {
        if app.busy() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Loading diff…").color(palette.muted));
            });
        }
        return;
    }
    let path = app.changes[index].path.clone();
    let conflicted = app.changes[index].kind == ChangeKind::Conflicted;
    let added = app
        .selected_diff()
        .map(|preview| preview.added)
        .unwrap_or(0);
    let removed = app
        .selected_diff()
        .map(|preview| preview.removed)
        .unwrap_or(0);
    let hunk = app.diff_selection_is_hunk();
    let stage_reason = (!conflicted).then(|| app.diff_edit_blocker(DiffEdit::Stage));
    let unstage_reason = (!conflicted).then(|| app.diff_edit_blocker(DiffEdit::Unstage));
    let discard_reason = (!conflicted).then(|| app.diff_edit_blocker(DiffEdit::Discard));

    super::header(ui, super::WORKSPACE_HEADER_HEIGHT, palette, |ui| {
        ui.spacing_mut().interact_size.y = 20.0;
        let wide = ui.available_width() >= 760.0;
        // Place the actions first so a right-to-left row pins them to the edge,
        // then fill the remaining width from the left. The row itself must not
        // take hover, or it covers the buttons and hides their tooltips.
        ui.scope_builder(
            eframe::egui::UiBuilder::new()
                .layout(Layout::right_to_left(Align::Center))
                .sense(eframe::egui::Sense::empty()),
            |ui| {
                if !conflicted {
                    let noun = if hunk { "hunk" } else { "lines" };
                    let actions = [
                        (
                            Icon::Check,
                            "Stage",
                            format!("Stage the selected {noun}"),
                            &stage_reason,
                            DiffEdit::Stage,
                        ),
                        (
                            Icon::File,
                            "Unstage",
                            format!("Unstage the selected {noun}"),
                            &unstage_reason,
                            DiffEdit::Unstage,
                        ),
                        (
                            Icon::Discard,
                            "Discard",
                            format!("Discard the selected {noun}"),
                            &discard_reason,
                            DiffEdit::Discard,
                        ),
                    ];
                    for (icon, label, tooltip, reason, edit) in actions.into_iter().rev() {
                        if line_action(ui, icon, label, &tooltip, reason) {
                            app.open_diff_edit(edit);
                        }
                    }
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let path_width =
                        (ui.available_width() - if wide { 460.0 } else { 390.0 }).max(64.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(path_width, 22.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_min_width(path_width);
                            ui.add(
                                eframe::egui::Label::new(RichText::new(&path).color(palette.muted))
                                    .show_tooltip_when_elided(false),
                            )
                            .help_text(&path);
                        },
                    );
                    ui.label(
                        RichText::new(format!("+{added}"))
                            .family(crate::theme::semibold_family())
                            .color(palette.added_text),
                    );
                    ui.label(
                        RichText::new(format!("−{removed}"))
                            .family(crate::theme::semibold_family())
                            .color(palette.removed_text),
                    );
                    let mut side = app.preferences.diff_side_by_side;
                    let before = side;
                    ui.selectable_value(&mut side, false, "Unified")
                        .help_text("Show the diff as one column");
                    ui.selectable_value(&mut side, true, "Side by Side")
                        .help_text("Show old and new lines in two columns");
                    if side != before {
                        app.preferences.diff_side_by_side = side;
                    }
                    let mut ignore = app.preferences.diff_ignore_whitespace;
                    let whitespace = if wide {
                        "Ignore whitespace"
                    } else {
                        "Whitespace"
                    };
                    if ui
                        .checkbox(&mut ignore, whitespace)
                        .help_text("Hide changes that only add or remove whitespace. Line edits stay disabled while this is on.")
                        .changed()
                    {
                        app.preferences.diff_ignore_whitespace = ignore;
                        app.reload_selected_diff();
                    }
                });
            },
        );
    });
    let side_by_side = app.preferences.diff_side_by_side;
    let selection = app.diff_line_selection.clone();
    let clicks = {
        let preview = app.selected_diff().unwrap();
        if side_by_side {
            render_side_by_side_with(ui, preview, palette, &path, Some(&selection))
        } else {
            render_preview_with(ui, preview, palette, &path, Some(&selection))
        }
    };
    apply_diff_clicks(&mut app.diff_line_selection, clicks);
}

fn conflict_bar(ui: &mut Ui, app: &mut SannyGitApp, index: usize) {
    let palette = app.palette();
    let state = app.changes[index].conflict_status.map_or_else(
        || "Conflicted".to_owned(),
        |status| format!("Conflicted ({status})"),
    );
    let compact = ui.available_width() < 570.0;
    let secondary = [Action::Ours, Action::Theirs, Action::Recreate];
    let mut requested = None;
    let mut stage = false;
    let stage_enabled = app.staging_allowed(index, true);
    let conflict_button = |ui: &mut Ui, action: Action| {
        ui.add_enabled(
            app.can_open_conflict(index, action),
            Button::new(action.label()),
        )
        .help_text(action.description())
        .disabled_help_text("Select a single file and wait for the working tree to be current")
        .clicked()
    };
    super::header(
        ui,
        30.0,
        crate::theme::Palette {
            header: palette.warning_background,
            panel_radius: 0,
            ..palette
        },
        |ui| {
            ui.spacing_mut().interact_size.y = 22.0;
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                stage = ui
                    .add_enabled(stage_enabled, Button::new("Stage"))
                    .help_text("Stage the working copy after resolving the conflict")
                    .disabled_help_text(if app.merging {
                        "Wait for the current operation or refresh the working tree"
                    } else {
                        "Direct staging of conflicts is available during a merge. Use Conflict Solver to stage a resolution."
                    })
                    .clicked();
                if compact {
                    ui.menu_button("Resolve", |ui| {
                        for action in secondary {
                            if conflict_button(ui, action) {
                                requested = Some(action);
                                ui.close();
                            }
                        }
                    });
                } else {
                    for action in secondary.into_iter().rev() {
                        if conflict_button(ui, action) {
                            requested = Some(action);
                        }
                    }
                }
                if conflict_button(ui, Action::Edit) {
                    requested = Some(Action::Edit);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    icons::paint(ui, Icon::Warning, palette.warning, 16.0);
                    ui.add(Label::new(&state).truncate().show_tooltip_when_elided(false))
                        .help_text(&state);
                });
            });
        },
    );
    if let Some(action) = requested {
        app.open_conflict(index, action);
    } else if stage {
        app.set_staged(index, true);
    }
}

fn line_action(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    tooltip: &str,
    reason: &Option<Option<String>>,
) -> bool {
    let enabled = reason.as_ref().is_some_and(Option::is_none);
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            ui.add(
                Button::image_and_text(icons::image(icon, ui.visuals().text_color(), 14.0), label)
                    .gap(4.0)
                    .frame(true)
                    .frame_when_inactive(false)
                    .wrap_mode(TextWrapMode::Extend),
            )
        })
        .inner;
    response.widget_info(|| {
        eframe::egui::WidgetInfo::labeled(eframe::egui::Role::Button, enabled, label)
    });
    if let Some(help) = reason.as_ref().and_then(|reason| reason.as_deref()) {
        response.disabled_help_text(help).clicked()
    } else {
        response.help_text(tooltip).clicked()
    }
}

fn file_header(ui: &mut Ui, path: &str, palette: crate::theme::Palette) -> bool {
    let mut close = false;
    super::header(ui, super::WORKSPACE_HEADER_HEIGHT, palette, |ui| {
        ui.spacing_mut().interact_size.y = 20.0;
        ui.label(RichText::new("Changes").family(crate::theme::semibold_family()));
        ui.add_space(8.0);
        let font = eframe::egui::TextStyle::Body.resolve(ui.style());
        let text = ui
            .painter()
            .layout_no_wrap(file_name(path).to_owned(), font, palette.text);
        let width = (text.size().x + 64.0).min(ui.available_width()).max(64.0);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, 26.0), eframe::egui::Sense::hover());
        ui.painter().rect(
            rect,
            4,
            palette.panel,
            (1.0, palette.border),
            eframe::egui::StrokeKind::Inside,
        );
        response.help_text(path);
        icons::image(Icon::for_file(path), palette.accent, 14.0).paint_at(
            ui,
            eframe::egui::Rect::from_center_size(
                eframe::egui::pos2(rect.left() + 13.0, rect.center().y),
                Vec2::splat(14.0),
            ),
        );
        let close_rect = eframe::egui::Rect::from_min_max(
            eframe::egui::pos2(rect.right() - 27.0, rect.top() + 1.0),
            eframe::egui::pos2(rect.right() - 1.0, rect.bottom() - 1.0),
        );
        let close_response = ui.interact(
            close_rect,
            ui.scope_id().with("diff.close"),
            eframe::egui::Sense::click(),
        );
        close_response.widget_info(|| {
            eframe::egui::WidgetInfo::labeled(eframe::egui::Role::Button, true, "Close diff")
        });
        if close_response.hovered() {
            ui.painter().rect_filled(close_rect, 3, palette.hover);
        }
        icons::image(Icon::Close, palette.text, 16.0).paint_at(
            ui,
            eframe::egui::Rect::from_center_size(close_rect.center(), Vec2::splat(16.0)),
        );
        close = close_response.help_text("Close diff").clicked();
        let text_rect = eframe::egui::Rect::from_min_max(
            eframe::egui::pos2(rect.left() + 24.0, rect.top() + 1.0),
            eframe::egui::pos2(close_rect.left() - 4.0, rect.bottom() - 1.0),
        );
        ui.painter()
            .with_clip_rect(ui.clip_rect().intersect(text_rect))
            .galley(
                eframe::egui::pos2(text_rect.left(), rect.center().y - text.size().y / 2.0),
                text,
                palette.text,
            );
    });
    close
}

#[derive(Default)]
struct DiffClicks {
    lines: Vec<usize>,
    hunks: Vec<BTreeSet<usize>>,
}

struct LineClick<'a> {
    index: usize,
    selected: bool,
    clicks: &'a mut DiffClicks,
    lines: &'a [DiffLine],
}

fn apply_diff_clicks(selection: &mut BTreeSet<usize>, clicks: DiffClicks) {
    for index in clicks.lines {
        if !selection.remove(&index) {
            selection.insert(index);
        }
    }
    for hunk in clicks.hunks {
        if *selection == hunk {
            selection.clear();
        } else {
            *selection = hunk;
        }
    }
}

pub fn render_preview(
    ui: &mut Ui,
    preview: &crate::models::DiffPreview,
    palette: crate::theme::Palette,
    id: &str,
) {
    let _ = render_preview_with(ui, preview, palette, id, None);
}

fn render_preview_with(
    ui: &mut Ui,
    preview: &crate::models::DiffPreview,
    palette: crate::theme::Palette,
    id: &str,
    selection: Option<&BTreeSet<usize>>,
) -> DiffClicks {
    let mut clicks = DiffClicks::default();
    let code_theme = CodeTheme::from_style(ui.style());
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.spacing_mut().interact_size.y = 18.0;
    let font = eframe::egui::TextStyle::Monospace.resolve(ui.style());
    let glyph_width = ui.fonts_mut(|fonts| fonts.glyph_width(&font, 'M'));
    let width = ui
        .available_width()
        .max(preview.max_columns as f32 * glyph_width + 72.0);
    ScrollArea::both()
        .id_salt(("diff_lines", id))
        .auto_shrink([false, false])
        .show_rows(ui, 18.0, preview.lines.len(), |ui, rows| {
            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
            ui.set_min_width(width);
            for (offset, line) in preview.lines[rows.clone()].iter().enumerate() {
                let index = rows.start + offset;
                let click = selection.map(|selected| LineClick {
                    index,
                    selected: selected.contains(&index),
                    clicks: &mut clicks,
                    lines: &preview.lines,
                });
                show_line(ui, line, palette, &code_theme, width, false, click);
            }
        });
    clicks
}

pub fn render_side_by_side(
    ui: &mut Ui,
    preview: &crate::models::DiffPreview,
    palette: crate::theme::Palette,
    id: &str,
) {
    let _ = render_side_by_side_with(ui, preview, palette, id, None);
}

fn render_side_by_side_with(
    ui: &mut Ui,
    preview: &crate::models::DiffPreview,
    palette: crate::theme::Palette,
    id: &str,
    selection: Option<&BTreeSet<usize>>,
) -> DiffClicks {
    let mut clicks = DiffClicks::default();
    ui.spacing_mut().interact_size.y = 18.0;
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    let code_theme = CodeTheme::from_style(ui.style());
    let font = eframe::egui::TextStyle::Monospace.resolve(ui.style());
    let glyph_width = ui.fonts_mut(|fonts| fonts.glyph_width(&font, 'M'));
    let available = (ui.available_width() - ui.spacing().scroll.allocated_width() - 4.0).max(0.0);
    let split_id = ui.scope_id().with("diff.side.split");
    let mut ratio = ui
        .data_mut(|data| data.get_persisted::<f32>(split_id))
        .unwrap_or(0.5);
    let origin = ui.available_rect_before_wrap().min;
    let minimum = 120.0_f32.min(available / 2.0);
    let mut left_width = (available * ratio).clamp(minimum, (available - minimum).max(minimum));
    let mut divider = eframe::egui::Rect::from_min_size(
        origin + Vec2::new(left_width, 0.0),
        Vec2::new(4.0, ui.available_height()),
    );
    let response = ui.interact(
        divider.expand2(Vec2::new(3.0, 0.0)),
        split_id,
        eframe::egui::Sense::click_and_drag(),
    );
    if response.dragged() {
        if let Some(pointer) = response.interact_pointer_pos() {
            left_width =
                (pointer.x - origin.x - 2.0).clamp(minimum, (available - minimum).max(minimum));
            ratio = if available > 0.0 {
                left_width / available
            } else {
                0.5
            };
            ui.data_mut(|data| data.insert_persisted(split_id, ratio));
            divider = divider.translate(Vec2::new(origin.x + left_width - divider.left(), 0.0));
        }
    }
    if response.hovered() || response.dragged() {
        ui.ctx()
            .set_cursor_icon(eframe::egui::CursorIcon::ResizeColumn);
    }
    let side_widths = [left_width, available - left_width];
    let code_widths =
        side_widths.map(|width| width.max(preview.max_columns as f32 * glyph_width + 72.0));
    ui.painter().rect_filled(divider, 0, palette.border);
    ui.painter().line_segment(
        [divider.center_top(), divider.center_bottom()],
        (1.0, palette.muted),
    );
    let mut horizontal = [0.0; 2];
    // Keep both horizontal bars at the bottom, independent of the visible rows.
    eframe::egui::Panel::bottom(eframe::egui::Id::unique(("diff.side.bars", id)))
        .exact_size(18.0)
        .frame(Frame::NONE)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (side, offset) in horizontal.iter_mut().enumerate() {
                    if side == 1 {
                        ui.add_space(4.0);
                    }
                    ui.allocate_ui_with_layout(
                        Vec2::new(side_widths[side], 18.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            let output = ScrollArea::horizontal()
                                .id_salt(("diff_side_horizontal", id, side))
                                .scroll_bar_visibility(
                                    eframe::egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                )
                                .auto_shrink([false, false])
                                .max_height(18.0)
                                .show(ui, |ui| {
                                    ui.allocate_exact_size(
                                        Vec2::new(code_widths[side], 1.0),
                                        eframe::egui::Sense::hover(),
                                    );
                                });
                            *offset = output.state.clamped_offset().x;
                        },
                    );
                }
            });
        });
    // One vertical viewport keeps old and new lines aligned.
    ScrollArea::vertical()
        .id_salt(("diff_sides", id))
        .auto_shrink([false, false])
        .show_rows(ui, 18.0, preview.side_by_side.len(), |ui, rows| {
            for row in &preview.side_by_side[rows] {
                ui.horizontal(|ui| {
                    for side in 0..2 {
                        if side == 1 {
                            ui.add_space(4.0);
                        }
                        let (rect, _) = ui.allocate_exact_size(
                            Vec2::new(side_widths[side], 18.0),
                            eframe::egui::Sense::hover(),
                        );
                        if let Some(index) = row[side] {
                            let mut child = ui.new_child(
                                eframe::egui::UiBuilder::new()
                                    .max_rect(eframe::egui::Rect::from_min_size(
                                        rect.min - Vec2::new(horizontal[side], 0.0),
                                        Vec2::new(code_widths[side], 18.0),
                                    ))
                                    .layout(Layout::top_down(Align::Min)),
                            );
                            child.set_clip_rect(rect.intersect(ui.clip_rect()));
                            let click = selection.map(|selected| LineClick {
                                index,
                                selected: selected.contains(&index),
                                clicks: &mut clicks,
                                lines: &preview.lines,
                            });
                            show_line(
                                &mut child,
                                &preview.lines[index],
                                palette,
                                &code_theme,
                                code_widths[side],
                                side == 0,
                                click,
                            );
                        } else {
                            ui.painter().rect_filled(rect, 0, palette.panel_alt);
                        }
                    }
                });
            }
        });
    clicks
}
fn show_line(
    ui: &mut Ui,
    line: &DiffLine,
    palette: crate::theme::Palette,
    code_theme: &CodeTheme,
    width: f32,
    old_side: bool,
    click: Option<LineClick<'_>>,
) {
    let fill = match line.kind {
        DiffLineKind::Added => palette.added_background,
        DiffLineKind::Removed => palette.removed_background,
        DiffLineKind::Hunk => palette.hunk_background,
        _ => palette.panel,
    };
    let color = match line.kind {
        DiffLineKind::Added => palette.added_text,
        DiffLineKind::Removed => palette.removed_text,
        DiffLineKind::Header | DiffLineKind::Hunk => palette.accent,
        DiffLineKind::Context => palette.text,
    };
    let marker = match line.kind {
        DiffLineKind::Added => "+",
        DiffLineKind::Removed => "-",
        _ => " ",
    };

    let selectable = click.is_some()
        && matches!(
            line.kind,
            DiffLineKind::Added | DiffLineKind::Removed | DiffLineKind::Hunk
        );
    let selected = selectable && click.as_ref().is_some_and(|click| click.selected);
    let line_index = click.as_ref().map(|click| click.index);
    // Stroke stays inside the row. `Frame::stroke` grows the outer rect and
    // pushes every following line down when selection toggles.
    let response = Frame::new()
        .fill(if selected {
            palette.accent.gamma_multiply(0.16)
        } else {
            fill
        })
        .inner_margin(Margin::symmetric(4, 0))
        .show(ui, |ui| {
            ui.set_min_width(width - 8.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.horizontal(|ui| {
                // The reference uses one gutter: old numbers on removals and
                // new numbers on additions and unchanged lines.
                let number = (if old_side {
                    line.old_line
                } else {
                    line.new_line.or(line.old_line)
                })
                .map(|n| n.to_string())
                .unwrap_or_default();
                ui.add_sized(
                    [32.0, 18.0],
                    Label::new(RichText::new(number).monospace().color(palette.muted)),
                );
                ui.add_sized(
                    [16.0, 18.0],
                    Label::new(RichText::new(marker).monospace().color(color)),
                );
                if line.kind == DiffLineKind::Context {
                    let mut job = syntax_highlighting::highlight(
                        ui.ctx(),
                        ui.style(),
                        code_theme,
                        &line.text,
                        "rs",
                    );
                    for section in &mut job.sections {
                        let token = section.byte_range.slice(&job.text);
                        section.format.color = if matches!(
                            token,
                            "fn" | "pub"
                                | "impl"
                                | "let"
                                | "mut"
                                | "if"
                                | "else"
                                | "Some"
                                | "self"
                                | "Self"
                        ) {
                            palette.accent
                        } else if token.starts_with('"') {
                            palette.modified
                        } else if token.starts_with("//") {
                            palette.muted
                        } else {
                            palette.text
                        };
                    }
                    ui.add(Label::new(job).wrap_mode(TextWrapMode::Extend));
                } else {
                    ui.add(
                        Label::new(RichText::new(&line.text).monospace().color(color))
                            .wrap_mode(TextWrapMode::Extend),
                    );
                }
            });
        })
        .response;
    if selected {
        ui.painter().rect_stroke(
            response.rect,
            0.0,
            eframe::egui::Stroke::new(1.0, palette.accent),
            eframe::egui::StrokeKind::Inside,
        );
    }
    let response = if selectable {
        ui.interact(
            response.rect,
            ui.scope_id().with(("diff.line", line_index, old_side)),
            eframe::egui::Sense::click(),
        )
    } else {
        response
    };
    if let Some(click) = click {
        if selectable && response.clicked() {
            if line.kind == DiffLineKind::Hunk {
                click
                    .clicks
                    .hunks
                    .push(crate::diff_edit::hunk_changes(click.lines, click.index));
            } else {
                click.clicks.lines.push(click.index);
            }
        }
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn conflict_actions_fit_in_wide_and_narrow_diff_panes() {
        use super::*;
        use crate::theme::{install_fonts, ThemeChoice};
        use eframe::egui::{Context, Pos2, RawInput, Rect, Shape};

        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for (width, actions) in [
                (320.0, &["Conflict Solver", "Resolve", "Stage"][..]),
                (
                    620.0,
                    &[
                        "Conflict Solver",
                        "Take Ours",
                        "Take Theirs",
                        "Recreate Conflict",
                        "Stage",
                    ][..],
                ),
            ] {
                let ctx = Context::default();
                install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.changes = crate::git::parse_status(b"UU README.md\0").unwrap();
                app.selection.active = 0;
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 180.0))),
                        ..Default::default()
                    },
                    |ui| show(ui, &mut app),
                );
                output.textures_delta.clear();
                let mut end = 0.0;
                for action in actions {
                    let (rect, clip) = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            Shape::Text(text) if text.galley.job.text == *action => Some((
                                Rect::from_min_size(text.pos, text.galley.size()),
                                shape.clip_rect,
                            )),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("Missing action {action} at width {width}"));
                    assert!(
                        clip.contains_rect(rect),
                        "Clipped action {action} at width {width}"
                    );
                    assert!(
                        rect.left() >= end,
                        "Overlapping action {action} at width {width}"
                    );
                    end = rect.right();
                }
            }
        }
    }

    #[test]
    fn dragging_diff_divider_resizes_both_sides_and_keeps_the_split_for_other_files() {
        use super::*;
        use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape};
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            let ctx = Context::default();
            crate::theme::install_fonts(&ctx);
            theme.apply(&ctx);
            let app = crate::app::SannyGitApp::with_theme(theme);
            let preview = crate::models::DiffPreview::new("@@ -1 +1 @@\n-old\n+new\n");
            let mut split_id = None;
            let mut render = |events, id| {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1000.0, 300.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        split_id = Some(ui.scope_id().with("diff.side.split"));
                        render_side_by_side(ui, &preview, app.palette(), id)
                    },
                );
                output.textures_delta.clear();
                output
            };
            let right_x = |output: &eframe::egui::FullOutput| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Text(text) if text.galley.job.text == "new" => Some(text.pos.x),
                        _ => None,
                    })
                    .unwrap()
            };
            for _ in 0..3 {
                render(vec![], "first");
            }
            let output = render(vec![], "first");
            let before = right_x(&output);
            let pointer = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::LineSegment { points, .. }
                        if (points[0].x - points[1].x).abs() < 0.1
                            && (points[1].y - points[0].y).abs() > 250.0 =>
                    {
                        Some((points[0] + points[1].to_vec2()) * 0.5)
                    }
                    _ => None,
                })
                .unwrap();
            render(vec![Event::PointerMoved(pointer)], "first");
            render(
                vec![Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }],
                "first",
            );
            let moved = pointer + Vec2::new(100.0, 0.0);
            render(vec![Event::PointerMoved(moved)], "first");
            render(
                vec![Event::PointerButton {
                    pos: moved,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
                "first",
            );
            let after = right_x(&render(vec![], "first"));
            assert!(after > before + 90.0);
            assert_eq!(right_x(&render(vec![], "second")), after);
            let stored = ctx
                .data_mut(|data| data.get_persisted::<f32>(split_id.unwrap()))
                .expect("Dragged split ratio is not in persisted memory");
            assert!(
                (stored - 0.5).abs() > 0.05,
                "Persisted ratio does not reflect the drag: {stored}"
            );
        }
    }
    #[test]
    fn both_diff_sides_and_their_line_numbers_fit_the_viewport() {
        use super::*;
        use crate::{
            models::DiffPreview,
            theme::{install_fonts, ThemeChoice},
        };
        use eframe::egui::{Context, Pos2, RawInput, Rect, Shape};
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for width in [900.0, 1200.0] {
                let ctx = Context::default();
                install_fonts(&ctx);
                theme.apply(&ctx);
                let app = crate::app::SannyGitApp::with_theme(theme);
                let patch = format!(
                    "@@ -10,2 +20,2 @@\n shared\n-old value\n+new value\n{}",
                    " more context\n".repeat(200)
                );
                let preview = DiffPreview::new(&patch);
                let mut output = None;
                for _ in 0..3 {
                    let mut frame = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(width, 250.0),
                            )),
                            ..Default::default()
                        },
                        |ui| render_side_by_side(ui, &preview, app.palette(), "test"),
                    );
                    frame.textures_delta.clear();
                    output = Some(frame);
                }
                let output = output.unwrap();
                for (label, right_side) in [
                    ("old value", false),
                    ("10", false),
                    ("new value", true),
                    ("20", true),
                ] {
                    let (rect, clip) = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            Shape::Text(text) if text.galley.job.text == label => Some((
                                text.galley.rect.translate(text.pos.to_vec2()),
                                shape.clip_rect,
                            )),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("Missing {label} in {theme:?}"));
                    assert!(
                        clip.contains_rect(rect),
                        "{label} is clipped at {width} in {theme:?}: {rect:?} {clip:?}"
                    );
                    assert_eq!(
                        rect.center().x > width / 2.0,
                        right_side,
                        "Wrong column for {label}"
                    );
                }
            }
        }
    }
    use super::*;

    #[test]
    fn file_tab_border_stays_inside_header() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            let ctx = eframe::egui::Context::default();
            crate::theme::install_fonts(&ctx);
            egui_extras::install_image_loaders(&ctx);
            theme.apply(&ctx);
            for _ in 0..3 {
                let mut output = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
                    file_header(ui, "app.rs", theme.palette());
                    super::super::header(ui, 26.0, theme.palette(), |ui| {
                        ui.label("src/app.rs");
                    });
                });
                let palette = theme.palette();
                let header = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        eframe::egui::Shape::Rect(rect)
                            if rect.fill == palette.header
                                && (rect.rect.height() - super::super::WORKSPACE_HEADER_HEIGHT)
                                    .abs()
                                    < 0.1 =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .expect("Diff header missing");
                let tab = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        eframe::egui::Shape::Rect(rect)
                            if rect.fill == palette.panel
                                && rect.stroke.color == palette.border
                                && rect.stroke.width == 1.0 =>
                        {
                            Some((rect.rect, shape.clip_rect))
                        }
                        _ => None,
                    })
                    .expect("File tab missing");
                assert!(
                    header.contains_rect(tab.0),
                    "Tab extends outside header: {tab:?}, {header:?}"
                );
                assert!(tab.1.contains_rect(tab.0), "Tab border is clipped: {tab:?}");
                let next_header = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        eframe::egui::Shape::Rect(rect)
                            if rect.fill == palette.header
                                && (rect.rect.height() - 26.0).abs() < 0.1 =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .expect("Path header missing");
                assert!(
                    tab.0.bottom() < next_header.top(),
                    "Path header covers the tab border"
                );
                assert!(
                    tab.0.bottom() < header.bottom(),
                    "Tab overlaps header separator"
                );
                output.textures_delta.clear();
            }
        }
    }

    #[test]
    fn line_actions_are_labeled_and_pinned_to_the_right() {
        use crate::theme::{install_fonts, ThemeChoice};
        use eframe::egui::{Context, Event, Pos2, RawInput, Rect, Shape};

        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let ctx = Context::default();
            install_fonts(&ctx);
            egui_extras::install_image_loaders(&ctx);
            theme.apply(&ctx);
            ctx.all_styles_mut(|style| {
                style.interaction.tooltip_delay = 0.0;
                style.interaction.tooltip_grace_time = 0.0;
                style.interaction.show_tooltips_only_when_still = false;
            });
            let mut app = SannyGitApp::with_theme(theme);
            app.repositories.push(crate::models::Repository {
                name: "demo".into(),
                path: "D:/demo".into(),
            });
            app.selected_repository = 0;
            app.state_current = true;
            app.changes = crate::git::parse_status(b" M src/theme.rs\0").unwrap();
            app.selection.active = 0;
            let preview =
                crate::models::DiffPreview::new("Working tree changes\n@@ -1 +1 @@\n-old\n+new\n");
            let added = preview
                .lines
                .iter()
                .position(|line| line.kind == crate::models::DiffLineKind::Added)
                .expect("added line");
            app.diff_line_selection.insert(added);
            app.show_diff_preview_for_test(preview);

            let render = |app: &mut SannyGitApp, events| {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1100.0, 320.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| show(ui, app),
                );
                output.textures_delta.clear();
                output
            };
            render(&mut app, vec![]);
            let output = render(&mut app, vec![]);
            let text = |output: &eframe::egui::FullOutput, label: &str| {
                output.shapes.iter().find_map(|shape| match &shape.shape {
                    Shape::Text(text) if text.galley.job.text == label => {
                        Some(Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                })
            };
            let path = text(&output, "src/theme.rs").expect("path");
            let unified = text(&output, "Unified").expect("Unified");
            let whitespace = text(&output, "Ignore whitespace").expect("Ignore whitespace");
            let stage = text(&output, "Stage").expect("Stage");
            let unstage = text(&output, "Unstage").expect("Unstage");
            let discard = text(&output, "Discard").expect("Discard");
            assert!(
                (path.center().y - unified.center().y).abs() < 2.0,
                "Path and view mode are on different rows in {theme:?}"
            );
            assert!(
                (path.center().y - whitespace.center().y).abs() < 2.0,
                "Whitespace toggle left the path row in {theme:?}"
            );
            for (label, rect) in [("Stage", stage), ("Unstage", unstage), ("Discard", discard)] {
                assert!(
                    (rect.center().y - path.center().y).abs() < 4.0,
                    "{label} left the path row in {theme:?}"
                );
            }
            let palette = app.palette();
            let header = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Rect(rect)
                        if rect.fill == palette.header
                            && rect.rect.contains(path.center())
                            && (rect.rect.height() - super::super::WORKSPACE_HEADER_HEIGHT)
                                .abs()
                                < 0.1 =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("Single-row path header");
            assert!(
                output.shapes.iter().all(|shape| match &shape.shape {
                    Shape::Rect(rect) => {
                        rect.fill != palette.header || (rect.rect.height() - 52.0).abs() > 0.1
                    }
                    _ => true,
                }),
                "Line actions still occupy a second header row in {theme:?}"
            );
            assert!(
                stage.left() > whitespace.right() + 16.0,
                "Actions sit against the view controls in {theme:?}: {whitespace:?} {stage:?}"
            );
            assert!(stage.right() < unstage.left(), "{theme:?}");
            assert!(unstage.right() < discard.left(), "{theme:?}");
            assert!(
                header.right() - discard.right() < 32.0 && discard.right() < header.right(),
                "Discard is not pinned to the right edge in {theme:?}: {discard:?} {header:?}"
            );
            let actions = stage.union(discard).expand(4.0);
            let borders: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Rect(rect)
                        if rect.stroke.width > 0.0
                            && rect.stroke.color.a() > 0
                            && actions.intersects(rect.rect) =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            assert!(
                borders.is_empty(),
                "Line actions still draw a border in {theme:?}: {borders:?}"
            );
            // Hover opens a tooltip on a sizing pass, which paints nothing.
            // Leaving another tooltip can take one more frame. Two still frames
            // are enough for the words to appear.
            let expect_tooltip = |app: &mut SannyGitApp, at, tooltip: &str| {
                render(app, vec![Event::PointerMoved(at)]);
                render(app, vec![]);
                let shown = render(app, vec![]);
                assert!(
                    text(&shown, tooltip).is_some(),
                    "{tooltip} is missing in {theme:?}"
                );
            };
            expect_tooltip(&mut app, stage.center(), "Stage the selected lines");
            expect_tooltip(&mut app, discard.center(), "Discard the selected lines");
            expect_tooltip(
                &mut app,
                unstage.center(),
                "Select staged lines to unstage.",
            );
        }
    }

    #[test]
    fn selecting_a_line_keeps_following_lines_in_place() {
        use std::collections::BTreeSet;

        use crate::models::{DiffLineKind, DiffPreview};
        use crate::theme::{install_fonts, ThemeChoice};
        use eframe::egui::{Context, Pos2, RawInput, Rect, Shape, Vec2};

        let preview = DiffPreview::new("@@ -1,4 +1,4 @@\n context\n-removed\n+added\n tail\n");
        let added = preview
            .lines
            .iter()
            .position(|line| line.kind == DiffLineKind::Added)
            .expect("added line");
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let ctx = Context::default();
            install_fonts(&ctx);
            theme.apply(&ctx);
            let palette = theme.palette();
            let tops = |selected: bool| {
                let selection = selected.then(|| BTreeSet::from([added]));
                let mut output = None;
                for _ in 0..2 {
                    let mut frame = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(800.0, 400.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            let _ = render_preview_with(
                                ui,
                                &preview,
                                palette,
                                "line-selection-layout",
                                selection.as_ref(),
                            );
                        },
                    );
                    frame.textures_delta.clear();
                    output = Some(frame);
                }
                let output = output.unwrap();
                let top = |label: &str| {
                    output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            Shape::Text(text) if text.galley.job.text == label => Some(text.pos.y),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("missing {label} in {theme:?}"))
                };
                let stroke = output.shapes.iter().find_map(|shape| match &shape.shape {
                    Shape::Rect(rect)
                        if rect.stroke.width == 1.0 && rect.stroke.color == palette.accent =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                });
                (
                    top("context"),
                    top("removed"),
                    top("added"),
                    top("tail"),
                    stroke,
                )
            };
            let plain = tops(false);
            let picked = tops(true);
            for (label, before, after) in [
                ("context", plain.0, picked.0),
                ("removed", plain.1, picked.1),
                ("added", plain.2, picked.2),
                ("tail", plain.3, picked.3),
            ] {
                assert!(
                    (before - after).abs() < 0.1,
                    "{label} moved when selected in {theme:?}: {before} -> {after}"
                );
            }
            let stroke = picked.4.expect("selection stroke");
            assert!(
                stroke.top() >= picked.2 - 4.0 && stroke.bottom() <= picked.3 + 0.5,
                "Selection stroke leaves the added row in {theme:?}: {stroke:?} added={} tail={}",
                picked.2,
                picked.3
            );
            assert!(plain.4.is_none(), "Unselected diff draws an accent stroke");
        }
    }
}
