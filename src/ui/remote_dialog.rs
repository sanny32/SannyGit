use super::controls::dialog_style::DialogResponse;
use super::controls::tooltips::TooltipResponse;
use crate::{app::SannyGitApp, remotes::Operation};
use eframe::egui::{self, Align, ComboBox, Layout, RichText, TextEdit};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    if app.remote_confirmation_hidden() {
        return;
    }
    let Some(mut dialog) = app.network_dialog.take() else {
        return;
    };
    let palette = app.palette();
    let busy = app.busy();
    let mut submit = false;
    let mut keep = true;
    super::controls::dialog_style::modal(ctx, palette, "remote.operation").show(ctx, |ui| {
        ui.set_width(580.0);
        super::controls::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        if matches!(dialog.operation, Operation::Push | Operation::Sync) {
            branch_confirmation(ui, app, &mut dialog, busy, &mut submit, &mut keep);
            return;
        }
        let description = match dialog.operation {
            Operation::Sync => unreachable!(),
            Operation::Fetch => "Download remote branches and tags without changing your working tree.",
            Operation::Pull => "Fetch a remote branch and fast-forward the current local branch. Local changes must be committed or moved aside.",
            Operation::Push => "Push the current local branch to the selected remote branch. Force pushes and tag pushes are not used.",
        };
        super::controls::dialog_style::header(ui, palette, dialog.operation.label(), description);
        super::controls::dialog_style::section(ui, palette.header, egui::Margin::same(18), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            let loaded = dialog.state.is_some();
            let state = dialog.state.clone().unwrap_or_default();
            // Render the same fields while loading, running or reporting errors.
            ui.add_enabled_ui(!busy && loaded, |ui| {
                egui::Grid::new("remote.fields").min_col_width(110.0).spacing([12.0,6.0]).show(ui, |ui| {
                    ui.label("Remote:");
                    ComboBox::from_id_salt("remote.name").width(310.0).truncate().selected_text(&dialog.remote).show_ui(ui, |ui| {
                        for remote in &state.remotes { ui.selectable_value(&mut dialog.remote, remote.clone(), remote); }
                    }); ui.end_row();
                    if matches!(dialog.operation, Operation::Pull | Operation::Sync) {
                        ui.label("Local branch:");
                        let branch = state.branch.as_deref().unwrap_or(if loaded { "No local branch with commits checked out" } else { "—" });
                        ui.add_sized([310.0, 26.0], egui::Label::new(branch).truncate()).help_text(branch);
                        ui.end_row();
                        ui.label("Remote branch:"); ui.add_sized([310.0,28.0], TextEdit::singleline(&mut dialog.branch)); ui.end_row();
                    }
                });
            });
            let (message, color) = if let Some(error) = &app.error {
                (error.as_str(), palette.removed_text)
            } else if !loaded {
                ("Loading remotes…", palette.muted)
            } else if busy {
                (app.status.as_str(), palette.muted)
            } else if state.remotes.is_empty() {
                ("No configured remotes. Add one through Remote → Manage Remotes, then reopen this dialog.", palette.muted)
            } else {
                ("Cancellation does not roll back local or remote changes.", palette.muted)
            };
            ui.add_sized([ui.available_width(), 26.0], egui::Label::new(RichText::new(message).color(color)).truncate()).help_text(message);
        });
        super::controls::dialog_style::section(ui, palette.header, egui::Margin::symmetric(18,12), |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let valid = dialog.state.as_ref().is_some_and(|state| state.remotes.contains(&dialog.remote)
                    && (dialog.operation == Operation::Fetch || (state.branch.is_some() && crate::branches::name_hint(&dialog.branch).is_none())));
                if ui.add_enabled(!busy && valid && app.state_current, super::controls::dialog_style::primary(dialog.operation.label(), palette)).dialog_accept() { submit = true; }
                if ui.add(super::controls::dialog_style::button("Cancel")).dialog_cancel() {
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

fn branch_confirmation(
    ui: &mut egui::Ui,
    app: &mut SannyGitApp,
    dialog: &mut crate::app::RemoteDialog,
    busy: bool,
    submit: &mut bool,
    keep: &mut bool,
) {
    let palette = app.palette();
    let sync = dialog.operation == Operation::Sync;
    let loaded = dialog.state.is_some();
    let state = dialog.state.clone().unwrap_or_default();
    super::controls::dialog_style::section(ui, palette.header, egui::Margin::same(20), |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(20.0, 8.0);
        ui.horizontal_top(|ui| {
            ui.add(super::controls::icons::image(
                if sync { super::controls::icons::Icon::Fetch } else { super::controls::icons::Icon::Push },
                palette.accent,
                56.0,
            ));
            ui.vertical(|ui| {
                ui.set_width(464.0);
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                let branch = state.branch.as_deref().unwrap_or_else(|| {
                    if !loaded && !app.current_branch.is_empty() {
                        &app.current_branch
                    } else {
                        "—"
                    }
                });
                let title = if sync {
                    "Do you want to proceed with sync?".to_owned()
                } else {
                    format!("Do you want to push branch '{branch}'?")
                };
                ui.add(
                    egui::Label::new(
                        RichText::new(&title)
                            .size(16.0)
                            .family(crate::theme::semibold_family()),
                    )
                    .truncate(),
                )
                .help_text(title);
                ui.add_space(6.0);
                if sync {
                    ui.label("First, local changes will be pushed, then possible remote changes pulled. The advantage over a normal push is that if it fails because of remote changes, they will be pulled automatically.");
                }
                // Reuse one compact line for all operation states. Neither a spinner
                // nor a long error may add rows or move the options and buttons.
                let (message, color, spinning) = if let Some(error) = &app.error {
                    (error.clone(), palette.removed_text, false)
                } else if !loaded {
                    ("Loading remotes…".into(), palette.text, true)
                } else if busy {
                    (app.status.clone(), palette.text, true)
                } else if dialog.remote.is_empty() {
                    ("No configured remotes.".into(), palette.text, false)
                } else {
                    let mut message = if sync {
                        format!("{} → {}/{} · Fast-forward only; clean working tree required.", branch, dialog.remote, dialog.branch)
                    } else {
                        format!("1 branch will be pushed to '{}'.", dialog.remote)
                    };
                    if !sync && dialog.branch != branch {
                        message.push_str(&format!(" Remote branch: {}", dialog.branch));
                    }
                    (message, palette.text, false)
                };
                ui.horizontal(|ui| {
                    ui.set_height(26.0);
                    if spinning {
                        ui.add(egui::Spinner::new().size(18.0));
                    }
                    ui.add(egui::Label::new(RichText::new(&message).color(color)).truncate())
                        .help_text(message);
                });
                ui.add_enabled_ui(!busy && loaded, |ui| {
                    egui::CollapsingHeader::new("Options").show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 8.0;
                        egui::Grid::new("push.options")
                            .num_columns(2)
                            .min_col_width(100.0)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new("Remote:")
                                        .wrap_mode(egui::TextWrapMode::Extend),
                                );
                                ComboBox::from_id_salt("push.remote")
                                    .width(220.0)
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
                                ui.add(
                                    egui::Label::new("Remote branch:")
                                        .wrap_mode(egui::TextWrapMode::Extend),
                                );
                                ui.add(
                                    TextEdit::singleline(&mut dialog.branch).desired_width(220.0),
                                );
                                ui.end_row();
                            });
                        if !sync {
                            ui.checkbox(
                                &mut dialog.set_upstream,
                                "Set as upstream (track this remote branch)",
                            );
                        }
                    });
                });
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
                    .disabled_help_text(
                        "Available when using the branch's configured upstream.",
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let valid = state.branch.is_some()
                            && state.remotes.contains(&dialog.remote)
                            && crate::branches::name_hint(&dialog.branch).is_none();
                        if ui
                            .add_enabled(
                                !busy && valid && app.state_current,
                                super::controls::dialog_style::primary(if sync { "Synchronize" } else { "Push" }, palette),
                            )
                            .dialog_accept()
                        {
                            *submit = true;
                        }
                        if ui
                            .add(super::controls::dialog_style::button("Cancel"))
                            .dialog_cancel()
                        {
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
    fn skipped_push_and_sync_do_not_render_a_modal() {
        for operation in [Operation::Push, Operation::Sync] {
            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                for size in [egui::vec2(1100.0, 700.0), egui::vec2(1400.0, 900.0)] {
                    let ctx = egui::Context::default();
                    theme.apply(&ctx);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.skip_push_confirmation = operation == Operation::Push;
                    app.preferences.skip_sync_confirmation = operation == Operation::Sync;
                    app.network_dialog = Some(RemoteDialog {
                        operation,
                        state: None,
                        remote: "origin".into(),
                        branch: "main".into(),
                        set_upstream: false,
                        dont_show_again: false,
                    });
                    for loaded in [false, true] {
                        if loaded {
                            app.network_dialog.as_mut().unwrap().state = Some(State {
                                remotes: vec!["origin".into()],
                                branch: Some("main".into()),
                                upstream_remote: Some("origin".into()),
                                upstream_ref: Some("main".into()),
                            });
                        }
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                ..Default::default()
                            },
                            |ui| show(ui.ctx(), &mut app),
                        );
                        assert!(output.shapes.is_empty());
                        assert!(app.network_dialog.is_some());
                        output.textures_delta.clear();
                    }
                }
            }
        }
    }

    #[test]
    fn branch_confirmation_size_is_stable_from_loading_through_progress_and_errors() {
        for operation in [Operation::Push, Operation::Sync] {
            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                for size in [egui::vec2(1100.0, 700.0), egui::vec2(1400.0, 900.0)] {
                    let ctx = egui::Context::default();
                    install_fonts(&ctx);
                    egui_extras::install_image_loaders(&ctx);
                    theme.apply(&ctx);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.current_branch = "main".into();
                    let mut dialog = RemoteDialog {
                        operation,
                        state: None,
                        remote: String::new(),
                        branch: String::new(),
                        set_upstream: false,
                        dont_show_again: false,
                    };
                    let mut sizes = Vec::new();
                    for stage in 0..4 {
                        if stage == 1 {
                            dialog.state = Some(State {
                                remotes: vec!["origin".into()],
                                branch: Some("main".into()),
                                ..Default::default()
                            });
                            dialog.remote = "origin".into();
                            dialog.branch = "different-remote-branch".into();
                        }
                        if stage == 2 {
                            app.status = "Pushing branch to origin…".into();
                        }
                        if stage == 3 {
                            app.error = Some("Remote rejected the push. ".repeat(100));
                        }
                        for _ in 0..3 {
                            let mut output = ctx.run_ui(
                                egui::RawInput {
                                    screen_rect: Some(egui::Rect::from_min_size(
                                        egui::Pos2::ZERO,
                                        size,
                                    )),
                                    ..Default::default()
                                },
                                |ui| {
                                    let response = super::super::controls::dialog_style::modal(
                                        ui.ctx(),
                                        theme.palette(),
                                        "push.size.test",
                                    )
                                    .show(ui.ctx(), |ui| {
                                        ui.set_width(580.0);
                                        super::super::controls::dialog_style::setup(ui);
                                        ui.spacing_mut().item_spacing.y = 0.0;
                                        branch_confirmation(
                                            ui,
                                            &mut app,
                                            &mut dialog,
                                            stage == 2,
                                            &mut false,
                                            &mut true,
                                        );
                                    });
                                    let size = response.response.rect.size();
                                    assert!(
                                        size.y
                                            <= if operation == Operation::Sync {
                                                290.0
                                            } else {
                                                200.0
                                            },
                                        "Push dialog has excess blank space: {size:?}"
                                    );
                                    sizes.push(size);
                                },
                            );
                            output.textures_delta.clear();
                        }
                    }
                    assert!(
                        sizes
                            .windows(2)
                            .all(|pair| (pair[0] - pair[1]).length() < 1.0),
                        "Push dialog changed size: {sizes:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn push_uses_the_same_confirmation_while_loading_and_on_error() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let ctx = egui::Context::default();
            install_fonts(&ctx);
            egui_extras::install_image_loaders(&ctx);
            theme.apply(&ctx);
            let mut app = SannyGitApp::with_theme(theme);
            app.current_branch = "main".into();
            app.network_dialog = Some(RemoteDialog {
                operation: Operation::Push,
                state: None,
                remote: String::new(),
                branch: String::new(),
                set_upstream: false,
                dont_show_again: false,
            });
            let mut title_positions = Vec::new();
            for stage in 0..3 {
                if stage == 1 {
                    let dialog = app.network_dialog.as_mut().unwrap();
                    dialog.remote = "origin".into();
                    dialog.branch = "main".into();
                    dialog.state = Some(State {
                        remotes: vec!["origin".into()],
                        branch: Some("main".into()),
                        upstream_remote: Some("origin".into()),
                        upstream_ref: Some("main".into()),
                    });
                } else if stage == 2 {
                    app.network_dialog.as_mut().unwrap().state = None;
                    app.error = Some("Cannot read remotes".into());
                }
                for frame in 0..3 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1100.0, 700.0),
                            )),
                            ..Default::default()
                        },
                        |ui| show(ui.ctx(), &mut app),
                    );
                    output.textures_delta.clear();
                    if frame < 2 {
                        continue;
                    }
                    let texts = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) => Some(text),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    let title = texts
                        .iter()
                        .find(|text| text.galley.job.text == "Do you want to push branch 'main'?")
                        .expect("Push confirmation missing");
                    title_positions.push(title.pos.x);
                    let expected = match stage {
                        0 => "Loading remotes…",
                        1 => "1 branch will be pushed to 'origin'.",
                        _ => "Cannot read remotes",
                    };
                    assert!(texts.iter().any(|text| text.galley.job.text == expected));
                    assert!(texts.iter().any(|text| text.galley.job.text == "Cancel"));
                    assert!(!texts.iter().any(|text| text
                        .galley
                        .job
                        .text
                        .contains("Force pushes")));
                }
            }
            assert!(title_positions
                .windows(2)
                .all(|x| (x[0] - x[1]).abs() < 1.0));
            assert!(!app.busy());
        }
    }

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
                let mut options_pos = egui::Pos2::ZERO;
                for frame in 0..8 {
                    let events = match frame {
                        3 | 4 => vec![
                            egui::Event::PointerMoved(options_pos),
                            egui::Event::PointerButton {
                                pos: options_pos,
                                button: egui::PointerButton::Primary,
                                pressed: frame == 3,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        _ => vec![],
                    };
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            time: Some(frame as f64 * 0.3),
                            events,
                            ..Default::default()
                        },
                        |ui| show(ui.ctx(), &mut app),
                    );
                    output.textures_delta.clear();
                    if frame != 2 && frame != 7 {
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
                    options_pos = labels
                        .iter()
                        .find(|(text, _)| *text == "Options")
                        .unwrap()
                        .1
                        + egui::vec2(4.0, 4.0);
                    let max_height = if frame == 7 { 240.0 } else { 150.0 };
                    assert!(
                        push.y > title.y && push.y - title.y < max_height,
                        "frame {frame}: title {title:?}, push {push:?}"
                    );
                    if frame == 7 {
                        for label in ["Remote:", "Remote branch:"] {
                            let text = output
                                .shapes
                                .iter()
                                .find_map(|shape| match &shape.shape {
                                    egui::Shape::Text(text) if text.galley.job.text == label => {
                                        Some(text)
                                    }
                                    _ => None,
                                })
                                .expect("Expanded options label missing");
                            assert_eq!(text.galley.rows.len(), 1, "Wrapped label: {label}");
                        }
                        let remote = labels.iter().find(|(text, _)| *text == "origin").unwrap().1;
                        let branch = labels.iter().find(|(text, _)| *text == "main").unwrap().1;
                        assert!((remote.x - branch.x).abs() < 6.0, "Fields must align");
                        assert!(branch.y - remote.y < 40.0, "Excess row spacing");
                    }
                    assert!(push.x < size.x && push.y < size.y);
                    output.textures_delta.clear();
                }
                assert!(!app.skip_push_confirmation);
                assert!(!app.busy());
            }
        }
    }
}
