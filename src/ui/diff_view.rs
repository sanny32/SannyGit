use eframe::egui::{Align, Frame, Layout, Margin, RichText, ScrollArea, Ui, Vec2};

use crate::{
    app::SannyGitApp,
    diff::parse_unified_diff,
    models::{DiffLine, DiffLineKind},
};

pub fn show(ui: &mut Ui, app: &SannyGitApp) {
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

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(file_name(change.path)).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!("-{removed}"))
                    .small()
                    .strong()
                    .color(palette.removed_text),
            );
            ui.label(
                RichText::new(format!("+{added}"))
                    .small()
                    .strong()
                    .color(palette.added_text),
            );
        });
    });
    ui.label(RichText::new(change.path).small().color(palette.muted));
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(RichText::new("Unified").small().color(palette.muted));
    });
    ui.separator();

    ScrollArea::both()
        .id_salt("diff_lines")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for line in &lines {
                show_line(ui, line, palette);
            }
        });
}

fn show_line(ui: &mut Ui, line: &DiffLine, palette: crate::theme::Palette) {
    let fill = match line.kind {
        DiffLineKind::Added => palette.added_background,
        DiffLineKind::Removed => palette.removed_background,
        DiffLineKind::Hunk => palette.panel_alt,
        _ => palette.background,
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
    let available_width = ui.available_width();

    Frame::new()
        .fill(fill)
        .inner_margin(Margin::symmetric(4, 1))
        .show(ui, |ui| {
            ui.set_min_width(available_width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.horizontal(|ui| {
                line_number(ui, line.old_line);
                line_number(ui, line.new_line);
                ui.add_space(6.0);
                ui.label(RichText::new(marker).monospace().color(color));
                ui.label(RichText::new(&line.text).monospace().color(color));
            });
        });
}

fn line_number(ui: &mut Ui, number: Option<u32>) {
    let text = number.map(|number| number.to_string()).unwrap_or_default();
    ui.allocate_ui_with_layout(
        Vec2::new(34.0, ui.spacing().interact_size.y),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.label(
                RichText::new(text)
                    .monospace()
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        },
    );
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
