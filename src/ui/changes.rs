use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::{ChangeFilter, SannyGitApp},
    models::ChangeKind,
    theme::{semibold_family, Palette},
};
use eframe::egui::{Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Ui, Vec2};
use egui_extras::{Column, TableBuilder};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    ui.spacing_mut().item_spacing.y = 0.0;
    let tight = ui.available_width() < 420.0;
    let filter_height = if tight { 50.0 } else { 30.0 };
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
                ui.add_space(4.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(270.0, 22.0),
                    Layout::left_to_right(Align::Center),
                    |ui| filters(ui, app, palette),
                );
            });
        }
    });
    let operations_enabled =
        !app.busy() && app.state_current && app.repositories.get(app.selected_repository).is_some();
    let mut staging_request = None;
    let visible = app
        .changes
        .iter()
        .enumerate()
        .filter(|(_, change)| filter_matches(app.change_filter, change.kind))
        .collect::<Vec<_>>();
    let row_height = 23.0;
    let table_clip = ui.clip_rect().intersect(ui.available_rect_before_wrap());
    // Table row backgrounds expand into cell spacing; keep them inside the panel.
    ui.shrink_clip_rect(table_clip);
    ui.spacing_mut().item_spacing.x = 4.0;
    let widths_id = eframe::egui::Id::unique("files.header_column_widths");
    let [mut index_width, mut status_width] = ui
        .data_mut(|data| data.get_persisted::<[f32; 2]>(widths_id))
        .unwrap_or([52.0, 72.0]);
    TableBuilder::new(ui)
        .id_salt("files.columns.v3")
        .resizable(false)
        .striped(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(index_width))
        .column(Column::remainder().at_least(120.0).clip(true))
        .column(Column::exact(status_width))
        .min_scrolled_height(0.0)
        .header(24.0, |mut header| {
            header.col(|ui| {
                ui.add_space(6.0);
                ui.label("Index");
                column_separator(ui, "index", &mut index_width, 1.0, 44.0..=80.0, palette);
            });
            header.col(|ui| {
                ui.label("File");
                column_separator(ui, "file", &mut status_width, -1.0, 64.0..=120.0, palette);
            });
            header.col(|ui| {
                ui.label("Status");
            });
        })
        .body(|mut body| {
            body.ui_mut().style_mut().interaction.selectable_labels = false;
            body.rows(row_height, visible.len(), |mut row| {
                let (index, change) = visible[row.index()];
                row.set_selected(app.selected_change == index);
                row.col(|ui| {
                    ui.add_space(6.0);
                    let mut checked = change.staged;
                    let tooltip = if change.kind == ChangeKind::Conflicted {
                        "Resolve this conflict outside SannyGit before staging"
                    } else if !operations_enabled {
                        "Wait for the current operation or refresh the repository"
                    } else if change.staged && change.unstaged {
                        "Some changes are staged. Click to unstage; use Stage to include the remaining changes"
                    } else if change.staged { "Unstage this file (preserve working-tree contents)" }
                    else { "Stage all changes in this file" };
                    if ui.add_enabled(operations_enabled && change.kind != ChangeKind::Conflicted,
                        eframe::egui::Checkbox::without_text(&mut checked)
                            .indeterminate(change.staged && change.unstaged))
                        .help_text(tooltip).disabled_help_text(tooltip).changed() {
                        staging_request = Some((index, !change.staged));
                    }
                });
                row.col(|ui| {
                    icons::paint(ui, Icon::for_file(&change.path), status_color(change.kind, palette), 16.0);
                    ui.add_space(4.0);
                    ui.add(eframe::egui::Label::new(RichText::new(&change.path).color(palette.text)).truncate())
                        .help_text(if let Some(source) = &change.original_path {
                            format!("{source} → {}", change.path)
                        } else {
                            change.path.clone()
                        });
                });
                row.col(|ui| {
                    let color = status_color(change.kind, palette);
                    let fill = match change.kind {
                        ChangeKind::Modified | ChangeKind::Renamed => palette.modified_background,
                        ChangeKind::Added | ChangeKind::Untracked => palette.status_added,
                        ChangeKind::Deleted | ChangeKind::Conflicted => palette.status_removed,
                    };
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(19.0, 18.0), Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, fill);
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        change.kind.code(),
                        FontId::new(12.0, semibold_family()),
                        color,
                    );
                    response.help_text(change.kind.label());
                });
                let response = row.response();
                ui_line(&response, table_clip, palette);
                if response.clicked() {
                    app.selected_change = index;
                }
                if response.double_clicked()
                    && app.preferences.file_double_click == crate::preferences::FileDoubleClick::ToggleStage
                    && operations_enabled && change.kind != ChangeKind::Conflicted
                {
                    staging_request = Some((index, !change.staged));
                }
            });
        });
    ui.data_mut(|data| data.insert_persisted(widths_id, [index_width, status_width]));
    if let Some((index, staged)) = staging_request {
        app.set_staged(index, staged);
    }
}

fn column_separator(
    ui: &mut Ui,
    id: &str,
    width: &mut f32,
    direction: f32,
    range: std::ops::RangeInclusive<f32>,
    palette: Palette,
) {
    let rect = ui.max_rect();
    let x = rect.right() - 0.5;
    let hit = Rect::from_min_max(
        eframe::egui::pos2(x - 5.0, rect.top()),
        eframe::egui::pos2(x + 5.0, rect.bottom()),
    );
    let response = ui
        .interact(
            hit,
            ui.make_persistent_id(("files.header.resize", id)),
            Sense::drag(),
        )
        .on_hover_cursor(eframe::egui::CursorIcon::ResizeColumn);
    if response.dragged() {
        *width = (*width + direction * response.drag_delta().x).clamp(*range.start(), *range.end());
    }
    let color = if response.hovered() || response.dragged() {
        palette.accent
    } else {
        palette.border
    };
    ui.painter().line_segment(
        [
            eframe::egui::pos2(x, rect.top() + 3.0),
            eframe::egui::pos2(x, rect.bottom() - 3.0),
        ],
        (1.0, color),
    );
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
        ui.label(RichText::new("Files").size(13.0).family(semibold_family()));
        ui.label(format!("({count} files)"));
    });
}

fn filters(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette) {
    let width_scale = (ui.available_width() / 270.0).min(1.0);
    ui.spacing_mut().item_spacing.x = 0.0;
    for (filter, label, width) in [
        (ChangeFilter::All, "All", 34.0),
        (ChangeFilter::Modified, "Modified", 88.0),
        (ChangeFilter::Added, "Added", 72.0),
        (ChangeFilter::Deleted, "Deleted", 76.0),
    ] {
        let count = app
            .changes
            .iter()
            .filter(|c| filter_matches(filter, c.kind))
            .count();
        let selected = app.change_filter == filter;
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width * width_scale, 22.0), Sense::click());
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
            rect.left_center() + Vec2::new(6.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.0),
            text_color,
        );
        if filter != ChangeFilter::All {
            let badge = Rect::from_center_size(
                rect.right_center() - Vec2::new(12.0, 0.0),
                Vec2::new(16.0, 17.0),
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
                FontId::proportional(11.0),
                palette.text,
            );
        }
        if response.clicked() {
            app.change_filter = filter;
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{install_fonts, ThemeChoice};
    use eframe::egui::{Context, Pos2, RawInput};

    #[test]
    fn header_separator_resizes_columns_and_keeps_the_new_width() {
        use eframe::egui::{Event, Modifiers, PointerButton};
        let ctx = Context::default();
        install_fonts(&ctx);
        ThemeChoice::Light.apply(&ctx);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        let mut render = |events| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 300.0))),
                    events,
                    ..Default::default()
                },
                |ui| show(ui, &mut app),
            );
            let line = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    eframe::egui::Shape::LineSegment { points, .. }
                        if (points[0].x - points[1].x).abs() < 0.1 =>
                    {
                        Some((points[0] + points[1].to_vec2()) * 0.5)
                    }
                    _ => None,
                })
                .unwrap();
            let x = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    eframe::egui::Shape::Text(text) if text.galley.job.text == "File" => {
                        Some(text.pos.x)
                    }
                    _ => None,
                })
                .unwrap();
            output.textures_delta.clear();
            (line, x)
        };
        for _ in 0..3 {
            render(vec![]);
        }
        let (pointer, before) = render(vec![]);
        render(vec![Event::PointerMoved(pointer)]);
        render(vec![Event::PointerButton {
            pos: pointer,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }]);
        let moved = pointer + Vec2::new(12.0, 0.0);
        for _ in 0..3 {
            render(vec![Event::PointerMoved(moved)]);
        }
        render(vec![Event::PointerButton {
            pos: moved,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        let (_, after) = render(vec![]);
        assert!(
            after > before + 8.0,
            "Header drag did not resize column: {before} -> {after}"
        );
        for _ in 0..5 {
            assert_eq!(render(vec![]).1, after);
        }
    }

    #[test]
    fn file_column_positions_do_not_change_with_rows_or_long_names() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for width in [350.0, 900.0] {
                let ctx = Context::default();
                install_fonts(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                let render = |app: &mut SannyGitApp| {
                    let mut positions = Vec::new();
                    for _ in 0..3 {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    Pos2::ZERO,
                                    Vec2::new(width, 300.0),
                                )),
                                ..Default::default()
                            },
                            |ui| show(ui, app),
                        );
                        for shape in &output.shapes {
                            if let eframe::egui::Shape::LineSegment { points, .. } = &shape.shape {
                                if (points[0].x - points[1].x).abs() < 0.1 {
                                    assert!(
                                        (points[0].y - points[1].y).abs() <= 24.0,
                                        "Column separator extends into body"
                                    );
                                }
                            }
                        }
                        positions = ["Index", "File", "Status"]
                            .into_iter()
                            .map(|label| {
                                output
                                    .shapes
                                    .iter()
                                    .find_map(|shape| {
                                        if let eframe::egui::Shape::Text(text) = &shape.shape {
                                            if text.galley.job.text == label {
                                                return Some(text.pos);
                                            }
                                        }
                                        None
                                    })
                                    .unwrap_or_else(|| {
                                        panic!("Missing column {label} at width {width}")
                                    })
                            })
                            .collect::<Vec<_>>();
                        output.textures_delta.clear();
                    }
                    positions
                };
                let empty = render(&mut app);
                for path in [
                    "file.rs".to_owned(),
                    format!("{}/file.rs", "very-long-directory-name/".repeat(25)),
                ] {
                    app.changes = (0..80)
                        .map(|i| crate::models::ChangeFile {
                            path: format!("{i}-{path}"),
                            original_path: None,
                            kind: ChangeKind::Modified,
                            staged: false,
                            unstaged: true,
                            diff: String::new(),
                        })
                        .collect();
                    let populated = render(&mut app);
                    for (before, after) in empty.iter().zip(populated) {
                        assert!(
                            (*before - after).length() < 1.0,
                            "Columns moved in {theme:?} at width {width}: {before:?} -> {after:?}"
                        );
                    }
                    app.changes.clear();
                    assert_eq!(render(&mut app), empty);
                }
            }
        }
    }
}
