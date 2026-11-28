use super::controls::dialog_style::DialogResponse;
use super::controls::icons::{self, Icon};
use super::controls::popup_menu;
use super::controls::section_header;
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::{RepositoryJob, SannyGitApp},
    menu::MenuAction,
    theme::semibold_family,
};
use eframe::egui::{Align, Layout, RichText, ScrollArea, TextStyle, Ui};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    if app.open_dialog && !app.background_progress_only {
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
                        ui.label(crate::i18n::tr_msg("Repository:"));
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
            ui.heading(crate::i18n::tr_msg("Repositories"));
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
    let modern = palette.panel_radius > 0;
    let height = repository_row_height(ui, modern);
    let ungrouped_exists = app.repositories.iter().any(|repository| {
        !app.manager
            .preferences
            .groups
            .iter()
            .any(|group| group.paths.contains(&repository.path))
    });
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
                let response = section_header(group.name.as_str())
                    .id_salt(("repository.group", &group.name))
                    .default_open(true)
                    .show_unindented(ui, |ui| {
                        super::controls::tree_indent(
                            ui,
                            ("repository.group.body", &group.name),
                            |ui| {
                                let group_repositories: Vec<_> = app
                                    .repositories
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, r)| group.paths.contains(&r.path))
                                    .collect();
                                let group_count = group_repositories.len();
                                for (position, (index, repository)) in
                                    group_repositories.into_iter().enumerate()
                                {
                                    let draw_separator = modern
                                        && (position + 1 < group_count
                                            || (position + 1 == group_count && ungrouped_exists));
                                    let branch = shown_branch(
                                        index,
                                        app.selected_repository,
                                        &app.branches.current_branch,
                                        app.manager
                                            .repository_branches
                                            .get(&repository.path)
                                            .map(String::as_str),
                                    );
                                    if let Some(action) = repository_row(
                                        ui,
                                        index,
                                        repository,
                                        branch.as_deref(),
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
                                        draw_separator,
                                    ) {
                                        context_action = Some((repository.path.clone(), action));
                                    }
                                }
                            },
                        )
                    });
                if response.header_response.clicked() {
                    app.manager.preferences.selected_group = Some(group.name);
                }
            }
            let ungrouped_repositories: Vec<_> = app
                .repositories
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    !app.manager
                        .preferences
                        .groups
                        .iter()
                        .any(|g| g.paths.contains(&r.path))
                })
                .collect();
            let ungrouped_count = ungrouped_repositories.len();
            for (position, (index, repository)) in ungrouped_repositories.into_iter().enumerate() {
                let draw_separator = modern && position + 1 < ungrouped_count;
                let branch = shown_branch(
                    index,
                    app.selected_repository,
                    &app.branches.current_branch,
                    app.manager
                        .repository_branches
                        .get(&repository.path)
                        .map(String::as_str),
                );
                if let Some(action) = repository_row(
                    ui,
                    index,
                    repository,
                    branch.as_deref(),
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
                    draw_separator,
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
    let reorder = ui.data(|data| data.get_temp::<RepositoryReorder>(repository_reorder_id()));
    ui.data_mut(|data| data.remove::<RepositoryReorder>(repository_reorder_id()));
    if let Some(reorder) = reorder {
        reorder_listed_repositories(
            &mut app.repositories,
            &app.manager.preferences,
            &mut app.selected_repository,
            &reorder.path,
            &reorder.target,
            reorder.before,
        );
    }
    if ui.input(|input| input.pointer.any_released()) {
        ui.data_mut(|data| data.remove::<RepositoryDrag>(repository_drag_id()));
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

fn shown_branch(
    index: usize,
    selected: usize,
    current: &str,
    cached: Option<&str>,
) -> Option<String> {
    if index == selected && !current.is_empty() {
        Some(current.to_owned())
    } else {
        cached
            .filter(|branch| !branch.is_empty())
            .map(ToOwned::to_owned)
    }
}

fn repository_name_line(
    ui: &mut Ui,
    name: eframe::egui::Label,
    branch: Option<&str>,
    favorite: bool,
    merging: bool,
    palette: crate::theme::Palette,
    line_height: f32,
) {
    let spacing = ui.spacing().item_spacing.x;
    let branch_text = branch
        .filter(|branch| !branch.is_empty())
        .map(|branch| format!("({branch})"));
    let measure = |ui: &Ui, text: &str| {
        ui.painter()
            .layout_no_wrap(
                text.to_owned(),
                eframe::egui::FontId::proportional(crate::theme::ui_font_size()),
                palette.muted,
            )
            .size()
            .x
    };
    let full_branch = branch_text
        .as_deref()
        .map(|text| measure(ui, text))
        .unwrap_or(0.0);
    let star = if favorite { measure(ui, "★") } else { 0.0 };
    let marker = merging.then(|| {
        ui.painter().layout_no_wrap(
            "<merging>".into(),
            eframe::egui::FontId::proportional(crate::theme::ui_font_size()),
            palette.outgoing,
        )
    });
    let marker_width = marker.as_ref().map(|galley| galley.size().x).unwrap_or(0.0);
    let mut gaps = 0.0;
    if full_branch > 0.0 {
        gaps += spacing;
    }
    if favorite {
        gaps += spacing;
    }
    if marker.is_some() {
        gaps += spacing;
    }
    ui.allocate_ui_with_layout(
        eframe::egui::vec2(ui.available_width(), line_height),
        Layout::left_to_right(Align::Center),
        |ui| {
            let available = ui.available_width();
            let mut branch_width = full_branch;
            let fixed = star + marker_width + gaps;
            let mut name_width = available - branch_width - fixed;
            if name_width < 32.0 && branch_width > 0.0 {
                let give = (32.0 - name_width).min(branch_width);
                branch_width -= give;
                name_width += give;
            }
            let show_branch = branch_text.is_some() && branch_width >= 12.0;
            if !show_branch && branch_width > 0.0 {
                name_width += branch_width + spacing;
                branch_width = 0.0;
            }
            ui.scope(|ui| {
                ui.set_max_width(name_width.max(0.0));
                ui.add(name);
            });
            if show_branch {
                ui.scope(|ui| {
                    ui.set_max_width(branch_width.max(0.0));
                    ui.add(
                        eframe::egui::Label::new(
                            RichText::new(branch_text.clone().unwrap_or_default())
                                .color(palette.muted),
                        )
                        .truncate()
                        .show_tooltip_when_elided(true),
                    );
                });
            }
            if favorite {
                ui.label(RichText::new(crate::i18n::tr_msg("★")).color(palette.warning));
            }
            if let Some(marker) = marker {
                ui.add(eframe::egui::Label::new(marker))
                    .help_text(crate::i18n::tr_msg("Merge in progress"));
            }
        },
    );
}

/// Gap between the repository name and the path line in the Modern list.
const MODERN_REPOSITORY_PATH_GAP: f32 = 1.0;

/// Horizontal inset for Modern list row separators (matches row outset).
const MODERN_REPOSITORY_ROW_SEPARATOR_INSET: f32 = 8.0;
/// Vertical gap between rows; the separator sits on its center line.
const MODERN_REPOSITORY_SEPARATOR_GAP: f32 = 9.0;

fn repository_row_separator(ui: &Ui, gap: eframe::egui::Rect, palette: crate::theme::Palette) {
    let y = gap.center().y;
    ui.painter().line_segment(
        [
            eframe::egui::pos2(gap.left() + MODERN_REPOSITORY_ROW_SEPARATOR_INSET, y),
            eframe::egui::pos2(gap.right() - MODERN_REPOSITORY_ROW_SEPARATOR_INSET, y),
        ],
        (1.0, palette.border.gamma_multiply(0.6)),
    );
}

/// Inactive repository icons are a step paler than other muted ink.
fn inactive_repository_icon_color(palette: crate::theme::Palette) -> eframe::egui::Color32 {
    let toward = palette.panel;
    let mix = |from: u8, to: u8| -> u8 {
        (f32::from(from) + (f32::from(to) - f32::from(from)) * 0.5).round() as u8
    };
    eframe::egui::Color32::from_rgb(
        mix(palette.muted.r(), toward.r()),
        mix(palette.muted.g(), toward.g()),
        mix(palette.muted.b(), toward.b()),
    )
}

/// Classic rows are a single line. Modern rows stack the path under the name
/// and stay only as tall as those two lines, plus the row inset.
fn repository_icon(
    ui: &mut Ui,
    icon: Icon,
    color: eframe::egui::Color32,
) -> eframe::egui::Response {
    const GUTTER: f32 = 16.0;
    // Filled provider marks cover much more of their SVG viewport than the
    // outlined Git icon. Keep their layout gutter equal while balancing weight.
    let size = match icon {
        Icon::GitHub | Icon::GitLab | Icon::Bitbucket => 13.0,
        Icon::AzureDevOps => 14.0,
        _ => GUTTER,
    };
    let (rect, response) = ui.allocate_exact_size(
        eframe::egui::Vec2::splat(GUTTER),
        eframe::egui::Sense::hover(),
    );
    icons::image(icon, color, size).paint_at(
        ui,
        eframe::egui::Rect::from_center_size(rect.center(), eframe::egui::Vec2::splat(size)),
    );
    response
}

fn repository_row_height(ui: &Ui, modern: bool) -> f32 {
    if !modern {
        return 24.0;
    }
    let name_height = ui.text_style_height(&TextStyle::Body);
    let mut path_font = TextStyle::Body.resolve(ui.style());
    path_font.size = (path_font.size - 2.0).max(1.0);
    let path_height = ui.fonts_mut(|fonts| fonts.row_height(&path_font));
    // `selectable_row_with_outset` insets the content by 1px on each side.
    // One more pixel above and below keeps the glyphs off the selection edge.
    (name_height + MODERN_REPOSITORY_PATH_GAP + path_height + 4.0).ceil()
}

#[allow(clippy::too_many_arguments)]
fn repository_row(
    ui: &mut Ui,
    index: usize,
    repository: &crate::models::Repository,
    branch: Option<&str>,
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
    draw_separator: bool,
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
    let icon_color = if active && branch_status.is_some_and(|(badge, _)| badge.contains(['↑', '↓']))
    {
        palette.accent
    } else if active {
        palette.text
    } else {
        inactive_repository_icon_color(palette)
    };
    let response = super::controls::selectable_row_with_outset(
        ui,
        selected_row,
        height,
        8.0,
        eframe::egui::Sense::click_and_drag(),
        |ui| {
            if modern {
                ui.spacing_mut().item_spacing.y = MODERN_REPOSITORY_PATH_GAP;
            }
            let icon_response = repository_icon(ui, icon, icon_color);
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
                icon_response.help_text(crate::i18n::tr_msg("Unresolved conflicts"));
            }
            let name = RichText::new(label).color(palette.text);
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
                    repository_name_line(
                        ui,
                        name,
                        branch,
                        favorite && !(active && merging),
                        active && merging,
                        palette,
                        if modern {
                            name_height
                        } else {
                            ui.available_height()
                        },
                    );
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
        },
    );
    if modern && draw_separator {
        let (gap, _) = ui.allocate_exact_size(
            eframe::egui::Vec2::new(ui.available_width(), MODERN_REPOSITORY_SEPARATOR_GAP),
            eframe::egui::Sense::hover(),
        );
        repository_row_separator(ui, gap, palette);
    }
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
    let response = repository_drag(
        ui,
        response,
        &repository.path,
        label,
        icon,
        icon_color,
        preferences,
        palette,
    );
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

/// Drag payload for one bookmark. The path is the bookmark identity.
#[derive(Clone)]
struct RepositoryDrag {
    path: String,
}

#[derive(Clone)]
struct RepositoryReorder {
    path: String,
    target: String,
    before: bool,
}

fn repository_reorder_id() -> eframe::egui::Id {
    eframe::egui::Id::unique("sannygit.repository-reorder")
}

fn repository_drag_id() -> eframe::egui::Id {
    eframe::egui::Id::unique("sannygit.repository-drag")
}

/// `None` is the ungrouped section. Group membership follows bookmark paths.
fn repository_section(preferences: &crate::repository::Preferences, path: &str) -> Option<String> {
    preferences
        .groups
        .iter()
        .find(|group| group.paths.iter().any(|candidate| candidate == path))
        .map(|group| group.name.clone())
}

fn same_repository_section(
    preferences: &crate::repository::Preferences,
    left: &str,
    right: &str,
) -> bool {
    repository_section(preferences, left) == repository_section(preferences, right)
}

/// Move `path` before or after `target` inside their shared section.
///
/// Rows are drawn in bookmark order, filtered by group. Swapping only the
/// slots of that section keeps every other section where it is. The open
/// repository stays selected by path.
fn reorder_listed_repositories(
    repositories: &mut [crate::models::Repository],
    preferences: &crate::repository::Preferences,
    selected: &mut usize,
    path: &str,
    target: &str,
    before: bool,
) -> bool {
    if path == target || !same_repository_section(preferences, path, target) {
        return false;
    }
    let slots: Vec<usize> = repositories
        .iter()
        .enumerate()
        .filter(|(_, repository)| same_repository_section(preferences, &repository.path, path))
        .map(|(index, _)| index)
        .collect();
    let Some(from) = slots
        .iter()
        .position(|&index| repositories[index].path == path)
    else {
        return false;
    };
    let Some(target_at) = slots
        .iter()
        .position(|&index| repositories[index].path == target)
    else {
        return false;
    };
    let mut dest = if before { target_at } else { target_at + 1 };
    if from < dest {
        dest -= 1;
    }
    if from == dest {
        return false;
    }
    let selected_path = repositories
        .get(*selected)
        .map(|repository| repository.path.clone());
    if from < dest {
        for index in from..dest {
            repositories.swap(slots[index], slots[index + 1]);
        }
    } else {
        for index in (dest..from).rev() {
            repositories.swap(slots[index], slots[index + 1]);
        }
    }
    if let Some(selected_path) = selected_path {
        if let Some(index) = repositories
            .iter()
            .position(|repository| repository.path == selected_path)
        {
            *selected = index;
        }
    }
    true
}

fn repository_drop_line(
    ui: &Ui,
    rect: eframe::egui::Rect,
    before: bool,
    color: eframe::egui::Color32,
) {
    let y = if before {
        rect.top() + 1.0
    } else {
        rect.bottom() - 1.0
    };
    let inset = MODERN_REPOSITORY_ROW_SEPARATOR_INSET;
    let left = rect.left() + inset;
    let right = rect.right() - inset;
    if right <= left {
        return;
    }
    ui.painter()
        .hline(left..=right, y, eframe::egui::Stroke::new(2.0, color));
}

/// Click, double-click, and the context menu stay on the row. A drag past the
/// click threshold reorders within the same group, or within the ungrouped list.
///
/// The pointer stays the arrow. A small copy of the row follows it, offset
/// down and to the right, the way a Windows drag image does.
#[allow(clippy::too_many_arguments)]
fn repository_drag(
    ui: &mut Ui,
    response: eframe::egui::Response,
    path: &str,
    label: &str,
    icon: Icon,
    icon_color: eframe::egui::Color32,
    preferences: &crate::repository::Preferences,
    palette: crate::theme::Palette,
) -> eframe::egui::Response {
    if ui.input(|input| input.key_pressed(eframe::egui::Key::Escape)) {
        ui.data_mut(|data| data.remove::<RepositoryDrag>(repository_drag_id()));
        return response;
    }
    if response.dragged() {
        ui.data_mut(|data| {
            data.insert_temp(
                repository_drag_id(),
                RepositoryDrag {
                    path: path.to_owned(),
                },
            );
        });
        if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
            paint_repository_drag_image(ui, pointer, icon, icon_color, label, palette);
        }
    }
    if let Some(drag) = ui.data(|data| data.get_temp::<RepositoryDrag>(repository_drag_id())) {
        if response.contains_pointer() && same_repository_section(preferences, &drag.path, path) {
            if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
                let before = pointer.y < response.rect.center().y;
                repository_drop_line(ui, response.rect, before, palette.accent);
                if drag.path != path && ui.input(|input| input.pointer.any_released()) {
                    ui.data_mut(|data| {
                        data.insert_temp(
                            repository_reorder_id(),
                            RepositoryReorder {
                                path: drag.path.clone(),
                                target: path.to_owned(),
                                before,
                            },
                        );
                    });
                }
            }
        }
    }
    response
}

/// Windows keeps the arrow and draws the dragged item just below and to the right of it.
fn paint_repository_drag_image(
    ui: &Ui,
    pointer: eframe::egui::Pos2,
    icon: Icon,
    icon_color: eframe::egui::Color32,
    label: &str,
    palette: crate::theme::Palette,
) {
    let font = eframe::egui::FontId::proportional(crate::theme::ui_font_size());
    let text_width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, palette.text)
        .size()
        .x
        .min(200.0);
    eframe::egui::Area::new(eframe::egui::Id::unique("sannygit.repository-drag-image"))
        .order(eframe::egui::Order::Tooltip)
        .fixed_pos(pointer + eframe::egui::vec2(12.0, 16.0))
        .interactable(false)
        .fade_in(false)
        .show(ui.ctx(), |ui| {
            ui.set_opacity(0.92);
            eframe::egui::Frame::new()
                .fill(palette.panel)
                .stroke(eframe::egui::Stroke::new(1.0, palette.border))
                .corner_radius(4)
                .shadow(eframe::egui::Shadow {
                    offset: [1, 2],
                    blur: 8,
                    spread: 0,
                    color: eframe::egui::Color32::from_black_alpha(48),
                })
                .inner_margin(eframe::egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.horizontal(|ui| {
                        repository_icon(ui, icon, icon_color);
                        ui.add_sized(
                            [text_width, 16.0],
                            eframe::egui::Label::new(
                                eframe::egui::RichText::new(label).color(palette.text),
                            )
                            .truncate(),
                        );
                    });
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

    #[test]
    fn repository_icons_remain_centered_in_a_shared_gutter() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            let context = Context::default();
            crate::theme::install_fonts(&context);
            egui_extras::install_image_loaders(&context);
            theme.apply(&context);
            let palette = theme.palette();
            for color in [palette.text, inactive_repository_icon_color(palette)] {
                let mut label_left = None;
                for icon in [
                    Icon::Repository,
                    Icon::GitHub,
                    Icon::GitLab,
                    Icon::Bitbucket,
                    Icon::AzureDevOps,
                ] {
                    let mut gutter = Rect::NOTHING;
                    let mut output = context.run_ui(RawInput::default(), |ui| {
                        ui.horizontal(|ui| {
                            gutter = repository_icon(ui, icon, color).rect;
                            ui.label("Repository name");
                        });
                    });
                    output.textures_delta.clear();
                    let name = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            eframe::egui::Shape::Text(text)
                                if text.galley.job.text == "Repository name" =>
                            {
                                Some(text.pos.x)
                            }
                            _ => None,
                        })
                        .expect("repository label");
                    if let Some(expected) = label_left {
                        assert_eq!(name, expected, "{icon:?} shifted the label");
                    } else {
                        label_left = Some(name);
                    }
                    assert_eq!(gutter.size(), Vec2::splat(16.0));
                    assert!(
                        output.shapes.iter().any(|shape| match &shape.shape {
                            eframe::egui::Shape::Rect(rect)
                                if rect.brush.is_some() && rect.fill == color =>
                            {
                                let center = rect.rect.center();
                                gutter.contains_rect(rect.rect)
                                    && (center - gutter.center()).length() < 0.8
                                    && rect.rect.width() >= 13.0
                                    && rect.rect.width() <= 16.0
                            }
                            _ => false,
                        }),
                        "{icon:?} should be centered and visible in {theme:?}"
                    );
                }
            }
        }
    }

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
                                None,
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
                                false,
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
    fn inactive_repository_icons_are_paler_than_muted_text() {
        use crate::theme::{InterfaceStyle, ThemeChoice};

        for (theme, lighter) in [(ThemeChoice::Light, true), (ThemeChoice::Dark, false)] {
            let palette = InterfaceStyle::Classic.palette(theme);
            let icon = inactive_repository_icon_color(palette);
            let icon_luma = u16::from(icon.r()) + u16::from(icon.g()) + u16::from(icon.b());
            let muted_luma = u16::from(palette.muted.r())
                + u16::from(palette.muted.g())
                + u16::from(palette.muted.b());
            let panel_luma = u16::from(palette.panel.r())
                + u16::from(palette.panel.g())
                + u16::from(palette.panel.b());
            if lighter {
                assert!(icon_luma > muted_luma);
            } else {
                assert!(icon_luma < muted_luma);
            }
            let icon_gap = (i32::from(icon_luma) - i32::from(panel_luma)).abs();
            let muted_gap = (i32::from(muted_luma) - i32::from(panel_luma)).abs();
            assert!(icon_gap < muted_gap);
        }
    }

    #[test]
    fn classic_unopened_repository_uses_regular_text_color() {
        use crate::theme::{InterfaceStyle, ThemeChoice};

        let ctx = Context::default();
        crate::theme::install_fonts(&ctx);
        let theme = ThemeChoice::Light;
        let interface = InterfaceStyle::Classic;
        theme.apply_style(&ctx, interface);
        let palette = interface.palette(theme);
        let open = crate::models::Repository {
            name: "OpenRepo".into(),
            path: "/open".into(),
        };
        let closed = crate::models::Repository {
            name: "ClosedRepo".into(),
            path: "/closed".into(),
        };
        let mut highlighted = None;
        let mut requested = None;
        let mut move_to = None;
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 120.0))),
                ..Default::default()
            },
            |ui| {
                let height = repository_row_height(ui, false);
                let preferences = crate::repository::Preferences::default();
                repository_row(
                    ui,
                    0,
                    &open,
                    Some("main"),
                    Icon::Repository,
                    0,
                    false,
                    false,
                    None,
                    &mut highlighted,
                    palette,
                    &[],
                    &preferences,
                    &mut requested,
                    &mut move_to,
                    height,
                    &[],
                    false,
                );
                repository_row(
                    ui,
                    1,
                    &closed,
                    Some("dev"),
                    Icon::Repository,
                    0,
                    false,
                    false,
                    None,
                    &mut highlighted,
                    palette,
                    &[],
                    &preferences,
                    &mut requested,
                    &mut move_to,
                    height,
                    &[],
                    false,
                );
            },
        );
        output.textures_delta.clear();
        let text = |label: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    eframe::egui::Shape::Text(text) if text.galley.job.text == label => {
                        Some(text.galley.job.sections[0].format.clone())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing {label}"))
        };
        let open_name = text("OpenRepo");
        let closed_name = text("ClosedRepo");
        assert_eq!(open_name.color, palette.text);
        assert_eq!(closed_name.color, palette.text);
        assert_eq!(open_name.font_id.family, semibold_family());
        assert_eq!(
            closed_name.font_id.family,
            eframe::egui::FontFamily::Proportional
        );
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
                    let mut row_top = 0.0;
                    let mut row_height = 0.0;
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
                            row_top = ui.next_widget_position().y;
                            row_height = repository_row_height(ui, modern);
                            repository_row(
                                ui,
                                0,
                                &repository,
                                Some("main"),
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
                                row_height,
                                &[],
                                false,
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
                    assert!(marker.pos.x > name.pos.x);
                    assert!((marker.pos.y - name.pos.y).abs() < 2.0);
                    assert_eq!(name.galley.job.text, "SannyGit");
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
                    let row_bottom = row_top + row_height;
                    let name_bottom = name.pos.y + name.galley.size().y;
                    let path_bottom = text
                        .iter()
                        .find(|t| t.galley.job.text == repository.path)
                        .map(|path| path.pos.y + path.galley.size().y)
                        .unwrap_or(name_bottom);
                    assert!(
                        name.pos.y >= row_top - 0.5 && path_bottom <= row_bottom + 0.5,
                        "Text leaves the row at width {width}: {row_top}..{row_bottom}, text {}..{path_bottom}",
                        name.pos.y
                    );
                    assert!(
                        tracking.pos.y >= row_top - 0.5
                            && tracking.pos.y + tracking.galley.size().y <= row_bottom + 0.5,
                        "Sync badge leaves the row at width {width}"
                    );
                    if modern {
                        assert!(row_height < 44.0, "Modern row stayed tall: {row_height}");
                        let path = text
                            .iter()
                            .find(|t| t.galley.job.text == repository.path)
                            .unwrap();
                        let gap = path.pos.y - name_bottom;
                        assert!(
                            (0.0..3.0).contains(&gap),
                            "Path gap is {gap} at width {width}"
                        );
                    }
                    let conflict = text.iter().find(|t| t.galley.job.text == "!").unwrap();
                    assert!(conflict.pos.x < name.pos.x);
                    assert!(output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, eframe::egui::Shape::Rect(rect)
                            if rect.fill == palette.removed_text
                                && rect.rect.size() == Vec2::splat(10.0)
                                && rect.rect.top() >= row_top - 0.5
                                && rect.rect.bottom() <= row_bottom + 0.5)
                    }));
                    let branch_label = text.iter().find(|t| t.galley.job.text == "(main)");
                    if width >= 260.0 {
                        let branch_label = branch_label.expect("branch label");
                        assert!((branch_label.pos.y - name.pos.y).abs() < 2.0);
                        assert!(branch_label.pos.x > name.pos.x);
                        assert!(marker.pos.x > branch_label.pos.x);
                        assert!(
                            branch_label.pos.x + branch_label.galley.size().x
                                <= tracking.pos.x + 1.0
                        );
                        let branch_color = branch_label.galley.job.sections[0].format.color;
                        assert_eq!(branch_color, palette.muted);
                    } else if let Some(branch_label) = branch_label {
                        assert!((branch_label.pos.y - name.pos.y).abs() < 2.0);
                        assert!(branch_label.pos.x > name.pos.x);
                        assert!(
                            branch_label.pos.x + branch_label.galley.size().x
                                <= tracking.pos.x + 1.0
                        );
                    }
                    output.textures_delta.clear();
                }
            }
        }
    }

    #[test]
    fn modern_rows_draw_inset_separators_between_repositories() {
        use crate::theme::{InterfaceStyle, ThemeChoice};

        let ctx = Context::default();
        crate::theme::install_fonts(&ctx);
        let theme = ThemeChoice::Light;
        let interface = InterfaceStyle::Modern;
        theme.apply_style(&ctx, interface);
        let palette = interface.palette(theme);
        let first = crate::models::Repository {
            name: "First".into(),
            path: "/first".into(),
        };
        let second = crate::models::Repository {
            name: "Second".into(),
            path: "/second".into(),
        };
        let mut highlighted = None;
        let mut requested = None;
        let mut move_to = None;
        let preferences = crate::repository::Preferences::default();
        let border = palette.border.gamma_multiply(0.6);
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 160.0))),
                ..Default::default()
            },
            |ui| {
                let height = repository_row_height(ui, true);
                repository_row(
                    ui,
                    0,
                    &first,
                    Some("main"),
                    Icon::Repository,
                    0,
                    false,
                    false,
                    None,
                    &mut highlighted,
                    palette,
                    &[],
                    &preferences,
                    &mut requested,
                    &mut move_to,
                    height,
                    &[],
                    true,
                );
                repository_row(
                    ui,
                    1,
                    &second,
                    Some("dev"),
                    Icon::Repository,
                    usize::MAX,
                    false,
                    false,
                    None,
                    &mut highlighted,
                    palette,
                    &[],
                    &preferences,
                    &mut requested,
                    &mut move_to,
                    height,
                    &[],
                    false,
                );
            },
        );
        output.textures_delta.clear();
        let separators: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                eframe::egui::Shape::LineSegment { points, stroke }
                    if stroke.color == border && stroke.width == 1.0 =>
                {
                    Some((points[0], points[1]))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            separators.len(),
            1,
            "Expected one separator between two modern rows"
        );
        let (start, end) = separators[0];
        assert!((start.y - end.y).abs() < 0.01);
        assert!(start.x >= MODERN_REPOSITORY_ROW_SEPARATOR_INSET - 0.5);
        assert!((end.x - start.x) > 200.0);
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
                            None,
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
                            false,
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

    fn bookmark(path: &str) -> crate::models::Repository {
        crate::models::Repository {
            name: path.to_owned(),
            path: path.to_owned(),
        }
    }

    #[test]
    fn reorder_stays_inside_the_section_and_keeps_the_open_repository() {
        let mut preferences = crate::repository::Preferences::default();
        preferences.groups.push(crate::repository::RepositoryGroup {
            name: "Work".into(),
            paths: vec!["/a1".into(), "/a2".into()],
        });
        preferences.groups.push(crate::repository::RepositoryGroup {
            name: "Other".into(),
            paths: vec!["/b".into()],
        });
        let mut repositories = vec![
            bookmark("/a1"),
            bookmark("/b"),
            bookmark("/a2"),
            bookmark("/u"),
        ];
        let mut selected = 2;
        assert!(reorder_listed_repositories(
            &mut repositories,
            &preferences,
            &mut selected,
            "/a2",
            "/a1",
            true,
        ));
        assert_eq!(
            repositories
                .iter()
                .map(|repository| repository.path.as_str())
                .collect::<Vec<_>>(),
            vec!["/a2", "/b", "/a1", "/u"]
        );
        assert_eq!(selected, 0);

        selected = 1;
        assert!(!reorder_listed_repositories(
            &mut repositories,
            &preferences,
            &mut selected,
            "/u",
            "/a2",
            false,
        ));
        assert!(!reorder_listed_repositories(
            &mut repositories,
            &preferences,
            &mut selected,
            "/a1",
            "/a2",
            false,
        ));
        assert_eq!(selected, 1);
        assert_eq!(repositories[1].path, "/b");
        assert_eq!(repositories[2].path, "/a1");
    }

    #[test]
    fn drag_reorders_a_repository_before_the_row_under_the_pointer() {
        let ctx = Context::default();
        crate::theme::install_fonts(&ctx);
        let theme = crate::theme::ThemeChoice::Light;
        theme.apply(&ctx);
        let palette = theme.palette();
        let first = bookmark("/first");
        let second = bookmark("/second");
        let mut highlighted = None;
        let mut requested = None;
        let mut move_to = None;
        let height = 24.0;
        let mut render = |events: Vec<Event>| {
            let mut tops = [0.0_f32, 0.0];
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 200.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.set_width(260.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    tops[0] = ui.next_widget_position().y;
                    repository_row(
                        ui,
                        0,
                        &first,
                        None,
                        Icon::Repository,
                        usize::MAX,
                        false,
                        false,
                        None,
                        &mut highlighted,
                        palette,
                        &[],
                        &crate::repository::Preferences::default(),
                        &mut requested,
                        &mut move_to,
                        height,
                        &[],
                        false,
                    );
                    tops[1] = ui.next_widget_position().y;
                    repository_row(
                        ui,
                        1,
                        &second,
                        None,
                        Icon::Repository,
                        usize::MAX,
                        false,
                        false,
                        None,
                        &mut highlighted,
                        palette,
                        &[],
                        &crate::repository::Preferences::default(),
                        &mut requested,
                        &mut move_to,
                        height,
                        &[],
                        false,
                    );
                },
            );
            output.textures_delta.clear();
            (output, tops)
        };
        let mut tops = [0.0_f32, 0.0];
        for _ in 0..3 {
            tops = render(vec![]).1;
        }
        let start = Pos2::new(80.0, tops[1] + height * 0.5);
        let end = Pos2::new(80.0, tops[0] + height * 0.25);
        render(vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        render(vec![Event::PointerMoved(end)]);
        let (output, tops) = render(vec![Event::PointerMoved(end)]);
        let line_y = tops[0] + 1.0;
        let drop_line = output.shapes.iter().any(|shape| match &shape.shape {
            eframe::egui::Shape::LineSegment { points, stroke } => {
                stroke.color == palette.accent
                    && stroke.width == 2.0
                    && (points[0].y - line_y).abs() < 0.5
                    && (points[1].y - line_y).abs() < 0.5
            }
            _ => false,
        });
        assert!(drop_line, "Expected an insertion line on the upper row");
        assert_eq!(
            output.platform_output.cursor_icon,
            eframe::egui::CursorIcon::Default,
            "The pointer stays the arrow during a drag"
        );
        let preview_labels = output
            .shapes
            .iter()
            .filter(|shape| match &shape.shape {
                eframe::egui::Shape::Text(text) => text.galley.job.text == "/second",
                _ => false,
            })
            .count();
        assert!(
            preview_labels >= 2,
            "The dragged name should follow the pointer"
        );
        render(vec![Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        let reorder = ctx
            .data(|data| data.get_temp::<RepositoryReorder>(repository_reorder_id()))
            .expect("drop");
        assert_eq!(reorder.path, "/second");
        assert_eq!(reorder.target, "/first");
        assert!(reorder.before);
        assert!(requested.is_none());
    }
}
