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
        .max_size(ui.ctx().content_rect().size() - egui::vec2(40.0,40.0))
        .show(ui.ctx(), |ui| {
            let busy = app.busy();
            ui.add_enabled_ui(!busy, |ui| match dialog {
                Dialog::Group => {
                    ui.label("Group name"); ui.add(TextEdit::singleline(&mut app.manager.group_name).desired_width(f32::INFINITY));
                    if ui.add_enabled(!app.manager.group_name.trim().is_empty(), Button::new("Add Group")).clicked() { app.add_group(); }
                }
                Dialog::Clone => {
                    ui.label("Repository URL or local path"); ui.add(TextEdit::singleline(&mut app.manager.clone_source).desired_width(f32::INFINITY));
                    ui.label("Destination (new directory)"); ui.add(TextEdit::singleline(&mut app.manager.destination).desired_width(f32::INFINITY));
                    group_picker(ui, app);
                    if ui.add_enabled(!app.manager.clone_source.is_empty() && !app.manager.destination.is_empty(), Button::new("Clone")).clicked() {
                        job = Some(Job::Clone { source: app.manager.clone_source.clone(), destination: app.manager.destination.clone() });
                    }
                }
                Dialog::Search => {
                    ui.label("Search directory"); ui.add(TextEdit::singleline(&mut app.manager.search_root).desired_width(f32::INFINITY));
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
                Dialog::Settings => {
                    ui.label("Repository-local Git identity (empty fields use inherited Git settings)");
                    if let Some(settings) = &mut app.manager.settings {
                        ui.label("user.name"); ui.add(TextEdit::singleline(&mut settings.name).desired_width(f32::INFINITY));
                        ui.label("user.email"); ui.add(TextEdit::singleline(&mut settings.email).desired_width(f32::INFINITY));
                        if ui.button("Save Repository Settings").clicked() { job = Some(Job::SaveSettings(settings.clone())); }
                    }
                }
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
