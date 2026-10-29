use eframe::egui::{CollapsingHeader, RichText, ScrollArea, TextEdit, Ui};
use crate::app::SannyGitApp;
use super::icons::{self, Icon};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    ui.add_space(6.0);
    ui.heading("Branches");
    ui.add(TextEdit::singleline(&mut app.branch_filter).hint_text("Filter branches...").desired_width(f32::INFINITY));
    ui.add_space(4.0);
    let filter = app.branch_filter.to_lowercase();
    ScrollArea::vertical().id_salt("branches").auto_shrink([false, false]).show(ui, |ui| {
        branch_group(ui, app, "Local", &app.local_branches.clone(), &filter);
        branch_group(ui, app, "Remote", &app.remote_branches.clone(), &filter);
        branch_group(ui, app, "Tags", &app.tags.clone(), &filter);
    });
    if !filter.is_empty() && !app.local_branches.iter().chain(&app.remote_branches).chain(&app.tags).any(|branch| branch.to_lowercase().contains(&filter)) {
        ui.label(RichText::new("No matching branches").color(palette.muted));
    }
}

fn branch_group(ui: &mut Ui, app: &mut SannyGitApp, title: &str, branches: &[&'static str], filter: &str) {
    let matching = branches.iter().copied().filter(|branch| branch.to_lowercase().contains(filter)).collect::<Vec<_>>();
    if matching.is_empty() { return; }
    CollapsingHeader::new(format!("{title}  ({})", matching.len())).default_open(true).show(ui, |ui| {
        let palette = app.theme.palette();
        for branch in matching {
            let current = title == "Local" && branch == app.current_branch;
            let icon = if title == "Tags" { Icon::Tag } else { Icon::Branch };
            let label = if current { RichText::new(branch).strong().color(palette.accent) } else { RichText::new(branch).color(palette.text) };
            let response = ui.horizontal(|ui| {
                icons::paint(ui, icon, palette.muted, 13.0);
                ui.selectable_label(app.selected_branch == branch, label)
            });
            if response.inner.clicked() { app.selected_branch = branch.to_owned(); }
        }
    });
}
