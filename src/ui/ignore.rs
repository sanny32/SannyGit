use super::controls::{dialog_style, dialog_style::DialogResponse, tooltips::TooltipResponse};
use crate::{app::SannyGitApp, git};
use eframe::egui::{self, RichText};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let busy = app.busy();
    let current = app.state_current;
    let palette = app.palette();
    let Some(dialog) = &mut app.ignore_dialog else {
        return;
    };
    let mut cancel = false;
    let mut submit = false;
    let response = dialog_style::modal(ctx, palette, "ignore.files").show(ctx, |ui| {
        ui.set_width(460.0);
        dialog_style::setup(ui);
        dialog_style::header(
            ui,
            palette,
            "Ignore Files",
            "Add selected untracked files or a filename pattern to .gitignore.",
        );
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                let selection = if dialog.paths.len() == 1 {
                    dialog.paths[0].clone()
                } else {
                    format!("{} files selected", dialog.paths.len())
                };
                dialog_style::text_line(ui, RichText::new(&selection).color(palette.muted), 22.0)
                    .help_text(selection);
                ui.radio_value(&mut dialog.use_pattern, false, "Ignore selected files");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(dialog.parent.is_some(), |ui| {
                        ui.radio_value(&mut dialog.use_pattern, true, "Ignore by pattern");
                    })
                    .response
                    .disabled_help_text("Select files from one folder to use a pattern");
                    ui.add_enabled(
                        dialog.use_pattern,
                        egui::TextEdit::singleline(&mut dialog.pattern)
                            .hint_text("*.log")
                            .desired_width(220.0),
                    );
                });
                ui.label(
                    RichText::new(
                        "Use ? for one character or * for any number of characters in this folder.",
                    )
                    .color(palette.muted),
                );
                ui.horizontal(|ui| {
                    ui.label("Ignore file");
                    egui::ComboBox::from_id_salt("ignore.destination")
                        .selected_text(if dialog.use_parent {
                            ".gitignore in file’s folder"
                        } else {
                            ".gitignore in repository root"
                        })
                        .width(260.0)
                        .show_ui(ui, |ui| {
                            if dialog
                                .parent
                                .as_ref()
                                .is_some_and(|folder| !folder.as_os_str().is_empty())
                            {
                                ui.selectable_value(
                                    &mut dialog.use_parent,
                                    true,
                                    ".gitignore in file’s folder",
                                );
                            }
                            ui.selectable_value(
                                &mut dialog.use_parent,
                                false,
                                ".gitignore in repository root",
                            );
                        });
                });
            });
        });
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            dialog_style::button_row(ui, |ui| {
                let valid = !dialog.use_pattern || git::valid_ignore_glob(&dialog.pattern);
                submit = ui
                    .add_enabled(
                        !busy && current && valid,
                        dialog_style::primary("Ignore", palette),
                    )
                    .disabled_help_text(
                        "Enter a filename pattern or wait for the working tree to refresh",
                    )
                    .dialog_accept();
                cancel = ui
                    .add_enabled(!busy, dialog_style::button("Cancel"))
                    .dialog_cancel();
            });
        });
    });
    if !busy && (cancel || response.should_close()) {
        app.ignore_dialog = None;
    } else if submit {
        app.confirm_ignore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{install_fonts, ThemeChoice};

    #[test]
    fn ignore_dialog_renders_at_both_window_sizes_without_resizing() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                let ctx = egui::Context::default();
                install_fonts(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.repositories = crate::mock_data::repositories();
                app.changes = crate::mock_data::changes();
                app.selected_repository = 0;
                app.selection.active = 0;
                app.changes[0].kind = crate::models::ChangeKind::Untracked;
                app.state_current = true;
                app.open_ignore_dialog();
                assert!(!app.busy());
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                let mut baseline = None;
                for use_pattern in [false, true] {
                    let dialog = app.ignore_dialog.as_mut().unwrap();
                    dialog.use_pattern = use_pattern;
                    dialog.use_parent = !use_pattern;
                    for _ in 0..3 {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(screen),
                                ..Default::default()
                            },
                            |ui| show(ui.ctx(), &mut app),
                        );
                        output.textures_delta.clear();
                    }
                    let rect = ctx
                        .memory(|memory| memory.area_rect(egui::Id::unique("ignore.files")))
                        .unwrap();
                    assert!(screen.contains_rect(rect));
                    if let Some(before) = baseline {
                        assert_eq!(before, rect);
                    } else {
                        baseline = Some(rect);
                    }
                }
            }
        }
    }
}
