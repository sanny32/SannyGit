use super::controls::dialog_style::DialogResponse;
use super::controls::{dialog_style, tooltips::TooltipResponse};
use crate::{
    app::{stash::Action, SannyGitApp},
    models::DiffPreview,
};
use eframe::egui::{self, Align, Button, Layout, RichText, ScrollArea, TextEdit};

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
    let managing = dialog.managing;
    let modal_id = if managing {
        "stash.manage.dialog"
    } else {
        "stash.dialog"
    };
    dialog_style::modal(ctx, palette, modal_id).show(ctx, |ui| {
        ui.set_width(if managing { 680.0 } else { 500.0 });
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header_with_icon(
            ui,
            palette,
            if managing { "Saved Stashes" } else { "Save Stash" },
            if managing { "Apply keeps the stash. Pop removes it after a successful apply." } else { "Save staged and unstaged changes for later." },
            super::controls::icons::Icon::Stash,
        );
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(14, 10), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            if !managing {
                ui.label("Message:");
                message_editor(ui, &mut dialog.message, !busy);
                if ui.add_enabled(!busy, egui::Checkbox::new(&mut dialog.include_untracked, "Include untracked files")).changed() {
                    dialog.invalidate_preview();
                    dialog.error = None;
                    dialog.status = "Preview the selected local changes before saving.".into();
                }
            } else {
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
                    if ui.add_enabled(selected_ready, Button::new("Preview stash")).clicked() { dialog.preview_expanded = true; preview = Some(false); }
                    if ui.add_enabled(entry_ready, Button::new("Apply")).disabled_help_text("Select and preview a stash; applying requires a clean working tree.").clicked() { action = Some(Action::Apply); }
                    if ui.add_enabled(entry_ready, Button::new("Pop")).help_text("Apply and then remove this entry. Conflicts or apply errors retain the entry.").clicked() { action = Some(Action::Pop); }
                    if ui.add_enabled(entry_ready && dialog.confirm_drop, Button::new("Drop")).disabled_help_text("Preview the selected stash and confirm its deletion below.").clicked() { action = Some(Action::Drop); }
                });
                ui.add_enabled(!busy, egui::Checkbox::new(&mut dialog.restore_index, "Restore staged state"))
                    .help_text("Restore the original staging state with git stash apply --index. Uncheck to apply tracked contents as unstaged changes.");
                let confirmation = dialog.selected_entry().zip(dialog.selected).map_or_else(
                    || "Select a stash to confirm deletion".to_owned(),
                    |(entry, index)| format!("Confirm deleting stash@{{{index}}}: {}", entry.message),
                );
                ui.scope(|ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    ui.add_enabled(entry_ready, egui::Checkbox::new(&mut dialog.confirm_drop, &confirmation)).help_text(confirmation);
                });
            }
            if dialog.preview_expanded {
                ui.separator();
                // Loading and preview results keep the bounds chosen by the user.
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 160.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_min_height(160.0);
                    ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                    if let Some(diff) = &dialog.preview {
                        render_preview(ui, diff, palette);
                    } else {
                        ui.label("Preview contents will appear here.");
                    }
                });
            }
            let message = &dialog.status;
            let color = palette.muted;
            super::controls::dialog_style::text_line(ui, RichText::new(message).color(color), 26.0).help_text(message);
        });
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy, Button::new(if managing { "Save new stash…" } else { "Saved stashes…" })).clicked() {
                    dialog.managing = !managing;
                    dialog.preview_expanded = false;
                    dialog.invalidate_preview();
                    dialog.status = if dialog.managing { "Select and preview a saved stash." } else { "Preview changes before saving." }.into();
                    reload = dialog.managing;
                }
                if managing && dialog.preview_expanded && ui.add_enabled(!busy, Button::new("Hide preview")).clicked() {
                    dialog.preview_expanded = false;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add_enabled(!busy, dialog_style::button(if managing { "Close" } else { "Cancel" })).disabled_help_text("Wait for the current operation to finish.").dialog_cancel() { close = true; }
                    if !managing {
                        let save_ready = allowed && dialog.prepared_save.as_ref().is_some_and(|prepared| prepared.include_untracked == dialog.include_untracked);
                        if ui.add_enabled(save_ready, dialog_style::primary("Save Stash", palette)).disabled_help_text("Preview local changes before saving; contents are checked again when saving.").dialog_accept() { action = Some(Action::Save); }
                        if ui.add_enabled(allowed, Button::new(if dialog.preview_expanded { "Hide preview" } else { "Preview changes" })).clicked() {
                            dialog.preview_expanded = !dialog.preview_expanded;
                            if dialog.preview_expanded { preview = Some(true); }
                        }
                    }
                });
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

fn message_editor(ui: &mut egui::Ui, message: &mut String, enabled: bool) -> egui::Response {
    // Keep the input border outside the scrolling text so it remains fully visible.
    let frame = egui::Frame::new()
        .fill(ui.visuals().text_edit_bg_color())
        .corner_radius(ui.visuals().widgets.inactive.corner_radius)
        .inner_margin(4)
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("stash.message.scroll")
                .min_scrolled_height(118.0)
                .max_height(118.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_enabled(
                        enabled,
                        TextEdit::multiline(message)
                            .id(egui::Id::unique("stash.message"))
                            .frame(egui::Frame::NONE)
                            .desired_width(f32::INFINITY)
                            .desired_rows(1)
                            .min_size(egui::vec2(0.0, 118.0))
                            .hint_text("Optional stash message"),
                    )
                })
                .inner
        });
    dialog_style::preserve_multiline_enter(&frame.inner);
    let visuals = ui.style().interact(&frame.inner);
    ui.painter().rect_stroke(
        frame.response.rect,
        visuals.corner_radius,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    frame.response
}

fn render_preview(ui: &mut egui::Ui, preview: &DiffPreview, palette: crate::theme::Palette) {
    super::diff_view::render_preview(ui, preview, palette, "stash.contents");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{install_fonts, InterfaceStyle, ThemeChoice};

    #[test]
    fn message_border_is_not_clipped_with_empty_or_scrolling_text() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let ctx = egui::Context::default();
            install_fonts(&ctx);
            theme.apply(&ctx);
            let mut baseline = None;
            for mut message in [String::new(), "Long message line\n".repeat(100)] {
                for focused in [false, true] {
                    for _ in 0..3 {
                        let mut rect = egui::Rect::NOTHING;
                        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                            ui.set_width(470.0);
                            if focused {
                                ui.memory_mut(|memory| {
                                    memory.request_focus(egui::Id::unique("stash.message"))
                                });
                            }
                            let response = message_editor(ui, &mut message, true);
                            rect = response.rect;
                        });
                        output.textures_delta.clear();
                        assert_eq!(rect.height(), 126.0);
                        if let Some(before) = baseline {
                            assert_eq!(rect, before);
                        } else {
                            baseline = Some(rect);
                        }
                        assert!(
                            output.shapes.iter().any(|shape| {
                                matches!(&shape.shape, egui::Shape::Rect(border)
                                if border.rect == rect && border.stroke.width > 0.0
                                    && shape.clip_rect.contains_rect(rect))
                            }),
                            "The complete input border must remain visible"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn stash_dialog_keeps_compact_geometry_and_controls_across_states() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                    for (managing, expanded) in
                        [(false, false), (false, true), (true, false), (true, true)]
                    {
                        let ctx = egui::Context::default();
                        install_fonts(&ctx);
                        egui_extras::install_image_loaders(&ctx);
                        theme.apply(&ctx);
                        let mut app = SannyGitApp::with_theme(theme);
                        app.interface_style = style;
                        app.stash_dialog = Some(crate::mock_data::stash_dialog());
                        app.stash_dialog.as_mut().unwrap().managing = managing;
                        app.stash_dialog.as_mut().unwrap().preview_expanded = expanded;
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
                                    dialog.message = "Long message line\n".repeat(100);
                                    dialog.entries[0].message =
                                        "Very long stash message ".repeat(100);
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
                                    .memory(|memory| {
                                        memory.area_rect(egui::Id::unique(if managing {
                                            "stash.manage.dialog"
                                        } else {
                                            "stash.dialog"
                                        }))
                                    })
                                    .unwrap();
                                assert!(
                                    egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                                        .contains_rect(rect),
                                    "Stash dialog overflow: {rect:?} at {size:?}"
                                );
                                assert!(
                                    rect.height()
                                        < if expanded {
                                            650.0
                                        } else if managing {
                                            450.0
                                        } else {
                                            380.0
                                        },
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
                                                "Hide preview",
                                                "Save Stash",
                                                "Saved stashes",
                                                "Reload list",
                                                "Preview stash",
                                                "Restore staged state",
                                                "Apply",
                                                "Pop",
                                                "Drop",
                                                "Close",
                                                "Cancel",
                                            ]
                                            .contains(&text.galley.job.text.as_str()) =>
                                        {
                                            Some((text.galley.job.text.clone(), text.pos))
                                        }
                                        _ => None,
                                    })
                                    .collect::<Vec<_>>();
                                assert_eq!(
                                    positions.len(),
                                    if managing {
                                        8 + usize::from(expanded)
                                    } else {
                                        6
                                    }
                                );
                                assert!(rect.width() < if managing { 700.0 } else { 520.0 });
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
}
