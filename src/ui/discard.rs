use super::controls::dialog_style::DialogResponse;
use super::controls::{dialog_style, icons::Icon, tooltips::TooltipResponse};
use crate::app::SannyGitApp;
use eframe::egui::{self, RichText};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let busy = app.busy();
    let current = app.state_current;
    let Some(mut dialog) = app.discard_dialog.take() else {
        return;
    };
    let palette = app.palette();
    let mut cancel = false;
    let mut reload = false;
    let mut submit = false;
    let multiple = dialog.paths.len() > 1;
    let title = if multiple {
        "Discard Selected Files"
    } else {
        dialog.action.label()
    };
    let description = if multiple {
        "Restore tracked files from the index and permanently delete untracked files. Unstaged edits are lost; staged contents are preserved. Review every selected path before confirming."
    } else {
        dialog.action.explanation()
    };
    dialog_style::modal(ctx, palette, "discard.file").show(ctx, |ui| {
        ui.set_width(580.0);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header_with_icon(ui, palette, title, description, Icon::Discard);
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            let repository = format!("Repository: {:?}", dialog.root);
            super::controls::dialog_style::text_line(ui, RichText::new(&repository).color(palette.muted), 26.0).help_text(repository);
            if multiple {
                egui::ScrollArea::vertical().id_salt("discard.paths").min_scrolled_height(110.0).max_height(110.0).auto_shrink([false, false]).show(ui, |ui| {
                    for path in &dialog.paths {
                        dialog_style::text_line(ui, RichText::new(path).monospace(), 22.0).help_text(path);
                    }
                });
            } else {
                let path = format!("File: {:?}", dialog.path);
                super::controls::dialog_style::text_line(ui, RichText::new(&path).monospace(), 26.0).help_text(path);
            }
            ui.scope(|ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.add_enabled(!busy && dialog.prepared.is_some(), egui::Checkbox::new(&mut dialog.confirmed, if multiple { "I confirm this action on all reviewed files." } else { "I confirm this action on the reviewed file." }));
            });
            let (message, color) = if dialog.error.is_some() {
                ("Reload details to retry.", palette.muted)
            } else if dialog.running {
                ("Operation in progress…", palette.muted)
            } else if multiple && dialog.prepared.is_some() {
                ("Selected files reviewed. The index will be preserved.", palette.muted)
            } else if let Some(prepared) = &dialog.prepared {
                (prepared.summary.as_str(), palette.muted)
            } else {
                ("Checking file, index and repository state…", palette.muted)
            };
            super::controls::dialog_style::text_line(ui, RichText::new(message).color(color), 26.0).help_text(message);
        });
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            dialog_style::button_row(ui, |ui| {
                if ui.add_enabled(!busy && current && dialog.prepared.is_some() && dialog.confirmed,
                    dialog_style::primary(title, palette)).disabled_help_text("Review the files and confirm the action. Reload details if a file, index or HEAD changed.").dialog_accept() { submit = true; }
                if ui.add_enabled(!dialog.running, dialog_style::button("Cancel")).dialog_cancel() { cancel = true; }
                if ui.add_enabled(!busy && !dialog.running, dialog_style::button("Reload details")).clicked() { reload = true; }
            });
        });
    });
    if !cancel {
        app.discard_dialog = Some(dialog);
    }
    if reload {
        app.reload_discard();
    }
    if submit {
        app.confirm_discard();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        discard::Action,
        theme::{install_fonts, InterfaceStyle, ThemeChoice},
    };

    #[test]
    fn group_dialog_fits_and_retains_geometry_during_loading_and_errors() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                let ctx = egui::Context::default();
                install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let (_repo, mut dialog) = crate::mock_data::discard_dialog(Action::Restore);
                dialog.paths.push("another/selected/path".into());
                let prepared = dialog.prepared.take();
                let mut app = SannyGitApp::with_theme(theme);
                app.discard_dialog = Some(dialog);
                let mut baseline = None;
                for stage in 0..4 {
                    let dialog = app.discard_dialog.as_mut().unwrap();
                    dialog.prepared = if stage == 0 { None } else { prepared.clone() };
                    dialog.running = stage == 2;
                    dialog.error = (stage == 3).then(|| "Failure details ".repeat(100));
                    for _ in 0..4 {
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
                        output.textures_delta.clear();
                    }
                    let rect = ctx
                        .memory(|memory| memory.area_rect(egui::Id::unique("discard.file")))
                        .unwrap();
                    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect));
                    if let Some(before) = baseline {
                        assert_eq!(before, rect);
                    } else {
                        baseline = Some(rect);
                    }
                }
            }
        }
    }

    #[test]
    fn discard_dialog_keeps_compact_geometry_during_review_confirmation_progress_and_errors() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                    for action in [Action::Restore, Action::DeleteUntracked] {
                        let ctx = egui::Context::default();
                        install_fonts(&ctx);
                        egui_extras::install_image_loaders(&ctx);
                        theme.apply(&ctx);
                        let (_repo, dialog) = crate::mock_data::discard_dialog(action);
                        let prepared = dialog.prepared.clone();
                        let mut app = SannyGitApp::with_theme(theme);
                        app.interface_style = style;
                        app.state_current = true;
                        app.discard_dialog = Some(dialog);
                        let mut baseline = None;
                        let mut buttons = None;
                        for stage in 0..6 {
                            let dialog = app.discard_dialog.as_mut().unwrap();
                            match stage {
                                0 => dialog.prepared = None,
                                1 => dialog.prepared = prepared.clone(),
                                2 => dialog.confirmed = true,
                                3 => dialog.running = true,
                                4 => {
                                    dialog.running = false;
                                    dialog.confirmed = false;
                                    dialog.prepared = None;
                                    dialog.error = Some(
                                        "The file changed after review. Reload details. "
                                            .repeat(100),
                                    );
                                }
                                _ => {
                                    dialog.path = "long directory name/".repeat(100);
                                    dialog.root = "long repository path/".repeat(100);
                                }
                            }
                            for frame in 0..4 {
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
                                output.textures_delta.clear();
                                if frame < 3 {
                                    continue;
                                }
                                let rect = ctx
                                    .memory(|memory| {
                                        memory.area_rect(egui::Id::unique("discard.file"))
                                    })
                                    .unwrap();
                                assert!(
                                    rect.height() < 380.0,
                                    "Oversized discard dialog: {rect:?}"
                                );
                                assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                                    .contains_rect(rect));
                                if let Some(before) = baseline {
                                    assert_eq!(rect, before, "Discard dialog resized at stage {stage}: {theme:?} {style:?} {action:?}");
                                } else {
                                    baseline = Some(rect);
                                }
                                let positions = output
                                    .shapes
                                    .iter()
                                    .filter_map(|shape| match &shape.shape {
                                        egui::Shape::Text(text)
                                            if [
                                                action.label(),
                                                "Cancel",
                                                "Reload details",
                                                "I confirm this action on the reviewed file.",
                                            ]
                                            .contains(&text.galley.job.text.as_str()) =>
                                        {
                                            Some((text.galley.job.text.clone(), text.pos))
                                        }
                                        _ => None,
                                    })
                                    .collect::<Vec<_>>();
                                assert_eq!(positions.len(), 5);
                                if let Some(before) = &buttons {
                                    assert_eq!(&positions, before, "Discard controls moved at stage {stage}: {theme:?} {style:?}");
                                } else {
                                    buttons = Some(positions);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn clicking_cancel_preserves_file_bytes_and_index_even_after_confirmation() {
        for action in [Action::Restore, Action::DeleteUntracked] {
            let ctx = egui::Context::default();
            install_fonts(&ctx);
            egui_extras::install_image_loaders(&ctx);
            let (repo, mut dialog) = crate::mock_data::discard_dialog(action);
            dialog.confirmed = true;
            let path = repo.0.join(&dialog.path);
            let bytes = std::fs::read(&path).unwrap();
            let index = repo.git(&["ls-files", "--stage"]);
            let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
            let snapshot = crate::git::load(&repo.0).unwrap();
            app.repositories.push(snapshot.repository);
            app.changes = snapshot.changes;
            app.selected_repository = 0;
            app.selected_change = 0;
            app.state_current = true;
            app.discard_dialog = Some(dialog);
            let render = |app: &mut SannyGitApp, events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1100.0, 700.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| show(ui.ctx(), app),
                );
                output.textures_delta.clear();
                output
            };
            for _ in 0..3 {
                render(&mut app, vec![]);
            }
            let output = render(&mut app, vec![]);
            let cancel = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "Cancel" => {
                        Some(text.pos + text.galley.size() / 2.0)
                    }
                    _ => None,
                })
                .unwrap();
            for pressed in [true, false] {
                render(
                    &mut app,
                    vec![
                        egui::Event::PointerMoved(cancel),
                        egui::Event::PointerButton {
                            pos: cancel,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert!(app.discard_dialog.is_none());
            assert!(!app.busy());
            assert_eq!(std::fs::read(path).unwrap(), bytes);
            assert_eq!(repo.git(&["ls-files", "--stage"]), index);
        }
    }
}
