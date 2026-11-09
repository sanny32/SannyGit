use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::{RepositoryJob, SannyGitApp},
    clone_source::Provider,
    repository,
};
use eframe::egui::{self, Align, Button, ComboBox, Layout, RichText, TextEdit, Ui};

pub(super) fn choose_folder(ui: &mut Ui, value: &mut String) {
    let response = icons::button(ui, Icon::Folder, "Open directory chooser");
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
                        ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 258.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_min_height(258.0);
                                ui.add_enabled_ui(!busy, |ui| match step {
                                    0 => source_page(ui, app),
                                    1 => selection_page(ui, app),
                                    _ => directory_page(ui, app),
                                });
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 20.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.set_min_height(20.0);
                                if busy {
                                    ui.spinner();
                                    ui.add(egui::Label::new(&app.status).truncate())
                                        .help_text(&app.status);
                                } else if let Some(error) = &app.error {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(error).color(palette.removed_text),
                                        )
                                        .truncate(),
                                    )
                                    .help_text(error);
                                }
                            },
                        );
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
                                        0 => app.manager.clone_form.resolve().is_ok(),
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
                                        0 => match app.manager.clone_form.resolve() {
                                            Ok(source) => {
                                                app.manager.clone_source = source.clone();
                                                job = Some(RepositoryJob::InspectClone(source));
                                            }
                                            Err(error) => app.error = Some(error),
                                        },
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
            ui.set_width(184.0);
            ui.spacing_mut().item_spacing.y = 3.0;
            for provider in Provider::ALL {
                let icon = match provider {
                    Provider::Url => Icon::Repository,
                    Provider::GitHub | Provider::GitHubEnterprise => Icon::GitHub,
                    Provider::Bitbucket | Provider::BitbucketServer => Icon::Bitbucket,
                    Provider::GitLab | Provider::GitLabSelfHosted => Icon::GitLab,
                    Provider::Azure | Provider::AzureServer => Icon::AzureDevOps,
                };
                let button = Button::new((
                    icons::image(icon, ui.visuals().text_color(), 16.0),
                    provider.label(),
                    egui::Atom::grow(),
                ))
                .gap(8.0)
                .selected(app.manager.clone_form.provider == provider);
                if ui.add_sized([184.0, 26.0], button).clicked() {
                    app.manager.clone_form.provider = provider;
                    app.error = None;
                }
            }
        });
        ui.vertical(|ui| {
            let provider = app.manager.clone_form.provider;
            let width = ui.available_width();
            if provider.self_hosted() {
                ui.label("Server URL:");
                if ui
                    .add_sized(
                        [width, 28.0],
                        TextEdit::singleline(app.manager.clone_form.server())
                            .hint_text("https://git.example.com"),
                    )
                    .changed()
                {
                    app.error = None;
                }
            }
            ui.label(if provider == Provider::Url {
                "Repository URL or local path:"
            } else {
                "Repository path or clone URL:"
            });
            if ui
                .add_sized(
                    [width, 28.0],
                    TextEdit::singleline(app.manager.clone_form.input()).hint_text(provider.hint()),
                )
                .changed()
            {
                app.error = None;
            }
            if provider == Provider::Url {
                ui.horizontal(|ui| {
                    ui.label("Local repository:");
                    choose_folder(ui, app.manager.clone_form.input());
                });
            }
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
                    TextEdit::singleline(&mut app.manager.clone_parent),
                );
                choose_folder(ui, &mut app.manager.clone_parent);
            });
            ui.end_row();
            ui.label("Directory Name:");
            ui.add_sized(
                [388.0, 28.0],
                TextEdit::singleline(&mut app.manager.clone_name),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{install_fonts, ThemeChoice};

    #[test]
    fn providers_align_left_and_errors_preserve_footer_in_both_themes() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                let ctx = egui::Context::default();
                install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                let mut footer = None;
                for provider in Provider::ALL {
                    app.manager.clone_form.provider = provider;
                    for error in [None, Some("Connection failed. ".repeat(100))] {
                        app.error = error;
                        let mut output = egui::FullOutput::default();
                        for _ in 0..3 {
                            output = ctx.run_ui(
                                egui::RawInput {
                                    screen_rect: Some(egui::Rect::from_min_size(
                                        egui::Pos2::ZERO,
                                        size,
                                    )),
                                    ..Default::default()
                                },
                                |ui| show(ui.ctx(), &mut app),
                            );
                            output.textures_delta.clear();
                        }
                        let text = |label: &str| {
                            output
                                .shapes
                                .iter()
                                .find_map(|shape| match &shape.shape {
                                    egui::Shape::Text(text) if text.galley.job.text == label => {
                                        let rect = text.galley.rect.translate(text.pos.to_vec2());
                                        assert!(
                                            shape.clip_rect.contains_rect(rect),
                                            "Clipped {label}: {rect:?}"
                                        );
                                        assert_eq!(text.galley.rows.len(), 1, "Wrapped {label}");
                                        Some(text.pos)
                                    }
                                    _ => None,
                                })
                                .unwrap_or_else(|| panic!("Missing {label}"))
                        };
                        let left = text(Provider::Url.label()).x;
                        for source in Provider::ALL {
                            assert!((text(source.label()).x - left).abs() < 0.5);
                        }
                        let cancel = text("Cancel");
                        if let Some(previous) = footer {
                            assert!(
                                cancel.distance(previous) < 0.5,
                                "Footer moved for {provider:?}: {previous:?} -> {cancel:?}"
                            );
                        }
                        footer = Some(cancel);
                        assert!(text("Continue").y < size.y);
                        assert!(!app.busy(), "Rendering must not start Git operations");
                    }
                }
            }
        }
    }
}
