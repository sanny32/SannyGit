use super::controls::dialog_style::DialogResponse;
use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::{RepositoryJob, SannyGitApp},
    menu::MenuAction,
    theme::semibold_family,
};
use eframe::egui::{Align, CollapsingHeader, Layout, RichText, ScrollArea, Ui};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    super::register_panel(ui, "repositories");
    let palette = app.palette();
    if app.open_dialog {
        let ctx = ui.ctx().clone();
        let busy = app.busy();
        super::controls::dialog_style::modal(&ctx, palette, "repository.add").show(&ctx, |ui| {
            ui.set_width(560.0);
            super::controls::dialog_style::setup(ui);
            ui.spacing_mut().item_spacing.y = 0.0;
            super::controls::dialog_style::header(ui, palette, "Add existing or create new repository",
                "Specify the local Git repository to open. To create a new repository, specify an empty directory.");
            super::controls::dialog_style::section(ui, palette.header, eframe::egui::Margin::same(18), |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Repository:");
                        let width = (ui.available_width() - 36.0).max(100.0);
                        ui.add_sized([width, crate::theme::CONTROL_HEIGHT], crate::ui::controls::singleline(&mut app.repository_path));
                        super::clone_dialog::choose_folder(ui, &mut app.repository_path);
                    });
                });
                ui.horizontal(|ui| {
                    ui.set_height(20.0);
                    if busy {
                        ui.add(eframe::egui::Spinner::new().size(16.0));
                        ui.add(eframe::egui::Label::new(&app.status).truncate().show_tooltip_when_elided(false))
                            .help_text(&app.status);
                    }
                });
            });
            super::controls::dialog_style::section(ui, palette.header, eframe::egui::Margin::symmetric(18, 12), |ui| {
                super::controls::dialog_style::button_row(ui, |ui| {
                    if ui.add_enabled(!busy && !app.repository_path.trim().is_empty(), super::controls::dialog_style::primary("OK", palette)).dialog_accept() {
                        app.run_repository_job(RepositoryJob::AddOrCreate(app.repository_path.clone()));
                    }
                    if ui.add_enabled(!busy, super::controls::dialog_style::button("Cancel")).dialog_cancel() { app.open_dialog = false; }
                });
            });
        });
    }
    let header = ui.allocate_ui_with_layout(
        eframe::egui::vec2(ui.available_width(), super::WORKSPACE_HEADER_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            let title = ui.heading("Repositories");
            super::register_panel_title(ui, "repositories", title.rect);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                icons::unavailable(
                    ui,
                    Icon::Chevron,
                    "Repository management is not implemented yet",
                );
                if icons::button(ui, Icon::Plus, "Open repository").clicked() {
                    app.open_dialog = true;
                }
            });
        },
    );
    super::header_separator(
        ui,
        header.response.rect.expand2(eframe::egui::vec2(8.0, 0.0)),
        palette,
    );
    let badge = app.tracking.badge();
    let tracking_help = app.tracking.description();
    let branch_status = Some((badge.as_str(), tracking_help.as_str()));
    let height = if palette.panel_radius > 0 { 44.0 } else { 24.0 };
    let mut requested = None;
    let mut move_to = None;
    let mut context_action = None;
    let enabled_actions: Vec<_> = [
        MenuAction::NewWindow,
        MenuAction::CloseRepository,
        MenuAction::ReloadHistory,
        MenuAction::Pull,
        MenuAction::Push,
        MenuAction::Fetch,
        MenuAction::Refresh,
        MenuAction::RepositorySettings,
    ]
    .into_iter()
    .filter(|action| app.menu_action_enabled(*action))
    .collect();
    ScrollArea::vertical()
        .id_salt("repositories")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let group_names: Vec<_> = app
                .manager
                .preferences
                .groups
                .iter()
                .map(|g| g.name.clone())
                .collect();
            for group in app.manager.preferences.groups.clone() {
                let response = CollapsingHeader::new(&group.name)
                    .id_salt(("repository.group", &group.name))
                    .default_open(true)
                    .show(ui, |ui| {
                        for (index, repository) in app
                            .repositories
                            .iter()
                            .enumerate()
                            .filter(|(_, r)| group.paths.contains(&r.path))
                        {
                            if let Some(action) = repository_row(
                                ui,
                                index,
                                repository,
                                app.manager
                                    .hosting
                                    .get(&repository.path)
                                    .copied()
                                    .map_or(Icon::Repository, Icon::for_provider),
                                app.selected_repository,
                                branch_status,
                                &mut app.manager.highlighted_repository,
                                palette,
                                &group_names,
                                &mut requested,
                                &mut move_to,
                                height,
                                &enabled_actions,
                            ) {
                                context_action = Some((repository.path.clone(), action));
                            }
                        }
                    });
                if response.header_response.clicked() {
                    app.manager.preferences.selected_group = Some(group.name);
                }
            }
            for (index, repository) in app.repositories.iter().enumerate().filter(|(_, r)| {
                !app.manager
                    .preferences
                    .groups
                    .iter()
                    .any(|g| g.paths.contains(&r.path))
            }) {
                if let Some(action) = repository_row(
                    ui,
                    index,
                    repository,
                    app.manager
                        .hosting
                        .get(&repository.path)
                        .copied()
                        .map_or(Icon::Repository, Icon::for_provider),
                    app.selected_repository,
                    branch_status,
                    &mut app.manager.highlighted_repository,
                    palette,
                    &group_names,
                    &mut requested,
                    &mut move_to,
                    height,
                    &enabled_actions,
                ) {
                    context_action = Some((repository.path.clone(), action));
                }
            }
        });
    if let Some((path, action)) = context_action {
        match action {
            RepositoryAction::Open => app.open_repository(path),
            RepositoryAction::NewWindow => {
                app.run_repository_job(RepositoryJob::NewRepositoryWindow(path))
            }
            RepositoryAction::Reveal => app.run_repository_job(RepositoryJob::Reveal(path)),
            RepositoryAction::CopyPath => ui.ctx().copy_text(path),
            RepositoryAction::Remove => app.request_remove_repository(path),
            RepositoryAction::Commit => ui.ctx().memory_mut(|memory| {
                memory.request_focus(eframe::egui::Id::unique("commit.message"))
            }),
            RepositoryAction::Menu(action) => {
                if app
                    .repositories
                    .get(app.selected_repository)
                    .is_some_and(|repository| repository.path == path)
                {
                    app.handle_menu_action(action, ui.ctx());
                }
            }
        }
    }
    if let Some((path, group)) = move_to {
        app.assign_group(&path, group);
    }
    if let Some(path) = requested {
        app.open_repository(path);
    }
}

#[derive(Clone, Copy)]
enum RepositoryAction {
    Open,
    NewWindow,
    Reveal,
    CopyPath,
    Remove,
    Commit,
    Menu(MenuAction),
}

#[allow(clippy::too_many_arguments)]
fn repository_row(
    ui: &mut Ui,
    index: usize,
    repository: &crate::models::Repository,
    icon: Icon,
    selected: usize,
    branch_status: Option<(&str, &str)>,
    highlighted: &mut Option<String>,
    palette: crate::theme::Palette,
    groups: &[String],
    requested: &mut Option<String>,
    move_to: &mut Option<(String, Option<String>)>,
    height: f32,
    enabled: &[MenuAction],
) -> Option<RepositoryAction> {
    let active = selected == index;
    let selected_row = highlighted
        .as_deref()
        .map_or(active, |path| path == repository.path);
    let response =
        super::controls::selectable_row_with_outset(ui, selected_row, height, 8.0, |ui| {
            icons::paint(
                ui,
                icon,
                if active && branch_status.is_some_and(|(badge, _)| badge.contains(['↑', '↓']))
                {
                    palette.accent
                } else if active {
                    palette.text
                } else {
                    palette.muted
                },
                16.0,
            );
            let name = RichText::new(&repository.name).color(if active {
                palette.text
            } else {
                palette.muted
            });
            let (badge, _) = if active {
                branch_status.unwrap_or(("", ""))
            } else {
                ("", "")
            };
            let suffix = badge.to_owned();
            let suffix_width = ui
                .painter()
                .layout_no_wrap(
                    suffix.clone(),
                    eframe::egui::FontId::proportional(12.0),
                    palette.muted,
                )
                .size()
                .x;
            let width = (ui.available_width() - suffix_width - 4.0).max(20.0);
            ui.allocate_ui_with_layout(
                eframe::egui::vec2(width, if palette.panel_radius > 0 { 38.0 } else { 20.0 }),
                if palette.panel_radius > 0 {
                    Layout::top_down(Align::Min)
                } else {
                    Layout::left_to_right(Align::Center).with_main_align(Align::Min)
                },
                |ui| {
                    ui.set_min_width(width);
                    ui.add(
                        eframe::egui::Label::new(if active {
                            name.family(semibold_family())
                        } else {
                            name
                        })
                        .halign(Align::Min)
                        .truncate()
                        .show_tooltip_when_elided(false)
                        .sense(eframe::egui::Sense::hover()),
                    );
                    if palette.panel_radius > 0 {
                        ui.add(
                            eframe::egui::Label::new(
                                RichText::new(&repository.path).small().color(palette.muted),
                            )
                            .truncate()
                            .show_tooltip_when_elided(false),
                        );
                    }
                },
            );
            if !suffix.is_empty() {
                ui.add(
                    eframe::egui::Label::new(super::sync_badge(&suffix, palette, 12.0))
                        .sense(eframe::egui::Sense::hover()),
                );
            }
        });
    let response = if palette.panel_radius > 0 {
        // Modern rows already display the path below the repository name.
        response
    } else {
        response.help_text(if active {
            format!(
                "{}\n{}",
                repository.path,
                branch_status.map(|(_, help)| help).unwrap_or("")
            )
        } else {
            repository.path.clone()
        })
    };
    let mut action = None;
    response.context_menu(|ui| {
        ui.set_min_width(240.0);
        let idle = enabled.contains(&MenuAction::NewWindow);
        let mut item = |ui: &mut Ui, label: &str, available: bool, selected_action| {
            if ui
                .add_enabled(available, eframe::egui::Button::new(label))
                .clicked()
            {
                action = Some(selected_action);
                ui.close();
            }
        };
        item(
            ui,
            "Open Repository",
            idle && !active,
            RepositoryAction::Open,
        );
        item(
            ui,
            "Open Repository in New Window",
            idle,
            RepositoryAction::NewWindow,
        );
        ui.menu_button("Open", |ui| {
            let label = if cfg!(target_os = "macos") {
                "Reveal in Finder"
            } else if cfg!(target_os = "windows") {
                "Show in Explorer"
            } else {
                "Show in File Manager"
            };
            item(ui, label, idle, RepositoryAction::Reveal);
            ui.add_enabled(false, eframe::egui::Button::new("Open in Terminal"))
                .disabled_help_text("Opening an external terminal is not implemented yet");
        });
        item(
            ui,
            "Close Repository",
            active && enabled.contains(&MenuAction::CloseRepository),
            RepositoryAction::Menu(MenuAction::CloseRepository),
        );
        ui.separator();
        item(ui, "Commit…", active && idle, RepositoryAction::Commit);
        for (label, command) in [
            ("Log", MenuAction::ReloadHistory),
            ("Pull…", MenuAction::Pull),
            ("Push", MenuAction::Push),
            ("Synchronize…", MenuAction::Fetch),
        ] {
            item(
                ui,
                label,
                active && enabled.contains(&command),
                RepositoryAction::Menu(command),
            );
        }
        ui.separator();
        item(
            ui,
            "Refresh",
            active && enabled.contains(&MenuAction::Refresh),
            RepositoryAction::Menu(MenuAction::Refresh),
        );
        ui.separator();
        ui.menu_button("Move To", |ui| {
            if ui.button("Ungrouped").clicked() {
                *move_to = Some((repository.path.clone(), None));
                ui.close();
            }
            for group in groups {
                if ui.button(group).clicked() {
                    *move_to = Some((repository.path.clone(), Some(group.clone())));
                    ui.close();
                }
            }
        });
        for label in ["Mark as Favorite", "Rename…"] {
            ui.add_enabled(false, eframe::egui::Button::new(label))
                .disabled_help_text("Not implemented yet");
        }
        item(ui, "Remove…", idle, RepositoryAction::Remove);
        ui.separator();
        item(ui, "Copy Path", true, RepositoryAction::CopyPath);
        ui.separator();
        item(
            ui,
            "Settings…",
            active && enabled.contains(&MenuAction::RepositorySettings),
            RepositoryAction::Menu(MenuAction::RepositorySettings),
        );
    });
    if response.secondary_clicked() {
        *highlighted = Some(repository.path.clone());
    }
    if response.clicked() {
        *highlighted = Some(repository.path.clone());
    }
    if response.double_clicked() {
        *requested = Some(repository.path.clone());
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

    #[test]
    fn context_menu_targets_the_clicked_repository_without_activating_it() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for active in [false, true] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                theme.apply(&ctx);
                let repository = crate::models::Repository {
                    name: "Repository".into(),
                    path: "/clicked/repository".into(),
                };
                let mut highlighted = None;
                let mut requested = None;
                let mut move_to = None;
                let mut render = |events| {
                    let mut action = None;
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(1100.0, 700.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            ui.set_width(260.0);
                            action = repository_row(
                                ui,
                                0,
                                &repository,
                                Icon::Repository,
                                if active { 0 } else { usize::MAX },
                                None,
                                &mut highlighted,
                                theme.palette(),
                                &[],
                                &mut requested,
                                &mut move_to,
                                24.0,
                                &[MenuAction::NewWindow, MenuAction::Push],
                            );
                        },
                    );
                    output.textures_delta.clear();
                    let labels = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            eframe::egui::Shape::Text(text) => {
                                Some((text.galley.job.text.clone(), text.pos + Vec2::splat(3.0)))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    (action, labels)
                };
                for _ in 0..3 {
                    render(vec![]);
                }
                let pointer = Pos2::new(70.0, 12.0);
                for pressed in [true, false] {
                    render(vec![
                        Event::PointerMoved(pointer),
                        Event::PointerButton {
                            pos: pointer,
                            button: PointerButton::Secondary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ]);
                }
                let (_, mut labels) = render(vec![]);
                for _ in 0..3 {
                    labels = render(vec![]).1;
                }
                for expected in [
                    "Open Repository",
                    "Close Repository",
                    "Commit…",
                    "Pull…",
                    "Push",
                    "Synchronize…",
                    "Refresh",
                    "Copy Path",
                    "Settings…",
                ] {
                    assert!(
                        labels.iter().any(|(label, _)| label == expected),
                        "Missing {expected}"
                    );
                }
                let label = "Push";
                let pointer = labels.iter().find(|(text, _)| text == label).unwrap().1;
                render(vec![Event::PointerMoved(pointer)]);
                render(vec![Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }]);
                let (action, _) = render(vec![Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }]);
                assert!(if active {
                    matches!(action, Some(RepositoryAction::Menu(MenuAction::Push)))
                } else {
                    action.is_none()
                });
                assert_eq!(highlighted.as_deref(), Some(repository.path.as_str()));
                assert!(requested.is_none());
            }
        }
    }

    #[test]
    fn name_stays_next_to_icon_with_tracking_counts() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for width in [190.0, 260.0, 350.0] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                theme.apply(&ctx);
                let repository = crate::models::Repository {
                    name: "SannyGit".into(),
                    path: "/test/repository".into(),
                };
                let mut highlighted = None;
                let mut requested = None;
                let mut move_to = None;
                for _ in 0..3 {
                    let mut left = 0.0;
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(width, 200.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            left = ui.next_widget_position().x;
                            repository_row(
                                ui,
                                0,
                                &repository,
                                Icon::Repository,
                                0,
                                Some(("↑1 ↓1", "Upstream: origin/main")),
                                &mut highlighted,
                                theme.palette(),
                                &[],
                                &mut requested,
                                &mut move_to,
                                24.0,
                                &[],
                            );
                        },
                    );
                    let text = output
                        .shapes
                        .iter()
                        .filter_map(|s| {
                            if let eframe::egui::Shape::Text(t) = &s.shape {
                                Some(t)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    let name = text
                        .iter()
                        .find(|t| t.galley.job.text == "SannyGit")
                        .unwrap();
                    assert!(
                        name.pos.x < left + 35.0,
                        "Name detached from icon: {:?} at width {width}",
                        name.pos
                    );
                    assert!(text.iter().any(|t| t.galley.job.text == "↑1 ↓1"));
                    assert!(!text.iter().any(|t| t.galley.job.text.contains("(main)")));
                    output.textures_delta.clear();
                }
            }
        }
    }

    #[test]
    fn single_click_selects_and_double_click_requests_activation() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            let ctx = Context::default();
            crate::theme::install_fonts(&ctx);
            theme.apply(&ctx);
            let repository = crate::models::Repository {
                name: "Repository".into(),
                path: "/test/repository".into(),
            };
            let mut highlighted = None;
            let mut requested = None;
            let mut move_to = None;
            let mut pointer = Pos2::ZERO;
            for (frame, pressed) in [None, None, Some(true), Some(false), Some(true), Some(false)]
                .into_iter()
                .enumerate()
            {
                let mut events = vec![Event::PointerMoved(pointer)];
                if let Some(pressed) = pressed {
                    events.push(Event::PointerButton {
                        pos: pointer,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0))),
                        time: Some(frame as f64 * 0.05),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        pointer = ui.next_widget_position() + Vec2::new(70.0, 12.0);
                        repository_row(
                            ui,
                            0,
                            &repository,
                            Icon::Repository,
                            usize::MAX,
                            None,
                            &mut highlighted,
                            theme.palette(),
                            &[],
                            &mut requested,
                            &mut move_to,
                            24.0,
                            &[],
                        );
                    },
                );
                output.textures_delta.clear();
                if frame == 3 {
                    assert_eq!(highlighted.as_deref(), Some(repository.path.as_str()));
                    assert!(requested.is_none());
                }
            }
            assert_eq!(requested.as_deref(), Some(repository.path.as_str()));
        }
    }
}
