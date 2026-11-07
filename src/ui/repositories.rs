use super::icons::{self, Icon};
use crate::{
    app::{RepositoryJob, SannyGitApp},
    theme::semibold_family,
};
use eframe::egui::{Align, Button, CollapsingHeader, Layout, RichText, ScrollArea, TextEdit, Ui};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    if app.open_dialog {
        eframe::egui::Modal::new(eframe::egui::Id::unique("repository.add")).show(ui.ctx(), |ui| {
            ui.set_width(540.0);
            ui.heading("Add existing or create new repository");
            ui.label("Specify the local Git repository to open. To create a new repository, specify an empty directory.");
            ui.add_space(18.0);
            ui.add_enabled_ui(!app.busy(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Repository:");
                    ui.add(TextEdit::singleline(&mut app.repository_path).desired_width(390.0));
                    super::clone_dialog::choose_folder(ui, &mut app.repository_path);
                });
            });
            if let Some(error) = &app.error { ui.colored_label(palette.removed_text, error); }
            ui.add_space(18.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add_enabled(!app.busy() && !app.repository_path.trim().is_empty(), Button::new("OK")).clicked() {
                    app.run_repository_job(RepositoryJob::AddOrCreate(app.repository_path.clone()));
                }
                if ui.add_enabled(!app.busy(), Button::new("Cancel")).clicked() { app.open_dialog = false; }
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
