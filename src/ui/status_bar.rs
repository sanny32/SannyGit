use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use crate::app::SannyGitApp;
use eframe::egui::{Align, Frame, Layout, Margin, Panel, RichText, Ui};

pub fn show(ui: &mut Ui, app: &SannyGitApp) {
    let palette = app.palette();
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
                ui.label(if app.current_branch.is_empty() {
                    "No repository"
                } else {
                    &app.current_branch
                });
                let badge = app.tracking.badge();
                if !badge.is_empty() {
                    ui.add(
                        eframe::egui::Label::new(super::sync_badge(&badge, palette, 13.0))
                            .selectable(false),
                    )
                    .help_text(app.tracking.description());
                }
                ui.separator();
                ui.label(format!("{} changes", app.changes.len()));
                ui.separator();
                ui.add(
                    eframe::egui::Label::new(
                        RichText::new(&app.status).size(12.0).color(palette.muted),
                    )
                    .truncate(),
                )
                .help_text(&app.status);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(repository) = app.repositories.get(app.selected_repository) {
                        ui.label(RichText::new(&repository.path).size(12.0));
                        icons::paint(ui, Icon::Folder, palette.text, 14.0);
                    }
                });
            });
        });
}
