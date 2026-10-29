use eframe::egui::{Align, Context, Layout, RichText, TopBottomPanel};
use crate::app::SannyGitApp;
use super::icons::{self, Icon};

pub fn show(context: &Context, app: &SannyGitApp) {
    let palette = app.theme.palette();
    TopBottomPanel::bottom("status_bar").exact_height(28.0).show(context, |ui| {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            icons::paint(ui, Icon::Branch, palette.text, 14.0);
            ui.label(RichText::new(app.current_branch).strong());
            ui.separator();
            ui.label(format!("{} changes", app.changes.len()));
            ui.separator();
            ui.label(format!("Ahead {}  Behind {}", app.ahead, app.behind));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                if let Some(repository) = app.repositories.get(app.selected_repository) {
                    ui.label(RichText::new(repository.path).small().color(palette.muted));
                }
            });
        });
    });
}
