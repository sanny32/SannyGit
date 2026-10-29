use super::icons::{self, Icon};
use crate::{
    app::{ChangeFilter, SannyGitApp},
    models::ChangeKind,
    theme::{semibold_family, Palette},
};
use eframe::egui::{Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Ui, Vec2};
use egui_extras::{Column, TableBuilder};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    ui.spacing_mut().item_spacing.y = 0.0;
    let tight = ui.available_width() < 510.0;
    super::header(
        ui,
        if super::compact(ui) { 44.0 } else { 60.0 },
        palette,
        |ui| {
            if let Some(repository) = app.repositories.get(app.selected_repository) {
                ui.label(
                    RichText::new(repository.name)
                        .size(18.0)
                        .family(semibold_family()),
                );
                ui.add_space(10.0);
                ui.label(RichText::new("›").size(24.0).color(palette.muted));
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new(app.current_branch)
                    .size(18.0)
                    .family(semibold_family()),
            );
            ui.add_space(18.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(66.0, 26.0), Sense::hover());
            ui.painter().rect_filled(rect, 4.0, palette.accent);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                format!("↑ {} ↓ {}", app.ahead, app.behind),
                FontId::proportional(14.0),
                Color32::WHITE,
            );
        },
    );
    let filter_height = if tight { 72.0 } else { 58.0 };
    super::header(ui, filter_height, palette, |ui| {
        if tight {
            ui.vertical(|ui| {
                ui.add_space(3.0);
                changes_title(ui, app.changes.len());
                ui.horizontal(|ui| filters(ui, app, palette));
            });
        } else {
            changes_title(ui, app.changes.len());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                icons::unavailable(
                    ui,
                    Icon::List,
                    "Additional view options are not implemented yet",
                );
                ui.add_space(10.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(332.0, 32.0),
                    Layout::left_to_right(Align::Center),
                    |ui| filters(ui, app, palette),
                );
            });
        }
    });
    let visible = app
        .changes
        .iter()
        .enumerate()
        .filter(|(_, change)| filter_matches(app.change_filter, change.kind))
        .collect::<Vec<_>>();
    let row_height =
        ((ui.available_height() - 34.0) / visible.len().max(1) as f32).clamp(26.0, 39.0);
    let table_clip = ui.clip_rect().intersect(ui.available_rect_before_wrap());
    ui.spacing_mut().item_spacing.x = 8.0;
    TableBuilder::new(ui)
        .striped(false)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(52.0))
        .column(Column::remainder())
        .column(Column::exact(112.0))
        .min_scrolled_height(0.0)
        .header(34.0, |mut header| {
            header.col(|ui| {
                ui.add_space(18.0);
                checkmark(ui, false, palette);
            });
            header.col(|ui| {
                ui.label("File");
            });
            header.col(|ui| {
                ui.label("Status");
            });
        })
        .body(|body| {
            body.rows(row_height, visible.len(), |mut row| {
                let (index, change) = visible[row.index()];
                row.set_selected(app.selected_change == index);
                row.col(|ui| {
                    ui.add_space(18.0);
                    checkmark(ui, change.staged, palette);
                });
                row.col(|ui| {
                    let icon = if change.path.ends_with(".png") {
                        Icon::ImageFile
                    } else if change.kind == ChangeKind::Modified && change.path.ends_with(".rs") {
                        Icon::Folder
                    } else {
                        Icon::File
                    };
                    icons::paint(ui, icon, status_color(change.kind, palette), 20.0);
                    ui.add_space(8.0);
                    ui.label(RichText::new(change.path).color(palette.text))
                        .on_hover_text(change.path);
                });
                row.col(|ui| {
                    let color = status_color(change.kind, palette);
                    let fill = match change.kind {
                        ChangeKind::Modified | ChangeKind::Renamed => palette.modified_background,
                        ChangeKind::Added | ChangeKind::Untracked => palette.status_added,
                        ChangeKind::Deleted | ChangeKind::Conflicted => palette.status_removed,
                    };
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(27.0, 26.0), Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, fill);
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        change.kind.code(),
                        FontId::new(16.0, semibold_family()),
                        color,
                    );
                    response.on_hover_text(change.kind.label());
                });
                let response = row.response();
                ui_line(&response, table_clip, palette);
                if response.clicked() {
                    app.selected_change = index;
                }
            });
        });
}

fn ui_line(response: &eframe::egui::Response, clip: Rect, palette: Palette) {
    response
        .ctx
        .layer_painter(response.layer_id)
        .with_clip_rect(clip)
        .line_segment(
            [response.rect.left_bottom(), response.rect.right_bottom()],
            (1.0, palette.border.gamma_multiply(0.6)),
        );
}

fn changes_title(ui: &mut Ui, count: usize) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(
            RichText::new("Changes")
                .size(18.0)
                .family(semibold_family()),
        );
        ui.label(format!("({count} files)"));
    });
}

fn filters(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette) {
    let width_scale = (ui.available_width() / 332.0).min(1.0);
    ui.spacing_mut().item_spacing.x = 0.0;
    for (filter, label, width) in [
        (ChangeFilter::All, "All", 48.0),
        (ChangeFilter::Modified, "Modified", 104.0),
        (ChangeFilter::Added, "Added", 88.0),
        (ChangeFilter::Deleted, "Deleted", 92.0),
    ] {
        let count = app
            .changes
            .iter()
            .filter(|c| filter_matches(filter, c.kind))
            .count();
        let selected = app.change_filter == filter;
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width * width_scale, 32.0), Sense::click());
        ui.painter().rect_filled(
            rect,
            4.0,
            if selected {
                palette.accent
            } else if response.hovered() {
                palette.hover
            } else {
                palette.panel
            },
        );
        let text_color = if selected {
            Color32::WHITE
        } else {
            palette.text
        };
        ui.painter().text(
            rect.left_center() + Vec2::new(12.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(15.0),
            text_color,
        );
        if filter != ChangeFilter::All {
            let badge = Rect::from_center_size(
                rect.right_center() - Vec2::new(15.0, 0.0),
                Vec2::new(19.0, 23.0),
            );
            ui.painter().rect_filled(
                badge,
                3.0,
                if filter == ChangeFilter::Deleted {
                    palette.removed_background
                } else {
                    palette.border
                },
            );
            ui.painter().text(
                badge.center(),
                Align2::CENTER_CENTER,
                count,
                FontId::proportional(14.0),
                palette.text,
            );
        }
        if response.clicked() {
            app.change_filter = filter;
        }
    }
}

fn checkmark(ui: &mut Ui, checked: bool, palette: Palette) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(17.0), Sense::hover());
    ui.painter().rect_filled(
        rect,
        3.0,
        if checked {
            palette.accent
        } else {
            palette.panel
        },
    );
    ui.painter().rect_stroke(
        rect,
        3.0,
        (
            1.0,
            if checked {
                palette.accent
            } else {
                palette.muted
            },
        ),
        eframe::egui::StrokeKind::Inside,
    );
    if checked {
        let points = [
            rect.min + Vec2::new(3.0, 8.0),
            rect.min + Vec2::new(7.0, 12.0),
            rect.min + Vec2::new(14.0, 4.0),
        ];
        ui.painter()
            .line_segment([points[0], points[1]], (1.8, Color32::WHITE));
        ui.painter()
            .line_segment([points[1], points[2]], (1.8, Color32::WHITE));
    }
    response.on_hover_text("Staging is not implemented yet");
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
