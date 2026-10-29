use eframe::egui::{Align, Button, Layout, RichText, TextEdit, Ui};

use crate::app::SannyGitApp;

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    ui.add_space(6.0);
    ui.heading("Commit");
    ui.add_space(4.0);
    TextEdit::multiline(&mut app.commit_message)
        .hint_text("Write a commit message...")
        .desired_width(f32::INFINITY)
        .desired_rows(5)
        .show(ui);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_enabled_ui(false, |ui| {
            ui.checkbox(&mut app.amend, "Amend previous commit");
            ui.checkbox(&mut app.push_after_commit, "Push after commit");
        });
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("⑂  {}", app.current_branch)).color(palette.text));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_enabled(false, Button::new("✓  Commit"))
                .on_disabled_hover_text("Repository integration is not implemented yet");
        });
    });
}
