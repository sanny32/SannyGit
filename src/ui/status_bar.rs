use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use crate::app::SannyGitApp;
use eframe::egui::{Align, Frame, Label, Layout, Margin, Panel, RichText, Ui};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    let status = if app.tag_patch_pending() {
        "Exporting patch…"
    } else {
        &app.status
    };
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
                ui.label(if app.branches.current_branch.is_empty() {
                    "No repository"
                } else {
                    &app.branches.current_branch
                });
                let badge = app.tracking.badge();
                if !badge.is_empty() {
                    ui.add(
                        eframe::egui::Label::new(super::sync_badge(&badge, palette))
                            .selectable(false),
                    )
                    .help_text(app.tracking.description());
                }
                ui.separator();
                ui.label(format!("{} changes", app.changes.len()));
                ui.separator();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(repository) = app.repositories.get(app.selected_repository) {
                        ui.scope(|ui| {
                            ui.set_max_width(ui.available_width() * 0.45);
                            ui.add(Label::new(&repository.path).truncate());
                        });
                        icons::paint(ui, Icon::Folder, palette.text, 14.0);
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(
                            Label::new(RichText::new(status).color(palette.muted))
                                .truncate()
                                .show_tooltip_when_elided(false),
                        )
                        .help_text(status);
                    });
                });
            });
        });
}
