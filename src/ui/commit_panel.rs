use crate::{app::SannyGitApp, theme::semibold_family};
use eframe::egui::{
    Align, Align2, Button, FontId, Frame, Layout, Margin, RichText, Sense, TextEdit, Ui, Vec2,
};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    ui.spacing_mut().item_spacing.y = 0.0;
    super::header(ui, 28.0, palette, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(60.0, 28.0), Sense::hover());
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Commit",
            FontId::new(13.0, semibold_family()),
            palette.text,
        );
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            (3.0, palette.accent),
        );
    });
    Frame::new()
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 6,
            bottom: 6,
        })
        .show(ui, |ui| {
            let width = ui.available_width();
            let editor_height = (ui.available_height() - 38.0).max(32.0);
            ui.add_sized(
                [width, editor_height],
                TextEdit::multiline(&mut app.commit_message)
                    .hint_text(RichText::new("Write a commit message...").color(palette.muted))
                    .desired_rows(1)
                    .desired_width(f32::INFINITY)
                    .margin(Vec2::new(6.0, 4.0))
                    .background_color(palette.editor),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_enabled(
                    false,
                    eframe::egui::Checkbox::new(&mut app.amend, "Amend last commit"),
                )
                .on_disabled_hover_text("Amending commits is not implemented yet");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_enabled(false, Button::new("Commit").min_size(Vec2::new(72.0, 24.0)))
                        .on_disabled_hover_text("Repository integration is not implemented yet");
                });
            });
        });
}
