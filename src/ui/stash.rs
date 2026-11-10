use super::controls::{dialog_style, tooltips::TooltipResponse};
use crate::{
    app::{stash::Action, SannyGitApp},
    models::DiffPreview,
};
use eframe::egui::{self, Align, Button, Label, Layout, RichText, ScrollArea, TextEdit};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let allowed = app.stash_allowed();
    let busy = app.busy();
    let Some(mut dialog) = app.stash_dialog.take() else {
        return;
    };
    let palette = app.palette();
    let mut close = false;
    let mut preview = None;
    let mut action = None;
    let mut reload = false;
    dialog_style::modal(ctx, palette, "stash.dialog").show(ctx, |ui| {
        ui.set_width(760.0);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header_with_icon(ui, palette, "Stashes", "Save local changes for later. Apply keeps the stash; Pop removes it after a successful apply.", super::controls::icons::Icon::Stash);
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal(|ui| {
                ui.label("Message:");
                ui.add_enabled(!busy, TextEdit::singleline(&mut dialog.message).desired_width(490.0).hint_text("Optional stash message"));
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy, egui::Checkbox::new(&mut dialog.include_untracked, "Include untracked files")).changed() {
                    dialog.invalidate_preview();
                    dialog.error = None;
                    dialog.status = "Preview the selected local changes before saving.".into();
                }
                if ui.add_enabled(allowed, Button::new("Preview changes")).clicked() { preview = Some(true); }
                let save_ready = allowed && dialog.prepared_save.as_ref().is_some_and(|prepared| prepared.include_untracked == dialog.include_untracked);
                if ui.add_enabled(save_ready, Button::new("Save Stash")).disabled_help_text("Preview local changes before saving; contents are checked again when saving.").clicked() { action = Some(Action::Save); }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong("Saved stashes");
                if ui.add_enabled(!busy, Button::new("Reload list")).clicked() { reload = true; }
            });
            ScrollArea::vertical().id_salt("stash.entries").min_scrolled_height(96.0).max_height(96.0).auto_shrink([false, false]).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if dialog.entries.is_empty() {
                    ui.label(if dialog.entries_current { "No saved stashes" } else { "Loading stashes…" });
                }
                let mut selected = None;
                for (index, entry) in dialog.entries.iter().enumerate() {
                    let label = format!("stash@{{{index}}}  {}", entry.message);
                    if ui.add_enabled(!busy, Button::selectable(dialog.selected == Some(index), RichText::new(&label)).wrap_mode(egui::TextWrapMode::Truncate))
                        .help_text(format!("{label}\n{}", entry.oid)).clicked() { selected = Some(index); }
                }
                if let Some(selected) = selected {
                    dialog.selected = Some(selected);
                    dialog.invalidate_preview();
                    dialog.error = None;
                    dialog.status = "Preview the selected stash before applying or deleting it.".into();
                }
            });
            let selected_ready = allowed && dialog.entries_current && dialog.selected_entry().is_some();
            let entry_ready = selected_ready && dialog.previewed_entry.as_ref() == dialog.selected_entry();
            ui.horizontal(|ui| {
                if ui.add_enabled(selected_ready, Button::new("Preview stash")).clicked() { preview = Some(false); }
                ui.add_enabled(!busy, egui::Checkbox::new(&mut dialog.restore_index, "Restore staged state"))
                    .help_text("Restore the original staging state with git stash apply --index. Uncheck to apply tracked contents as unstaged changes.");
                if ui.add_enabled(entry_ready, Button::new("Apply")).disabled_help_text("Select and preview a stash; applying requires a clean working tree.").clicked() { action = Some(Action::Apply); }
                if ui.add_enabled(entry_ready, Button::new("Pop")).help_text("Apply and then remove this entry. Conflicts or apply errors retain the entry.").clicked() { action = Some(Action::Pop); }
                if ui.add_enabled(entry_ready && dialog.confirm_drop, Button::new("Drop")).disabled_help_text("Preview the selected stash and confirm its deletion below.").clicked() { action = Some(Action::Drop); }
            });
            let confirmation = dialog.selected_entry().zip(dialog.selected).map_or_else(
                || "Select a stash to confirm deletion".to_owned(),
                |(entry, index)| format!("Confirm deleting stash@{{{index}}}: {}", entry.message),
            );
            ui.scope(|ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.add_enabled(entry_ready, egui::Checkbox::new(&mut dialog.confirm_drop, &confirmation)).help_text(confirmation);
            });
            ui.separator();
            // A bounded review pane is shared by local-change and saved-stash previews.
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 160.0), Layout::top_down(Align::Min), |ui| {
                ui.set_min_height(160.0);
                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                if let Some(diff) = &dialog.preview {
                    render_preview(ui, diff, palette);
                } else {
                    ui.label("Use Preview changes or Preview stash to inspect contents here.");
                }
            });
            let message = dialog.error.as_deref().or(app.error.as_deref()).unwrap_or(&dialog.status);
            let color = if dialog.error.is_some() || app.error.is_some() { palette.removed_text } else { palette.muted };
            ui.add_sized([ui.available_width(), 26.0], Label::new(RichText::new(message).color(color)).truncate()).help_text(message);
        });
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add_enabled(!busy, dialog_style::button("Close")).disabled_help_text("Wait for the current operation to finish.").clicked() { close = true; }
            });
        });
    });
    if !close {
        app.stash_dialog = Some(dialog);
    }
    if reload {
        app.reload_stashes();
    }
    if let Some(saving) = preview {
        app.preview_stash(saving);
    }
    if let Some(action) = action {
        app.run_stash(action);
    }
}

fn render_preview(ui: &mut egui::Ui, preview: &DiffPreview, palette: crate::theme::Palette) {
    super::diff_view::render_preview(ui, preview, palette, "stash.contents");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{install_fonts, InterfaceStyle, ThemeChoice};

    #[test]
    fn stash_dialog_keeps_compact_geometry_and_controls_across_states() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                    let ctx = egui::Context::default();
                    install_fonts(&ctx);
                    egui_extras::install_image_loaders(&ctx);
                    theme.apply(&ctx);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.interface_style = style;
                    app.stash_dialog = Some(crate::mock_data::stash_dialog());
                    let mut baseline = None;
                    let mut controls = None;
                    for stage in 0..6 {
                        let dialog = app.stash_dialog.as_mut().unwrap();
                        match stage {
                            0 => {
                                dialog.entries.clear();
                                dialog.entries_current = false;
                            }
                            1 => {
                                dialog.entries = crate::mock_data::stash_entries();
                                dialog.entries_current = true;
                            }
                            2 => {
                                dialog.preview = Some(DiffPreview::new(
                                    &crate::mock_data::changes().remove(0).diff,
                                ));
                            }
                            3 => {
                                dialog.error =
                                    Some("Could not apply stash; entry retained. ".repeat(100));
                            }
                            4 => {
                                dialog.error = None;
                                dialog.status = "Stash operation in progress…".into();
                            }
                            _ => {
                                dialog.entries[0].message = "Very long stash message ".repeat(100);
                                dialog.confirm_drop = true;
                                dialog.previewed_entry = dialog.selected_entry().cloned();
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
                                .memory(|memory| memory.area_rect(egui::Id::unique("stash.dialog")))
                                .unwrap();
                            assert!(
                                egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                                    .contains_rect(rect),
                                "Stash dialog overflow: {rect:?} at {size:?}"
                            );
                            assert!(
                                rect.height() < 650.0,
                                "Stash dialog has excess height: {rect:?}"
                            );
                            if let Some(before) = baseline {
                                assert_eq!(
                                    rect, before,
                                    "Stash dialog resized at stage {stage}: {theme:?} {style:?}"
                                );
                            } else {
                                baseline = Some(rect);
                            }
                            let positions = output
                                .shapes
                                .iter()
                                .filter_map(|shape| match &shape.shape {
                                    egui::Shape::Text(text)
                                        if [
                                            "Message:",
                                            "Include untracked files",
                                            "Preview changes",
                                            "Save Stash",
                                            "Saved stashes",
                                            "Reload list",
                                            "Preview stash",
                                            "Restore staged state",
                                            "Apply",
                                            "Pop",
                                            "Drop",
                                            "Close",
                                        ]
                                        .contains(&text.galley.job.text.as_str()) =>
                                    {
                                        Some((text.galley.job.text.clone(), text.pos))
                                    }
                                    _ => None,
                                })
                                .collect::<Vec<_>>();
                            assert_eq!(positions.len(), 12);
                            if let Some(before) = &controls {
                                assert_eq!(
                                    &positions, before,
                                    "Stash controls moved at stage {stage}: {theme:?} {style:?}"
                                );
                            } else {
                                controls = Some(positions);
                            }
                        }
                    }
                }
            }
        }
    }
}
