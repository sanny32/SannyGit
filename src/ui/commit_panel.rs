use super::icons::{self, Icon};
use crate::{app::SannyGitApp, theme::semibold_family};
use eframe::egui::{
    Align, Align2, Button, Color32, FontId, Frame, Layout, Margin, RichText, Sense, TextEdit, Ui,
    Vec2,
};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    ui.spacing_mut().item_spacing.y = 0.0;
    super::header(ui, 44.0, palette, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(84.0, 44.0), Sense::hover());
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Commit",
            FontId::new(17.0, semibold_family()),
            palette.text,
        );
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            (3.0, palette.accent),
        );
        ui.add_enabled(false, Button::new("Amend").frame(false))
            .on_disabled_hover_text("Amending commits is not implemented yet");
    });
    Frame::new()
        .inner_margin(Margin {
            left: 16,
            right: 16,
            top: 14,
            bottom: 14,
        })
        .show(ui, |ui| {
            let width = ui.available_width();
            let editor_height = (ui.available_height() - 92.0).max(40.0);
            ui.add_sized(
                [width, editor_height],
                TextEdit::multiline(&mut app.commit_message)
                    .hint_text(RichText::new("Write a commit message...").color(palette.muted))
                    .desired_rows(1)
                    .desired_width(f32::INFINITY)
                    .margin(Vec2::new(12.0, 10.0))
                    .background_color(palette.editor),
            );
            ui.add_space(8.0);
            let checkbox_font = if width < 380.0 { 13.0 } else { 14.0 };
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                ui.add_enabled_ui(false, |ui| {
                    ui.checkbox(
                        &mut app.amend,
                        RichText::new("Amend previous commit").size(checkbox_font),
                    );
                    ui.checkbox(
                        &mut app.push_after_commit,
                        RichText::new("Push after commit").size(checkbox_font),
                    );
                })
                .response
                .on_disabled_hover_text("Repository integration is not implemented yet");
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                let branch_width = (width - 144.0).max(100.0);
                Frame::new()
                    .fill(palette.editor)
                    .stroke((1.0, palette.border))
                    .corner_radius(4)
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_width(branch_width - 24.0);
                        ui.horizontal(|ui| {
                            icons::paint(ui, Icon::Branch, palette.text, 20.0);
                            ui.label(RichText::new(app.current_branch).family(semibold_family()));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                icons::paint(ui, Icon::Chevron, palette.muted, 16.0)
                            });
                        });
                    });
                ui.add_enabled(
                    false,
                    Button::image_and_text(
                        icons::image(Icon::Check, Color32::WHITE, 20.0),
                        RichText::new("Commit")
                            .family(semibold_family())
                            .color(Color32::WHITE),
                    )
                    .fill(palette.accent)
                    .min_size(Vec2::new(128.0, 42.0)),
                )
                .on_disabled_hover_text("Repository integration is not implemented yet");
            });
        });
}
