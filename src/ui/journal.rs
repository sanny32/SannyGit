use super::controls::tooltips::TooltipResponse;
use crate::{app::SannyGitApp, models::HistoryEdge, theme::Palette};
use eframe::egui::{
    Align, Button, ComboBox, Label, Layout, RichText, ScrollArea, Sense, Ui, Vec2, Window,
};
use egui_extras::{Column, TableBuilder};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    super::register_panel(ui, "journal");
    details(ui, app);
    let palette = app.palette();
    ui.spacing_mut().item_spacing.y = 0.0;
    let wide = ui.available_width() >= 580.0;
    super::header(ui, 28.0, palette, |ui| {
        ui.spacing_mut().interact_size.y = 20.0;
        let title = ui.heading("Journal");
        super::register_panel_title(ui, "journal", title.rect);
        let search_width = (ui.available_width() - 360.0).clamp(60.0, 240.0);
        ui.add_enabled(
            !app.commits.is_empty(),
            crate::ui::controls::singleline(&mut app.journal_filter)
                .min_size(Vec2::ZERO)
                .hint_text("Search loaded commits…")
                .desired_width(search_width)
                .margin(Vec2::new(5.0, 2.0)),
        );
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
            Button::new(if cancelling { "Cancel" } else { "More" }).min_size(Vec2::new(60.0, 20.0)),
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
    let restore = app.take_history_scroll();
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
    let mut table = TableBuilder::new(ui)
        .id_salt("journal.commits")
        .striped(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(
            Column::initial(if wide { 70.0 } else { 46.0 })
                .range(46.0..=240.0)
                .resizable(true),
        )
        .column(Column::remainder().at_least(80.0))
        .column(Column::exact(if wide { 100.0 } else { 70.0 }))
        .column(Column::exact(date_width.clamp(116.0, 170.0)))
        .column(Column::exact(72.0))
        .min_scrolled_height(0.0);
    if let Some((row, _)) = offset {
        table = table.scroll_to_row(row, Some(Align::Min));
    }
    let output = table
        .header(24.0, |mut header| {
            for title in ["Graph", "Message", "Author", "Date", "Hash"] {
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
                if row.response().clicked() {
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
    if let Some(hash) = anchor {
        app.remember_history_scroll(hash, offset % 20.0);
    }
    if let Some(hash) = selected {
        let details_open = app.commit_details_open;
        app.select_commit(hash);
        app.commit_details_open = details_open;
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
    if !app.commit_details_open {
        return;
    }
    let Some(commit) = app
        .commits
        .iter()
        .find(|c| Some(&c.hash) == app.selected_commit.as_ref())
    else {
        return;
    };
    let palette = app.palette();
    let mut open = true;
    let mut retry = false;
    let mut select_file = None;
    Window::new("Commit details")
        .anchor(
            eframe::egui::Align2::CENTER_CENTER,
            eframe::egui::Vec2::ZERO,
        )
        .id(eframe::egui::Id::unique("journal.details"))
        .collapsible(false)
        .open(&mut open)
        .default_size([850.0, 570.0])
        .min_size([480.0, 320.0])
        .max_size(ui.ctx().content_rect().size() - Vec2::splat(40.0))
        .show(ui.ctx(), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.monospace(&commit.hash);
                if ui.small_button("Copy hash").clicked() {
                    ui.ctx().copy_text(commit.hash.clone());
                }
            });
            let metadata_height = (ui.available_height() * 0.35).clamp(48.0, 190.0);
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), metadata_height),
                Layout::top_down(Align::Min),
                |ui| {
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .id_salt(("commit.metadata", &commit.hash))
                        .min_scrolled_height(24.0)
                        .max_height(metadata_height)
                        .show(ui, |ui| {
                            ui.add(
                                Label::new(format!("Author: {} <{}>", commit.author, commit.email))
                                    .wrap()
                                    .selectable(true),
                            );
                            ui.add(
                                Label::new(format!(
                                    "Date: {}",
                                    app.preferences.commit_date(&commit.date)
                                ))
                                .wrap()
                                .selectable(true),
                            )
                            .help_text(&commit.date);
                            ui.add(
                                Label::new(if commit.parents.is_empty() {
                                    "Parents: none in available history (root or shallow boundary)"
                                        .into()
                                } else {
                                    format!("Parents: {}", commit.parents.join(", "))
                                })
                                .wrap()
                                .selectable(true),
                            );
                            ui.add(Label::new(&commit.message).wrap().selectable(true));
                        });
                },
            );
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(if commit.parents.len() > 1 {
                    "Merge changes against first parent"
                } else {
                    "Commit changes"
                });
                retry = ui
                    .add_enabled(
                        app.commit_preview_error.is_some() && !app.busy(),
                        Button::new("Retry"),
                    )
                    .clicked();
                ui.label(
                    RichText::new(if app.commit_preview_error.is_some() {
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
                    })
                    .color(palette.muted),
                );
            });
            let height = ui.available_height().max(100.0);
            let file_width = (ui.available_width() * 0.32).clamp(120.0, 280.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(file_width, height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_size(Vec2::new(file_width, height));
                        ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                        ScrollArea::both()
                            .id_salt(("commit.files", &commit.hash))
                            .auto_shrink([false, false])
                            .show_rows(
                                ui,
                                20.0,
                                app.commit_files.as_ref().map_or(0, Vec::len),
                                |ui, rows| {
                                    let Some(files) = &app.commit_files else {
                                        return;
                                    };
                                    for file in &files[rows] {
                                        let label = format!(
                                            "{}  {}",
                                            file.status,
                                            file.path.escape_debug()
                                        );
                                        let response = ui.selectable_label(
                                            app.selected_commit_file.as_ref() == Some(&file.path),
                                            label,
                                        );
                                        let clicked = response.clicked();
                                        response.help_text(
                                            if let Some(source) = &file.original_path {
                                                format!(
                                                    "{} → {}",
                                                    source.escape_debug(),
                                                    file.path.escape_debug()
                                                )
                                            } else {
                                                file.path.escape_debug().to_string()
                                            },
                                        );
                                        if clicked {
                                            select_file = Some(file.path.clone());
                                        }
                                    }
                                },
                            );
                    },
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_height(height);
                        ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                        if let Some(preview) = &app.commit_preview {
                            let id = format!("{}:{:?}", commit.hash, app.selected_commit_file);
                            super::diff_view::render_preview(ui, preview, palette, &id);
                        }
                    },
                );
            });
        });
    app.commit_details_open = open;
    if let Some(path) = select_file {
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
    fn clicking_commit_text_in_each_column_selects_the_commit() {
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
            }
        }
    }
}
