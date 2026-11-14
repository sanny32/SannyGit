use super::controls::{
    dialog_style::{self, DialogResponse},
    icons::Icon,
    tooltips::TooltipResponse,
};
use crate::{
    app::{merge::Dialog, SannyGitApp},
    merge::Mode,
    models::RefKind,
    theme::Palette,
};
use eframe::egui::{self, Align, Button, Label, Layout, RichText, ScrollArea};
use egui_extras::{Column, TableBuilder};
use std::collections::{BTreeSet, HashMap, HashSet};

const COMMIT_ROW_HEIGHT: f32 = 22.0;
const GRAPH_LANE_WIDTH: f32 = 14.0;

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let Some(mut dialog) = app.merge_dialog.take() else {
        return;
    };
    let palette = app.palette();
    let busy = app.busy();
    let mut close = false;
    let mut submit = None;
    dialog_style::modal(ctx, palette, "merge").show(ctx, |ui| {
        ui.set_width(780.0);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header_with_icon(ui, palette, "Merge", "Select the branch or commit to merge and how it should be merged into the Working Tree.", Icon::Merge);
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(12, 10), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal(|ui| {
                ui.add_sized([ui.available_width() - 94.0, 24.0], super::controls::singleline(&mut dialog.filter).hint_text("Filter loaded commits…"));
                if ui.add_enabled(dialog.snapshot.is_some() && !busy, Button::new("Branches…").min_size(egui::vec2(86.0, 24.0))).clicked() {
                    dialog.branches_open = true;
                }
            });
            graph(ui, &mut dialog, palette, &app.preferences, busy);
            let status = match &dialog.snapshot {
                None if busy => "Loading commits…".to_owned(),
                None => "Could not load commits. Close and reopen Merge to retry.".to_owned(),
                Some(_) if busy => "Merging…".to_owned(),
                Some(snapshot) => format!("{} commits loaded{} · Target: {}", snapshot.history.commits.len(), if snapshot.history.truncated { " (10,000 limit)" } else { "" }, dialog.selected.as_deref().map(|hash| &hash[..8]).unwrap_or("select a commit")),
            };
            dialog_style::text_line(ui, RichText::new(status).color(palette.muted), 20.0);
        });
        dialog_style::section(ui, palette.panel, egui::Margin::symmetric(12, 10), |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                close = ui.add_enabled(!busy && !dialog.branches_open, dialog_style::button("Cancel")).dialog_cancel();
                let enabled = !busy && !dialog.branches_open && dialog.snapshot.as_ref().is_some_and(|snapshot| dialog.selected.as_ref().is_some_and(|hash| hash != &snapshot.head));
                if ui.add_enabled(enabled, dialog_style::primary("Merge to Working Tree", palette)).help_text("Squash the selected commit into the index and working tree without creating a commit. Review the result and commit separately.").clicked() { submit = Some(Mode::WorkingTree); }
                if ui.add_enabled(enabled, dialog_style::button("Create Merge-Commit")).help_text("Merge the selected commit and create a merge commit, even when a fast-forward is possible. Uses your Git identity, hooks and signing configuration.").clicked() { submit = Some(Mode::Commit); }
            });
        });
    });
    if dialog.branches_open {
        branches(ctx, &mut dialog, palette);
    }
    if !close {
        app.merge_dialog = Some(dialog);
    }
    if let Some(mode) = submit {
        app.submit_merge(mode);
    }
}

fn graph(
    ui: &mut egui::Ui,
    dialog: &mut Dialog,
    palette: Palette,
    preferences: &crate::preferences::Preferences,
    loading: bool,
) {
    let shown = dialog.visible();
    let query = dialog.filter.trim().to_lowercase();
    let Some(snapshot) = &dialog.snapshot else {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 340.0),
            egui::Sense::hover(),
        );
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        painter.rect_filled(rect, 0.0, palette.panel);
        painter.rect_stroke(
            rect.shrink(0.5),
            0.0,
            (1.0, palette.border),
            egui::StrokeKind::Inside,
        );
        if loading {
            let fill = palette.panel.lerp_to_gamma(palette.muted, 0.12);
            for row in 0..15 {
                if (5..10).contains(&row) {
                    continue;
                }
                let y = rect.top() + 16.0 + row as f32 * COMMIT_ROW_HEIGHT;
                let start = rect.left() + 42.0;
                let width = (rect.width() - 220.0) * [0.72, 0.94, 0.58, 0.82][row % 4];
                painter.circle_filled(egui::pos2(rect.left() + 16.0, y), 3.0, fill);
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(start, y - 4.0), egui::vec2(width, 8.0)),
                    3.0,
                    fill,
                );
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(rect.right() - 118.0, y - 4.0),
                        egui::vec2(100.0, 8.0),
                    ),
                    3.0,
                    fill,
                );
            }
            ui.put(
                egui::Rect::from_center_size(
                    rect.center() - egui::vec2(0.0, 15.0),
                    egui::Vec2::splat(24.0),
                ),
                egui::Spinner::new().size(24.0).color(palette.accent),
            );
            painter.text(
                rect.center() + egui::vec2(0.0, 19.0),
                egui::Align2::CENTER_CENTER,
                "Loading commit graph…",
                egui::FontId::proportional(13.0),
                palette.muted,
            );
        }
        return;
    };
    let commits: Vec<_> = snapshot
        .history
        .commits
        .iter()
        .filter(|commit| shown.contains(&commit.hash) && commit.matches(&query))
        .collect();
    let indices: HashMap<_, _> = commits
        .iter()
        .enumerate()
        .map(|(index, commit)| (commit.hash.as_str(), index))
        .collect();
    // Hidden branches must not leave wide, empty lanes before the commit text.
    // Keep lane positions stable while the text filter changes.
    let lanes: HashMap<_, _> = snapshot
        .history
        .commits
        .iter()
        .filter(|commit| shown.contains(&commit.hash))
        .map(|commit| commit.lane)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(column, lane)| (lane, column))
        .collect();
    let graph_width =
        (lanes.len().saturating_sub(1) as f32 * GRAPH_LANE_WIDTH + 22.0).clamp(22.0, 96.0);
    egui::Frame::new()
        .fill(palette.panel)
        .stroke((1.0, palette.border))
        .show(ui, |ui| {
            // Table stripes extend into cell spacing; clip them inside the frame.
            ui.shrink_clip_rect(ui.available_rect_before_wrap());
            // The row pitch must match graph coordinates, including at clip edges.
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
            ui.style_mut().interaction.selectable_labels = false;
            super::controls::table_resize_style(ui, palette);
            TableBuilder::new(ui)
                .id_salt("merge.commits")
                .striped(true)
                .sense(egui::Sense::click())
                .cell_layout(Layout::left_to_right(Align::Center))
                .column(
                    Column::initial(graph_width)
                        .range(22.0..=180.0)
                        .resizable(true),
                )
                .column(Column::remainder().clip(true))
                .column(Column::exact(134.0))
                .min_scrolled_height(338.0)
                .max_scroll_height(338.0)
                .body(|body| {
                    body.rows(COMMIT_ROW_HEIGHT, commits.len(), |mut row| {
                        let index = row.index();
                        let commit = commits[index];
                        row.set_selected(dialog.selected.as_ref() == Some(&commit.hash));
                        row.col(|ui| {
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), COMMIT_ROW_HEIGHT),
                                egui::Sense::hover(),
                            );
                            if lanes.len().saturating_sub(1) as f32 * GRAPH_LANE_WIDTH + 22.0
                                > rect.width()
                            {
                                response.help_text(
                                    "Drag the graph column separator to show more lanes.",
                                );
                            }
                            let painter =
                                ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                            let pos = |row: usize, lane: usize| {
                                egui::pos2(
                                    rect.left()
                                        + 8.0
                                        + lanes.get(&lane).copied().unwrap_or(0) as f32
                                            * GRAPH_LANE_WIDTH,
                                    rect.center().y
                                        + (row as f32 - index as f32) * COMMIT_ROW_HEIGHT,
                                )
                            };
                            if query.is_empty() {
                                for edge in &snapshot.history.edges {
                                    let child = &snapshot.history.commits[edge.child];
                                    let parent = &snapshot.history.commits[edge.parent];
                                    if let (Some(&child_row), Some(&parent_row)) = (
                                        indices.get(child.hash.as_str()),
                                        indices.get(parent.hash.as_str()),
                                    ) {
                                        if child_row <= index && parent_row >= index {
                                            let bend = pos(child_row + 1, edge.parent_lane);
                                            painter.line_segment(
                                                [pos(child_row, edge.child_lane), bend],
                                                (
                                                    1.25,
                                                    if edge.child_lane == 0 {
                                                        palette.graph_main
                                                    } else {
                                                        palette.graph_branch
                                                    },
                                                ),
                                            );
                                            painter.line_segment(
                                                [bend, pos(parent_row, edge.parent_lane)],
                                                (
                                                    1.25,
                                                    if edge.child_lane == 0 {
                                                        palette.graph_main
                                                    } else {
                                                        palette.graph_branch
                                                    },
                                                ),
                                            );
                                        }
                                    }
                                }
                            }
                            painter.circle_filled(
                                pos(index, commit.lane),
                                3.5,
                                if commit.hash == snapshot.head {
                                    palette.graph_root
                                } else {
                                    palette.graph_main
                                },
                            );
                        });
                        row.col(|ui| {
                            let refs: Vec<_> = snapshot
                                .refs
                                .iter()
                                .filter(|entry| {
                                    entry.hash == commit.hash && entry.kind != RefKind::Tag
                                })
                                .collect();
                            let mut text = egui::text::LayoutJob::default();
                            let badge = egui::TextFormat {
                                font_id: egui::FontId::proportional(11.0),
                                color: palette.accent,
                                background: palette.selection,
                                valign: Align::Center,
                                ..Default::default()
                            };
                            for entry in refs.iter().take(2) {
                                text.append(&format!(" {} ", entry.name), 0.0, badge.clone());
                                text.append(" ", 0.0, egui::TextFormat::default());
                            }
                            if refs.len() > 2 {
                                text.append(&format!(" +{} ", refs.len() - 2), 0.0, badge);
                            }
                            text.append(
                                commit.subject(),
                                4.0,
                                egui::TextFormat {
                                    font_id: egui::FontId::proportional(13.0),
                                    color: palette.text,
                                    valign: Align::Center,
                                    ..Default::default()
                                },
                            );
                            ui.add(Label::new(text).truncate().selectable(false));
                        });
                        row.col(|ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(preferences.commit_date(&commit.date))
                                        .color(palette.muted),
                                )
                                .truncate(),
                            );
                        });
                        if row.response().clicked() {
                            dialog.selected = Some(commit.hash.clone());
                        }
                        let refs = snapshot
                            .refs
                            .iter()
                            .filter(|entry| entry.hash == commit.hash)
                            .map(|entry| entry.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        row.response().help_text(format!(
                            "{}\n{}\n{}\n{}",
                            commit.hash, commit.author, refs, commit.message
                        ));
                    });
                });
        });
}

fn branches(ctx: &egui::Context, dialog: &mut Dialog, palette: Palette) {
    let Some(snapshot) = &dialog.snapshot else {
        return;
    };
    dialog_style::modal(ctx, palette, "merge.branches").show(ctx, |ui| {
        ui.set_width(460.0);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header_with_icon(
            ui,
            palette,
            "Select shown branches",
            "Select the branches for which to show commits in the graph.",
            Icon::Branch,
        );
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(12, 10), |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            egui::Frame::new()
                .fill(palette.panel)
                .stroke((1.0, palette.border))
                .inner_margin(6)
                .show(ui, |ui| {
                    ScrollArea::both()
                        .id_salt("merge.branch.tree")
                        .min_scrolled_height(200.0)
                        .max_height(200.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.checkbox(
                                &mut dialog.include_head,
                                format!("HEAD ({})", &snapshot.head[..8]),
                            );
                            egui::CollapsingHeader::new("Local Branches")
                                .id_salt("merge.local")
                                .default_open(true)
                                .show(ui, |ui| {
                                    for entry in snapshot
                                        .refs
                                        .iter()
                                        .filter(|entry| entry.kind == RefKind::Local)
                                    {
                                        let label = format!(
                                            "{}{}{}",
                                            if entry.current { "▶ " } else { "" },
                                            entry.name,
                                            entry
                                                .upstream
                                                .as_ref()
                                                .map(|upstream| format!(" → {upstream}"))
                                                .unwrap_or_default()
                                        );
                                        branch_checkbox(
                                            ui,
                                            &mut dialog.shown,
                                            &entry.full_name,
                                            &label,
                                        );
                                    }
                                });
                            let remotes: std::collections::BTreeSet<_> = snapshot
                                .refs
                                .iter()
                                .filter(|entry| entry.kind == RefKind::Remote)
                                .filter_map(|entry| {
                                    entry.name.split_once('/').map(|(remote, _)| remote)
                                })
                                .collect();
                            for remote in remotes {
                                egui::CollapsingHeader::new(remote)
                                    .id_salt(("merge.remote", remote))
                                    .show(ui, |ui| {
                                        for entry in snapshot.refs.iter().filter(|entry| {
                                            entry.kind == RefKind::Remote
                                                && entry
                                                    .name
                                                    .split_once('/')
                                                    .is_some_and(|(name, _)| name == remote)
                                        }) {
                                            branch_checkbox(
                                                ui,
                                                &mut dialog.shown,
                                                &entry.full_name,
                                                &entry.name,
                                            );
                                        }
                                    });
                            }
                        });
                });
            ui.checkbox(
                &mut dialog.include_tracked,
                "Include tracked remote branches",
            );
        });
        dialog_style::section(ui, palette.panel, egui::Margin::symmetric(12, 10), |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(dialog_style::button("Close")).dialog_close() {
                    dialog.branches_open = false;
                }
            });
        });
    });
}

fn branch_checkbox(ui: &mut egui::Ui, shown: &mut HashSet<String>, name: &str, label: &str) {
    let mut checked = shown.contains(name);
    if ui.checkbox(&mut checked, label).changed() {
        if checked {
            shown.insert(name.to_owned());
        } else {
            shown.remove(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{install_fonts, ThemeChoice};

    #[test]
    fn merge_layout_stays_stable_when_loading_filtering_and_reporting_errors() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                let ctx = egui::Context::default();
                install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                let mut dialog = crate::mock_data::merge_dialog();
                let snapshot = dialog.snapshot.take();
                app.merge_dialog = Some(dialog);
                let mut baseline = None;
                for stage in 0..4 {
                    if stage == 1 {
                        app.merge_dialog.as_mut().unwrap().snapshot = snapshot
                            .as_ref()
                            .map(|_| crate::mock_data::merge_dialog().snapshot.unwrap());
                    }
                    if stage == 2 {
                        app.merge_dialog.as_mut().unwrap().filter = "no matches".into();
                    }
                    if stage == 3 {
                        app.error = Some("Failure details ".repeat(200));
                    }
                    for frame in 0..4 {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                ..Default::default()
                            },
                            |ui| show(ui.ctx(), &mut app),
                        );
                        output.textures_delta.clear();
                        if frame < 3 {
                            continue;
                        }
                        let rect = ctx
                            .memory(|memory| memory.area_rect(egui::Id::unique("merge")))
                            .unwrap();
                        assert!(
                            egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect)
                        );
                        if let Some(before) = baseline {
                            assert_eq!(rect, before, "stage {stage} {theme:?}");
                        } else {
                            baseline = Some(rect);
                        }
                        assert!(!app.busy());
                    }
                }
                app.merge_dialog.as_mut().unwrap().branches_open = true;
                for _ in 0..4 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ui| show(ui.ctx(), &mut app),
                    );
                    output.textures_delta.clear();
                }
                let rect = ctx
                    .memory(|memory| memory.area_rect(egui::Id::unique("merge.branches")))
                    .unwrap();
                assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect));
            }
        }
    }
}
