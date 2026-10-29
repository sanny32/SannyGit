use eframe::egui::{RichText, ScrollArea, Ui};

use crate::app::SannyGitApp;

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading("Repositories");
        ui.label(RichText::new(app.repositories.len().to_string()).small().color(palette.muted));
    });
    ui.add_space(4.0);

    ScrollArea::vertical()
        .id_salt("repositories")
        .max_height(174.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (index, repository) in app.repositories.iter().enumerate() {
                let response = ui
                    .selectable_label(
                        app.selected_repository == index,
                        RichText::new(format!("▣  {}", repository.name)).strong(),
                    )
                    .on_hover_text(repository.path);
                if response.clicked() {
                    app.selected_repository = index;
                }
                ui.label(RichText::new(repository.path).small().color(palette.muted));
                ui.add_space(5.0);
            }
        });
}
