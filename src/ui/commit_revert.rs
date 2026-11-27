use crate::{
    app::SannyGitApp,
    ui::controls::{
        dialog_style::{self, DialogResponse},
        tooltips::TooltipResponse,
    },
};
use eframe::egui::{self, Button, RichText};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let Some(dialog) = app.revert_merge_dialog.clone() else {
        return;
    };
    let palette = app.palette();
    let mut cancel = false;
    let mut parent: Option<u8> = None;
    dialog_style::modal(ctx, palette, "revert.merge").show(ctx, |ui| {
        ui.set_width(520.0);
        dialog_style::setup(ui);
        dialog_style::header_with_icon(
            ui,
            palette,
            "Revert merge commit",
            "Choose the parent to keep when reverting a merge commit.",
            crate::ui::controls::icons::Icon::Warning,
        );
        dialog_style::section(ui, palette.panel, egui::Margin::symmetric(12, 10), |ui| {
            ui.label(
                RichText::new(crate::i18n::tr_args(
                    "Commit %{commit}",
                    &[("commit", &dialog.commit[..12.min(dialog.commit.len())])],
                ))
                .small(),
            );
            for (index, hash) in dialog.parents.iter().take(2).enumerate() {
                let label = format!("Parent {} — {}", index + 1, &hash[..8.min(hash.len())]);
                if ui
                    .add(Button::new(label))
                    .help_text(crate::i18n::tr_msg("Revert relative to this parent"))
                    .clicked()
                {
                    parent = Some((index + 1) as u8);
                }
            }
            dialog_style::button_row(ui, |ui| {
                cancel = ui.add(dialog_style::button("Cancel")).dialog_cancel();
            });
        });
    });
    if cancel {
        app.revert_merge_dialog = None;
    } else if let Some(parent) = parent {
        app.confirm_revert_merge_parent(parent);
    }
}
