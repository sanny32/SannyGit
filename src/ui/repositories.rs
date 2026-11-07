use super::icons::{self, Icon};
use crate::{app::SannyGitApp, theme::semibold_family};
use eframe::egui::{Align, Button, Layout, RichText, ScrollArea, TextEdit, Ui, Window};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    let mut open = app.open_dialog;
    Window::new("Open repository")
        .open(&mut open)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.label("Path to a local Git working tree (or a folder inside it)");
            ui.add(
                TextEdit::singleline(&mut app.repository_path)
                    .desired_width(420.0)
                    .hint_text("Repository directory"),
            );
            let enabled = !app.busy() && !app.repository_path.is_empty();
            if ui.add_enabled(enabled, Button::new("Open")).clicked() {
                app.open_repository(app.repository_path.clone());
            }
        });
    app.open_dialog = open;
    ui.horizontal(|ui| {
        ui.set_min_height(26.0);
        ui.heading("Repositories");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            icons::unavailable(
                ui,
                Icon::Chevron,
                "Repository management is not implemented yet",
            );
            if icons::button(
                ui,
                Icon::Plus,
                "Open repository",
                eframe::egui::Vec2::splat(20.0),
            )
            .clicked()
            {
                app.open_dialog = true;
            }
        });
    });
    let height = 24.0;
    let mut requested = None;
    ScrollArea::vertical()
        .id_salt("repositories")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if app.repositories.is_empty() {
                ui.label(RichText::new("Use + to open a local repository").color(palette.muted));
            }
            for (index, repository) in app.repositories.iter().enumerate() {
                let response =
                    super::selectable_row(ui, app.selected_repository == index, height, |ui| {
                        icons::paint(ui, Icon::Repository, palette.text, 16.0);
                        ui.label(RichText::new(&repository.name).family(semibold_family()));
                    })
                    .on_hover_text(&repository.path);
                if response.clicked() {
                    requested = Some(repository.path.clone());
                }
            }
        });
    if let Some(path) = requested {
        app.open_repository(path);
    }
}
