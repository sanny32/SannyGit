use super::icons::{self, Icon};
use crate::{
    app::{RepositoryJob, SannyGitApp},
    repository,
};
use eframe::egui::{self, Align, Button, ComboBox, Layout, RichText, TextEdit, Ui};

pub(super) fn choose_folder(ui: &mut Ui, value: &mut String) {
    if icons::button(
        ui,
        Icon::Folder,
        "Open directory chooser",
        egui::vec2(24.0, 24.0),
    )
    .clicked()
    {
        let mut picker = rfd::FileDialog::new()
            .set_title("Choose directory")
            .set_can_create_directories(true);
        let path = repository::expand_path(value);
        if path.is_dir() {
            picker = picker.set_directory(path);
        }
        if let Some(path) = picker.pick_folder() {
            *value = path.to_string_lossy().into_owned();
        }
    }
}

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    let busy = app.busy();
    let mut job = None;
    egui::Modal::new(egui::Id::unique("clone.wizard")).show(ctx, |ui| {
        ui.set_width(740.0);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.horizontal_top(|ui| {
            egui::Frame::new().fill(palette.panel).inner_margin(14.0).show(ui, |ui| {
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.set_width(120.0); ui.set_min_height(410.0);
                ui.strong("Steps"); ui.add_space(12.0);
                for (index, title) in ["Repository", "Selection", "Local Directory"].iter().enumerate() {
                    ui.label(if index == app.manager.clone_step { RichText::new(format!("› {title}")).strong() } else { RichText::new(*title).color(palette.muted) });
                }
                });
            });
            ui.vertical(|ui| {
                ui.set_width(570.0);
                let step = app.manager.clone_step;
                egui::Frame::new().fill(palette.panel).inner_margin(12.0).show(ui, |ui| {
                    ui.set_width(546.0);
                    ui.heading(["Repository", "Selection", "Local Directory"][step]);
                    ui.label(["Specify the repository to clone.", "Customize how and what to clone.", "Specify the local directory for the new repository."][step]);
                });
                ui.add_space(14.0);
                ui.allocate_ui(egui::vec2(570.0, 260.0), |ui| {
                    ui.set_min_height(260.0);
                    ui.add_enabled_ui(!busy, |ui| match step {
                        0 => { ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(140.0);
                                ui.add(egui::Button::selectable(true, "URL or Path")).on_hover_text("Clone using Git URL or local path");
                                for provider in ["GitHub", "GitHub Enterprise", "Bitbucket", "Bitbucket Server", "GitLab", "GitLab (Self Hosted)", "Azure DevOps", "Azure DevOps Server"] {
                                    ui.add_enabled(false, Button::new(provider)).on_hover_text("Hosting accounts are not supported. Use URL or Path with your existing Git credentials.");
                                }
                            });
                            ui.vertical(|ui| {
                                ui.add(TextEdit::singleline(&mut app.manager.clone_source).desired_width(400.0));
                                ui.label(RichText::new("e.g. https://server/path/to/repository").small().color(palette.muted));
                            });
                        }); },
                        1 => {
                            ui.checkbox(&mut app.manager.clone_options.include_submodules, "Include Submodules");
                            ui.checkbox(&mut app.manager.clone_options.fetch_all, "Fetch all Heads and Tags");
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                ui.label("Check Out Branch:");
                                ComboBox::from_id_salt("clone.branch").selected_text(if app.manager.clone_options.branch.is_empty() { "Default (empty repository)" } else { &app.manager.clone_options.branch }).show_ui(ui, |ui| {
                                    for branch in &app.manager.clone_branches { ui.selectable_value(&mut app.manager.clone_options.branch, branch.clone(), branch); }
                                });
                            });
                            ui.add_space(24.0);
                            ui.add_enabled(false, egui::Checkbox::new(&mut false, "Configure SannyGit as credential helper")).on_hover_text("SannyGit uses your existing Git credential helpers and SSH configuration.");
                        },
                        _ => {
                            egui::Grid::new("clone.destination").min_col_width(120.0).spacing([10.0,12.0]).show(ui, |ui| {
                                ui.label("Parent Directory:");
                                ui.horizontal(|ui| { ui.add(TextEdit::singleline(&mut app.manager.clone_parent).desired_width(360.0)); choose_folder(ui, &mut app.manager.clone_parent); }); ui.end_row();
                                ui.label("Directory Name:"); ui.add(TextEdit::singleline(&mut app.manager.clone_name).desired_width(360.0)); ui.end_row();
                                ui.label("Clone Into:");
                                let mut path = repository::expand_path(&app.manager.clone_parent).join(&app.manager.clone_name).display().to_string();
                                ui.add(TextEdit::singleline(&mut path).desired_width(390.0).interactive(false)); ui.end_row();
                            });
                        }
                    });
                });
                if let Some(error) = &app.error { ui.add(egui::Label::new(RichText::new(error).color(palette.removed_text)).wrap()); }
                if busy { ui.horizontal(|ui| { ui.spinner(); ui.label(&app.status); }); }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() { if busy { app.cancel_repository_job(); } else { app.manager.dialog = None; } }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let valid_name = !app.manager.clone_name.is_empty() && std::path::Path::new(&app.manager.clone_name).components().count() == 1 && !matches!(app.manager.clone_name.as_str(), "." | "..") && !app.manager.clone_name.contains(['/', '\\']);
                        let enabled = !busy && if step == 0 { !app.manager.clone_source.trim().is_empty() } else if step == 2 { valid_name && !app.manager.clone_parent.is_empty() } else { true };
                        if ui.add_enabled(enabled, Button::new(if step == 2 { "Finish" } else { "Continue" })).clicked() {
                            match step {
                                0 => job = Some(RepositoryJob::InspectClone(app.manager.clone_source.clone())),
                                1 => app.manager.clone_step = 2,
                                _ => job = Some(RepositoryJob::Clone { source: app.manager.clone_source.clone(), destination: repository::expand_path(&app.manager.clone_parent).join(&app.manager.clone_name).display().to_string(), options: app.manager.clone_options.clone() }),
                            }
                        }
                        if ui.add_enabled(!busy && step > 0, Button::new("Go Back")).clicked() { app.manager.clone_step -= 1; }
                    });
                });
            });
        });
    });
    if let Some(job) = job {
        app.run_repository_job(job);
    }
}
