use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use crate::{app::SannyGitApp, models::HistoryEdge, theme::Palette};
use eframe::egui::{
    self, Align, Button, CentralPanel, ComboBox, Frame, Label, Layout, Margin, Panel, RichText,
    ScrollArea, Sense, Ui, Vec2, ViewportBuilder, ViewportCommand, ViewportId, Window,
};
use egui_extras::{Column, TableBuilder};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    details(ui, app);
    rows(ui, app, false);
}

fn rows(ui: &mut Ui, app: &mut SannyGitApp, history_window: bool) {
    if !history_window {
        super::register_panel(ui, "journal");
    }
    let palette = app.palette();
    ui.spacing_mut().item_spacing.y = 0.0;
    let wide = ui.available_width() >= 580.0;
    super::header(ui, 28.0, palette, |ui| {
        ui.spacing_mut().interact_size.y = 20.0;
        let title = if history_window {
            ui.label("Graph")
        } else {
            ui.heading("Journal")
        };
        if !history_window {
            super::register_panel_title(ui, "journal", title.rect);
        }
        ui.add_space(8.0);
        let search_width = (ui.available_width() - 360.0).clamp(60.0, 240.0);
        let search = ui.add_enabled(
            !app.commits.is_empty(),
            crate::ui::controls::singleline(&mut app.journal_filter)
                .min_size(Vec2::ZERO)
                .hint_text("Search loaded commits…")
                .desired_width(search_width)
                .margin(Vec2::new(5.0, 2.0)),
        );
        ui.spacing_mut().interact_size.y = search.rect.height();
        ui.spacing_mut().button_padding.y = 2.0;
        let mut all = app.history_all_branches;
        ui.add_enabled_ui(!app.busy(), |ui| {
            ComboBox::from_id_salt("journal.scope")
                .width(100.0)
                .selected_text(if all {
                    "All branches"
                } else {
                    "Current branch"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut all, false, "Current branch");
                    ui.selectable_value(&mut all, true, "All branches");
                });
        });
        if all != app.history_all_branches {
            app.history_all_branches = all;
            app.reload_history();
        }
        if ui
            .add_enabled(
                !app.busy() && app.repositories.get(app.selected_repository).is_some(),
                Button::new("Reload"),
            )
            .clicked()
        {
            app.reload_history();
        }
        let cancelling = app.history_cancellable();
        let response = ui.add_enabled(
            cancelling || app.history_can_load_more(),
            Button::new(if cancelling { "Cancel" } else { "More" })
                .min_size(Vec2::new(60.0, search.rect.height())),
        ).help_text(if cancelling {
            "Cancel history loading; keep existing rows, selection and scroll position."
        } else {
            "Load up to 500 more commits from the same snapshot. Search covers loaded commits only."
        }).disabled_help_text(if app.history_at_limit() {
            "The 10,000-commit safety limit is reached. Search covers loaded commits only."
        } else { "No more history is available yet; wait for loading or use Reload." });
        if response.clicked() && ui.ctx().current_pass_index() == 0 {
            if cancelling {
                app.cancel_history();
            } else {
                app.load_more_history();
            }
        }
        let (status, help, color) = if app.history_error.is_some() {
            (
                "History unavailable".into(),
                String::from("Reload to retry; displayed rows are from the previous load."),
                palette.removed_text,
            )
        } else if app.history_loading {
            (
                "Loading…".into(),
                "Loading history; existing rows remain available.".into(),
                palette.muted,
            )
        } else if let Some(notice) = app.history_notice() {
            ("Cancelled".into(), notice.into(), palette.muted)
        } else {
            (
                if app.history_at_limit() { format!("{} (limit)", app.commits.len()) }
                else if app.history_truncated { format!("{}+ loaded", app.commits.len()) }
                else { format!("{} commits", app.commits.len()) },
                "500 commits per page; 10,000 loaded commits maximum. Search applies to loaded history only. More retains existing rows; Reload updates the snapshot.".into(),
                palette.muted,
            )
        };
        // One clipped status in the header keeps controls, rows and scroll IDs
        // stationary during loading, errors and retries, including narrow panels.
        ui.add_sized(
            [ui.available_width().max(0.0), 20.0],
            Label::new(RichText::new(status).small().color(color))
                .truncate()
                .show_tooltip_when_elided(false),
        )
        .help_text(help);
    });
    let restore = if history_window {
        None
    } else {
        app.take_history_scroll()
    };
    if app.commits.is_empty() {
        return;
    }
    let filter = app.journal_filter.trim().to_lowercase();
    let visible: Vec<_> = app
        .commits
        .iter()
        .enumerate()
        .filter(|(_, c)| c.matches(&filter))
        .collect();
    if visible.is_empty() {
        return;
    }
    let mut selected = None;
    let mut open_details = false;
    let date_width = ui
        .painter()
        .layout_no_wrap(
            format!(
                "{} {}",
                app.preferences.date_format.label(),
                app.preferences.time_format.label()
            ),
            eframe::egui::FontId::proportional(13.0),
            palette.muted,
        )
        .size()
        .x
        + 8.0;
    // Table row backgrounds expand into cell spacing; keep them inside the panel.
    ui.shrink_clip_rect(ui.available_rect_before_wrap());
    let offset = restore.and_then(|(hash, offset)| {
        visible
            .iter()
            .position(|(_, commit)| commit.hash == hash)
            .map(|index| (index, offset))
    });
    if offset.is_some() {
        ui.style_mut().scroll_animation = eframe::egui::style::ScrollAnimation::none();
    }
    let reveal = history_window
        .then(|| ui.data_mut(|data| data.remove_temp::<String>(egui::Id::unique("history.reveal"))))
        .flatten();
    super::controls::table_resize_style(ui, palette);
    let mut table = TableBuilder::new(ui)
        .id_salt("journal.commits")
        .striped(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(
            Column::initial(if history_window {
                42.0
            } else if wide {
                70.0
            } else {
                46.0
            })
            .range(if history_window {
                28.0..=240.0
            } else {
                46.0..=240.0
            })
            .resizable(true),
        )
        .column(Column::remainder().at_least(80.0))
        .column(Column::exact(if wide { 100.0 } else { 70.0 }))
        .column(Column::exact(date_width.clamp(116.0, 170.0)))
        .min_scrolled_height(0.0);
    if !history_window {
        table = table.column(Column::exact(72.0));
    }
    if let Some((row, _)) = offset {
        table = table.scroll_to_row(row, Some(Align::Min));
    }
    if let Some(hash) = reveal {
        if let Some(index) = visible.iter().position(|(_, commit)| commit.hash == hash) {
            table = table.scroll_to_row(index, Some(Align::Center));
        }
    }
    let output = table
        .header(24.0, |mut header| {
            for title in ["Graph", "Message", "Author", "Date", "Hash"] {
                if history_window && title == "Hash" {
                    continue;
                }
                header.col(|ui| {
                    if title == "Graph" {
                        ui.add_space(6.0);
                    }
                    ui.label(RichText::new(title).size(12.0));
                });
            }
        })
        .body(|mut body| {
            if let Some((_, within_row)) = offset {
                body.ui_mut().scroll_with_delta_animation(
                    Vec2::new(0.0, -within_row),
                    eframe::egui::style::ScrollAnimation::none(),
                );
            }
            // The table row handles selection, including clicks on cell text.
            body.ui_mut().style_mut().interaction.selectable_labels = false;
            body.rows(20.0, visible.len(), |mut row| {
                let (original, commit) = visible[row.index()];
                row.set_selected(app.selected_commit.as_ref() == Some(&commit.hash));
                row.col(|ui| {
                    ui.add_space(6.0);
                    graph(ui, app, original, filter.is_empty(), palette);
                });
                row.col(|ui| {
                    ui.add(Label::new(commit.subject()).show_tooltip_when_elided(false))
                        .help_text(&commit.message);
                });
                row.col(|ui| {
                    ui.add(
                        Label::new(RichText::new(&commit.author).color(palette.muted))
                            .show_tooltip_when_elided(false),
                    )
                    .help_text(&commit.email);
                });
                row.col(|ui| {
                    let date = app.preferences.commit_date(&commit.date);
                    ui.add(
                        Label::new(RichText::new(date).color(palette.muted))
                            .show_tooltip_when_elided(false),
                    )
                    .help_text(&commit.date);
                });
                if !history_window {
                    row.col(|ui| {
                        ui.add(
                            Label::new(
                                RichText::new(commit.hash.get(..8).unwrap_or(&commit.hash))
                                    .monospace()
                                    .color(palette.muted),
                            )
                            .show_tooltip_when_elided(false),
                        )
                        .help_text(&commit.hash);
                    });
                }
                if row.response().double_clicked() {
                    selected = Some(commit.hash.clone());
                    open_details = true;
                } else if row.response().clicked() {
                    selected = Some(commit.hash.clone());
                }
            });
        });
    if offset.is_some() {
        // Resolve the anchor after the new content bounds are known. Rendering
        // another pass prevents the old scroll physics from pulling it back.
        ui.ctx()
            .request_discard("restore history viewport after reload");
    }
    let offset = output.state.unclamped_offset().y.max(0.0);
    let anchor = visible
        .get((offset / 20.0).floor() as usize)
        .map(|(_, commit)| commit.hash.clone());
    if !history_window {
        if let Some(hash) = anchor {
            app.remember_history_scroll(hash, offset % 20.0);
        }
    }
    if let Some(hash) = selected {
        let details_open = app.commit_details_open;
        if open_details && !details_open {
            ui.data_mut(|data| data.insert_temp(egui::Id::unique("history.reveal"), hash.clone()));
        }
        app.select_commit(hash);
        app.commit_details_open = details_open || open_details;
    }
}

fn graph(ui: &mut Ui, app: &SannyGitApp, index: usize, connected: bool, palette: Palette) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), Sense::hover());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    // Appending older branches must not move already visible graph nodes.
    let step = 14.0;
    if app.history_max_lane as f32 * step + 12.0 > rect.width() {
        response.clone().help_text("Additional graph lanes are outside the column. Drag the graph column separator to widen it, or open Commit details to inspect all parents.");
    }
    let position = |row: usize, lane: usize| {
        eframe::egui::pos2(
            rect.left() + 8.0 + lane as f32 * step,
            rect.center().y + (row as f32 - index as f32) * 20.0,
        )
    };
    if connected {
        for HistoryEdge {
            child,
            parent,
            child_lane,
            parent_lane,
        } in &app.history_edges
        {
            if *child <= index && *parent >= index {
                let color = if *child_lane == 0 {
                    palette.graph_main
                } else {
                    palette.graph_branch
                };
                let bend = position(child + 1, *parent_lane);
                painter.line_segment([position(*child, *child_lane), bend], (1.25, color));
                painter.line_segment([bend, position(*parent, *parent_lane)], (1.25, color));
            }
        }
    } else {
        response.help_text("Graph connections are hidden while filtering");
    }
    let commit = &app.commits[index];
    painter.circle_filled(
        position(index, commit.lane),
        3.5,
        if commit.parents.is_empty() {
            palette.graph_root
        } else if commit.lane == 0 {
            palette.graph_main
        } else {
            palette.graph_branch
        },
    );
}

fn details(ui: &mut Ui, app: &mut SannyGitApp) {
    let position_id = egui::Id::unique("history.initial_position");
    if !app.commit_details_open || app.selected_commit.is_none() {
        ui.data_mut(|data| data.remove::<Option<egui::Pos2>>(position_id));
        return;
    }
    let name = app
        .repositories
        .get(app.selected_repository)
        .map_or("Repository", |repository| repository.name.as_str());
    let title = format!("SannyGit — {name} — History");
    let ctx = ui.ctx().clone();
    if ctx.embed_viewports() {
        let mut open = true;
        Window::new("Repository history")
            .id(egui::Id::unique("journal.details"))
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .open(&mut open)
            .default_size([1000.0, 600.0])
            .min_size([900.0, 540.0])
            .max_size(ctx.content_rect().size() - Vec2::splat(40.0))
            .show(&ctx, |ui| history_contents(ui, app));
        app.commit_details_open = open;
        return;
    }
    let cached_position = ctx.data(|data| data.get_temp::<Option<egui::Pos2>>(position_id));
    let position = cached_position.unwrap_or_else(|| {
        let position = ctx.input(|input| {
            let viewport = input.viewport();
            viewport
                .outer_rect
                .map(|outer| history_position(outer, viewport.inner_rect))
        });
        ctx.data_mut(|data| data.insert_temp(position_id, position));
        position
    });
    let mut builder = ViewportBuilder::default()
        .with_title(title)
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([1100.0, 700.0])
        .with_decorations(true);
    if let Some(position) = position {
        builder = builder.with_position(position);
    }
    ctx.show_viewport_immediate(
        ViewportId::from_hash_of("sannygit.history"),
        builder,
        |ui, _class| {
            if ui.input(|input| input.viewport().close_requested()) {
                app.commit_details_open = false;
                ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                return;
            }
            history_contents(ui, app);
            super::show_errors(ui, app);
        },
    );
}

fn history_position(outer: egui::Rect, inner: Option<egui::Rect>) -> egui::Pos2 {
    let decorations = inner.map_or(Vec2::ZERO, |inner| {
        (outer.size() - inner.size()).max(Vec2::ZERO)
    });
    outer.center() - (Vec2::new(1200.0, 800.0) + decorations) / 2.0
}

fn history_contents(ui: &mut Ui, app: &mut SannyGitApp) {
    Frame::new()
        .fill(app.palette().background)
        .inner_margin(2)
        .show(ui, |ui| {
            ui.take_available_space();
            history_panels(ui, app);
        });
}

fn history_panels(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    let mode_id = egui::Id::unique("history.diff.side_by_side");
    let mut side_by_side = ui.data(|data| data.get_temp::<bool>(mode_id).unwrap_or(true));
    let panel_frame = || super::panel_frame(palette).corner_radius(0);
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.spacing_mut().button_padding = Vec2::new(6.0, 3.0);
    Panel::top("history.header")
        .frame(Frame::new().fill(palette.panel))
        .show(ui, |ui| {
            super::controls::dialog_style::header(
                ui,
                palette,
                "Repository history",
                "Browse commits, changed files and their diffs.",
            );
        });
    Panel::bottom("history.diff")
        .resizable(true)
        .default_size(240.0)
        .size_range(120.0..=(ui.available_height() - 300.0).max(120.0))
        .frame(panel_frame().outer_margin(Margin {
            top: 2,
            ..Margin::ZERO
        }))
        .show(ui, |ui| {
            ui.take_available_space();
            super::header(ui, 28.0, palette, |ui| {
                ui.spacing_mut().interact_size.y = 20.0;
                ui.spacing_mut().button_padding = Vec2::new(6.0, 1.0);
                ui.label("Changes");
                ui.selectable_value(&mut side_by_side, false, "Unified");
                ui.selectable_value(&mut side_by_side, true, "Side by Side");
                let path = app
                    .selected_commit_file
                    .as_deref()
                    .unwrap_or("Select a file");
                ui.add(Label::new(path).truncate()).help_text(path);
                if let Some(preview) = &app.commit_preview {
                    ui.label(
                        RichText::new(format!("+{}", preview.added)).color(palette.added_text),
                    );
                    ui.label(
                        RichText::new(format!("−{}", preview.removed)).color(palette.removed_text),
                    );
                }
            });
            if let Some(preview) = &app.commit_preview {
                let id = format!(
                    "history:{:?}:{:?}",
                    app.selected_commit, app.selected_commit_file
                );
                if side_by_side {
                    super::diff_view::render_side_by_side(ui, preview, palette, &id);
                } else {
                    super::diff_view::render_preview(ui, preview, palette, &id);
                }
            }
        });
    ui.data_mut(|data| data.insert_temp(mode_id, side_by_side));
    Panel::right("history.inspector")
        .resizable(true)
        .default_size(360.0)
        .size_range(280.0..=(ui.available_width() - 560.0).max(280.0))
        .frame(panel_frame().outer_margin(Margin {
            left: 2,
            ..Margin::ZERO
        }))
        .show(ui, |ui| {
            ui.take_available_space();
            Panel::top("history.metadata")
                .resizable(true)
                .default_size(240.0)
                .size_range(180.0..=(ui.available_height() - 110.0).max(180.0))
                .frame(Frame::NONE)
                .show(ui, |ui| {
                    ui.take_available_space();
                    super::header(ui, 28.0, palette, |ui| {
                        ui.label("Commit");
                    });
                    metadata(ui, app);
                });
            CentralPanel::default()
                .frame(Frame::NONE)
                .show(ui, |ui| files(ui, app));
        });
    CentralPanel::default().frame(panel_frame()).show(ui, |ui| {
        ui.push_id("history.graph", |ui| rows(ui, app, true));
    });
}

fn metadata(ui: &mut Ui, app: &SannyGitApp) {
    let palette = app.palette();
    let Some(commit) = app
        .commits
        .iter()
        .find(|commit| Some(&commit.hash) == app.selected_commit.as_ref())
    else {
        return;
    };
    Frame::new().inner_margin(8).show(ui, |ui| {
        ScrollArea::vertical()
            .id_salt("history.metadata.scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
                ui.horizontal_top(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
                    ui.painter().rect_filled(rect, 0, palette.accent);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        commit
                            .author
                            .chars()
                            .next()
                            .unwrap_or('?')
                            .to_uppercase()
                            .to_string(),
                        egui::FontId::proportional(32.0),
                        palette.on_accent,
                    );
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label("Commit");
                            ui.monospace(commit.hash.get(..8).unwrap_or(&commit.hash))
                                .help_text(&commit.hash);
                            if icons::button(ui, Icon::Copy, "Copy full commit hash").clicked() {
                                ui.ctx().copy_text(commit.hash.clone());
                            }
                        });
                        ui.label(format!("by {}", commit.author))
                            .help_text(&commit.email);
                        ui.label(app.preferences.commit_date(&commit.date))
                            .help_text(&commit.date);
                        let parents = commit
                            .parents
                            .iter()
                            .map(|hash| hash.get(..8).unwrap_or(hash))
                            .collect::<Vec<_>>();
                        ui.add(
                            Label::new(if parents.is_empty() {
                                "Parent: none (root or shallow boundary)".into()
                            } else {
                                format!("Parent: {}", parents.join(", "))
                            })
                            .wrap(),
                        )
                        .help_text(commit.parents.join("\n"));
                    });
                });
                ui.add_space(10.0);
                ui.add(
                    Label::new(RichText::new(&commit.message).strong())
                        .wrap()
                        .selectable(true),
                );
                if commit.parents.len() > 1 {
                    ui.label(
                        RichText::new("Merge changes against first parent").color(palette.muted),
                    );
                }
                let refs = app
                    .refs
                    .iter()
                    .filter(|reference| reference.hash == commit.hash)
                    .map(|reference| reference.name.as_str())
                    .collect::<Vec<_>>();
                if !refs.is_empty() {
                    ui.add_space(8.0);
                    ui.add(
                        Label::new(format!("Refs: {}", refs.join(", ")))
                            .wrap()
                            .selectable(true),
                    );
                }
            });
    });
}

fn files(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    let filter_id = ui.scope_id().with("history.files.filter");
    let mut filter = ui.data_mut(|data| data.get_temp::<String>(filter_id).unwrap_or_default());
    let mut retry = false;
    super::header(ui, 28.0, palette, |ui| {
        ui.spacing_mut().interact_size.y = 20.0;
        ui.spacing_mut().button_padding = Vec2::new(6.0, 1.0);
        ui.label("Files");
        ui.add(
            super::controls::singleline(&mut filter)
                .hint_text("File filter")
                .desired_width(100.0)
                .min_size(Vec2::ZERO)
                .margin(Vec2::new(5.0, 2.0)),
        );
        retry = ui
            .add_enabled(
                app.commit_preview_error.is_some() && !app.busy(),
                Button::new("Retry"),
            )
            .clicked();
        let status = if app.commit_preview_error.is_some() {
            "Preview unavailable"
        } else if let Some(files) = &app.commit_files {
            if files.is_empty() {
                "No changed files"
            } else if app.commit_preview.is_none() {
                "Loading file diff…"
            } else {
                ""
            }
        } else {
            "Loading files…"
        };
        ui.add(Label::new(RichText::new(status).color(palette.muted)).truncate());
    });
    ui.data_mut(|data| data.insert_temp(filter_id, filter.clone()));
    let query = filter.trim().to_lowercase();
    let files = app
        .commit_files
        .iter()
        .flatten()
        .filter(|file| file.path.to_lowercase().contains(&query))
        .collect::<Vec<_>>();
    let mut selected = None;
    ui.spacing_mut().item_spacing.x = 8.0;
    super::controls::table_resize_style(ui, palette);
    TableBuilder::new(ui)
        .id_salt("history.files")
        .resizable(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::initial(160.0).range(70.0..=600.0).resizable(true))
        .column(Column::initial(100.0).range(65.0..=300.0).resizable(true))
        .column(Column::remainder().at_least(50.0))
        .min_scrolled_height(0.0)
        .header(24.0, |mut header| {
            for title in ["Name", "Modification", "Relative Directory"] {
                header.col(|ui| {
                    ui.add_space(6.0);
                    ui.add(Label::new(title).truncate()).help_text(title);
                });
            }
        })
        .body(|body| {
            body.rows(22.0, files.len(), |mut row| {
                let file = files[row.index()];
                let (directory, name) = file.path.rsplit_once('/').unwrap_or(("", &file.path));
                row.set_selected(app.selected_commit_file.as_ref() == Some(&file.path));
                row.col(|ui| {
                    ui.add_space(6.0);
                    ui.add(icons::image(
                        Icon::for_file(&file.path),
                        palette.muted,
                        14.0,
                    ));
                    ui.add(
                        Label::new(name.escape_debug().to_string())
                            .truncate()
                            .selectable(false),
                    );
                });
                row.col(|ui| {
                    let status = match file.status.as_bytes().first() {
                        Some(b'A') => "Added",
                        Some(b'D') => "Deleted",
                        Some(b'M') => "Modified",
                        Some(b'R') => "Renamed",
                        Some(b'C') => "Copied",
                        Some(b'T') => "Type changed",
                        _ => &file.status,
                    };
                    ui.add(
                        Label::new(RichText::new(status).color(palette.muted))
                            .truncate()
                            .selectable(false),
                    );
                });
                row.col(|ui| {
                    ui.add(
                        Label::new(
                            RichText::new(directory.escape_debug().to_string())
                                .color(palette.muted),
                        )
                        .truncate()
                        .selectable(false),
                    );
                });
                let response = row.response();
                if response.clicked() {
                    selected = Some(file.path.clone());
                }
                response.help_text(if let Some(source) = &file.original_path {
                    format!("{} → {}", source.escape_debug(), file.path.escape_debug())
                } else {
                    file.path.escape_debug().to_string()
                });
            });
        });
    if let Some(path) = selected {
        app.select_commit_file(path);
    }
    if retry {
        app.retry_commit_preview();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{install_fonts, ThemeChoice};
    use eframe::egui::{self, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

    #[test]
    fn history_is_centered_over_the_parent_including_native_decorations() {
        let outer = Rect::from_min_size(egui::pos2(200.0, 100.0), Vec2::new(1536.0, 1024.0));
        let inner = Rect::from_min_size(egui::pos2(208.0, 130.0), Vec2::new(1520.0, 986.0));
        assert_eq!(
            history_position(outer, Some(inner)),
            egui::pos2(360.0, 193.0)
        );
        let outer = Rect::from_min_size(egui::pos2(-1920.0, 0.0), Vec2::new(1600.0, 900.0));
        assert_eq!(history_position(outer, None), egui::pos2(-1720.0, 50.0));
    }

    #[test]
    fn file_columns_resize_and_keep_widths_when_the_commit_changes() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let ctx = Context::default();
            install_fonts(&ctx);
            theme.apply(&ctx);
            let mut app = SannyGitApp::with_theme(theme);
            let render = |app: &mut SannyGitApp, events| {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 280.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| files(ui, app),
                );
                output.textures_delta.clear();
                output
            };
            let column_x = |output: &egui::FullOutput| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == "Modification" => {
                            Some(text.pos.x)
                        }
                        _ => None,
                    })
                    .unwrap()
            };
            for _ in 0..3 {
                render(&mut app, vec![]);
            }
            let output = render(&mut app, vec![]);
            let before = column_x(&output);
            let pointer = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::LineSegment { points, .. }
                        if (points[0].x - points[1].x).abs() < 0.1 =>
                    {
                        Some((points[0] + points[1].to_vec2()) * 0.5)
                    }
                    _ => None,
                })
                .unwrap();
            render(&mut app, vec![Event::PointerMoved(pointer)]);
            render(
                &mut app,
                vec![Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }],
            );
            let moved = pointer + Vec2::new(24.0, 0.0);
            for _ in 0..3 {
                render(&mut app, vec![Event::PointerMoved(moved)]);
            }
            render(
                &mut app,
                vec![Event::PointerButton {
                    pos: moved,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
            );
            let after = column_x(&render(&mut app, vec![]));
            assert!(
                after > before + 16.0,
                "Column did not resize: {before} -> {after}"
            );
            app.selected_commit = Some("another commit".into());
            assert_eq!(column_x(&render(&mut app, vec![])), after);
        }
    }

    #[test]
    fn copy_icon_copies_the_full_commit_hash() {
        let ctx = Context::default();
        install_fonts(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.commits = crate::mock_data::commits();
        let hash = app.commits[0].hash.clone();
        app.select_commit(hash.clone());
        let render = |events| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(360.0, 240.0))),
                    events,
                    ..Default::default()
                },
                |ui| metadata(ui, &app),
            );
            output.textures_delta.clear();
            output
        };
        let output = render(vec![]);
        let pointer = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.job.text == hash.get(..8).unwrap_or(&hash) =>
                {
                    Some(egui::pos2(
                        text.pos.x + text.galley.size().x + 8.0 + icons::BUTTON_SIZE / 2.0,
                        text.pos.y + text.galley.size().y / 2.0,
                    ))
                }
                _ => None,
            })
            .unwrap();
        render(vec![
            Event::PointerMoved(pointer),
            Event::PointerButton {
                pos: pointer,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        let output = render(vec![Event::PointerButton {
            pos: pointer,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        assert!(output.platform_output.commands.iter().any(
            |command| matches!(command, egui::OutputCommand::CopyText(text) if text == &hash)
        ));
    }

    #[test]
    fn file_table_selects_from_each_column_and_filters_without_losing_selection() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for (label, occurrence) in [("app.rs", 0), ("Modified", 1), ("crates/sannygit/src", 1)]
            {
                let ctx = Context::default();
                install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.commit_files = Some(
                    crate::mock_data::changes()
                        .into_iter()
                        .take(2)
                        .map(|file| crate::history::CommitFile {
                            status: "M".into(),
                            path: file.path,
                            original_path: None,
                        })
                        .collect(),
                );
                let selected = app.commit_files.as_ref().unwrap()[1].path.clone();
                let render = |app: &mut SannyGitApp, events| {
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(450.0, 250.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| files(ui, app),
                    );
                    output.textures_delta.clear();
                    output
                };
                let position = |output: &egui::FullOutput, label: &str, occurrence| {
                    output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.job.text == label => {
                                Some(text.pos + text.galley.size() / 2.0)
                            }
                            _ => None,
                        })
                        .nth(occurrence)
                        .unwrap()
                };
                for _ in 0..3 {
                    render(&mut app, vec![]);
                }
                let pointer = position(&render(&mut app, vec![]), label, occurrence);
                for pressed in [true, false] {
                    render(
                        &mut app,
                        vec![
                            Event::PointerMoved(pointer),
                            Event::PointerButton {
                                pos: pointer,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                    );
                }
                assert_eq!(
                    app.selected_commit_file.as_ref(),
                    Some(&selected),
                    "{theme:?}: {label}"
                );
                let pointer = position(&render(&mut app, vec![]), "File filter", 0);
                for pressed in [true, false] {
                    render(
                        &mut app,
                        vec![
                            Event::PointerMoved(pointer),
                            Event::PointerButton {
                                pos: pointer,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                    );
                }
                render(&mut app, vec![Event::Text("app.rs".into())]);
                let output = render(&mut app, vec![]);
                assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "main.rs")));
                assert_eq!(app.selected_commit_file.as_ref(), Some(&selected));
                assert_eq!(app.commit_files.as_ref().unwrap().len(), 2);
            }
        }
    }

    #[test]
    fn commit_details_geometry_is_stable_across_file_loading_errors_and_retry() {
        use crate::theme::InterfaceStyle;
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                    let ctx = Context::default();
                    install_fonts(&ctx);
                    theme.apply(&ctx);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.interface_style = style;
                    app.commits = crate::mock_data::commits();
                    app.select_commit(app.commits[0].hash.clone());
                    let render = |app: &mut SannyGitApp| {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| details(ui, app),
                        );
                        output.textures_delta.clear();
                        output
                    };
                    let mut baseline = None;
                    for stage in 0..6 {
                        match stage {
                            1 => app.commit_preview_error = Some("Long diagnostic ".repeat(100)),
                            2 => {
                                app.retry_commit_preview();
                                app.commit_files = Some(vec![crate::history::CommitFile {
                                    status: "A".into(),
                                    path: "file.txt".into(),
                                    original_path: None,
                                }]);
                                app.selected_commit_file = Some("file.txt".into());
                            }
                            3 => {
                                app.commit_preview = Some(crate::models::DiffPreview::new(
                                    &"+preview line\n".repeat(500),
                                ))
                            }
                            4 => {
                                app.commit_preview = None;
                                app.commit_preview_error = Some("File diff limit ".repeat(100));
                            }
                            5 => app.retry_commit_preview(),
                            _ => {}
                        }
                        for _ in 0..5 {
                            render(&mut app);
                        }
                        let output = render(&mut app);
                        let rect = egui::AreaState::load(&ctx, egui::Id::unique("journal.details"))
                            .unwrap()
                            .rect();
                        if let Some(before) = baseline {
                            assert_eq!(rect, before, "stage {stage}: {theme:?} {style:?} {size:?}");
                        } else {
                            baseline = Some(rect);
                        }
                        assert!(rect.width() <= size.x && rect.height() <= size.y);
                        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains("Long diagnostic") || text.galley.job.text.contains("File diff limit"))));
                        if stage >= 2 {
                            assert_eq!(app.selected_commit_file.as_deref(), Some("file.txt"));
                            assert_eq!(app.commit_files.as_ref().unwrap().len(), 1);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn appending_and_cancelling_keep_scrolled_rows_graph_nodes_and_selection_stationary() {
        use crate::theme::InterfaceStyle;
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                    let ctx = Context::default();
                    install_fonts(&ctx);
                    egui_extras::install_image_loaders(&ctx);
                    theme.apply(&ctx);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.interface_style = style;
                    app.repositories = crate::mock_data::repositories();
                    app.selected_repository = 0;
                    app.state_current = true;
                    let mut commits = crate::mock_data::long_history();
                    let tail = commits.split_off(60);
                    app.commits = commits;
                    app.history_max_lane = 1;
                    app.selected_commit = Some(app.commits[20].hash.clone());
                    let selected = app.selected_commit.clone();
                    let mut time = 0.0;
                    let mut render = |app: &mut SannyGitApp, events| {
                        time += 0.1;
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                time: Some(time),
                                events,
                                ..Default::default()
                            },
                            |ui| crate::ui::show(ui, app),
                        );
                        output.textures_delta.clear();
                        output
                    };
                    let rows = |output: &egui::FullOutput| {
                        output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Text(text)
                                    if text.galley.job.text.starts_with("History ")
                                        && text.galley.job.text.contains(':') =>
                                {
                                    Some((text.galley.job.text.clone(), text.pos))
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    };
                    let nodes = |output: &egui::FullOutput| {
                        output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Circle(circle)
                                    if circle.radius == 3.5
                                        && shape.clip_rect.contains(circle.center) =>
                                {
                                    Some(circle.center)
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    };
                    for _ in 0..4 {
                        render(&mut app, vec![]);
                    }
                    let initial = render(&mut app, vec![]);
                    let pointer = rows(&initial)[0].1;
                    render(
                        &mut app,
                        vec![
                            Event::PointerMoved(pointer),
                            Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0.0, -10000.0),
                                phase: egui::TouchPhase::Move,
                                source: egui::MouseWheelSource::Wheel,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                    );
                    render(&mut app, vec![Event::PointerMoved(Pos2::ZERO)]);
                    for _ in 0..12 {
                        render(&mut app, vec![]);
                    }
                    let before = render(&mut app, vec![]);
                    let baseline = rows(&before);
                    let graph = nodes(&before);
                    assert_ne!(baseline, rows(&initial));
                    assert!(!graph.is_empty());
                    app.reload_history();
                    for _ in 0..3 {
                        let output = render(&mut app, vec![]);
                        assert_eq!(rows(&output), baseline);
                        assert_eq!(nodes(&output), graph);
                    }
                    app.cancel_history();
                    let output = render(&mut app, vec![]);
                    assert_eq!(rows(&output), baseline);
                    app.commits.extend(tail);
                    app.history_max_lane = 6;
                    for _ in 0..3 {
                        let output = render(&mut app, vec![]);
                        assert_eq!(
                            rows(&output),
                            baseline,
                            "Rows moved after append: {theme:?} {style:?} {size:?}"
                        );
                        assert_eq!(
                            nodes(&output),
                            graph,
                            "Graph nodes moved when later lanes were added"
                        );
                        assert_eq!(app.selected_commit, selected);
                    }
                }
            }
        }
    }

    #[test]
    fn history_status_preserves_rows_controls_scroll_and_selection() {
        use crate::theme::InterfaceStyle;
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                    let context = Context::default();
                    install_fonts(&context);
                    egui_extras::install_image_loaders(&context);
                    theme.apply(&context);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.interface_style = style;
                    app.repositories = crate::mock_data::repositories();
                    app.selected_repository = 0;
                    app.commits = crate::mock_data::long_history();
                    app.selected_commit = Some(app.commits[10].hash.clone());
                    let selected = app.selected_commit.clone();
                    let mut time = 0.0;
                    let mut render = |app: &mut SannyGitApp, events| {
                        time += 0.1;
                        let mut output = context.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                time: Some(time),
                                events,
                                ..Default::default()
                            },
                            |ui| crate::ui::show(ui, app),
                        );
                        output.textures_delta.clear();
                        output
                    };
                    for _ in 0..4 {
                        render(&mut app, vec![]);
                    }
                    let positions = |output: &egui::FullOutput| {
                        output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Text(text)
                                    if (text.galley.job.text.starts_with("History ")
                                        && text.galley.job.text.contains(':'))
                                        || [
                                            "Journal",
                                            "Reload",
                                            "Current branch",
                                            "Message",
                                            "Author",
                                            "Date",
                                            "Hash",
                                        ]
                                        .contains(&text.galley.job.text.as_str()) =>
                                {
                                    Some((text.galley.job.text.clone(), text.pos))
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    };
                    let initial = render(&mut app, vec![]);
                    let initial_positions = positions(&initial);
                    let pointer = initial_positions
                        .iter()
                        .find(|(label, _)| label.starts_with("History "))
                        .unwrap()
                        .1;
                    render(
                        &mut app,
                        vec![
                            Event::PointerMoved(pointer),
                            Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                phase: egui::TouchPhase::Move,
                                source: egui::MouseWheelSource::Wheel,
                                delta: egui::vec2(0.0, -240.0),
                                modifiers: Modifiers::NONE,
                            },
                        ],
                    );
                    render(&mut app, vec![Event::PointerMoved(Pos2::ZERO)]);
                    for _ in 0..12 {
                        render(&mut app, vec![]);
                    }
                    let baseline = positions(&render(&mut app, vec![]));
                    assert_ne!(
                        baseline, initial_positions,
                        "History did not scroll: {theme:?} {style:?} {size:?}"
                    );
                    assert!(baseline
                        .iter()
                        .any(|(label, _)| label.starts_with("History ")));
                    for stage in 0..5 {
                        match stage {
                            0 => app.history_loading = true,
                            1 => {
                                app.history_loading = false;
                                app.history_error = Some("Cannot read history. ".repeat(100));
                            }
                            2 => app.reload_history(),
                            3 => {
                                app.history_loading = false;
                                app.history_error = Some("History reload cancelled".into());
                            }
                            _ => {
                                app.history_error = None;
                            }
                        }
                        for _ in 0..3 {
                            let output = render(&mut app, vec![]);
                            let after = positions(&output);
                            assert_eq!(after.len(), baseline.len());
                            for ((label, position), (before_label, before_position)) in
                                after.iter().zip(&baseline)
                            {
                                assert_eq!(label, before_label);
                                assert!((*position - *before_position).length() < 1.0,
                                    "History moved at stage {stage}: {theme:?} {style:?} {size:?}: {label} {before_position:?} -> {position:?}");
                            }
                            assert_eq!(app.selected_commit, selected);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn single_click_selects_and_double_click_opens_history() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for label in [
                "Improve diff view highlighting",
                "SaNNy",
                "2 hours ago",
                "a1b2c3d",
            ] {
                let context = Context::default();
                install_fonts(&context);
                theme.apply(&context);
                let mut app = SannyGitApp::with_theme(theme);
                app.commits = crate::mock_data::commits();
                let render = |app: &mut SannyGitApp, events| {
                    context.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(900.0, 300.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| show(ui, app),
                    )
                };
                let mut output = render(&mut app, Vec::new());
                let position = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => {
                            Some(text.galley.rect.translate(text.pos.to_vec2()).center())
                        }
                        _ => None,
                    })
                    .expect("Commit cell is missing");
                output.textures_delta.clear();
                for pressed in [true, false] {
                    let mut output = render(
                        &mut app,
                        vec![
                            Event::PointerMoved(position),
                            Event::PointerButton {
                                pos: position,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                    );
                    output.textures_delta.clear();
                }
                assert_eq!(
                    app.selected_commit.as_deref(),
                    Some("a1b2c3d"),
                    "Click on {label} in {theme:?}"
                );
                assert!(!app.commit_details_open);
                for pressed in [true, false] {
                    let mut output = render(
                        &mut app,
                        vec![
                            Event::PointerMoved(position),
                            Event::PointerButton {
                                pos: position,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                    );
                    output.textures_delta.clear();
                }
                assert!(
                    app.commit_details_open,
                    "Double click on {label} in {theme:?}"
                );
                assert_eq!(app.selected_commit.as_deref(), Some("a1b2c3d"));
            }
        }
    }
}
