mod branches;
mod changes;
mod clone_dialog;
mod commit_panel;
mod controls;
mod diff_view;
mod journal;
mod remote_dialog;
mod repositories;
mod repository_dialogs;
mod settings;
mod status_bar;
mod toolbar;

use controls::dialog_style;

use crate::{app::SannyGitApp, theme::Palette};
use eframe::egui::{
    Align, CentralPanel, Frame, InnerResponse, Layout, Margin, Panel, Sense, Ui, UiBuilder, Vec2,
};

fn sync_badge(text: &str, palette: Palette, size: f32) -> eframe::egui::text::LayoutJob {
    use eframe::egui::{text::LayoutJob, FontId, TextFormat};
    let mut job = LayoutJob::default();
    for (index, part) in text.split_whitespace().enumerate() {
        if index > 0 {
            job.append(
                " ",
                0.0,
                TextFormat {
                    font_id: FontId::proportional(size),
                    color: palette.muted,
                    ..Default::default()
                },
            );
        }
        let color = if part.starts_with('↑') {
            palette.outgoing
        } else if part.starts_with(['↓', '✓']) {
            palette.incoming
        } else {
            palette.muted
        };
        job.append(
            part,
            0.0,
            TextFormat {
                font_id: FontId::proportional(size),
                color,
                ..Default::default()
            },
        );
    }
    job
}

fn header<R>(
    ui: &mut Ui,
    height: f32,
    palette: Palette,
    contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, palette.header);
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        (1.0, palette.border),
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(8.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    InnerResponse::new(contents(&mut child), response)
}

fn panel_frame(palette: Palette) -> Frame {
    Frame::new()
        .fill(palette.panel)
        .stroke((1.0, palette.border))
        .corner_radius(0)
}

fn sidebar_contents(ui: &mut Ui, app: &mut SannyGitApp) -> eframe::egui::Rect {
    let palette = app.theme.palette();
    let branches = Panel::bottom("working_tree.v3.branches")
        .resizable(true)
        .default_size(ui.available_height() * 0.36)
        .size_range(130.0..=(ui.available_height() - 120.0).max(130.0))
        .frame(panel_frame(palette).inner_margin(8).outer_margin(Margin {
            top: 4,
            ..Margin::ZERO
        }))
        .show(ui, |ui| {
            // Persist the allocated height, not the smaller content height.
            ui.take_available_space();
            branches::show(ui, app);
        });
    CentralPanel::default()
        .frame(panel_frame(palette).inner_margin(8))
        .show(ui, |ui| repositories::show(ui, app));
    branches.response.rect
}

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    // Keep dense panel rows compact without truncating floating tooltips.
    ui.style_mut().wrap_mode = Some(eframe::egui::TextWrapMode::Truncate);
    let context = ui.ctx().clone();
    let palette = app.theme.palette();
    toolbar::show(ui, app);
    settings::show(ui.ctx(), app);
    repository_dialogs::show(ui, app);
    eframe::egui::Window::new("About SannyGit")
        .open(&mut app.about_dialog)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.heading("SannyGit");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.label("A desktop Git client built with Rust and egui.");
            ui.label("MIT License");
        });
    if let Some(error) = app.error.clone() {
        let mut dismiss = false;
        dialog_style::modal(ui.ctx(), palette, "application.error").show(ui.ctx(), |ui| {
            ui.set_width(460.0);
            dialog_style::setup(ui);
            ui.spacing_mut().item_spacing.y = 0.0;
            dialog_style::section(ui, palette.panel, Margin::same(18), |ui| {
                ui.label(
                    eframe::egui::RichText::new("Operation failed")
                        .size(16.0)
                        .family(crate::theme::semibold_family()),
                );
                ui.add_space(12.0);
                eframe::egui::ScrollArea::vertical()
                    .id_salt("application.error.message")
                    .max_height(240.0)
                    .show(ui, |ui| {
                        ui.add(eframe::egui::Label::new(error).wrap());
                    });
            });
            dialog_style::section(ui, palette.header, Margin::symmetric(18, 12), |ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(dialog_style::primary("OK", palette)).clicked() {
                        dismiss = true;
                    }
                });
            });
        });
        if dismiss {
            app.error = None;
        }
    }
    status_bar::show(ui, app);

    Panel::left("working_tree.v3.sidebar")
        .resizable(true)
        .default_size((context.content_rect().width() * 0.17).clamp(200.0, 270.0))
        .size_range(190.0..=350.0)
        .frame(Frame::new().fill(palette.background).inner_margin(4))
        .show(ui, |ui| {
            ui.take_available_space();
            sidebar_contents(ui, app);
        });

    CentralPanel::default()
        .frame(Frame::new().fill(palette.background).inner_margin(Margin {
            left: 0,
            right: 4,
            top: 4,
            bottom: 4,
        }))
        .show(ui, |ui| {
            Panel::bottom("working_tree.v3.journal")
                .resizable(true)
                .default_size(180.0)
                .size_range(150.0..=300.0)
                .frame(panel_frame(palette))
                .show(ui, |ui| {
                    ui.take_available_space();
                    journal::show(ui, app);
                });
            Panel::top("working_tree.v3.files")
                .resizable(true)
                .default_size((context.content_rect().height() * 0.38).max(300.0))
                .size_range(260.0..=550.0)
                .frame(Frame::new().fill(palette.background).inner_margin(Margin {
                    bottom: 4,
                    ..Margin::ZERO
                }))
                .show(ui, |ui| {
                    ui.take_available_space();
                    Panel::right("working_tree.v3.commit")
                        .resizable(true)
                        .default_size(ui.available_width() * 0.29)
                        .size_range(260.0..=450.0)
                        .frame(panel_frame(palette).outer_margin(Margin {
                            left: 4,
                            ..Margin::ZERO
                        }))
                        .show(ui, |ui| {
                            ui.take_available_space();
                            commit_panel::show(ui, app);
                        });
                    CentralPanel::default()
                        .frame(panel_frame(palette))
                        .show(ui, |ui| changes::show(ui, app));
                });
            CentralPanel::default()
                .frame(panel_frame(palette).outer_margin(Margin {
                    bottom: 4,
                    ..Margin::ZERO
                }))
                .show(ui, |ui| diff_view::show(ui, app));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeChoice;
    use eframe::egui::{pos2, vec2, Context, Event, Modifiers, PointerButton, RawInput, Rect};

    #[test]
    fn repository_switch_keeps_panel_rectangles() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [vec2(1100.0, 700.0), vec2(1536.0, 1024.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.repositories = crate::mock_data::repositories();
                app.selected_repository = 0;
                app.changes = crate::mock_data::changes();
                app.commits = crate::mock_data::commits();
                app.refs = crate::mock_data::refs();
                app.current_branch = "main".into();
                let render = |app: &mut SannyGitApp, events: Vec<Event>| {
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
                            events,
                            ..Default::default()
                        },
                        |ui| show(ui, app),
                    );
                    output.textures_delta.clear();
                    let mut rects = ctx.data(|data| {
                        data.iter_temp_by_type::<eframe::egui::PanelState>()
                            .map(|state| state.outer_rect)
                            .collect::<Vec<_>>()
                    });
                    rects.sort_by(|a, b| {
                        a.min
                            .x
                            .total_cmp(&b.min.x)
                            .then(a.min.y.total_cmp(&b.min.y))
                            .then(a.max.x.total_cmp(&b.max.x))
                    });
                    rects
                };
                for _ in 0..3 {
                    render(&mut app, vec![]);
                }
                let initial = render(&mut app, vec![]);
                let journal = initial
                    .iter()
                    .find(|r| {
                        r.min.x > 100.0
                            && r.height() > 100.0
                            && r.height() <= 300.0
                            && r.max.x > size.x - 10.0
                            && r.max.y > size.y - 40.0
                    })
                    .unwrap();
                let pointer = pos2(journal.center().x, journal.top());
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
                let moved = pointer - vec2(0.0, 35.0);
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
                let before = render(&mut app, vec![]);
                assert!(
                    before
                        .iter()
                        .any(|r| r.max == journal.max && r.height() > journal.height() + 20.0),
                    "Journal did not retain custom height"
                );
                assert!(before.len() >= 5);
                // Simulate the loading/empty view of another repository.
                app.selected_repository = 1;
                app.changes.clear();
                app.commits.clear();
                app.refs.clear();
                app.current_branch.clear();
                app.refs_loading = true;
                app.history_loading = true;
                for _ in 0..10 {
                    let after = render(&mut app, vec![]);
                    assert_eq!(before.len(), after.len());
                    for (a, b) in before.iter().zip(&after) {
                        assert!(
                            (a.min - b.min).length() < 1.0 && (a.max - b.max).length() < 1.0,
                            "Panel moved on repository switch {theme:?} {size:?}: {a:?} -> {b:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn error_dialog_does_not_move_panels_and_can_be_dismissed() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let ctx = Context::default();
            crate::theme::install_fonts(&ctx);
            theme.apply(&ctx);
            let mut app = SannyGitApp::with_theme(theme);
            let render = |app: &mut SannyGitApp, events| {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1100.0, 700.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| show(ui, app),
                );
                output.textures_delta.clear();
                let mut panels = ctx.data(|data| {
                    data.iter_temp_by_type::<eframe::egui::PanelState>()
                        .map(|state| state.outer_rect)
                        .collect::<Vec<_>>()
                });
                panels.sort_by(|a, b| {
                    a.min
                        .x
                        .total_cmp(&b.min.x)
                        .then(a.min.y.total_cmp(&b.min.y))
                        .then(a.max.x.total_cmp(&b.max.x))
                });
                (panels, output)
            };
            for _ in 0..3 {
                render(&mut app, vec![]);
            }
            let (before, _) = render(&mut app, vec![]);
            app.error = Some(
                "LLM API returned HTTP 404. Check the endpoint, model, API key and provider limits."
                    .into(),
            );
            for _ in 0..3 {
                render(&mut app, vec![]);
            }
            let (after, output) = render(&mut app, vec![]);
            assert_eq!(before.len(), after.len());
            for (before, after) in before.iter().zip(&after) {
                assert!(
                    (before.min - after.min).length() < 1.0
                        && (before.max - after.max).length() < 1.0,
                    "Error moved a panel in {theme:?}: {before:?} -> {after:?}"
                );
            }
            for label in [
                "Operation failed",
                "LLM API returned HTTP 404. Check the endpoint, model, API key and provider limits.",
                "OK",
            ] {
                assert!(
                    output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, eframe::egui::Shape::Text(text) if text.galley.job.text == label)
                    }),
                    "Missing error dialog text {label} in {theme:?}"
                );
            }
            let ok = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    eframe::egui::Shape::Text(text) if text.galley.job.text == "OK" => {
                        Some(text.pos)
                    }
                    _ => None,
                })
                .unwrap();
            render(&mut app, vec![Event::PointerMoved(ok)]);
            render(
                &mut app,
                vec![Event::PointerButton {
                    pos: ok,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }],
            );
            render(
                &mut app,
                vec![Event::PointerButton {
                    pos: ok,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
            );
            assert!(app.error.is_none(), "OK did not dismiss the error dialog");
        }
    }

    #[test]
    fn branch_panel_can_be_resized_by_dragging_divider() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [vec2(1100.0, 700.0), vec2(1536.0, 1024.0)] {
                let ctx = Context::default();
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                let mut rect = Rect::NOTHING;
                let mut frame = |events| {
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            Panel::left("test.sidebar")
                                .exact_size(260.0)
                                .frame(Frame::new().inner_margin(4))
                                .show(ui, |ui| {
                                    rect = sidebar_contents(ui, &mut app);
                                });
                        },
                    );
                    output.textures_delta.clear();
                    rect
                };
                for _ in 0..3 {
                    frame(vec![]);
                }
                let before = frame(vec![]);
                for _ in 0..20 {
                    let idle = frame(vec![]);
                    assert!(
                        (idle.height() - before.height()).abs() < 1.0,
                        "Branch panel shrank without input: {before:?} -> {idle:?}"
                    );
                }
                let pointer = pos2(before.center().x, before.top());
                frame(vec![Event::PointerMoved(pointer)]);
                frame(vec![Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }]);
                let moved = pointer - vec2(0.0, 70.0);
                for _ in 0..3 {
                    frame(vec![Event::PointerMoved(moved)]);
                }
                frame(vec![Event::PointerButton {
                    pos: moved,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }]);
                let after = frame(vec![]);
                for _ in 0..20 {
                    let idle = frame(vec![]);
                    assert!(
                        (idle.height() - after.height()).abs() < 1.0,
                        "Branch panel lost dragged height: {after:?} -> {idle:?}"
                    );
                }
                assert!(
                    after.height() > before.height() + 50.0,
                    "Divider failed to grow {theme:?} {size:?}: {before:?} -> {after:?}"
                );
                let pointer = pos2(after.center().x, after.top());
                frame(vec![Event::PointerMoved(pointer)]);
                frame(vec![Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }]);
                let moved = pos2(pointer.x, after.bottom() - 140.0);
                for _ in 0..3 {
                    frame(vec![Event::PointerMoved(moved)]);
                }
                frame(vec![Event::PointerButton {
                    pos: moved,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }]);
                let shrunk = frame(vec![]);
                assert!(
                    shrunk.height() < 160.0,
                    "Divider failed to shrink: {shrunk:?}"
                );
            }
        }
    }
}
