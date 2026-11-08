use super::tooltips::TooltipResponse;
use super::EditableText;
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
        let staged = app.changes.iter().filter(|c| c.staged).count();
        ui.label(
            RichText::new(format!("{staged} staged"))
                .small()
                .color(palette.muted),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            eframe::egui::containers::menu::MenuButton::from_button(
                Button::image(super::icons::image(
                    super::icons::Icon::Chevron,
                    palette.muted,
                    14.0,
                ))
                .frame(false)
                .min_size(Vec2::new(20.0, 26.0)),
            )
            .ui(ui, |ui| {
                let blocker = app.generate_commit_message_blocker();
                if ui
                    .add_enabled(blocker.is_none(), Button::new("Generate commit message"))
                    .disabled_help_text(blocker.unwrap_or(""))
                    .clicked()
                {
                    app.generate_commit_message();
                    ui.close();
                }
                if ui.button("LLM settings…").clicked() {
                    app.open_settings();
                    app.settings_window.page = crate::app::SettingsPage::Llm;
                    ui.close();
                }
            })
            .0
            .help_text("Commit message generation options");

            let blocker = app.generate_commit_message_blocker();
            if ui
                .add_enabled(
                    blocker.is_none(),
                    Button::image(super::icons::image(
                        super::icons::Icon::Sparkles,
                        palette.muted,
                        20.0,
                    ))
                    .frame(false)
                    .min_size(Vec2::new(26.0, 26.0)),
                )
                .help_text("Generate a commit message from all staged changes")
                .disabled_help_text(blocker.unwrap_or(""))
                .clicked()
            {
                app.generate_commit_message();
            }
        });
    });
    Frame::new()
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 6,
            bottom: 16,
        })
        .show(ui, |ui| {
            let width = ui.available_width();
            let editor_height = (ui.available_height() - 38.0).max(32.0);
            let busy = app.busy();
            ui.add_enabled_ui(!busy, |ui| {
                ui.add_sized(
                    [width, editor_height],
                    TextEdit::multiline(&mut app.commit_message)
                        .editable()
                        .id(eframe::egui::Id::unique("commit.message"))
                        .desired_rows(1)
                        .desired_width(f32::INFINITY)
                        .margin(Vec2::new(6.0, 4.0)),
                );
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_enabled(
                    false,
                    eframe::egui::Checkbox::new(&mut app.amend, "Amend last commit"),
                )
                .disabled_help_text("Amending commits is not implemented yet");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let blocker = app.commit_blocker();
                    if ui
                        .add_enabled(
                            blocker.is_none(),
                            Button::new("Commit").min_size(Vec2::new(72.0, 24.0)),
                        )
                        .help_text("Commit staged changes and all tracked working-tree changes")
                        .disabled_help_text(blocker.unwrap_or(""))
                        .clicked()
                    {
                        app.create_commit();
                    }
                });
            });
        });
}
