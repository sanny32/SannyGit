use super::controls::tooltips::TooltipResponse;
use eframe::egui::{
    epaint::text::ByteRangeExt, Align, Frame, Label, Layout, Margin, RichText, ScrollArea,
    TextWrapMode, Ui, Vec2,
};
use egui_extras::syntax_highlighting::{self, CodeTheme};

use crate::{
    app::SannyGitApp,
    models::{DiffLine, DiffLineKind},
};

use super::controls::icons::{self, Icon};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    super::register_panel(ui, "diff");
    let palette = app.palette();
    ui.spacing_mut().item_spacing.y = 0.0;
    let Some(change) = app.changes.get(app.selected_change) else {
        super::header(ui, super::WORKSPACE_HEADER_HEIGHT, palette, |ui| {
            let title = ui.label("Changes");
            super::register_panel_title(ui, "diff", title.rect);
        });
        return;
    };
    if file_header(ui, &change.path, palette) {
        app.selected_change = usize::MAX;
        return;
    };
    let Some(preview) = app.selected_diff() else {
        if app.busy() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Loading diff…").color(palette.muted));
            });
        }
        return;
    };
    let added = preview.added;
    let removed = preview.removed;

    super::header(ui, 26.0, palette, |ui| {
        ui.spacing_mut().interact_size.y = 20.0;
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let path_width = (ui.available_width() - 118.0).max(100.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(path_width, 22.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_width(path_width);
                        ui.add(
                            eframe::egui::Label::new(
                                RichText::new(&change.path).size(12.0).color(palette.muted),
                            )
                            .show_tooltip_when_elided(false),
                        )
                        .help_text(&change.path);
                    },
                );
                ui.label(
                    RichText::new(format!("+{added}"))
                        .size(12.0)
                        .family(crate::theme::semibold_family())
                        .color(palette.added_text),
                );
                ui.label(
                    RichText::new(format!("−{removed}"))
                        .size(12.0)
                        .family(crate::theme::semibold_family())
                        .color(palette.removed_text),
                );
                icons::unavailable(
                    ui,
                    Icon::More,
                    "Additional diff actions are not implemented yet",
                );
            });
        });
    });
    render_preview(ui, preview, palette, &change.path);
}

fn file_header(ui: &mut Ui, path: &str, palette: crate::theme::Palette) -> bool {
    let mut close = false;
    super::header(ui, super::WORKSPACE_HEADER_HEIGHT, palette, |ui| {
        ui.spacing_mut().interact_size.y = 20.0;
        let title = ui.label("Changes");
        super::register_panel_title(ui, "diff", title.rect);
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

pub fn render_preview(
    ui: &mut Ui,
    preview: &crate::models::DiffPreview,
    palette: crate::theme::Palette,
    id: &str,
) {
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
            for line in &preview.lines[rows] {
                show_line(ui, line, palette, &code_theme, width, false);
            }
        });
}

pub fn render_side_by_side(
    ui: &mut Ui,
    preview: &crate::models::DiffPreview,
    palette: crate::theme::Palette,
    id: &str,
) {
    ui.spacing_mut().interact_size.y = 18.0;
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    let code_theme = CodeTheme::from_style(ui.style());
    let font = eframe::egui::TextStyle::Monospace.resolve(ui.style());
    let glyph_width = ui.fonts_mut(|fonts| fonts.glyph_width(&font, 'M'));
    let available = (ui.available_width() - ui.spacing().scroll.allocated_width() - 4.0).max(0.0);
    let split_id = ui.scope_id().with("diff.side.split");
    let mut ratio = ui.data(|data| data.get_temp::<f32>(split_id).unwrap_or(0.5));
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
            ui.data_mut(|data| data.insert_temp(split_id, ratio));
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
                            show_line(
                                &mut child,
                                &preview.lines[index],
                                palette,
                                &code_theme,
                                code_widths[side],
                                side == 0,
                            );
                        } else {
                            ui.painter().rect_filled(rect, 0, palette.panel_alt);
                        }
                    }
                });
            }
        });
}
fn show_line(
    ui: &mut Ui,
    line: &DiffLine,
    palette: crate::theme::Palette,
    code_theme: &CodeTheme,
    width: f32,
    old_side: bool,
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

    Frame::new()
        .fill(fill)
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
                    Label::new(
                        RichText::new(number)
                            .monospace()
                            .size(11.0)
                            .color(palette.muted),
                    ),
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
        });
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
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
            let render = |events, id| {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1000.0, 300.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| render_side_by_side(ui, &preview, app.palette(), id),
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
}
