use eframe::egui::{Align, Checkbox, Color32, Layout, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use crate::{
    app::SannyGitApp,
    models::{ChangeFile, ChangeKind},
};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    let repository = app.repositories.get(app.selected_repository);

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if let Some(repository) = repository {
            ui.label(RichText::new(repository.name).strong());
            ui.label(RichText::new("›").color(palette.muted));
        }
        ui.label(RichText::new(app.current_branch).color(palette.accent));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(format!("↑ {}  ↓ {}", app.ahead, app.behind)).small().color(palette.muted));
        });
    });
    ui.separator();
    ui.horizontal(|ui| {
        ui.heading(format!("Changes  ({} files)", app.changes.len()));
    });
    ui.add_space(4.0);

    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::initial(30.0))
        .column(Column::remainder())
        .column(Column::initial(72.0))
        .min_scrolled_height(0.0)
        .header(24.0, |mut header| {
            header.col(|ui| { ui.label(""); });
            header.col(|ui| { ui.label(RichText::new("File").strong()); });
            header.col(|ui| { ui.label(RichText::new("Status").strong()); });
        })
        .body(|body| {
            body.rows(24.0, app.changes.len(), |mut row| {
                let index = row.index();
                let selected = app.selected_change == index;
                let change = &app.changes[index];
                row.set_selected(selected);
                row.col(|ui| {
                    let mut staged = change.staged;
                    ui.add_enabled(false, Checkbox::new(&mut staged, ""));
                });
                row.col(|ui| {
                    if ui.selectable_label(selected, change.path).clicked() {
                        app.selected_change = index;
                    }
                });
                row.col(|ui| {
                    ui.label(RichText::new(change.kind.code()).strong().color(status_color(change, palette.modified, palette)))
                        .on_hover_text(change.kind.label());
                });
            });
        });
}

fn status_color(change: &ChangeFile, modified: Color32, palette: crate::theme::Palette) -> Color32 {
    match change.kind {
        ChangeKind::Added | ChangeKind::Untracked => palette.added_text,
        ChangeKind::Deleted | ChangeKind::Conflicted => palette.removed_text,
        ChangeKind::Modified | ChangeKind::Renamed => modified,
    }
}
