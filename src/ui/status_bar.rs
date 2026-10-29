use super::icons::{self, Icon};
use crate::app::SannyGitApp;
use eframe::egui::{Align, Context, Frame, Layout, Margin, RichText, TopBottomPanel};

pub fn show(context: &Context, app: &SannyGitApp) {
    let palette = app.theme.palette();
    TopBottomPanel::bottom("working_tree.v2.status")
        .exact_height(if context.screen_rect().height() < 850.0 {
            40.0
        } else {
            54.0
        })
        .frame(
            Frame::new()
                .fill(palette.background)
                .stroke((1.0, palette.border))
                .inner_margin(Margin::symmetric(24, 0)),
        )
        .show(context, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                icons::paint(ui, Icon::Branch, palette.text, 18.0);
                ui.label(app.current_branch);
                icons::paint(ui, Icon::Chevron, palette.text, 14.0);
                ui.separator();
                icons::paint(ui, Icon::Check, palette.text, 18.0);
                ui.label(format!("{} changes", app.changes.len()));
                ui.separator();
                icons::paint(ui, Icon::Fetch, palette.text, 18.0);
                ui.label(format!("↑ {} ↓ {}", app.ahead, app.behind));
                ui.separator();
                ui.label(RichText::new("Demo data").size(14.0).color(palette.muted));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("origin/main").size(14.0));
                    icons::paint(ui, Icon::Cloud, palette.text, 20.0);
                    ui.add_space(14.0);
                    if let Some(repository) = app.repositories.get(app.selected_repository) {
                        ui.label(RichText::new(repository.path).size(14.0));
                        icons::paint(ui, Icon::Folder, palette.text, 18.0);
                    }
                });
            });
        });
}
