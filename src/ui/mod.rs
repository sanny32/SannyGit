mod branches;
mod changes;
mod commit_panel;
mod diff_view;
mod icons;
mod journal;
mod repositories;
mod status_bar;
mod toolbar;

use eframe::egui::{CentralPanel, Context, SidePanel, TopBottomPanel};

use crate::app::SannyGitApp;

pub fn show(context: &Context, app: &mut SannyGitApp) {
    toolbar::show(context, app);
    status_bar::show(context, app);
    SidePanel::left("repositories_and_branches")
        .resizable(true)
        .default_width(210.0)
        .width_range(170.0..=340.0)
        .show(context, |ui| {
            repositories::show(ui, app);
            ui.separator();
            branches::show(ui, app);
        });
    TopBottomPanel::bottom("commit_and_journal")
        .resizable(true)
        .default_height(230.0)
        .height_range(160.0..=420.0)
        .show(context, |ui| {
            SidePanel::right("journal")
                .resizable(true)
                .default_width(ui.available_width() * 0.58)
                .width_range(320.0..=900.0)
                .show_inside(ui, |ui| journal::show(ui, app));
            CentralPanel::default().show_inside(ui, |ui| commit_panel::show(ui, app));
        });
    CentralPanel::default().show(context, |ui| {
        SidePanel::right("diff")
            .resizable(true)
            .default_width(560.0)
            .width_range(300.0..=1000.0)
            .show_inside(ui, |ui| diff_view::show(ui, app));
        CentralPanel::default().show_inside(ui, |ui| changes::show(ui, app));
    });
}
