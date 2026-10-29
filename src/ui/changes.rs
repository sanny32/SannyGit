use eframe::egui::{Align, Checkbox, Color32, Frame, Layout, Margin, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use crate::{
    app::{ChangeFilter, SannyGitApp},
    models::ChangeKind,
    theme::Palette,
};

use super::icons::{self, Icon};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    let repository = app.repositories.get(app.selected_repository);

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if let Some(repository) = repository {
            ui.label(RichText::new(repository.name).strong());
            ui.label(RichText::new("/").color(palette.muted));
        }
        ui.label(RichText::new(app.current_branch).color(palette.accent));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!("Ahead {}  Behind {}", app.ahead, app.behind))
                    .small()
                    .color(palette.muted),
            );
        });
    });
    ui.separator();
    ui.horizontal(|ui| {
        ui.heading(format!("Changes  ({} files)", app.changes.len()));
        ui.add_space(10.0);
        for (filter, label) in [
            (ChangeFilter::All, "All"),
            (ChangeFilter::Modified, "Modified"),
            (ChangeFilter::Added, "Added"),
            (ChangeFilter::Deleted, "Deleted"),
        ] {
            let count = app
                .changes
                .iter()
                .filter(|change| filter_matches(filter, change.kind))
                .count();
            if ui
                .selectable_label(app.change_filter == filter, format!("{label}  {count}"))
                .clicked()
            {
                app.change_filter = filter;
            }
        }
    });
    ui.add_space(6.0);

    let visible = app
        .changes
        .iter()
        .enumerate()
        .filter(|(_, change)| filter_matches(app.change_filter, change.kind))
        .collect::<Vec<_>>();

    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::initial(30.0))
        .column(Column::remainder())
        .column(Column::initial(64.0))
        .min_scrolled_height(0.0)
        .header(24.0, |mut header| {
            header.col(|ui| {
                ui.label("");
            });
            header.col(|ui| {
                ui.label(RichText::new("File").strong());
            });
            header.col(|ui| {
                ui.label(RichText::new("Status").strong());
            });
        })
        .body(|body| {
            body.rows(23.0, visible.len(), |mut row| {
                let (index, change) = visible[row.index()];
                let selected = app.selected_change == index;

                row.set_selected(selected);
                row.col(|ui| {
                    let mut staged = change.staged;
                    ui.add_enabled(false, Checkbox::new(&mut staged, ""));
                });
                row.col(|ui| {
                    let response = ui.horizontal(|ui| {
                        icons::paint(ui, Icon::File, palette.muted, 12.0);
                        ui.selectable_label(selected, change.path)
                    });
                    if response.inner.clicked() {
                        app.selected_change = index;
                    }
                });
                row.col(|ui| {
                    let color = status_color(change.kind, palette);
                    Frame::new()
                        .fill(color.gamma_multiply(0.12))
                        .corner_radius(3)
                        .inner_margin(Margin::symmetric(6, 2))
                        .show(ui, |ui| {
                            ui.label(RichText::new(change.kind.code()).strong().color(color))
                                .on_hover_text(change.kind.label());
                        });
                });
            });
        });
}

fn filter_matches(filter: ChangeFilter, kind: ChangeKind) -> bool {
    match filter {
        ChangeFilter::All => true,
        ChangeFilter::Modified => kind == ChangeKind::Modified,
        ChangeFilter::Added => kind == ChangeKind::Added,
        ChangeFilter::Deleted => kind == ChangeKind::Deleted,
    }
}

fn status_color(kind: ChangeKind, palette: Palette) -> Color32 {
    match kind {
        ChangeKind::Added | ChangeKind::Untracked => palette.added_text,
        ChangeKind::Deleted | ChangeKind::Conflicted => palette.removed_text,
        ChangeKind::Modified | ChangeKind::Renamed => palette.modified,
    }
}
