use crate::{app::SannyGitApp, remotes::Operation};
use eframe::egui::{self, Align, ComboBox, Layout, RichText, TextEdit};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let Some(mut dialog) = app.network_dialog.take() else {
        return;
    };
    let palette = app.theme.palette();
    let busy = app.busy();
    let mut submit = false;
    let mut keep = true;
    super::dialog_style::modal(ctx, palette, "remote.operation").show(ctx, |ui| {
        ui.set_width(580.0);
        super::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        let description = match dialog.operation {
            Operation::Fetch => "Download remote branches and tags without changing your working tree.",
            Operation::Pull => "Fetch a remote branch and fast-forward the current local branch. Local changes must be committed or moved aside.",
            Operation::Push => "Push the current local branch to the selected remote branch. Force pushes and tag pushes are not used.",
        };
        super::dialog_style::header(ui, palette, dialog.operation.label(), description);
        super::dialog_style::section(ui, palette.header, egui::Margin::same(18), |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            if let Some(state) = &dialog.state {
                if state.remotes.is_empty() { ui.label("No configured remotes. Add one through Repository → Edit Git Config, then reopen this dialog."); }
                ui.add_enabled_ui(!busy, |ui| {
                    egui::Grid::new("remote.fields").min_col_width(110.0).spacing([12.0,12.0]).show(ui, |ui| {
                        ui.label("Remote:");
                        ComboBox::from_id_salt("remote.name").width(310.0).selected_text(&dialog.remote).show_ui(ui, |ui| {
                            for remote in &state.remotes { ui.selectable_value(&mut dialog.remote, remote.clone(), remote); }
                        }); ui.end_row();
                        if dialog.operation != Operation::Fetch {
                            ui.label("Local branch:"); ui.label(state.branch.as_deref().unwrap_or("No local branch with commits checked out")); ui.end_row();
                            ui.label("Remote branch:"); ui.add_sized([310.0,28.0], TextEdit::singleline(&mut dialog.branch)); ui.end_row();
                        }
                    });
                });
            } else { ui.spinner(); ui.label("Loading remotes…"); }
            if busy && dialog.state.is_some() { ui.horizontal(|ui| { ui.spinner(); ui.label(&app.status); }); }
            if let Some(error) = &app.error { ui.add(egui::Label::new(RichText::new(error).color(palette.removed_text)).wrap()); }
        });
        super::dialog_style::section(ui, palette.header, egui::Margin::symmetric(18,12), |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let valid = dialog.state.as_ref().is_some_and(|state| state.remotes.contains(&dialog.remote)
                    && (dialog.operation == Operation::Fetch || (state.branch.is_some() && crate::branches::name_hint(&dialog.branch).is_none())));
                if ui.add_enabled(!busy && valid && app.state_current, super::dialog_style::primary(dialog.operation.label(), palette)).clicked() { submit = true; }
                if ui.add(super::dialog_style::button("Cancel")).clicked() {
                    if busy && app.manager.cancel.is_some() { app.cancel_repository_job(); } else { keep = false; }
                }
            });
        });
    });
    if keep {
        app.network_dialog = Some(dialog);
    }
    if submit {
        app.run_remote();
    }
}
