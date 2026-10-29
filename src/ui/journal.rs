use super::icons::{self, Icon};
use crate::{app::SannyGitApp, models::Commit, theme::Palette};
use eframe::egui::{Align, Button, Color32, Layout, RichText, Sense, TextEdit, Ui, Vec2};
use egui_extras::{Column, TableBuilder};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    ui.spacing_mut().item_spacing.y = 0.0;
    let wide = ui.available_width() >= 580.0;
    super::header(ui, 28.0, palette, |ui| {
        ui.heading("Journal");
        let reserved = if wide { 168.0 } else { 40.0 };
        let search_width = (ui.available_width() - reserved).max(80.0);
        ui.add(
            TextEdit::singleline(&mut app.journal_filter)
                .hint_text(RichText::new("Search commits...").color(palette.muted))
                .desired_width(search_width)
                .margin(Vec2::new(5.0, 2.0)),
        );
        if wide {
            ui.add_enabled(
                false,
                Button::image_and_text(
                    icons::image(Icon::Chevron, palette.text, 14.0),
                    "All branches",
                ),
            )
            .on_disabled_hover_text("Branch history filtering is not implemented yet");
        }
        icons::unavailable(
            ui,
            Icon::List,
            "Additional history views are not implemented yet",
        );
    });
    let filter = app.journal_filter.to_lowercase();
    let visible = app
        .commits
        .iter()
        .filter(|c| {
            filter.is_empty()
                || c.message.to_lowercase().contains(&filter)
                || c.author.to_lowercase().contains(&filter)
                || c.hash.to_lowercase().contains(&filter)
        })
        .collect::<Vec<_>>();
    let row_height = 20.0;
    TableBuilder::new(ui)
        .striped(false)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(if wide { 70.0 } else { 52.0 }))
        .column(Column::remainder().at_least(80.0))
        .column(Column::exact(if wide { 82.0 } else { 62.0 }))
        .column(Column::exact(if wide { 116.0 } else { 88.0 }))
        .column(Column::exact(if wide { 100.0 } else { 72.0 }))
        .min_scrolled_height(0.0)
        .header(24.0, |mut header| {
            for title in ["Graph", "Message", "Author", "Date", "Hash"] {
                header.col(|ui| {
                    if title == "Graph" {
                        ui.add_space(14.0);
                    }
                    ui.label(RichText::new(title).size(12.0));
                });
            }
        })
        .body(|body| {
            body.rows(row_height, visible.len(), |mut row| {
                let index = row.index();
                let commit = visible[index];
                row.col(|ui| graph(ui, &visible, index, row_height, palette));
                row.col(|ui| {
                    ui.label(commit.message).on_hover_text(commit.message);
                });
                row.col(|ui| {
                    ui.label(RichText::new(commit.author).color(palette.muted));
                });
                row.col(|ui| {
                    ui.label(RichText::new(commit.date).color(palette.muted));
                });
                row.col(|ui| {
                    ui.label(RichText::new(commit.hash).monospace().color(palette.muted));
                });
            });
        });
}

fn graph(ui: &mut Ui, commits: &[&Commit], index: usize, height: f32, palette: Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let position = |row: usize, lane: usize| {
        eframe::egui::pos2(
            rect.left() + 22.0 + lane as f32 * 22.0,
            rect.center().y + (row as f32 - index as f32) * height,
        )
    };
    for (child_index, commit) in commits.iter().enumerate() {
        for parent in commit.parents {
            if let Some(parent_index) = commits.iter().position(|c| c.hash == *parent) {
                let parent_commit = commits[parent_index];
                let color = if commit.lane == 1 || parent_commit.lane == 1 {
                    palette.graph_branch
                } else {
                    palette.graph_main
                };
                painter.line_segment(
                    [
                        position(child_index, commit.lane),
                        position(parent_index, parent_commit.lane),
                    ],
                    (1.25, color),
                );
            }
        }
    }
    let commit = commits[index];
    let color: Color32 = if commit.parents.is_empty() {
        palette.graph_root
    } else if commit.lane == 1 {
        palette.graph_branch
    } else {
        palette.graph_main
    };
    painter.circle_filled(position(index, commit.lane), 4.0, color);
}
