use super::controls::dialog_style::DialogResponse;
use crate::app::{RepositoryDialog as Dialog, RepositoryJob as Job, SannyGitApp};
use eframe::egui::{self, Button, ComboBox, RichText, ScrollArea, TextEdit, Ui, Window};

fn add_worktree_dialog(ctx: &egui::Context, app: &mut SannyGitApp) {
    use super::controls::dialog_style as style;

    let palette = app.palette();
    if app.manager.worktree_branches.is_empty() {
        let mut close = false;
        style::modal(ctx, palette, "repository.add_worktree.unavailable").show(ctx, |ui| {
            ui.set_width(370.0);
            style::setup(ui);
            ui.spacing_mut().item_spacing.y = 0.0;
            style::header_with_icon(
                ui,
                palette,
                "No (more) local branches available.",
                "You only can create worktrees for existing local branches that don't yet have an associated worktree.",
                super::controls::icons::Icon::Warning,
            );
            style::section(ui, palette.header, egui::Margin::symmetric(12, 10), |ui| {
                style::button_row(ui, |ui| {
                    close = ui.add(style::primary("OK", palette)).dialog_close();
                });
            });
        });
        if close {
            app.manager.dialog = None;
        }
        return;
    }
    let busy = app.busy();
    let mut accept = false;
    let mut cancel = false;
    style::modal(ctx, palette, "repository.add_worktree").show(ctx, |ui| {
        ui.set_width(480.0);
        style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        style::header(
            ui,
            palette,
            "Create another worktree from this repository",
            "Select the branch and directory to use for the new worktree.",
        );
        style::section(ui, palette.header, egui::Margin::symmetric(12, 10), |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                egui::Grid::new("worktree.fields")
                    .num_columns(2)
                    .spacing([8.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Branch:");
                        ComboBox::from_id_salt("worktree.add.branch")
                            .width(ui.available_width())
                            .truncate()
                            .selected_text(if app.manager.worktree_branch.is_empty() {
                                "Select a local branch"
                            } else {
                                &app.manager.worktree_branch
                            })
                            .show_ui(ui, |ui| {
                                for branch in &app.manager.worktree_branches {
                                    ui.selectable_value(
                                        &mut app.manager.worktree_branch,
                                        branch.clone(),
                                        branch,
                                    );
                                }
                            });
                        ui.end_row();
                        ui.label("Directory:");
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - 36.0).max(80.0);
                            ui.add(
                                TextEdit::singleline(&mut app.manager.destination)
                                    .desired_width(width),
                            );
                            super::clone_dialog::choose_folder(ui, &mut app.manager.destination);
                        });
                        ui.end_row();
                    });
            });
        });
        style::section(ui, palette.header, egui::Margin::symmetric(12, 10), |ui| {
            style::button_row(ui, |ui| {
                cancel = ui
                    .add_enabled(!busy, style::button("Cancel"))
                    .dialog_cancel();
                accept = ui
                    .add_enabled(
                        !busy
                            && !app.manager.destination.trim().is_empty()
                            && app
                                .manager
                                .worktree_branches
                                .contains(&app.manager.worktree_branch),
                        style::primary("OK", palette),
                    )
                    .dialog_accept();
                if busy {
                    ui.spinner();
                }
            });
        });
    });
    if cancel {
        app.manager.dialog = None;
    } else if accept {
        app.run_repository_job(Job::AddWorktree {
            destination: app.manager.destination.clone(),
            branch: app.manager.worktree_branch.clone(),
            new_branch: false,
        });
    }
}

fn remove_repository_dialog(ctx: &egui::Context, app: &mut SannyGitApp) {
    let Some(path) = app.manager.repository_to_remove.clone() else {
        app.manager.dialog = None;
        return;
    };
    let palette = app.palette();
    let mut remove = false;
    let mut cancel = false;
    super::controls::dialog_style::modal(ctx, palette, "repository.remove").show(ctx, |ui| {
        ui.set_width(480.0);
        super::controls::dialog_style::setup(ui);
        super::controls::dialog_style::header(
            ui,
            palette,
            "Remove Repository",
            "Remove this repository from the list? Files on disk will be kept.",
        );
        super::controls::dialog_style::section(ui, palette.header, egui::Margin::same(18), |ui| {
            ui.add(egui::Label::new(&path).truncate())
                .on_hover_text(&path);
            ui.label("Any saved commit-message draft for this repository will be discarded.");
        });
        super::controls::dialog_style::section(
            ui,
            palette.header,
            egui::Margin::symmetric(18, 12),
            |ui| {
                super::controls::dialog_style::button_row(ui, |ui| {
                    remove = ui
                        .add_enabled(!app.busy(), super::controls::dialog_style::button("Remove"))
                        .clicked();
                    cancel = ui
                        .add(super::controls::dialog_style::button("Cancel"))
                        .dialog_cancel();
                });
            },
        );
    });
    if cancel {
        app.manager.dialog = None;
        app.manager.repository_to_remove = None;
    } else if remove {
        app.confirm_remove_repository();
    }
}

pub(super) fn group_picker(ui: &mut Ui, app: &mut SannyGitApp) {
    if app.manager.preferences.groups.is_empty() {
        return;
    }
    ComboBox::from_id_salt("repository.group")
        .selected_text(
            app.manager
                .preferences
                .selected_group
                .as_deref()
                .unwrap_or("Ungrouped"),
        )
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut app.manager.preferences.selected_group,
                None,
                "Ungrouped",
            );
            for group in &app.manager.preferences.groups {
                ui.selectable_value(
                    &mut app.manager.preferences.selected_group,
                    Some(group.name.clone()),
                    &group.name,
                );
            }
        });
}

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    if app.manager.dialog == Some(Dialog::AddWorktree) {
        add_worktree_dialog(ui.ctx(), app);
        return;
    }
    if app.network_dialog.is_some() {
        super::remote_dialog::show(ui.ctx(), app);
        return;
    }
    if app.manager.dialog == Some(Dialog::Clone) {
        super::clone_dialog::show(ui.ctx(), app);
        return;
    }
    if app.manager.dialog == Some(Dialog::Settings) {
        settings_dialog(ui.ctx(), app);
        return;
    }
    if app.manager.dialog == Some(Dialog::RemoveRepository) {
        remove_repository_dialog(ui.ctx(), app);
        return;
    }
    if app.manager.dialog == Some(Dialog::Search) {
        search_dialog(ui.ctx(), app);
        return;
    }
    if app.busy() && app.manager.cancel.is_some() {
        Window::new("Repository operation")
            .anchor(
                eframe::egui::Align2::CENTER_CENTER,
                eframe::egui::Vec2::ZERO,
            )
            .id(egui::Id::unique("repository.progress"))
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(&app.status);
                });
                if ui.button("Cancel operation").clicked() {
                    app.cancel_repository_job();
                }
            });
    }
    let Some(dialog) = app.manager.dialog else {
        return;
    };
    let title = match dialog {
        Dialog::Group => "Add Group",
        Dialog::Clone => "Clone Repository",
        Dialog::Search => "Search for Repositories",
        Dialog::Worktrees => "Worktrees",
        Dialog::AddWorktree => "Add Worktree",
        Dialog::Prune => "Prune Obsolete Worktrees",
        Dialog::Settings => "Repository Settings",
        Dialog::CloseRepository => "Close Repository",
        Dialog::RemoveRepository => "Remove Repository",
    };
    let mut close = false;
    let mut job = None;
    let palette = app.palette();
    super::controls::dialog_style::modal(ui.ctx(), palette, "repository.management")
        .show(ui.ctx(), |ui| {
            ui.set_width(540.0);
            super::controls::dialog_style::setup(ui);
            ui.spacing_mut().item_spacing.y = 0.0;
            let description = match dialog {
                Dialog::Group => "Organize repositories into a named group.",
                Dialog::Search => "Find Git repositories in a local directory.",
                Dialog::Worktrees => "Manage working directories attached to this repository.",
                Dialog::Prune => "Review obsolete worktree metadata before pruning.",
                Dialog::CloseRepository => "Close the active repository.",
                _ => "Manage your local repositories.",
            };
            super::controls::dialog_style::header(ui, palette, title, description);
            super::controls::dialog_style::section(ui, palette.header, egui::Margin::same(18), |ui| {
            ScrollArea::vertical()
                .id_salt(("repository.management.content", title))
                .max_height((ui.ctx().content_rect().height() - 210.0).max(120.0))
                .show(ui, |ui| {
            let busy = app.busy();
            ui.add_enabled_ui(!busy, |ui| match dialog {
                Dialog::Group => {
                    ui.label("Group name"); ui.add(TextEdit::singleline(&mut app.manager.group_name).desired_width(f32::INFINITY));
                }
                Dialog::Clone => {},
                Dialog::Search => {},
                Dialog::Worktrees => {
                    let active = app.repositories.get(app.selected_repository).map(|r| r.path.as_str());
                    ScrollArea::vertical().id_salt("repository.worktrees").max_height(180.0).show(ui, |ui| {
                        for tree in &app.manager.worktrees {
                            let label = format!("{}  {}{}{}{}", tree.path, tree.branch.as_deref().unwrap_or("detached"), if tree.main { " [main]" } else { "" }, if tree.locked { " [locked]" } else { "" }, if tree.prunable { " [missing]" } else { "" });
                            if ui.selectable_label(app.manager.selected_worktree.as_deref() == Some(&tree.path), label).clicked() { app.manager.selected_worktree = Some(tree.path.clone()); app.manager.confirm_remove = false; }
                        }
                    });
                    let removable = app.manager.worktrees.iter().find(|t| Some(&t.path) == app.manager.selected_worktree.as_ref()).is_some_and(|t| !t.main && !t.locked && !t.bare && Some(t.path.as_str()) != active && !t.prunable);
                    if ui.add_enabled(removable, Button::new("Remove Selected…")).clicked() { app.manager.confirm_remove = true; }
                    if app.manager.confirm_remove {
                        ui.separator();
                        ui.label(format!("Remove worktree directory {}?", app.manager.selected_worktree.as_deref().unwrap_or("")));
                        ui.label("Removal checks for local, untracked and ignored files. The branch is kept.");
                        ui.horizontal(|ui| {
                            if ui.button("Cancel removal").clicked() { app.manager.confirm_remove = false; }
                            if ui.add_enabled(removable, Button::new("Remove Worktree")).clicked() { if let Some(path) = app.manager.selected_worktree.clone() { job = Some(Job::RemoveWorktree(path)); } }
                        });
                    }
                    ui.separator(); ui.label("Add worktree: destination (new directory)");
                    ui.add(TextEdit::singleline(&mut app.manager.destination).desired_width(f32::INFINITY));
                    ui.checkbox(&mut app.manager.worktree_new_branch, "Create a new branch from HEAD");
                    if app.manager.worktree_new_branch {
                        ui.label("New branch name"); ui.add(TextEdit::singleline(&mut app.manager.worktree_branch).desired_width(f32::INFINITY));
                    } else {
                        ComboBox::from_id_salt("worktree.branch").selected_text(if app.manager.worktree_branch.is_empty() { "Select a local branch" } else { &app.manager.worktree_branch }).show_ui(ui, |ui| {
                            for branch in app.refs.iter().filter(|r| r.kind == crate::models::RefKind::Local) { ui.selectable_value(&mut app.manager.worktree_branch, branch.name.clone(), &branch.name); }
                        });
                    }
                    if ui.add_enabled(!app.manager.destination.is_empty() && !app.manager.worktree_branch.is_empty(), Button::new("Add Worktree")).clicked() { job = Some(Job::AddWorktree { destination: app.manager.destination.clone(), branch: app.manager.worktree_branch.clone(), new_branch: app.manager.worktree_new_branch }); }
                    if ui.button("Reload Worktrees").clicked() { job = Some(Job::ListWorktrees); }
                }
                Dialog::Prune => {
                    if let Some(preview) = &app.manager.prune_preview {
                        ui.label("Only obsolete administrative metadata is removed; working directories are not deleted.");
                        ScrollArea::vertical().max_height(240.0).show(ui, |ui| { ui.add(egui::Label::new(preview).wrap().selectable(true)); });
                        if preview.is_empty() { ui.label("No obsolete worktree metadata to prune."); }
                        if ui.add_enabled(!preview.is_empty(), Button::new("Prune Obsolete Metadata")).clicked() { job = Some(Job::Prune); }
                    }
                }
                Dialog::Settings | Dialog::RemoveRepository | Dialog::AddWorktree => {}
                Dialog::CloseRepository => {
                    ui.label("Close this repository and discard its commit message drafts?");
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() { app.manager.dialog = None; }
                        if ui.button("Close Repository").clicked() { app.close_repository(true); }
                    });
                }
            });
            });
            });
            super::controls::dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
                super::controls::dialog_style::button_row(ui, |ui| {
                    if matches!(dialog, Dialog::Group)
                        && ui.add_enabled(
                            !app.busy() && !app.manager.group_name.trim().is_empty(),
                            super::controls::dialog_style::primary("Add Group", palette),
                        ).dialog_accept()
                    {
                        app.add_group();
                    }
                    close = ui.add(super::controls::dialog_style::button("Close")).dialog_cancel();
                });
            });
        });
    if close {
        app.manager.dialog = None;
    }
    if let Some(job) = job {
        app.run_repository_job(job);
    }
}

fn search_dialog(ctx: &egui::Context, app: &mut SannyGitApp) {
    use super::controls::dialog_style as style;

    let palette = app.palette();
    let busy = app.busy();
    let mut start = false;
    let mut accept = false;
    let mut cancel = false;
    style::modal(ctx, palette, "repository.search").show(ctx, |ui| {
        ui.set_width(620.0);
        style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        style::header(
            ui,
            palette,
            "Search existing local repositories",
            "Select a root directory where the search should start and click 'Start Search'.",
        );
        style::section(ui, palette.header, egui::Margin::symmetric(12, 10), |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search In:");
                    let width = (ui.available_width() - 150.0).max(80.0);
                    ui.add(TextEdit::singleline(&mut app.manager.search_root).desired_width(width));
                    super::clone_dialog::choose_folder(ui, &mut app.manager.search_root);
                    start = ui
                        .add_enabled(
                            !app.manager.search_root.trim().is_empty(),
                            Button::new("Start Search"),
                        )
                        .clicked();
                });
            });
            ui.label("Found repositories (selected ones will be remembered):");
            egui::Frame::new()
                .fill(palette.panel)
                .stroke((1.0, palette.border))
                .inner_margin(4)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        ui.label("Path");
                    });
                    ui.separator();
                    ScrollArea::both()
                        .id_salt("repository.search.results")
                        .auto_shrink([false, false])
                        .max_height(210.0)
                        .show(ui, |ui| {
                            ui.set_min_height(210.0);
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                            ui.add_enabled_ui(!busy, |ui| {
                                for (_, path, selected) in &mut app.manager.found {
                                    ui.horizontal(|ui| {
                                        ui.checkbox(selected, "");
                                        if ui.selectable_label(*selected, path.as_str()).clicked() {
                                            *selected = !*selected;
                                        }
                                    });
                                }
                            });
                        });
                });
            ui.add_enabled_ui(!busy, |ui| group_picker(ui, app));
            let status = if busy {
                "Searching repositories…"
            } else {
                &app.manager.search_summary
            };
            style::text_line(ui, RichText::new(status).small().color(palette.muted), 18.0)
                .on_hover_text(status);
        });
        style::section(ui, palette.header, egui::Margin::symmetric(12, 10), |ui| {
            style::button_row(ui, |ui| {
                cancel = ui.add(style::button("Cancel")).dialog_cancel();
                accept = ui
                    .add_enabled(
                        !busy && app.manager.found.iter().any(|r| r.2),
                        style::primary("OK", palette),
                    )
                    .dialog_accept();
            });
        });
    });
    if cancel {
        if busy {
            app.cancel_repository_job();
        }
        app.manager.dialog = None;
    } else if accept {
        app.add_found_repositories();
    } else if start {
        app.manager.search_summary.clear();
        app.run_repository_job(Job::Search {
            root: app.manager.search_root.trim().to_owned(),
            depth: app.manager.search_depth,
        });
    }
}

fn settings_dialog(ctx: &egui::Context, app: &mut SannyGitApp) {
    let palette = app.palette();
    let busy = app.busy();
    let mut job = None;
    let mut close = false;
    super::controls::dialog_style::modal(ctx, palette, "repository.settings").show(ctx, |ui| {
        ui.set_width(660.0_f32.min(ctx.content_rect().width() - 48.0));
        super::controls::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        super::controls::dialog_style::header(
            ui,
            palette,
            "Edit the repository settings",
            "Here you can edit the Git settings for the repository. The default settings can be found in the Preferences.",
        );
        super::controls::dialog_style::section(ui, palette.header, egui::Margin::same(12), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            let tab_id = egui::Id::unique("repository.settings.tab");
            let mut tab = ctx.data_mut(|data| data.get_temp::<usize>(tab_id).unwrap_or(0));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (index, label) in [
                    "User", "Fetch and Pull", "Push", "Signing", "Encoding",
                ].iter().enumerate() {
                    if ui.selectable_label(tab == index, *label).clicked() {
                        tab = index;
                    }
                }
            });
            ctx.data_mut(|data| data.insert_temp(tab_id, tab));
            egui::Frame::new()
                .fill(palette.panel)
                .stroke((1.0, palette.border))
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ScrollArea::vertical()
                        .id_salt("repository.settings.content")
                        .auto_shrink([false, false])
                        .max_height(160.0)
                        .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.spacing_mut().interact_size.y = 22.0;
                    if tab == 0 {
                        if let Some(settings) = &mut app.manager.settings {
                            ui.add_enabled_ui(!busy, |ui| {
                                let field_width = ui.available_width() - 118.0;
                                egui::Grid::new("repository.settings.identity")
                                    .num_columns(2)
                                    .min_col_width(110.0)
                                    .spacing([8.0, 4.0])
                                    .show(ui, |ui| {
                                        ui.label("Full Name:");
                                        ui.add(TextEdit::singleline(&mut settings.name)
                                            .desired_width(field_width));
                                        ui.end_row();
                                        ui.label("Git Email:");
                                        ui.add(TextEdit::singleline(&mut settings.email)
                                            .desired_width(field_width));
                                        ui.end_row();
                                    });
                            });
                            ui.label(RichText::new(
                                "Leave a field empty to use the inherited Git setting."
                            ).small().color(palette.muted));
                        } else if app.error.is_none() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Loading repository settings…");
                            });
                        }
                    } else {
                        ui.add_enabled_ui(false, |ui| {
                            match tab {
                                1 => {
                                    let _ = ui.radio(true, "Merge fetched remote changes");
                                    let _ = ui.radio(false, "Rebase local branch onto fetched changes");
                                    ui.add_space(4.0);
                                    ui.checkbox(&mut true, "Prune obsolete remote tracked branches");
                                    ui.add_space(4.0);
                                    ui.checkbox(&mut false, "Always fetch new commits, tags and branches from submodule");
                                    ui.checkbox(&mut true, "Update registered submodules");
                                    ui.checkbox(&mut true, "Initialize new submodules");
                                }
                                2 => {
                                    ui.horizontal(|ui| {
                                        ui.label("Submodule handling*:");
                                        ComboBox::from_id_salt("repository.settings.push")
                                            .selected_text("abort if any needs to be pushed")
                                            .show_ui(ui, |ui| {
                                                for label in ["ignore submodules", "abort if any needs to be pushed", "push submodules first"] {
                                                    ui.label(label);
                                                }
                                            });
                                    });
                                    ui.label("* if repository commit references non-pushed submodule commit");
                                }
                                3 => {
                                    let mut program = String::new();
                                    let mut key = String::new();
                                    let field_width = ui.available_width() - 118.0;
                                    egui::Grid::new("repository.settings.signing")
                                        .num_columns(2)
                                        .min_col_width(110.0)
                                        .spacing([8.0, 4.0])
                                        .show(ui, |ui| {
                                            ui.label("GPG Program:");
                                            ui.horizontal(|ui| {
                                                let width = field_width
                                                    - super::controls::icons::BUTTON_SIZE
                                                    - ui.spacing().item_spacing.x;
                                                ui.add(TextEdit::singleline(&mut program).desired_width(width));
                                                super::controls::icons::button(
                                                    ui, super::controls::icons::Icon::Folder,
                                                    "Choose GPG program",
                                                );
                                            });
                                            ui.end_row();
                                            ui.label("Signing Key:");
                                            ui.add(TextEdit::singleline(&mut key)
                                                .desired_width(field_width));
                                            ui.end_row();
                                            ui.label("");
                                            ui.checkbox(&mut false, "Sign all commits");
                                            ui.end_row();
                                        });
                                }
                                4 => {
                                    ui.horizontal(|ui| {
                                        ui.label("Text File Encoding:");
                                        ComboBox::from_id_salt("repository.settings.encoding")
                                            .selected_text("System Default (UTF-8)")
                                            .show_ui(ui, |_| {});
                                    });
                                }
                                _ => {}
                            }
                        });
                    }
                        });
                });
        });
        super::controls::dialog_style::section(
            ui,
            palette.header,
            egui::Margin::symmetric(12, 8),
            |ui| {
                super::controls::dialog_style::button_row(ui, |ui| {
                        let settings = app.manager.settings.clone();
                        if ui
                            .add_enabled(
                                !busy && settings.is_some(),
                                super::controls::dialog_style::primary("OK", palette),
                            )
                            .dialog_accept()
                        {
                            if let Some(settings) = settings {
                                job = Some(Job::SaveSettings(settings));
                            }
                        }
                        if ui.add(super::controls::dialog_style::button("Cancel")).dialog_cancel() {
                            close = true;
                        }
                    },
                );
            },
        );
    });
    if close {
        app.manager.dialog = None;
    }
    if let Some(job) = job {
        app.run_repository_job(job);
    }
}

#[cfg(test)]
mod removal_tests {
    use super::*;

    #[test]
    fn removal_dialog_fits_both_window_sizes_and_escape_cancels() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                let ctx = egui::Context::default();
                crate::theme::install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.repositories = crate::mock_data::repositories();
                let path = app.repositories[0].path.clone();
                app.request_remove_repository(path);
                for _ in 0..4 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ui| remove_repository_dialog(ui.ctx(), &mut app),
                    );
                    output.textures_delta.clear();
                }
                let rect = ctx
                    .memory(|m| m.area_rect(egui::Id::unique("repository.remove")))
                    .unwrap();
                assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect));
                assert!(rect.height() < 320.0);
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        events: vec![egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                        ..Default::default()
                    },
                    |ui| remove_repository_dialog(ui.ctx(), &mut app),
                );
                output.textures_delta.clear();
                assert!(app.manager.dialog.is_none());
                assert!(app.manager.repository_to_remove.is_none());
                assert_eq!(app.repositories.len(), 4);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{install_fonts, ThemeChoice};

    #[test]
    fn settings_dialog_stays_compact_and_stable_when_switching_tabs() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                let ctx = egui::Context::default();
                install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                let mut expected_rect = None;
                // Include the initial loading state and the return to the User tab.
                for tab in [0usize, 0, 1, 2, 3, 4, 0] {
                    ctx.data_mut(|data| {
                        data.insert_temp(egui::Id::unique("repository.settings.tab"), tab);
                    });
                    for frame in 0..4 {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                ..Default::default()
                            },
                            |ui| settings_dialog(ui.ctx(), &mut app),
                        );
                        output.textures_delta.clear();
                        let rect = ctx
                            .memory(|m| m.area_rect(egui::Id::unique("repository.settings")))
                            .unwrap();
                        if let Some(expected) = expected_rect {
                            assert_eq!(rect, expected, "Tab {tab}: {theme:?} {size:?}");
                        } else if frame == 3 {
                            assert!(rect.height() < 390.0, "{rect:?}");
                            assert!(rect.width() <= 664.0, "{rect:?}");
                            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                                .contains_rect(rect));
                            expected_rect = Some(rect);
                        }
                    }
                    app.manager.settings = Some(crate::repository::Settings::default());
                }
            }
        }
    }

    #[test]
    fn settings_labels_fit_one_line_in_both_themes_and_window_sizes() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                for (tab, labels) in [
                    (0usize, ["Full Name:", "Git Email:"]),
                    (3, ["GPG Program:", "Signing Key:"]),
                ] {
                    let ctx = egui::Context::default();
                    install_fonts(&ctx);
                    egui_extras::install_image_loaders(&ctx);
                    theme.apply(&ctx);
                    ctx.data_mut(|data| {
                        data.insert_temp(egui::Id::unique("repository.settings.tab"), tab);
                    });
                    let mut app = SannyGitApp::with_theme(theme);
                    app.manager.settings = Some(crate::repository::Settings::default());
                    for frame in 0..4 {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                ..Default::default()
                            },
                            |ui| settings_dialog(ui.ctx(), &mut app),
                        );
                        output.textures_delta.clear();
                        if frame == 3 {
                            for label in labels {
                                let text = output
                                    .shapes
                                    .iter()
                                    .find_map(|shape| match &shape.shape {
                                        egui::Shape::Text(text)
                                            if text.galley.job.text == label =>
                                        {
                                            Some(text)
                                        }
                                        _ => None,
                                    })
                                    .unwrap_or_else(|| panic!("Missing {label}"));
                                assert_eq!(
                                    text.galley.rows.len(),
                                    1,
                                    "Wrapped {label}: {theme:?} {size:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use crate::theme::{install_fonts, ThemeChoice};

    #[test]
    fn add_worktree_stays_compact_with_long_fields_and_errors_in_both_themes() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                let ctx = egui::Context::default();
                install_fonts(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.manager.dialog = Some(Dialog::AddWorktree);
                app.manager.worktree_branches = vec!["main".into()];
                let mut expected_size = None;
                for long in [false, true, false] {
                    app.manager.worktree_branch = if long {
                        "feature/".repeat(50)
                    } else {
                        "main".into()
                    };
                    app.manager.destination = if long {
                        "C:/work/".repeat(100)
                    } else {
                        "C:/work/project-main".into()
                    };
                    app.error = long.then(|| "Creation failed ".repeat(100));
                    for _ in 0..4 {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                ..Default::default()
                            },
                            |ui| add_worktree_dialog(ui.ctx(), &mut app),
                        );
                        output.textures_delta.clear();
                    }
                    let rect = ctx
                        .memory(|memory| {
                            memory.area_rect(egui::Id::unique("repository.add_worktree"))
                        })
                        .unwrap();
                    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect));
                    assert!(rect.height() < 260.0, "{rect:?}");
                    if let Some(expected) = expected_size {
                        assert_eq!(rect.size(), expected);
                    } else {
                        expected_size = Some(rect.size());
                    }
                }
            }
        }
    }

    #[test]
    fn search_dialog_fits_both_themes_and_keeps_results_geometry_stable() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                let ctx = egui::Context::default();
                install_fonts(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.manager.dialog = Some(Dialog::Search);
                let mut empty_size = None;
                for count in [0, 100, 0] {
                    app.manager.found = (0..count)
                        .map(|i| {
                            (
                                format!("repository-{i}"),
                                format!(
                                    "C:/repositories/{}/repository-{i}",
                                    "long-path/".repeat(20)
                                ),
                                true,
                            )
                        })
                        .collect();
                    app.manager.search_summary = if count == 0 {
                        String::new()
                    } else {
                        "Search complete ".repeat(100)
                    };
                    for _ in 0..4 {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                ..Default::default()
                            },
                            |ui| search_dialog(ui.ctx(), &mut app),
                        );
                        output.textures_delta.clear();
                    }
                    let rect = ctx
                        .memory(|memory| memory.area_rect(egui::Id::unique("repository.search")))
                        .unwrap();
                    assert!(
                        egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect),
                        "{rect:?}"
                    );
                    if let Some(expected) = empty_size {
                        assert_eq!(rect.size(), expected);
                    } else {
                        empty_size = Some(rect.size());
                    }
                }
            }
        }
    }
}
