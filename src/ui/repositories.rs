use super::controls::dialog_style::DialogResponse;
use super::controls::icons::{self, Icon};
use super::controls::popup_menu;
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::{RepositoryJob, SannyGitApp},
    menu::MenuAction,
    theme::semibold_family,
};
use eframe::egui::{Align, CollapsingHeader, Layout, RichText, ScrollArea, TextStyle, Ui};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
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
            ui.heading("Repositories");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if app.busy() {
                    icons::unavailable(
                        ui,
                        Icon::Chevron,
                        "Wait for the current operation to finish",
                    );
                } else {
                    let groups: Vec<_> = app
                        .manager
                        .preferences
                        .groups
                        .iter()
                        .map(|group| group.name.clone())
                        .collect();
                    if let Some(action) =
                        popup_menu::button(ui, Icon::Chevron, "Manage repository groups", || {
                            use popup_menu::Entry as E;
                            let mut entries = vec![E::item("Add Group…", true, GroupAction::Add)];
                            if !groups.is_empty() {
                                entries.push(E::Separator);
                            }
                            for group in &groups {
                                entries.push(E::submenu(
                                    group.clone(),
                                    vec![
                                        E::item(
                                            "Rename…",
                                            true,
                                            GroupAction::Rename(group.clone()),
                                        ),
                                        E::item(
                                            "Delete…",
                                            true,
                                            GroupAction::Delete(group.clone()),
                                        ),
                                    ],
                                ));
                            }
                            entries
                        })
                    {
                        match action {
                            GroupAction::Add => app.open_add_group(),
                            GroupAction::Rename(name) => app.open_rename_group(name),
                            GroupAction::Delete(name) => app.open_delete_group(name),
                        }
                    }
                }
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
    let conflicted = app
        .changes
        .iter()
        .any(|change| change.kind == crate::models::ChangeKind::Conflicted);
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
                                app.merging,
                                conflicted,
                                branch_status,
                                &mut app.manager.highlighted_repository,
                                palette,
                                &group_names,
                                &app.manager.preferences,
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
                    app.merging,
                    conflicted,
                    branch_status,
                    &mut app.manager.highlighted_repository,
                    palette,
                    &group_names,
                    &app.manager.preferences,
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
            RepositoryAction::MoveTo(group) => app.assign_group(&path, group),
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
            RepositoryAction::Favorite => app.toggle_favorite(&path),
            RepositoryAction::Rename => app.open_rename_repository(path),
        }
    }
    if let Some((path, group)) = move_to {
        app.assign_group(&path, group);
    }
    if let Some(path) = requested {
        app.open_repository(path);
    }
}

#[derive(Clone)]
enum GroupAction {
    Add,
    Rename(String),
    Delete(String),
}

#[derive(Clone)]
enum RepositoryAction {
    Open,
    NewWindow,
    Reveal,
    CopyPath,
    Remove,
    Commit,
    Menu(MenuAction),
    MoveTo(Option<String>),
    Favorite,
    Rename,
}

#[allow(clippy::too_many_arguments)]
fn repository_row(
    ui: &mut Ui,
    index: usize,
    repository: &crate::models::Repository,
    icon: Icon,
    selected: usize,
    merging: bool,
    conflicted: bool,
    branch_status: Option<(&str, &str)>,
    highlighted: &mut Option<String>,
    palette: crate::theme::Palette,
    groups: &[String],
    preferences: &crate::repository::Preferences,
    requested: &mut Option<String>,
    move_to: &mut Option<(String, Option<String>)>,
    height: f32,
    enabled: &[MenuAction],
) -> Option<RepositoryAction> {
    let active = selected == index;
    let favorite = preferences
        .favorites
        .iter()
        .any(|path| path == &repository.path);
    let label = preferences
        .names
        .get(&repository.path)
        .map(String::as_str)
        .unwrap_or(repository.name.as_str());
    let modern = palette.panel_radius > 0;
    let selected_row = highlighted
        .as_deref()
        .map_or(active, |path| path == repository.path);
    let response =
        super::controls::selectable_row_with_outset(ui, selected_row, height, 8.0, |ui| {
            let icon_response = ui.add(icons::image(
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
            ));
            if active && conflicted {
                let badge = eframe::egui::Rect::from_min_size(
                    icon_response.rect.right_bottom() - eframe::egui::Vec2::splat(10.0),
                    eframe::egui::Vec2::splat(10.0),
                );
                ui.painter().rect_filled(badge, 2.0, palette.removed_text);
                ui.painter().text(
                    badge.center(),
                    eframe::egui::Align2::CENTER_CENTER,
                    "!",
                    eframe::egui::FontId::new(9.0, semibold_family()),
                    palette.editor,
                );
                icon_response.help_text("Unresolved conflicts");
            }
            let name = RichText::new(label).color(if active || modern {
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
                    eframe::egui::FontId::proportional(crate::theme::ui_font_size()),
                    palette.muted,
                )
                .size()
                .x;
            let width = (ui.available_width() - suffix_width - 4.0).max(20.0);
            let name_height = ui.text_style_height(&TextStyle::Body);
            let mut path_font = TextStyle::Body.resolve(ui.style());
            path_font.size -= 2.0;
            let text_height = if modern {
                name_height
                    + ui.spacing().item_spacing.y
                    + ui.fonts_mut(|f| f.row_height(&path_font))
            } else {
                20.0
            };
            ui.allocate_ui_with_layout(
                eframe::egui::vec2(width, text_height),
                if modern {
                    Layout::top_down(Align::Min)
                } else {
                    Layout::left_to_right(Align::Center).with_main_align(Align::Min)
                },
                |ui| {
                    ui.set_min_width(width);
                    let name = eframe::egui::Label::new(if active {
                        name.clone().family(semibold_family())
                    } else {
                        name.clone()
                    })
                    .halign(Align::Min)
                    .truncate()
                    .show_tooltip_when_elided(false)
                    .sense(eframe::egui::Sense::hover());
                    if active && merging {
                        let marker = ui.painter().layout_no_wrap(
                            "<merging>".into(),
                            eframe::egui::FontId::proportional(crate::theme::ui_font_size()),
                            palette.outgoing,
                        );
                        let height = if modern {
                            name_height
                        } else {
                            ui.available_height()
                        };
                        ui.allocate_ui_with_layout(
                            eframe::egui::vec2(ui.available_width(), height),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                let name_width = ui.available_width()
                                    - marker.size().x
                                    - ui.spacing().item_spacing.x;
                                ui.scope(|ui| {
                                    ui.set_max_width(name_width.max(0.0));
                                    ui.add(name);
                                });
                                ui.add(eframe::egui::Label::new(marker))
                                    .help_text("Merge in progress");
                            },
                        );
                    } else if favorite {
                        ui.horizontal(|ui| {
                            ui.add(name);
                            ui.label(RichText::new("★").color(palette.warning));
                        });
                    } else {
                        ui.add(name);
                    }
                    if modern {
                        ui.add(
                            eframe::egui::Label::new(
                                RichText::new(&repository.path)
                                    .font(path_font)
                                    .color(palette.muted),
                            )
                            .truncate()
                            .show_tooltip_when_elided(false),
                        );
                    }
                },
            );
            if !suffix.is_empty() {
                ui.add(
                    eframe::egui::Label::new(super::sync_badge(&suffix, palette))
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
    let mut action = popup_menu::context(&response, palette, || {
        use popup_menu::Entry as E;
        let idle = enabled.contains(&MenuAction::NewWindow);
        let item = |label, available, action| E::item(label, available, Some(action));
        let reveal = if cfg!(target_os = "macos") {
            "Reveal in Finder"
        } else if cfg!(target_os = "windows") {
            "Show in Explorer"
        } else {
            "Show in File Manager"
        };
        let mut entries = vec![
            item("Open Repository", idle && !active, RepositoryAction::Open),
            item(
                "Open Repository in New Window",
                idle,
                RepositoryAction::NewWindow,
            ),
            E::submenu(
                "Open",
                vec![
                    item(reveal, idle, RepositoryAction::Reveal),
                    E::item("Open in Terminal", false, None)
                        .help("Opening an external terminal is not implemented yet"),
                ],
            ),
            item(
                "Close Repository",
                active && enabled.contains(&MenuAction::CloseRepository),
                RepositoryAction::Menu(MenuAction::CloseRepository),
            ),
            E::Separator,
            item("Commit…", active && idle, RepositoryAction::Commit),
        ];
        for (label, command) in [
            ("Log", MenuAction::ReloadHistory),
            ("Pull…", MenuAction::Pull),
            ("Push", MenuAction::Push),
            ("Synchronize…", MenuAction::Fetch),
        ] {
            entries.push(item(
                label,
                active && enabled.contains(&command),
                RepositoryAction::Menu(command),
            ));
        }
        entries.extend([
            E::Separator,
            item(
                "Refresh",
                active && enabled.contains(&MenuAction::Refresh),
                RepositoryAction::Menu(MenuAction::Refresh),
            ),
            E::Separator,
            E::submenu(
                "Move To",
                std::iter::once(E::item(
                    "Ungrouped",
                    true,
                    Some(RepositoryAction::MoveTo(None)),
                ))
                .chain(groups.iter().map(|group| {
                    E::item(
                        group,
                        true,
                        Some(RepositoryAction::MoveTo(Some(group.clone()))),
                    )
                }))
                .collect(),
            ),
            item(
                if favorite {
                    "Unmark as Favorite"
                } else {
                    "Mark as Favorite"
                },
                true,
                RepositoryAction::Favorite,
            ),
            item("Rename…", idle, RepositoryAction::Rename)
                .help("Wait for the current operation to finish"),
            item("Remove…", idle, RepositoryAction::Remove),
            E::Separator,
            item("Copy Path", true, RepositoryAction::CopyPath),
            E::Separator,
            item(
                "Settings…",
                active && enabled.contains(&MenuAction::RepositorySettings),
                RepositoryAction::Menu(MenuAction::RepositorySettings),
            ),
        ]);
        entries
    })
    .flatten();
    if matches!(action, Some(RepositoryAction::MoveTo(_))) {
        if let Some(RepositoryAction::MoveTo(group)) = action.take() {
            *move_to = Some((repository.path.clone(), group));
        }
    }
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
                                false,
                                false,
                                None,
                                &mut highlighted,
                                theme.palette(),
                                &[],
                                &crate::repository::Preferences::default(),
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
        use crate::theme::{InterfaceStyle, ThemeChoice};

        for (theme, interface) in [
            (ThemeChoice::Light, InterfaceStyle::Classic),
            (ThemeChoice::Dark, InterfaceStyle::Classic),
            (ThemeChoice::Light, InterfaceStyle::Modern),
            (ThemeChoice::Dark, InterfaceStyle::Modern),
        ] {
            for width in [190.0, 260.0, 350.0] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                theme.apply_style(&ctx, interface);
                let palette = interface.palette(theme);
                let modern = interface == InterfaceStyle::Modern;
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
                            ui.spacing_mut().item_spacing.y = 0.0;
                            left = ui.next_widget_position().x;
                            repository_row(
                                ui,
                                0,
                                &repository,
                                Icon::Repository,
                                0,
                                true,
                                true,
                                Some(("↑1 ↓1", "Upstream: origin/main")),
                                &mut highlighted,
                                palette,
                                &[],
                                &crate::repository::Preferences::default(),
                                &mut requested,
                                &mut move_to,
                                if modern { 44.0 } else { 24.0 },
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
                    let marker = text
                        .iter()
                        .find(|t| t.galley.job.text == "<merging>")
                        .unwrap();
                    assert!(marker.pos.x > name.pos.x && marker.pos.x < name.pos.x + 85.0);
                    assert!((marker.pos.y - name.pos.y).abs() < 2.0);
                    let tracking = text.iter().find(|t| t.galley.job.text == "↑1 ↓1").unwrap();
                    let text_center = if modern {
                        let path = text
                            .iter()
                            .find(|t| t.galley.job.text == repository.path)
                            .unwrap();
                        assert_eq!(name.pos.x, path.pos.x);
                        let name_format = &name.galley.job.sections[0].format;
                        let path_format = &path.galley.job.sections[0].format;
                        assert!(path_format.font_id.size < name_format.font_id.size);
                        assert_eq!(name_format.color, palette.text);
                        assert_eq!(path_format.color, palette.muted);
                        (name.pos.y + path.pos.y + path.galley.size().y) / 2.0
                    } else {
                        name.pos.y + name.galley.size().y / 2.0
                    };
                    let tracking_center = tracking.pos.y + tracking.galley.size().y / 2.0;
                    assert!((text_center - tracking_center).abs() < 2.0);
                    let conflict = text.iter().find(|t| t.galley.job.text == "!").unwrap();
                    assert!(conflict.pos.x < name.pos.x);
                    assert!(output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, eframe::egui::Shape::Rect(rect)
                            if rect.fill == palette.removed_text
                                && rect.rect.size() == Vec2::splat(10.0))
                    }));
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
                            false,
                            false,
                            None,
                            &mut highlighted,
                            theme.palette(),
                            &[],
                            &crate::repository::Preferences::default(),
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
