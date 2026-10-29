use eframe::egui::{
    text::LayoutJob, FontId, RichText, ScrollArea, SelectableLabel, TextFormat, Ui,
};

use crate::app::SannyGitApp;

use super::icons::{self, Icon};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading("Repositories");
        ui.label(
            RichText::new(app.repositories.len().to_string())
                .small()
                .color(palette.muted),
        );
    });
    ui.add_space(4.0);

    ScrollArea::vertical()
        .id_salt("repositories")
        .max_height(190.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (index, repository) in app.repositories.iter().enumerate() {
                let selected = app.selected_repository == index;
                let mut text = LayoutJob::default();
                text.append(
                    repository.name,
                    0.0,
                    TextFormat {
                        font_id: FontId::proportional(13.0),
                        color: palette.text,
                        ..Default::default()
                    },
                );
                text.append(
                    "\n",
                    0.0,
                    TextFormat {
                        font_id: FontId::proportional(4.0),
                        color: palette.muted,
                        ..Default::default()
                    },
                );
                text.append(
                    repository.path,
                    0.0,
                    TextFormat {
                        font_id: FontId::proportional(10.0),
                        color: palette.muted,
                        ..Default::default()
                    },
                );
                let response = ui.horizontal(|ui| {
                    icons::paint(ui, Icon::Repository, palette.muted, 15.0);
                    ui.add_sized(
                        [ui.available_width(), 36.0],
                        SelectableLabel::new(selected, text),
                    )
                    .on_hover_text(repository.path)
                });
                if response.inner.clicked() {
                    app.selected_repository = index;
                }
                ui.add_space(3.0);
            }
        });
}
