use super::icons::{self, Icon};
use crate::app::SannyGitApp;
use eframe::egui::{Align, Frame, Layout, Margin, Panel, RichText, Ui};

pub fn show(ui: &mut Ui, app: &SannyGitApp) {
    let palette = app.theme.palette();
    Panel::bottom("working_tree.v2.status")
        .exact_size(26.0)
        .frame(
            Frame::new()
                .fill(palette.background)
                .stroke((1.0, palette.border))
                .inner_margin(Margin::symmetric(8, 0)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                icons::paint(ui, Icon::Branch, palette.text, 14.0);
                ui.label(app.current_branch);
                icons::paint(ui, Icon::Chevron, palette.text, 12.0);
                ui.separator();
                icons::paint(ui, Icon::Check, palette.text, 14.0);
                ui.label(format!("{} changes", app.changes.len()));
                ui.separator();
                icons::paint(ui, Icon::Fetch, palette.text, 14.0);
                ui.label(format!("↑ {} ↓ {}", app.ahead, app.behind));
                ui.separator();
                ui.label(RichText::new("Demo data").size(12.0).color(palette.muted));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("origin/main").size(12.0));
                    icons::paint(ui, Icon::Cloud, palette.text, 14.0);
                    ui.add_space(6.0);
                    if let Some(repository) = app.repositories.get(app.selected_repository) {
                        ui.label(RichText::new(repository.path).size(12.0));
                        icons::paint(ui, Icon::Folder, palette.text, 14.0);
                    }
                });
            });
        });
}
