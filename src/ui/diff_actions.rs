//! Confirmation for discarding diff lines, and the rename-file dialog.
use super::controls::dialog_style::{self, DialogResponse};
use super::controls::tooltips::TooltipResponse;
use crate::app::SannyGitApp;
use eframe::egui::{self, RichText};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    show_discard(ctx, app);
    show_rename(ctx, app);
}

fn show_discard(ctx: &egui::Context, app: &mut SannyGitApp) {
    let Some(request) = app.diff_discard.clone() else {
        return;
    };
    let palette = app.palette();
    let busy = app.busy();
    let mut confirm = false;
    let mut cancel = false;
    dialog_style::modal(ctx, palette, "diff.line_discard").show(ctx, |ui| {
        ui.set_width(480.0);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header(
            ui,
            palette,
            "Discard lines",
            "The selected lines are removed from the working tree. Other changes in the file stay.",
        );
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            let summary = request.summary.clone();
            dialog_style::text_line(ui, RichText::new(&summary), 44.0).help_text(&summary);
        });
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 12), |ui| {
            dialog_style::button_row(ui, |ui| {
                confirm = ui
                    .add_enabled(!busy, dialog_style::primary("Discard", palette))
                    .dialog_accept();
                cancel = ui
                    .add_enabled(!busy, dialog_style::button("Cancel"))
                    .dialog_cancel();
            });
        });
    });
    if cancel {
        app.diff_discard = None;
    } else if confirm {
        app.confirm_diff_discard();
    }
}

fn show_rename(ctx: &egui::Context, app: &mut SannyGitApp) {
    let Some(mut dialog) = app.rename_dialog.take() else {
        return;
    };
    let palette = app.palette();
    let busy = app.busy();
    let mut confirm = false;
    let mut cancel = false;
    dialog_style::modal(ctx, palette, "diff.rename").show(ctx, |ui| {
        ui.set_width(480.0);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header(
            ui,
            palette,
            "Rename file",
            "Move the file inside this repository. The destination directory must already exist.",
        );
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            let from = format!("From: {}", dialog.from);
            dialog_style::text_line(ui, RichText::new(&from).monospace(), 26.0).help_text(&from);
            ui.label("To");
            ui.add_enabled(
                !busy,
                crate::ui::controls::singleline(&mut dialog.to).desired_width(f32::INFINITY),
            );
            if let Some(error) = &dialog.error {
                dialog_style::text_line(ui, RichText::new(error).color(palette.removed_text), 26.0)
                    .help_text(error);
            }
        });
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 12), |ui| {
            dialog_style::button_row(ui, |ui| {
                let ready =
                    !busy && crate::app::diff_actions::destination_error(&dialog.to).is_none();
                confirm = ui
                    .add_enabled(ready, dialog_style::primary("Rename", palette))
                    .disabled_help_text("Enter a different relative path inside the repository.")
                    .dialog_accept();
                cancel = ui
                    .add_enabled(!busy, dialog_style::button("Cancel"))
                    .dialog_cancel();
            });
        });
    });
    if cancel {
        return;
    }
    app.rename_dialog = Some(dialog);
    if confirm {
        app.confirm_rename_path();
    }
}
