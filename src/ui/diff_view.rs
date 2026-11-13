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
                show_line(ui, line, palette, &code_theme, width);
            }
        });
}

fn show_line(
    ui: &mut Ui,
    line: &DiffLine,
    palette: crate::theme::Palette,
    code_theme: &CodeTheme,
    width: f32,
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
                let number = line
                    .new_line
                    .or(line.old_line)
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
