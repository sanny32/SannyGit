use super::controls::EditableText;
use crate::app::{RepositoryDialog as Dialog, RepositoryJob as Job, SannyGitApp};
use eframe::egui::{self, Button, ComboBox, RichText, ScrollArea, TextEdit, Ui, Window};

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
    if app.busy() && app.manager.cancel.is_some() {
        Window::new("Repository operation")
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
        Dialog::Prune => "Prune Obsolete Worktrees",
        Dialog::Settings => "Repository Settings",
        Dialog::CloseRepository => "Close Repository",
    };
    let mut open = true;
    let mut job = None;
    Window::new(title).id(egui::Id::unique(title)).open(&mut open).default_width(540.0)
        .collapsible(false)
        .frame(super::controls::dialog_style::frame(ui.ctx(), app.theme.palette()).inner_margin(18))
        .max_size(ui.ctx().content_rect().size() - egui::vec2(40.0,40.0))
        .show(ui.ctx(), |ui| {
            super::controls::dialog_style::setup(ui);
            let busy = app.busy();
            ui.add_enabled_ui(!busy, |ui| match dialog {
                Dialog::Group => {
                    ui.label("Group name"); ui.add(TextEdit::singleline(&mut app.manager.group_name).editable().desired_width(f32::INFINITY));
                    if ui.add_enabled(!app.manager.group_name.trim().is_empty(), Button::new("Add Group")).clicked() { app.add_group(); }
                }
                Dialog::Clone => {},
                Dialog::Search => {
                    ui.label("Search directory"); ui.add(TextEdit::singleline(&mut app.manager.search_root).editable().desired_width(f32::INFINITY));
                    ui.horizontal(|ui| { ui.label("Maximum depth"); ui.add(egui::DragValue::new(&mut app.manager.search_depth).range(0..=16)); });
                    if ui.add_enabled(!app.manager.search_root.is_empty(), Button::new("Search")).clicked() {
                        app.manager.found.clear(); app.manager.search_summary.clear();
                        job = Some(Job::Search { root: app.manager.search_root.clone(), depth: app.manager.search_depth });
                    }
                    if !app.manager.search_summary.is_empty() { ui.label(&app.manager.search_summary); }
                    ScrollArea::vertical().id_salt("repository.search.results").max_height(240.0).show(ui, |ui| {
                        for (name,path,selected) in &mut app.manager.found { ui.checkbox(selected, format!("{name} — {path}")); }
                    });
                    group_picker(ui, app);
                    if ui.add_enabled(app.manager.found.iter().any(|r| r.2), Button::new("Add Selected")).clicked() { app.add_found_repositories(); }
                }
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
                    ui.add(TextEdit::singleline(&mut app.manager.destination).editable().desired_width(f32::INFINITY));
                    ui.checkbox(&mut app.manager.worktree_new_branch, "Create a new branch from HEAD");
                    if app.manager.worktree_new_branch {
                        ui.label("New branch name"); ui.add(TextEdit::singleline(&mut app.manager.worktree_branch).editable().desired_width(f32::INFINITY));
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
                Dialog::Settings => {}
                Dialog::CloseRepository => {
                    ui.label("Close this repository and discard its unsaved commit message?");
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() { app.manager.dialog = None; }
                        if ui.button("Close Repository").clicked() { app.close_repository(true); }
                    });
                }
            });
            if let Some(error) = &app.error { ui.label(RichText::new(error).color(app.theme.palette().removed_text)); }
        });
    if !open {
        app.manager.dialog = None;
    }
    if let Some(job) = job {
        app.run_repository_job(job);
    }
}

fn settings_dialog(ctx: &egui::Context, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
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
                        .max_height(210.0)
                        .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.spacing_mut().interact_size.y = 22.0;
                    if tab == 0 {
                        if let Some(settings) = &mut app.manager.settings {
                            ui.add_enabled_ui(!busy, |ui| {
                                egui::Grid::new("repository.settings.identity")
                                    .num_columns(2)
                                    .spacing([8.0, 4.0])
                                    .show(ui, |ui| {
                                        ui.label("Full Name:");
                                        ui.add(TextEdit::singleline(&mut settings.name).editable()
                                            .desired_width(ui.available_width()));
                                        ui.end_row();
                                        ui.label("Git Email:");
                                        ui.add(TextEdit::singleline(&mut settings.email).editable()
                                            .desired_width(ui.available_width()));
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
                                    egui::Grid::new("repository.settings.signing")
                                        .num_columns(2)
                                        .spacing([8.0, 4.0])
                                        .show(ui, |ui| {
                                            ui.label("GPG Program:");
                                            ui.horizontal(|ui| {
                                                let width = (ui.available_width() - 30.0).max(80.0);
                                                ui.add(TextEdit::singleline(&mut program).desired_width(width));
                                                super::controls::icons::button(
                                                    ui, super::controls::icons::Icon::Folder,
                                                    "Choose GPG program",
                                                    egui::vec2(22.0, 22.0),
                                                );
                                            });
                                            ui.end_row();
                                            ui.label("Signing Key:");
                                            ui.add(TextEdit::singleline(&mut key)
                                                .desired_width(ui.available_width()));
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
            if let Some(error) = &app.error {
                ui.add(egui::Label::new(RichText::new(error).color(palette.removed_text)).wrap());
            }
        });
        super::controls::dialog_style::section(
            ui,
            palette.header,
            egui::Margin::symmetric(12, 8),
            |ui| {
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let settings = app.manager.settings.clone();
                        if ui
                            .add_enabled(
                                !busy && settings.is_some(),
                                super::controls::dialog_style::primary("OK", palette),
                            )
                            .clicked()
                        {
                            if let Some(settings) = settings {
                                job = Some(Job::SaveSettings(settings));
                            }
                        }
                        if ui.add(super::controls::dialog_style::button("Cancel")).clicked() {
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
