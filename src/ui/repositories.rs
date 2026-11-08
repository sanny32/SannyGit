use super::icons::{self, Icon};
use crate::{
    app::{RepositoryJob, SannyGitApp},
    theme::semibold_family,
};
use eframe::egui::{Align, CollapsingHeader, Layout, RichText, ScrollArea, TextEdit, Ui};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    if app.open_dialog {
        let ctx = ui.ctx().clone();
        let busy = app.busy();
        super::dialog_style::modal(&ctx, palette, "repository.add").show(&ctx, |ui| {
            ui.set_width(560.0);
            super::dialog_style::setup(ui);
            ui.spacing_mut().item_spacing.y = 0.0;
            super::dialog_style::header(ui, palette, "Add existing or create new repository",
                "Specify the local Git repository to open. To create a new repository, specify an empty directory.");
            super::dialog_style::section(ui, palette.header, eframe::egui::Margin::same(18), |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Repository:");
                        let width = (ui.available_width() - 36.0).max(100.0);
                        ui.add_sized([width, 28.0], TextEdit::singleline(&mut app.repository_path));
                        super::clone_dialog::choose_folder(ui, &mut app.repository_path);
                    });
                });
                if let Some(error) = &app.error { ui.add(eframe::egui::Label::new(RichText::new(error).color(palette.removed_text)).wrap()); }
                if busy { ui.horizontal(|ui| { ui.spinner(); ui.label(&app.status); }); }
            });
            super::dialog_style::section(ui, palette.header, eframe::egui::Margin::symmetric(18, 12), |ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add_enabled(!busy && !app.repository_path.trim().is_empty(), super::dialog_style::primary("OK", palette)).clicked() {
                        app.run_repository_job(RepositoryJob::AddOrCreate(app.repository_path.clone()));
                    }
                    if ui.add_enabled(!busy, super::dialog_style::button("Cancel")).clicked() { app.open_dialog = false; }
                });
            });
        });
    }
    ui.horizontal(|ui| {
        ui.set_min_height(26.0);
        ui.heading("Repositories");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            icons::unavailable(
                ui,
                Icon::Chevron,
                "Repository management is not implemented yet",
            );
            if icons::button(
                ui,
                Icon::Plus,
                "Open repository",
                eframe::egui::Vec2::splat(20.0),
            )
            .clicked()
            {
                app.open_dialog = true;
            }
        });
    });
    let badge = app.tracking.badge();
    let tracking_help = app.tracking.description();
    let branch_status = Some((badge.as_str(), tracking_help.as_str()));
    let height = 24.0;
    let mut requested = None;
    let mut move_to = None;
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
                            repository_row(
                                ui,
                                index,
                                repository,
                                app.selected_repository,
                                branch_status,
                                &mut app.manager.highlighted_repository,
                                palette,
                                &group_names,
                                &mut requested,
                                &mut move_to,
                                height,
                            );
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
                repository_row(
                    ui,
                    index,
                    repository,
                    app.selected_repository,
                    branch_status,
                    &mut app.manager.highlighted_repository,
                    palette,
                    &group_names,
                    &mut requested,
                    &mut move_to,
                    height,
                );
            }
        });
    if let Some((path, group)) = move_to {
        app.assign_group(&path, group);
    }
    if let Some(path) = requested {
        app.open_repository(path);
    }
}

#[allow(clippy::too_many_arguments)]
fn repository_row(
    ui: &mut Ui,
    index: usize,
    repository: &crate::models::Repository,
    selected: usize,
    branch_status: Option<(&str, &str)>,
    highlighted: &mut Option<String>,
    palette: crate::theme::Palette,
    groups: &[String],
    requested: &mut Option<String>,
    move_to: &mut Option<(String, Option<String>)>,
    height: f32,
) {
    let active = selected == index;
    let selected_row = highlighted
        .as_deref()
        .map_or(active, |path| path == repository.path);
    let response = super::selectable_row(ui, selected_row, height, |ui| {
        icons::paint(
            ui,
            Icon::Repository,
            if active && branch_status.is_some_and(|(badge, _)| badge.contains(['↑', '↓'])) {
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
            eframe::egui::vec2(width, 20.0),
            Layout::left_to_right(Align::Center).with_main_align(Align::Min),
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
                    .selectable(false)
                    .sense(eframe::egui::Sense::hover()),
                );
            },
        );
        if !suffix.is_empty() {
            ui.add(
                eframe::egui::Label::new(super::sync_badge(&suffix, palette, 12.0))
                    .selectable(false)
                    .sense(eframe::egui::Sense::hover()),
            );
        }
    })
    .on_hover_text(if active {
        format!(
            "{}\n{}",
            repository.path,
            branch_status.map(|(_, help)| help).unwrap_or("")
        )
    } else {
        repository.path.clone()
    });
    response.context_menu(|ui| {
        ui.label("Move to group");
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
    if response.clicked() {
        *highlighted = Some(repository.path.clone());
    }
    if response.double_clicked() {
        *requested = Some(repository.path.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

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
                                0,
                                Some(("↑1 ↓1", "Upstream: origin/main")),
                                &mut highlighted,
                                theme.palette(),
                                &[],
                                &mut requested,
                                &mut move_to,
                                24.0,
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
                            usize::MAX,
                            None,
                            &mut highlighted,
                            theme.palette(),
                            &[],
                            &mut requested,
                            &mut move_to,
                            24.0,
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
