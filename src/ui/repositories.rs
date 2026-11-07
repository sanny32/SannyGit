use super::icons::{self, Icon};
use crate::{
    app::{RepositoryJob, SannyGitApp},
    theme::semibold_family,
};
use eframe::egui::{
    Align, Button, CollapsingHeader, Layout, RichText, ScrollArea, TextEdit, Ui, Window,
};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    let mut open = app.open_dialog;
    Window::new("Add or Create Repository")
        .open(&mut open)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.add_enabled_ui(!app.busy(), |ui| {
                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut app.manager.create_new,
                        false,
                        "Add existing repository",
                    );
                    ui.radio_value(&mut app.manager.create_new, true, "Create new repository");
                });
            });
            ui.label("Repository directory");
            ui.add(
                TextEdit::singleline(&mut app.repository_path)
                    .desired_width(420.0)
                    .hint_text("Repository directory"),
            );
            super::repository_dialogs::group_picker(ui, app);
            let enabled = !app.busy() && !app.repository_path.is_empty();
            if ui
                .add_enabled(
                    enabled,
                    Button::new(if app.manager.create_new {
                        "Create"
                    } else {
                        "Open"
                    }),
                )
                .clicked()
            {
                if app.manager.create_new {
                    app.run_repository_job(RepositoryJob::Init(app.repository_path.clone()));
                } else {
                    app.open_repository(app.repository_path.clone());
                }
            }
        });
    app.open_dialog = open;
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
    palette: crate::theme::Palette,
    groups: &[String],
    requested: &mut Option<String>,
    move_to: &mut Option<(String, Option<String>)>,
    height: f32,
) {
    let response = super::selectable_row(ui, selected == index, height, |ui| {
        icons::paint(ui, Icon::Repository, palette.text, 16.0);
        ui.label(RichText::new(&repository.name).family(semibold_family()));
    })
    .on_hover_text(&repository.path);
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
        *requested = Some(repository.path.clone());
    }
}
