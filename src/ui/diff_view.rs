use eframe::egui::{
    Align, Frame, Label, Layout, Margin, RichText, ScrollArea, TextWrapMode, Ui, Vec2,
};
use egui_extras::syntax_highlighting::{self, CodeTheme};

use crate::{
    app::SannyGitApp,
    diff::parse_unified_diff,
    models::{DiffLine, DiffLineKind},
};

use super::icons::{self, Icon};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    let Some(change) = app.changes.get(app.selected_change) else {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("Select a file to view its diff").color(palette.muted));
        });
        return;
    };
    let lines = parse_unified_diff(change.diff);
    let added = lines
        .iter()
        .filter(|line| line.kind == DiffLineKind::Added)
        .count();
    let removed = lines
        .iter()
        .filter(|line| line.kind == DiffLineKind::Removed)
        .count();

    ui.spacing_mut().item_spacing.y = 0.0;
    let mut close = false;
    super::header(ui, 48.0, palette, |ui| {
        Frame::new()
            .fill(palette.panel)
            .stroke((1.0, palette.border))
            .corner_radius(4)
            .inner_margin(Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::paint(ui, Icon::File, palette.accent, 18.0);
                    ui.label(file_name(change.path));
                    ui.add_space(8.0);
                    close =
                        icons::button(ui, Icon::Close, "Close diff", Vec2::splat(22.0)).clicked();
                });
            });
    });
    if close {
        app.selected_change = usize::MAX;
        return;
    }
    super::header(ui, 70.0, palette, |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let path_width = (ui.available_width() - 118.0).max(100.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(path_width, 30.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_width(path_width);
                        ui.label(RichText::new(change.path).size(15.0).color(palette.muted))
                            .on_hover_text(change.path);
                    },
                );
                ui.label(
                    RichText::new(format!("+{added}"))
                        .size(15.0)
                        .family(crate::theme::semibold_family())
                        .color(palette.added_text),
                );
                ui.label(
                    RichText::new(format!("−{removed}"))
                        .size(15.0)
                        .family(crate::theme::semibold_family())
                        .color(palette.removed_text),
                );
                icons::unavailable(
                    ui,
                    Icon::More,
                    "Additional diff actions are not implemented yet",
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                icons::unavailable(
                    ui,
                    Icon::Settings,
                    "Additional diff options are not implemented yet",
                );
                ui.separator();
                ui.add_enabled_ui(false, |ui| {
                    let mut ignore_whitespace = false;
                    ui.checkbox(
                        &mut ignore_whitespace,
                        RichText::new("Ignore whitespace").size(14.0),
                    );
                    let _ = ui.button("Unified");
                })
                .response
                .on_disabled_hover_text(
                    "Only unified diff is supported; whitespace filtering is not implemented",
                );
            });
        });
    });
    let code_theme = CodeTheme::from_style(ui.style());
    ScrollArea::both()
        .id_salt(("diff_lines", change.path))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.spacing_mut().interact_size.y = 24.0;
            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
            // Reserve the longest line before painting so every row background has
            // the same width, including when the content scrolls horizontally.
            let font = eframe::egui::TextStyle::Monospace.resolve(ui.style());
            let width = lines
                .iter()
                .map(|line| {
                    ui.painter()
                        .layout_no_wrap(line.text.clone(), font.clone(), palette.text)
                        .size()
                        .x
                        + 72.0
                })
                .fold(ui.available_width(), f32::max);
            ui.set_min_width(width);
            for line in &lines {
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
                    [38.0, 22.0],
                    Label::new(
                        RichText::new(number)
                            .monospace()
                            .size(12.0)
                            .color(palette.muted),
                    ),
                );
                ui.add_sized(
                    [20.0, 22.0],
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
                        let token = &job.text[section.byte_range.clone()];
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
