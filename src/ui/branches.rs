use super::icons::{self, Icon};
use crate::{app::SannyGitApp, theme::semibold_family};
use eframe::egui::{Align, CollapsingHeader, Id, Layout, RichText, ScrollArea, TextEdit, Ui, Vec2};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    ui.horizontal(|ui| {
        ui.set_min_height(26.0);
        ui.heading("Branches");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            icons::unavailable(
                ui,
                Icon::Chevron,
                "Branch management is not implemented yet",
            );
            if icons::button(ui, Icon::Search, "Filter branches", Vec2::splat(20.0)).clicked() {
                ui.memory_mut(|m| m.request_focus(Id::unique("branch_filter")));
            }
            icons::unavailable(ui, Icon::Plus, "Creating branches is not implemented yet");
        });
    });
    ui.add(
        TextEdit::singleline(&mut app.branch_filter)
            .id(Id::unique("branch_filter"))
            .hint_text(RichText::new("Filter branches...").color(palette.muted))
            .margin(Vec2::new(5.0, 3.0))
            .desired_width(f32::INFINITY),
    );
    ui.add_space(3.0);
    let filter = app.branch_filter.to_lowercase();
    ScrollArea::vertical()
        .id_salt("branches")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            branch_group(ui, app, "Local", &app.local_branches.clone(), &filter);
            ui.add_space(4.0);
            branch_group(ui, app, "Remote", &app.remote_branches.clone(), &filter);
            ui.add_space(4.0);
            branch_group(ui, app, "Tags", &app.tags.clone(), &filter);
            if !filter.is_empty()
                && !app
                    .local_branches
                    .iter()
                    .chain(&app.remote_branches)
                    .chain(&app.tags)
                    .any(|b| b.to_lowercase().contains(&filter))
            {
                ui.label(RichText::new("No matching branches").color(palette.muted));
            }
        });
}

fn branch_group(
    ui: &mut Ui,
    app: &mut SannyGitApp,
    title: &str,
    branches: &[&'static str],
    filter: &str,
) {
    let matching = branches
        .iter()
        .copied()
        .filter(|b| b.to_lowercase().contains(filter))
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return;
    }
    let height = 22.0;
    CollapsingHeader::new(title)
        .default_open(title != "Tags")
        .show_unindented(ui, |ui| {
            let palette = app.theme.palette();
            for branch in matching {
                let current = title == "Local" && branch == app.current_branch;
                let text = if current {
                    RichText::new(branch).family(semibold_family())
                } else {
                    RichText::new(branch)
                };
                let response =
                    super::selectable_row(ui, app.selected_branch == branch, height, |ui| {
                        ui.add_space(8.0);
                        icons::paint(
                            ui,
                            if title == "Tags" {
                                Icon::Tag
                            } else {
                                Icon::Branch
                            },
                            palette.text,
                            14.0,
                        );
                        ui.add_space(2.0);
                        ui.label(text);
                        if current {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                icons::paint(ui, Icon::Star, palette.accent, 14.0)
                            });
                        }
                    });
                if response.clicked() {
                    app.selected_branch = branch.to_owned();
                }
            }
        });
}
