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
        if dialog.operation == Operation::Push && dialog.state.is_some() {
            push_confirmation(ui, app, &mut dialog, busy, &mut submit, &mut keep);
            return;
        }
        let description = match dialog.operation {
            Operation::Fetch => "Download remote branches and tags without changing your working tree.",
            Operation::Pull => "Fetch a remote branch and fast-forward the current local branch. Local changes must be committed or moved aside.",
            Operation::Push => "Push the current local branch to the selected remote branch. Force pushes and tag pushes are not used.",
        };
        super::dialog_style::header(ui, palette, dialog.operation.label(), description);
        super::dialog_style::section(ui, palette.header, egui::Margin::same(18), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            if let Some(state) = &dialog.state {
                if state.remotes.is_empty() { ui.label("No configured remotes. Add one through Repository → Edit Git Config, then reopen this dialog."); }
                ui.add_enabled_ui(!busy, |ui| {
                    egui::Grid::new("remote.fields").min_col_width(110.0).spacing([12.0,6.0]).show(ui, |ui| {
                        ui.label("Remote:");
                        ComboBox::from_id_salt("remote.name").width(310.0).selected_text(&dialog.remote).show_ui(ui, |ui| {
                            for remote in &state.remotes { ui.selectable_value(&mut dialog.remote, remote.clone(), remote); }
                        }); ui.end_row();
                        if dialog.operation != Operation::Fetch {
                            ui.label("Local branch:"); ui.label(state.branch.as_deref().unwrap_or("No local branch with commits checked out")); ui.end_row();
                            ui.label("Remote branch:"); ui.add_sized([310.0,28.0], TextEdit::singleline(&mut dialog.branch)); ui.end_row();
                        }
                    });
                    if dialog.operation == Operation::Push { ui.checkbox(&mut dialog.set_upstream, "Set as upstream (track this remote branch)"); }
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

fn push_confirmation(
    ui: &mut egui::Ui,
    app: &mut SannyGitApp,
    dialog: &mut crate::app::RemoteDialog,
    busy: bool,
    submit: &mut bool,
    keep: &mut bool,
) {
    let palette = app.theme.palette();
    let Some(state) = dialog.state.clone() else {
        return;
    };
    super::dialog_style::section(ui, palette.header, egui::Margin::same(20), |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(24.0, 14.0);
        ui.horizontal_top(|ui| {
            ui.add(super::icons::image(
                super::icons::Icon::Push,
                palette.accent,
                56.0,
            ));
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                let branch = state.branch.as_deref().unwrap_or("—");
                ui.label(
                    RichText::new(format!("Do you want to push branch '{branch}'?"))
                        .size(16.0)
                        .family(crate::theme::semibold_family()),
                );
                ui.add_space(6.0);
                if dialog.remote.is_empty() {
                    ui.label("No configured remotes.");
                } else {
                    ui.label(format!("1 branch will be pushed to '{}'.", dialog.remote));
                    if dialog.branch != branch {
                        ui.label(format!("Remote branch: {}", dialog.branch));
                    }
                }
                ui.add_enabled_ui(!busy, |ui| {
                    egui::CollapsingHeader::new("Options").show(ui, |ui| {
                        egui::Grid::new("push.options")
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                ui.label("Remote:");
                                ComboBox::from_id_salt("push.remote")
                                    .selected_text(&dialog.remote)
                                    .show_ui(ui, |ui| {
                                        for remote in &state.remotes {
                                            ui.selectable_value(
                                                &mut dialog.remote,
                                                remote.clone(),
                                                remote,
                                            );
                                        }
                                    });
                                ui.end_row();
                                ui.label("Remote branch:");
                                ui.add(
                                    TextEdit::singleline(&mut dialog.branch).desired_width(220.0),
                                );
                                ui.end_row();
                            });
                        ui.checkbox(
                            &mut dialog.set_upstream,
                            "Set as upstream (track this remote branch)",
                        );
                    });
                });
                if busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(&app.status);
                    });
                }
                if let Some(error) = &app.error {
                    ui.add(
                        egui::Label::new(RichText::new(error).color(palette.removed_text)).wrap(),
                    );
                }
                ui.add_space(8.0);
                let can_skip = dialog.can_skip_confirmation();
                if !can_skip {
                    dialog.dont_show_again = false;
                }
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        !busy && can_skip,
                        egui::Checkbox::new(&mut dialog.dont_show_again, "Don't show again"),
                    )
                    .on_disabled_hover_text(
                        "Available when pushing to the branch's configured upstream.",
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let valid = state.branch.is_some()
                            && state.remotes.contains(&dialog.remote)
                            && crate::branches::name_hint(&dialog.branch).is_none();
                        if ui
                            .add_enabled(
                                !busy && valid && app.state_current,
                                super::dialog_style::primary("Push", palette),
                            )
                            .clicked()
                        {
                            *submit = true;
                        }
                        if ui.add(super::dialog_style::button("Cancel")).clicked() {
                            if busy && app.manager.cancel.is_some() {
                                app.cancel_repository_job();
                            } else {
                                *keep = false;
                            }
                        }
                    });
                });
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::RemoteDialog,
        remotes::State,
        theme::{install_fonts, ThemeChoice},
    };

    #[test]
    fn push_confirmation_is_compact_in_both_themes_and_window_sizes() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1400.0, 900.0)] {
                let ctx = egui::Context::default();
                install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.network_dialog = Some(RemoteDialog {
                    operation: Operation::Push,
                    state: Some(State {
                        remotes: vec!["origin".into()],
                        branch: Some("main".into()),
                        upstream_remote: Some("origin".into()),
                        upstream_ref: Some("main".into()),
                    }),
                    remote: "origin".into(),
                    branch: "main".into(),
                    set_upstream: false,
                    dont_show_again: false,
                });
                for frame in 0..3 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ui| show(ui.ctx(), &mut app),
                    );
                    output.textures_delta.clear();
                    if frame < 2 {
                        continue;
                    }
                    let labels = output
                        .shapes
                        .iter()
                        .filter_map(|shape| {
                            if let egui::Shape::Text(text) = &shape.shape {
                                Some((text.galley.job.text.as_str(), text.pos))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    let title = labels
                        .iter()
                        .find(|(text, _)| *text == "Do you want to push branch 'main'?")
                        .unwrap()
                        .1;
                    let push = labels.iter().find(|(text, _)| *text == "Push").unwrap().1;
                    assert!(labels
                        .iter()
                        .any(|(text, _)| *text == "1 branch will be pushed to 'origin'."));
                    assert!(labels.iter().any(|(text, _)| *text == "Don't show again"));
                    assert!(push.y > title.y && push.y - title.y < 150.0);
                    assert!(push.x < size.x && push.y < size.y);
                    output.textures_delta.clear();
                }
                assert!(!app.skip_push_confirmation);
                assert!(!app.busy());
            }
        }
    }
}
