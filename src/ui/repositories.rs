use super::icons::{self, Icon};
use crate::{app::SannyGitApp, theme::semibold_family};
use eframe::egui::{Align, Layout, RichText, ScrollArea, Ui, Vec2};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    ui.horizontal(|ui| {
        ui.set_min_height(36.0);
        ui.heading("Repositories");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            icons::unavailable(
                ui,
                Icon::Chevron,
                "Repository management is not implemented yet",
            );
            icons::unavailable(
                ui,
                Icon::Plus,
                "Opening repositories is not implemented yet",
            );
        });
    });
    let height = if super::compact(ui) { 46.0 } else { 56.0 };
    ScrollArea::vertical()
        .id_salt("repositories")
        .max_height(height * 4.0 + 12.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (index, repository) in app.repositories.iter().enumerate() {
                let response =
                    super::selectable_row(ui, app.selected_repository == index, height, |ui| {
                        icons::paint(ui, Icon::Repository, palette.text, 21.0);
                        ui.add_space(6.0);
                        ui.with_layout(Layout::top_down(Align::Min), |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                            ui.label(
                                RichText::new(repository.name)
                                    .size(18.0)
                                    .family(semibold_family()),
                            );
                            ui.label(
                                RichText::new(repository.path)
                                    .size(15.0)
                                    .color(palette.muted),
                            );
                        });
                    })
                    .on_hover_text(repository.path);
                if response.clicked() {
                    app.selected_repository = index;
                }
            }
        });
}
