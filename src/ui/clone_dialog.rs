use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use super::controls::EditableText;
use crate::{
    app::{RepositoryJob, SannyGitApp},
    repository,
};
use eframe::egui::{self, Align, Button, ComboBox, Layout, RichText, TextEdit, Ui};

pub(super) fn choose_folder(ui: &mut Ui, value: &mut String) {
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(4.0, 4.0);
            ui.add(
                Button::image(icons::image(Icon::Folder, ui.visuals().text_color(), 16.0))
                    .min_size(egui::vec2(28.0, 28.0)),
            )
            .help_text("Open directory chooser")
        })
        .inner;
    if response.clicked() {
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
    let palette = app.palette();
    let busy = app.busy();
    let mut job = None;
    super::controls::dialog_style::modal(ctx, palette, "clone.wizard").show(ctx, |ui| {
        ui.set_width(760.0);
        super::controls::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        ui.horizontal_top(|ui| {
            egui::Frame::new()
                .fill(palette.panel)
                .corner_radius(egui::CornerRadius {
                    nw: 7,
                    ne: 0,
                    sw: 7,
                    se: 0,
                })
                .inner_margin(18)
                .show(ui, |ui| {
                    ui.set_width(114.0);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.set_width(114.0);
                        ui.set_min_height(408.0);
                        ui.spacing_mut().item_spacing.y = 10.0;
                        ui.strong("Steps");
                        ui.add_space(8.0);
                        for (index, title) in ["Repository", "Selection", "Local Directory"]
                            .iter()
                            .enumerate()
                        {
                            let text = if index == app.manager.clone_step {
                                RichText::new(format!("› {title}")).strong()
                            } else {
                                RichText::new(*title).color(palette.muted)
                            };
                            ui.label(text);
                        }
                    });
                });
            ui.vertical(|ui| {
                ui.set_width(610.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let step = app.manager.clone_step;
                super::controls::dialog_style::header(
                    ui,
                    palette,
                    ["Repository", "Selection", "Local Directory"][step],
                    [
                        "Specify the repository to clone.",
                        "Customize how and what to clone.",
                        "Specify the local directory for the new repository.",
                    ][step],
                );
                super::controls::dialog_style::section(
                    ui,
                    palette.header,
                    egui::Margin::same(18),
                    |ui| {
                        ui.set_min_height(270.0);
                        ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
                        ui.add_enabled_ui(!busy, |ui| match step {
                            0 => source_page(ui, app),
                            1 => selection_page(ui, app),
                            _ => directory_page(ui, app),
                        });
                        if let Some(error) = &app.error {
                            ui.add(
                                egui::Label::new(RichText::new(error).color(palette.removed_text))
                                    .wrap(),
                            );
                        }
                        if busy {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(&app.status);
                            });
                        }
                    },
                );
                super::controls::dialog_style::section(
                    ui,
                    palette.header,
                    egui::Margin::symmetric(18, 12),
                    |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .add(super::controls::dialog_style::button("Cancel"))
                                .clicked()
                            {
                                if busy {
                                    app.cancel_repository_job();
                                } else {
                                    app.manager.dialog = None;
                                }
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let valid_name = !app.manager.clone_name.is_empty()
                                    && std::path::Path::new(&app.manager.clone_name)
                                        .components()
                                        .count()
                                        == 1
                                    && !matches!(app.manager.clone_name.as_str(), "." | "..")
                                    && !app.manager.clone_name.contains(['/', '\\']);
                                let enabled = !busy
                                    && match step {
                                        0 => !app.manager.clone_source.trim().is_empty(),
                                        2 => valid_name && !app.manager.clone_parent.is_empty(),
                                        _ => true,
                                    };
                                if ui
                                    .add_enabled(
                                        enabled,
                                        super::controls::dialog_style::primary(
                                            if step == 2 { "Finish" } else { "Continue" },
                                            palette,
                                        ),
                                    )
                                    .clicked()
                                {
                                    match step {
                                        0 => {
                                            job = Some(RepositoryJob::InspectClone(
                                                app.manager.clone_source.clone(),
                                            ))
                                        }
                                        1 => app.manager.clone_step = 2,
                                        _ => {
                                            job = Some(RepositoryJob::Clone {
                                                source: app.manager.clone_source.clone(),
                                                destination: repository::expand_path(
                                                    &app.manager.clone_parent,
                                                )
                                                .join(&app.manager.clone_name)
                                                .display()
                                                .to_string(),
                                                options: app.manager.clone_options.clone(),
                                            })
                                        }
                                    }
                                }
                                if ui
                                    .add_enabled(
                                        !busy && step > 0,
                                        super::controls::dialog_style::button("Go Back"),
                                    )
                                    .clicked()
                                {
                                    app.manager.clone_step -= 1;
                                }
                            });
                        });
                    },
                );
            });
        });
    });
    if let Some(job) = job {
        app.run_repository_job(job);
    }
}

fn source_page(ui: &mut Ui, app: &mut SannyGitApp) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(152.0);
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.add_sized([152.0, 26.0], Button::selectable(true, "URL or Path"));
            for provider in ["GitHub", "GitHub Enterprise", "Bitbucket", "Bitbucket Server", "GitLab", "GitLab (Self Hosted)", "Azure DevOps", "Azure DevOps Server"] {
                ui.add_enabled_ui(false, |ui| { ui.add_sized([152.0, 26.0], Button::new(provider)); })
                    .response.help_text("Hosting accounts are not supported. Use URL or Path with your existing Git credentials.");
            }
        });
        ui.vertical(|ui| {
            let width = ui.available_width();
            ui.add_sized(
                [width, 28.0],
                TextEdit::singleline(&mut app.manager.clone_source).editable(),
            );
            ui.label(RichText::new("e.g. https://server/path/to/repository").small().color(app.palette().muted));
        });
    });
}

fn selection_page(ui: &mut Ui, app: &mut SannyGitApp) {
    ui.checkbox(
        &mut app.manager.clone_options.include_submodules,
        "Include Submodules",
    );
    ui.checkbox(
        &mut app.manager.clone_options.fetch_all,
        "Fetch all Heads and Tags",
    );
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label("Check Out Branch:");
        ComboBox::from_id_salt("clone.branch")
            .width(260.0)
            .selected_text(if app.manager.clone_options.branch.is_empty() {
                "Default (empty repository)"
            } else {
                &app.manager.clone_options.branch
            })
            .show_ui(ui, |ui| {
                for branch in &app.manager.clone_branches {
                    ui.selectable_value(
                        &mut app.manager.clone_options.branch,
                        branch.clone(),
                        branch,
                    );
                }
            });
    });
    ui.add_space(24.0);
    ui.add_enabled(
        false,
        egui::Checkbox::new(&mut false, "Configure SannyGit as credential helper"),
    )
    .help_text("SannyGit uses your existing Git credential helpers and SSH configuration.");
}

fn directory_page(ui: &mut Ui, app: &mut SannyGitApp) {
    egui::Grid::new("clone.destination")
        .min_col_width(110.0)
        .spacing([12.0, 12.0])
        .show(ui, |ui| {
            ui.label("Parent Directory:");
            ui.horizontal(|ui| {
                ui.add_sized(
                    [388.0, 28.0],
                    TextEdit::singleline(&mut app.manager.clone_parent).editable(),
                );
                choose_folder(ui, &mut app.manager.clone_parent);
            });
            ui.end_row();
            ui.label("Directory Name:");
            ui.add_sized(
                [388.0, 28.0],
                TextEdit::singleline(&mut app.manager.clone_name).editable(),
            );
            ui.end_row();
            ui.label("Clone Into:");
            let mut path = repository::expand_path(&app.manager.clone_parent)
                .join(&app.manager.clone_name)
                .display()
                .to_string();
            ui.add_sized(
                [424.0, 28.0],
                TextEdit::singleline(&mut path).interactive(false),
            );
            ui.end_row();
        });
}
