use self::controls::dialog_style::DialogResponse;
#[cfg(all(target_os = "macos", not(test)))]
pub(crate) use controls::popup_menu::Entry as PopupMenuEntry;
mod about;
mod branches;
mod changes;
mod clone_dialog;
mod commit_panel;
mod controls;
mod diff_view;
mod discard;
mod journal;
mod merge;
mod ref_management;
mod remote_dialog;
mod repositories;
mod repository_dialogs;
mod settings;
mod stash;
mod status_bar;
mod toolbar;
mod workspace;

use controls::dialog_style;

use crate::{app::SannyGitApp, theme::Palette};
use eframe::egui::{
    Align, CentralPanel, Frame, InnerResponse, Layout, Margin, Panel, Sense, Ui, UiBuilder, Vec2,
};

const PANEL_GAP: i8 = 2;
const WORKSPACE_HEADER_HEIGHT: f32 = 34.0;

fn sync_badge(text: &str, palette: Palette) -> eframe::egui::text::LayoutJob {
    use eframe::egui::{text::LayoutJob, FontId, TextFormat};
    let mut job = LayoutJob::default();
    for (index, part) in text.split_whitespace().enumerate() {
        if index > 0 {
            job.append(
                " ",
                0.0,
                TextFormat {
                    font_id: FontId::proportional(crate::theme::ui_font_size()),
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
                font_id: FontId::proportional(crate::theme::ui_font_size()),
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
    ui.painter().rect_filled(
        rect,
        eframe::egui::CornerRadius {
            nw: palette.panel_radius,
            ne: palette.panel_radius,
            sw: 0,
            se: 0,
        },
        palette.header,
    );
    header_separator(ui, rect, palette);
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(8.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    InnerResponse::new(contents(&mut child), response)
}

fn header_separator(ui: &Ui, rect: eframe::egui::Rect, palette: Palette) {
    ui.painter().line_segment(
        [
            rect.left_bottom() - Vec2::new(0.0, 0.5),
            rect.right_bottom() - Vec2::new(0.0, 0.5),
        ],
        (1.0, palette.border),
    );
}

fn panel_frame(palette: Palette) -> Frame {
    Frame::new()
        .fill(palette.panel)
        .stroke((1.0, palette.border))
        .corner_radius(palette.panel_radius)
}

fn sidebar_contents(ui: &mut Ui, app: &mut SannyGitApp) -> eframe::egui::Rect {
    let palette = app.palette();
    let branches = Panel::bottom("working_tree.v3.branches")
        .resizable(true)
        .show_separator_line(false)
        .default_size(
            ui.available_height()
                * if app.interface_style == crate::theme::InterfaceStyle::Modern {
                    0.65
                } else {
                    0.36
                },
        )
        .size_range(130.0..=(ui.available_height() - 120.0).max(130.0))
        .frame(
            panel_frame(palette)
                .inner_margin(Margin {
                    top: 0,
                    ..Margin::same(8)
                })
                .outer_margin(Margin {
                    top: PANEL_GAP,
                    ..Margin::ZERO
                }),
        )
        .show(ui, |ui| {
            // Persist the allocated height, not the smaller content height.
            ui.take_available_space();
            branches::show(ui, app);
        });
    CentralPanel::default()
        .frame(panel_frame(palette).inner_margin(Margin {
            top: 0,
            ..Margin::same(8)
        }))
        .show(ui, |ui| repositories::show(ui, app));
    branches.response.rect
}

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    if app.modal_dialog_open() {
        ui.disable();
    }
    // Keep dense panel rows compact without truncating floating tooltips.
    ui.style_mut().wrap_mode = Some(eframe::egui::TextWrapMode::Truncate);
    let context = ui.ctx().clone();
    let palette = app.palette();
    toolbar::show(ui, app);
    settings::show(ui, app);
    repository_dialogs::show(ui, app);
    stash::show(ui.ctx(), app);
    discard::show(ui.ctx(), app);
    ref_management::show(ui.ctx(), app);
    merge::show(ui.ctx(), app);
    welcome(ui, app);
    about::show(ui.ctx(), app);
    status_bar::show(ui, app);

    // Outer margins are transparent. Paint the complete workspace first so
    // resize gutters never expose the native window's clear color.
    ui.painter()
        .rect_filled(ui.available_rect_before_wrap(), 0.0, palette.background);

    Panel::left("working_tree.v3.sidebar")
        .resizable(true)
        .show_separator_line(false)
        .default_size((context.content_rect().width() * 0.17).clamp(200.0, 270.0))
        .size_range(190.0..=(ui.available_width() - 740.0).max(190.0))
        .frame(
            Frame::new()
                .fill(palette.background)
                .inner_margin(Margin {
                    left: PANEL_GAP,
                    right: 0,
                    top: PANEL_GAP,
                    bottom: PANEL_GAP,
                })
                .outer_margin(Margin {
                    right: PANEL_GAP,
                    ..Margin::ZERO
                }),
        )
        .show(ui, |ui| {
            ui.take_available_space();
            sidebar_contents(ui, app);
        });

    if app.interface_style == crate::theme::InterfaceStyle::Modern {
        modern_workspace(ui, app);
        if !app.settings_window.open
            && (context.embed_viewports() || (!app.commit_details_open && !app.about_dialog))
        {
            show_errors(ui, app);
        }
        return;
    }

    CentralPanel::default()
        .frame(Frame::new().fill(palette.background).inner_margin(Margin {
            left: 0,
            right: PANEL_GAP,
            top: PANEL_GAP,
            bottom: PANEL_GAP,
        }))
        .show(ui, |ui| {
            Panel::bottom("working_tree.v3.journal")
                .resizable(true)
                .show_separator_line(false)
                .default_size(180.0)
                .size_range(150.0..=(ui.available_height() - 380.0).max(150.0))
                .frame(panel_frame(palette))
                .show(ui, |ui| {
                    ui.take_available_space();
                    journal::show(ui, app);
                });
            Panel::top("working_tree.v3.files")
                .resizable(true)
                .show_separator_line(false)
                .default_size((context.content_rect().height() * 0.38).max(300.0))
                .size_range(180.0..=(ui.available_height() - 160.0).max(180.0))
                .frame(Frame::new().fill(palette.background).inner_margin(Margin {
                    bottom: PANEL_GAP,
                    ..Margin::ZERO
                }))
                .show(ui, |ui| {
                    ui.take_available_space();
                    Panel::right("working_tree.v3.commit")
                        .resizable(true)
                        .show_separator_line(false)
                        .default_size(ui.available_width() * 0.29)
                        .size_range(260.0..=(ui.available_width() - 310.0).max(260.0))
                        .frame(panel_frame(palette).outer_margin(Margin {
                            left: PANEL_GAP,
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
                    bottom: PANEL_GAP,
                    ..Margin::ZERO
                }))
                .show(ui, |ui| diff_view::show(ui, app));
        });
    if !app.settings_window.open
        && (context.embed_viewports() || (!app.commit_details_open && !app.about_dialog))
    {
        show_errors(ui, app);
    }
}

fn welcome(ui: &mut Ui, app: &mut SannyGitApp) {
    use crate::app::WelcomeAction;
    use eframe::egui::{Button, RichText, ScrollArea};

    if !app.welcome_open {
        return;
    }
    if app.repositories.is_empty() && app.welcome_action == WelcomeAction::Reopen {
        app.welcome_action = WelcomeAction::Add;
    }
    if !app
        .repositories
        .iter()
        .any(|repository| app.welcome_repository.as_deref() == Some(repository.path.as_str()))
    {
        app.welcome_repository = app
            .repositories
            .first()
            .map(|repository| repository.path.clone());
    }
    let palette = app.palette();
    let busy = app.busy();
    let mut accept = false;
    dialog_style::modal(ui.ctx(), palette, "application.welcome").show(ui.ctx(), |ui| {
        ui.set_width(640.0);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header(
            ui,
            palette,
            "What do you want to do?",
            "Select whether to open or create a local repository, clone a remote repository, or reopen a previously used repository.",
        );
        dialog_style::section(ui, palette.header, Margin::same(18), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.add_enabled_ui(!busy, |ui| {
                ui.radio_value(
                    &mut app.welcome_action,
                    WelcomeAction::Add,
                    "Add an existing local or create a new repository",
                );
                ui.radio_value(
                    &mut app.welcome_action,
                    WelcomeAction::Clone,
                    "Clone existing repository",
                );
                ui.add_enabled_ui(!app.repositories.is_empty(), |ui| {
                    ui.radio_value(
                        &mut app.welcome_action,
                        WelcomeAction::Reopen,
                        "Reopen previously used repository:",
                    );
                })
                .response
                .on_disabled_hover_text("No previously used repositories. Open or clone a repository first.");
                ui.visuals_mut().indent_has_left_vline = false;
                ui.indent("welcome.repositories.indent", |ui| {
                    Frame::new()
                        .fill(palette.editor)
                        .stroke((1.0, palette.border))
                        .inner_margin(2)
                        .show(ui, |ui| {
                            ScrollArea::vertical()
                                .id_salt("welcome.repositories")
                                .max_height(176.0)
                                .min_scrolled_height(176.0)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    if app.repositories.is_empty() {
                                        ui.label(RichText::new("No previously used repositories").color(palette.muted));
                                    }
                                    for repository in &app.repositories {
                                        ui.push_id(&repository.path, |ui| {
                                            let selected = app.welcome_action == WelcomeAction::Reopen
                                                && app.welcome_repository.as_deref() == Some(repository.path.as_str());
                                            let response = ui.add_sized(
                                                [ui.available_width(), 24.0],
                                                Button::selectable(selected, (
                                                    RichText::new(&repository.name),
                                                    RichText::new(format!("({})", repository.path)).color(palette.muted),
                                                    eframe::egui::Atom::grow(),
                                                )).gap(5.0).truncate(),
                                            ).on_hover_text(&repository.path);
                                            if response.clicked() {
                                                app.welcome_action = WelcomeAction::Reopen;
                                                app.welcome_repository = Some(repository.path.clone());
                                            }
                                            accept |= response.double_clicked();
                                        });
                                    }
                                });
                        });
                });
            });
        });
        dialog_style::section(ui, palette.header, Margin::symmetric(18, 12), |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut app.preferences.show_welcome,
                    "Show this dialog if no repository was opened",
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let can_accept = !busy && (app.welcome_action != WelcomeAction::Reopen
                        || app.welcome_repository.is_some());
                    accept |= ui.add_enabled(can_accept, dialog_style::primary("OK", palette)).dialog_accept();
                    if ui.add(dialog_style::button("Close")).dialog_cancel() {
                        app.welcome_open = false;
                    }
                });
            });
        });
    });
    if accept && !busy {
        app.welcome_open = false;
        match app.welcome_action {
            WelcomeAction::Add => app.open_dialog = true,
            WelcomeAction::Clone => {
                app.handle_menu_action(crate::menu::MenuAction::Clone, ui.ctx())
            }
            WelcomeAction::Reopen => {
                if let Some(path) = app.welcome_repository.clone() {
                    app.open_repository(path);
                }
            }
        }
    }
}

// Separate panel IDs preserve each style's user-adjusted split positions.
fn modern_workspace(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    CentralPanel::default()
        .frame(Frame::new().fill(palette.background).inner_margin(Margin {
            left: 0,
            right: PANEL_GAP,
            top: PANEL_GAP,
            bottom: PANEL_GAP,
        }))
        .show(ui, |ui| {
            workspace::show(ui, app);
        });
}

/// Show errors above every other dialog in the active viewport.
fn show_errors(ui: &mut Ui, app: &mut SannyGitApp) {
    app.collect_errors();
    let palette = app.palette();
    if let Some(error) = app.error_dialogs.pending.front().cloned() {
        let mut dismiss = false;
        dialog_style::modal(ui.ctx(), palette, "application.error").show(ui.ctx(), |ui| {
            ui.set_width(460.0);
            dialog_style::setup(ui);
            ui.spacing_mut().item_spacing.y = 0.0;
            dialog_style::header_with_icon(
                ui,
                palette,
                "Attention required",
                "The operation could not be completed.",
                controls::icons::Icon::Warning,
            );
            dialog_style::section(ui, palette.panel, Margin::same(18), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                eframe::egui::ScrollArea::vertical()
                    .id_salt("application.error.message")
                    .max_height(240.0)
                    .show(ui, |ui| {
                        ui.add(eframe::egui::Label::new(error).wrap().selectable(true));
                    });
            });
            dialog_style::section(ui, palette.header, Margin::symmetric(18, 12), |ui| {
                dialog_style::button_row(ui, |ui| {
                    if ui.add(dialog_style::primary("OK", palette)).dialog_close() {
                        dismiss = true;
                    }
                });
            });
        });
        if dismiss {
            app.dismiss_error();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeChoice;
    use eframe::egui::{pos2, vec2, Context, Event, Modifiers, PointerButton, RawInput, Rect};

    #[test]
    fn welcome_fits_supported_sizes_and_keeps_geometry_when_selecting_actions() {
        use crate::app::WelcomeAction;
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [vec2(1100.0, 700.0), vec2(1536.0, 1024.0)] {
                for zoom in [1.0, 1.5] {
                    let ctx = Context::default();
                    crate::theme::install_fonts(&ctx);
                    egui_extras::install_image_loaders(&ctx);
                    theme.apply(&ctx);
                    ctx.set_zoom_factor(zoom);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.welcome_open = true;
                    app.repositories = crate::mock_data::repositories();
                    let mut initial = None;
                    for action in [
                        WelcomeAction::Reopen,
                        WelcomeAction::Add,
                        WelcomeAction::Clone,
                    ] {
                        app.welcome_action = action;
                        for _ in 0..3 {
                            let mut output = ctx.run_ui(
                                RawInput {
                                    screen_rect: Some(Rect::from_min_size(
                                        eframe::egui::Pos2::ZERO,
                                        size,
                                    )),
                                    ..Default::default()
                                },
                                |ui| welcome(ui, &mut app),
                            );
                            output.textures_delta.clear();
                        }
                        let rect = ctx
                            .memory(|memory| {
                                memory.area_rect(eframe::egui::Id::unique("application.welcome"))
                            })
                            .unwrap();
                        assert!(
                            ctx.content_rect().contains_rect(rect),
                            "{theme:?} {size:?} {zoom}: {rect:?}"
                        );
                        if let Some(initial) = initial {
                            assert_eq!(
                                rect, initial,
                                "Selecting an action resized the welcome dialog"
                            );
                        }
                        initial = Some(rect);
                    }
                }
            }
        }
    }

    #[test]
    fn clicking_workspace_titles_does_not_add_an_accent_marker() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [
                crate::theme::InterfaceStyle::Classic,
                crate::theme::InterfaceStyle::Modern,
            ] {
                for size in [vec2(1100.0, 700.0), vec2(1536.0, 1024.0)] {
                    let ctx = Context::default();
                    crate::theme::install_fonts(&ctx);
                    egui_extras::install_image_loaders(&ctx);
                    theme.apply_style(&ctx, style);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.interface_style = style;
                    let mut render = |events| {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                assert!(!ui.style().interaction.selectable_labels);
                                show(ui, &mut app);
                            },
                        );
                        output.textures_delta.clear();
                        output
                    };
                    let _ = render(vec![]);
                    let output = render(vec![]);
                    for title in [
                        "Files",
                        "Changes",
                        "Repositories",
                        "Branches",
                        "Commit",
                        "Journal",
                    ] {
                        let pointer = output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                eframe::egui::Shape::Text(text)
                                    if text.galley.job.text == title =>
                                {
                                    Some(text.pos + text.galley.size() / 2.0)
                                }
                                _ => None,
                            })
                            .unwrap_or_else(|| {
                                panic!("Missing {title} in {theme:?} {style:?} {size:?}")
                            });
                        for pressed in [true, false] {
                            let output = render(vec![
                                Event::PointerMoved(pointer),
                                Event::PointerButton {
                                    pos: pointer,
                                    button: PointerButton::Primary,
                                    pressed,
                                    modifiers: Modifiers::NONE,
                                },
                            ]);
                            assert!(!output.shapes.iter().any(|shape| {
                                matches!(&shape.shape, eframe::egui::Shape::Rect(rect)
                                    if rect.fill == style.palette(theme).accent && rect.rect.height() == 3.0)
                            }), "Unexpected accent marker on {title} in {theme:?} {style:?} {size:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn sidebar_resize_highlight_is_visible_in_the_vertical_gutter() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [vec2(1100.0, 700.0), vec2(1536.0, 1024.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                theme.apply_style(&ctx, crate::theme::InterfaceStyle::Modern);
                let mut app = SannyGitApp::with_theme(theme);
                app.interface_style = crate::theme::InterfaceStyle::Modern;
                let render = |app: &mut SannyGitApp, events| {
                    let mut sidebar_id = None;
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            sidebar_id =
                                Some(ui.scope_id().with_salt(eframe::egui::IdSalt::new(
                                    "working_tree.v3.sidebar",
                                )));
                            show(ui, app);
                        },
                    );
                    output.textures_delta.clear();
                    // The branches panel can have the same approximate bounds;
                    // select the actual sidebar rather than HashMap iteration order.
                    let sidebar = eframe::egui::PanelState::load(&ctx, sidebar_id.unwrap())
                        .unwrap()
                        .outer_rect;
                    (sidebar, output)
                };
                for _ in 0..3 {
                    render(&mut app, vec![]);
                }
                let (before, idle) = render(&mut app, vec![]);
                let has_highlight = |output: &eframe::egui::FullOutput, rect: Rect| {
                    output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, eframe::egui::Shape::LineSegment { points, .. }
                            if points[0].x == points[1].x
                                && points[0].x > rect.right() - f32::from(PANEL_GAP)
                                && points[0].x < rect.right()
                                && points[0].y == rect.top()
                                && points[1].y == rect.bottom())
                    })
                };
                // The gutter must have the theme background even with no hover
                // highlight; transparent outer margins otherwise expose black.
                let gutter = pos2(
                    before.right() - f32::from(PANEL_GAP) / 2.0,
                    before.center().y,
                );
                let background = idle.shapes.iter().rev().find_map(|shape| {
                    if !shape.clip_rect.contains(gutter) {
                        return None;
                    }
                    match &shape.shape {
                        eframe::egui::Shape::Rect(rect)
                            if rect.rect.contains(gutter) && rect.fill.is_opaque() =>
                        {
                            Some(rect.fill)
                        }
                        _ => None,
                    }
                });
                assert_eq!(
                    background,
                    Some(app.palette().background),
                    "Unpainted sidebar gutter {theme:?} {size:?}"
                );
                assert!(!has_highlight(&idle, before));
                let pointer = pos2(
                    before.right() - f32::from(PANEL_GAP) / 2.0,
                    before.center().y,
                );
                render(&mut app, vec![Event::PointerMoved(pointer)]);
                let (hovered, output) = render(&mut app, vec![Event::PointerMoved(pointer)]);
                assert_eq!(before, hovered);
                assert!(
                    has_highlight(&output, hovered),
                    "Missing sidebar highlight {theme:?} {size:?}"
                );
                render(
                    &mut app,
                    vec![Event::PointerButton {
                        pos: pointer,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    }],
                );
                let moved = pointer + vec2(30.0, 0.0);
                for _ in 0..3 {
                    render(&mut app, vec![Event::PointerMoved(moved)]);
                }
                let (dragged, output) = render(
                    &mut app,
                    vec![Event::PointerButton {
                        pos: moved,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    }],
                );
                assert!(
                    dragged.width() > before.width() + 25.0,
                    "Sidebar did not resize {theme:?} {size:?}: {before:?} -> {dragged:?}"
                );
                assert!(has_highlight(&output, dragged));
            }
        }
    }

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
                        .any(|r| r.max == journal.max && r.height() > journal.height() + 10.0),
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
    fn resized_panels_restore_after_a_restart() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let size = vec2(1536.0, 1024.0);
            let screen = Rect::from_min_size(pos2(0.0, 0.0), size);
            let render = |ctx: &Context, app: &mut SannyGitApp, events: Vec<Event>| {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(screen),
                        events,
                        ..Default::default()
                    },
                    |ui| show(ui, app),
                );
                output.textures_delta.clear();
            };
            let panel_rects = |ctx: &Context| {
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
            let mut app = SannyGitApp::with_theme(theme);
            app.repositories = crate::mock_data::repositories();
            app.selected_repository = 0;
            app.changes = crate::mock_data::changes();
            app.commits = crate::mock_data::commits();
            let ctx = Context::default();
            crate::theme::install_fonts(&ctx);
            egui_extras::install_image_loaders(&ctx);
            theme.apply(&ctx);
            for _ in 0..3 {
                render(&ctx, &mut app, vec![]);
            }
            let before = panel_rects(&ctx);
            let sidebar = before
                .iter()
                .copied()
                .filter(|rect| rect.left() < 5.0 && rect.height() > size.y * 0.5)
                .max_by(|a, b| a.height().total_cmp(&b.height()))
                .expect("Sidebar panel is missing");
            let pointer = pos2(sidebar.right() - 1.0, sidebar.center().y);
            render(&ctx, &mut app, vec![Event::PointerMoved(pointer)]);
            render(
                &ctx,
                &mut app,
                vec![Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }],
            );
            let moved = pointer + vec2(60.0, 0.0);
            for _ in 0..3 {
                render(&ctx, &mut app, vec![Event::PointerMoved(moved)]);
            }
            render(
                &ctx,
                &mut app,
                vec![Event::PointerButton {
                    pos: moved,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
            );
            render(&ctx, &mut app, vec![]);
            let resized = panel_rects(&ctx);
            assert!(
                resized
                    .iter()
                    .any(|rect| rect.left() < 5.0 && rect.width() > sidebar.width() + 40.0),
                "Sidebar did not resize in {theme:?}"
            );
            // Exit: eframe serializes the egui memory (which holds the panel
            // sizes) into the app storage. Restart: a fresh context begins
            // from that saved memory, like the native runner does.
            let restored_memory = crate::test_support::restart_memory(&ctx);
            let restored_ctx = Context::default();
            crate::theme::install_fonts(&restored_ctx);
            egui_extras::install_image_loaders(&restored_ctx);
            restored_ctx.memory_mut(|memory| *memory = restored_memory);
            theme.apply(&restored_ctx);
            let mut restored_app = SannyGitApp::with_theme(theme);
            restored_app.repositories = crate::mock_data::repositories();
            restored_app.selected_repository = 0;
            restored_app.changes = crate::mock_data::changes();
            restored_app.commits = crate::mock_data::commits();
            for _ in 0..3 {
                render(&restored_ctx, &mut restored_app, vec![]);
            }
            let restored = panel_rects(&restored_ctx);
            assert_eq!(
                resized.len(),
                restored.len(),
                "Panel set changed after restart in {theme:?}"
            );
            for (expected, actual) in resized.iter().zip(&restored) {
                assert!(
                    (expected.min - actual.min).length() < 0.5
                        && (expected.max - actual.max).length() < 0.5,
                    "Panel size was not restored in {theme:?}: {expected:?} -> {actual:?}"
                );
            }
        }
    }

    #[test]
    fn error_dialog_does_not_move_panels_and_can_be_dismissed() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [
                crate::theme::InterfaceStyle::Classic,
                crate::theme::InterfaceStyle::Modern,
            ] {
                for size in [vec2(1100.0, 700.0), vec2(1536.0, 1024.0)] {
                    let ctx = Context::default();
                    crate::theme::install_fonts(&ctx);
                    theme.apply_style(&ctx, style);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.interface_style = style;
                    let render = |app: &mut SannyGitApp, events| {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
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
                    let error = concat!(
                        "LLM API returned HTTP 404. ",
                        "Check the endpoint, model, API key and provider limits."
                    );
                    app.error = Some(error.into());
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
                    for label in ["Attention required", error, "OK"] {
                        assert!(
                            output.shapes.iter().any(|shape| {
                                matches!(&shape.shape, eframe::egui::Shape::Text(text)
                                if text.galley.job.text == label)
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
                    let (_, output) = render(&mut app, vec![]);
                    assert!(!output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, eframe::egui::Shape::Text(text)
                    if text.galley.job.text == "Attention required")
                    }));
                    app.commit_message = "Keep this draft".into();
                    for diagnostic in [
                    "fatal: unable to access 'https://alice:hunter2@example.invalid/repo?auth=abc123': connection refused".to_owned(),
                    format!("{}\nAPI-Key: unlabelled-value", "ordinary text ".repeat(5_000)),
                ] {
                    let error = crate::git::diagnostics::CommandError::exited(
                        "status", Some(128), diagnostic.as_bytes(),
                    ).to_string();
                    app.error = Some(error.clone());
                    for _ in 0..3 {
                        render(&mut app, vec![]);
                    }
                    let (panels, output) = render(&mut app, vec![]);
                    assert_eq!(panels, before);
                    assert_eq!(app.commit_message, "Keep this draft");
                    let shown = output.shapes.iter().filter_map(|shape| match &shape.shape {
                        eframe::egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                        _ => None,
                    }).collect::<Vec<_>>();
                    assert!(shown.contains(&"Attention required"));
                    assert!(shown.contains(&error.as_str()));
                    assert!(error.contains("exit code: 128"));
                    for text in shown {
                        for hidden in ["alice", "hunter2", "abc123", "unlabelled-value"] {
                            assert!(!text.contains(hidden));
                        }
                    }
                    app.dismiss_error();
                    render(&mut app, vec![]);
                }
                }
            }
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
