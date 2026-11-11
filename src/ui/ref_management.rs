use super::controls::dialog_style::DialogResponse;
use super::controls::{dialog_style, icons::Icon, tooltips::TooltipResponse};
use crate::{
    app::{
        ref_management::{Kind, RemoteEditor},
        SannyGitApp,
    },
    remote_config,
};
use eframe::egui::{self, Align, Button, ComboBox, Label, Layout, RichText, TextEdit};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let busy = app.busy();
    let current = app.state_current;
    let Some(mut dialog) = app.ref_dialog.take() else {
        return;
    };
    let palette = app.palette();
    let (id, title, description, icon, width) = match &dialog.kind {
        Kind::Delete { .. } => ("ref.delete", "Delete Local Branch", "Delete a local branch only after it is fully merged into the current HEAD. No force deletion; current and worktree branches are protected.", Icon::Branch, 580.0),
        Kind::Tag { .. } => ("ref.tag.push", "Push Selected Tag", "Push exactly one tag object to one reviewed push address. Other branches and tags are not sent. Existing remote tags are not replaced.", Icon::Tag, 580.0),
        Kind::Remotes(_) => ("remote.manager", "Manage Remotes", "Edit fetch and push URLs for this repository.", Icon::Repository, 620.0),
    };
    let mut close = false;
    let mut reload = false;
    let mut submit = false;
    let mut config_change = None;
    dialog_style::modal(ctx, palette, id).show(ctx, |ui| {
        ui.set_width(width);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header_with_icon(ui, palette, title, description, icon);
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            if !matches!(dialog.kind, Kind::Remotes(_)) {
                line(ui, format!("Repository: {:?}", dialog.root), palette.muted);
            }
            match &mut dialog.kind {
                Kind::Delete { source, prepared } => {
                    line(ui, source.clone(), palette.text);
                    line(
                        ui,
                        format!(
                            "Branch object: {}",
                            prepared
                                .as_ref()
                                .map(|value| value.oid.as_str())
                                .unwrap_or("Checking…")
                        ),
                        palette.muted,
                    );
                    line(
                        ui,
                        format!(
                            "Current HEAD: {}",
                            prepared
                                .as_ref()
                                .map(|value| value.head.as_str())
                                .unwrap_or("Checking…")
                        ),
                        palette.muted,
                    );
                    ui.add_enabled(
                        !busy && prepared.is_some(),
                        egui::Checkbox::new(
                            &mut dialog.confirmed,
                            "Confirm deleting this merged local branch",
                        ),
                    );
                }
                Kind::Tag {
                    source,
                    names,
                    remote,
                    prepared,
                } => {
                    line(ui, source.clone(), palette.text);
                    line(
                        ui,
                        format!(
                            "Tag object: {}",
                            prepared
                                .as_ref()
                                .map(|value| value.oid.as_str())
                                .unwrap_or("Checking…")
                        ),
                        palette.muted,
                    );
                    ui.horizontal(|ui| {
                        ui.label("Remote:");
                        ui.add_enabled_ui(!busy && !names.is_empty(), |ui| {
                            ComboBox::from_id_salt("ref.tag.remote")
                                .width(320.0)
                                .truncate()
                                .selected_text(remote.as_str())
                                .show_ui(ui, |ui| {
                                    for name in names {
                                        if ui
                                            .selectable_value(remote, name.clone(), name.as_str())
                                            .changed()
                                        {
                                            *prepared = None;
                                            dialog.confirmed = false;
                                            reload = true;
                                        }
                                    }
                                });
                        });
                    });
                    line(
                        ui,
                        format!(
                            "Push address: {}",
                            prepared
                                .as_ref()
                                .map(|value| value.endpoint_display.as_str())
                                .unwrap_or("Not yet verified")
                        ),
                        palette.muted,
                    );
                    ui.add_enabled(
                        !busy && prepared.is_some(),
                        egui::Checkbox::new(
                            &mut dialog.confirmed,
                            "Confirm pushing only this tag to the displayed address",
                        ),
                    );
                }
                Kind::Remotes(editor) => {
                    config_change = remote_editor(ui, editor, busy, current);
                }
            }
            let message = if dialog.running {
                app.status.as_str()
            } else if dialog.status.is_empty() {
                if let Kind::Remotes(editor) = &dialog.kind {
                    editor
                        .snapshot
                        .as_ref()
                        .and_then(|snapshot| {
                            snapshot
                                .entries
                                .iter()
                                .find(|entry| Some(&entry.name) == editor.selected.as_ref())
                        })
                        .map_or("", |entry| entry.note.as_str())
                } else {
                    ""
                }
            } else {
                dialog.status.as_str()
            };
            line(ui, message.into(), palette.muted);
        });
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let ready = match &dialog.kind {
                    Kind::Delete { prepared, .. } => prepared.is_some(),
                    Kind::Tag { prepared, .. } => prepared.is_some(),
                    Kind::Remotes(_) => false,
                };
                if !matches!(dialog.kind, Kind::Remotes(_))
                    && ui
                        .add_enabled(
                            !busy && current && ready && dialog.confirmed,
                            dialog_style::primary(
                                if matches!(dialog.kind, Kind::Delete { .. }) {
                                    "Delete Branch"
                                } else {
                                    "Push Tag"
                                },
                                palette,
                            ),
                        )
                        .dialog_accept()
                {
                    submit = true;
                }
                if ui
                    .add_enabled(
                        !dialog.running || matches!(dialog.kind, Kind::Tag { .. }),
                        dialog_style::button(if matches!(dialog.kind, Kind::Remotes(_)) {
                            "Close"
                        } else {
                            "Cancel"
                        }),
                    )
                    .dialog_cancel()
                {
                    if dialog.running && matches!(dialog.kind, Kind::Tag { .. }) {
                        app.cancel_repository_job();
                    } else {
                        close = true;
                    }
                }
                if ui
                    .add_enabled(!busy, dialog_style::button("Reload details"))
                    .clicked()
                {
                    reload = true;
                }
            });
        });
    });
    if !close {
        app.ref_dialog = Some(dialog);
    }
    if reload {
        app.reload_ref_dialog();
    }
    if submit {
        app.run_ref_action();
    }
    if let Some(remove) = config_change {
        app.change_remote(remove);
    }
}

fn line(ui: &mut egui::Ui, text: String, color: egui::Color32) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 26.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.add(
                Label::new(RichText::new(&text).color(color))
                    .truncate()
                    .show_tooltip_when_elided(false),
            )
            .help_text(text);
        },
    );
}

fn remote_editor(
    ui: &mut egui::Ui,
    editor: &mut RemoteEditor,
    busy: bool,
    current: bool,
) -> Option<bool> {
    let mut change = None;
    ui.horizontal(|ui| {
        ui.strong("Configured remotes");
        if ui.add_enabled(!busy, Button::new("New Remote")).clicked() {
            editor.select(None);
        }
    });
    let mut selected = None;
    egui::ScrollArea::vertical()
        .id_salt("remote.manager.list")
        .min_scrolled_height(58.0)
        .max_height(58.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if let Some(snapshot) = &editor.snapshot {
                if snapshot.entries.is_empty() {
                    ui.label("No configured remotes");
                }
                for entry in &snapshot.entries {
                    let label = format!("{}   {}", entry.name, entry.fetch_display);
                    if ui
                        .add_enabled(
                            !busy,
                            Button::selectable(
                                editor.selected.as_deref() == Some(&entry.name),
                                &label,
                            )
                            .wrap_mode(egui::TextWrapMode::Truncate),
                        )
                        .help_text(format!(
                            "Fetch: {}\nPush: {}\n{}",
                            entry.fetch_display, entry.push_display, entry.note
                        ))
                        .clicked()
                    {
                        selected = Some(entry.name.clone());
                    }
                }
            } else {
                ui.label("Loading remotes…");
            }
        });
    if let Some(name) = selected {
        editor.select(Some(name));
    }
    let entry = editor.selected.as_deref().and_then(|name| {
        editor
            .snapshot
            .as_ref()?
            .entries
            .iter()
            .find(|entry| entry.name == name)
    });
    let can_edit = entry.is_none_or(|entry| entry.can_edit) && editor.current && !busy;
    let can_remove = entry.is_some_and(|entry| entry.can_remove) && editor.current && !busy;
    let field_width = ui.available_width() - 102.0;
    ui.add_enabled_ui(can_edit, |ui| {
        egui::Grid::new("remote.manager.fields")
            .min_col_width(90.0)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Name:");
                if ui
                    .add_enabled_ui(editor.selected.is_none(), |ui| {
                        ui.add_sized(
                            [field_width, 26.0],
                            TextEdit::singleline(&mut editor.name).char_limit(4096),
                        )
                    })
                    .inner
                    .changed()
                {
                    editor.confirm_remove = false;
                }
                ui.end_row();
                ui.label("Fetch URL:");
                let masked =
                    !editor.url.is_empty() && remote_config::url_hint(&editor.url).is_some();
                if ui
                    .add_sized(
                        [field_width, 26.0],
                        TextEdit::singleline(&mut editor.url)
                            .char_limit(4096)
                            .password(masked),
                    )
                    .changed()
                {
                    editor.confirm_remove = false;
                }
                ui.end_row();
                ui.label("Push URL:");
                let masked = !editor.push_url.is_empty()
                    && remote_config::url_hint(&editor.push_url).is_some();
                if ui
                    .add_sized(
                        [field_width, 26.0],
                        TextEdit::singleline(&mut editor.push_url)
                            .char_limit(4096)
                            .password(masked)
                            .hint_text("Same as fetch when empty"),
                    )
                    .changed()
                {
                    editor.confirm_remove = false;
                }
                ui.end_row();
            });
    });
    let validation = remote_config::name_hint(&editor.name)
        .or_else(|| remote_config::url_hint(&editor.url))
        .or_else(|| {
            if editor.push_url.is_empty() {
                None
            } else {
                remote_config::url_hint(&editor.push_url)
            }
        });
    ui.horizontal(|ui| {
        let valid = can_edit && current && validation.is_none();
        if ui.add_enabled(valid && editor.selected.is_none(), Button::new("Add Remote")).disabled_help_text(validation.unwrap_or("Choose New Remote to add an entry")).dialog_accept() { change = Some(false); }
        if ui.add_enabled(valid && editor.selected.is_some(), Button::new("Save URLs")).disabled_help_text(validation.unwrap_or("Select an editable repository-local remote")).dialog_accept() { change = Some(false); }
        if ui.add_enabled(can_remove && current && editor.confirm_remove, Button::new("Remove Remote")).disabled_help_text("Select a repository-local remote with standard fetch destinations and confirm removal below").clicked() { change = Some(true); }
    });
    ui.scope(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        let text = format!(
            "Confirm removing '{}' and its cached remote-tracking refs",
            editor.selected.as_deref().unwrap_or("selected remote")
        );
        ui.add_enabled(
            can_remove,
            egui::Checkbox::new(&mut editor.confirm_remove, &text),
        )
        .help_text(text);
    });
    change
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mock_data::{self, RefDialogFixture},
        theme::{install_fonts, InterfaceStyle, ThemeChoice},
    };

    #[test]
    fn ref_dialogs_keep_geometry_across_states_and_never_render_credentials() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                    for fixture in [
                        RefDialogFixture::Delete,
                        RefDialogFixture::Tag,
                        RefDialogFixture::Remotes,
                    ] {
                        let ctx = egui::Context::default();
                        install_fonts(&ctx);
                        egui_extras::install_image_loaders(&ctx);
                        theme.apply(&ctx);
                        let (_repo, dialog) = mock_data::ref_dialog(fixture);
                        let saved_delete = if let Kind::Delete { prepared, .. } = &dialog.kind {
                            prepared.clone()
                        } else {
                            None
                        };
                        let saved_tag = if let Kind::Tag { prepared, .. } = &dialog.kind {
                            prepared.clone()
                        } else {
                            None
                        };
                        let saved_config = if let Kind::Remotes(editor) = &dialog.kind {
                            editor.snapshot.clone()
                        } else {
                            None
                        };
                        let id = match fixture {
                            RefDialogFixture::Delete => "ref.delete",
                            RefDialogFixture::Tag => "ref.tag.push",
                            RefDialogFixture::Remotes => "remote.manager",
                        };
                        let mut app = SannyGitApp::with_theme(theme);
                        app.interface_style = style;
                        app.state_current = true;
                        app.ref_dialog = Some(dialog);
                        let mut baseline = None;
                        let mut buttons = None;
                        for stage in 0..6 {
                            let dialog = app.ref_dialog.as_mut().unwrap();
                            dialog.running = stage == 3;
                            dialog.confirmed = stage >= 2;
                            dialog.error = if stage == 4 {
                                Some("Operation could not complete. Reload details before retrying. ".repeat(100))
                            } else {
                                None
                            };
                            app.status = "Operation in progress…".into();
                            match &mut dialog.kind {
                                Kind::Delete { source, prepared } => {
                                    *prepared = if stage == 0 {
                                        None
                                    } else {
                                        saved_delete.clone()
                                    };
                                    if stage == 5 {
                                        *source =
                                            format!("refs/heads/{}", "long-name/".repeat(100));
                                    }
                                }
                                Kind::Tag { prepared, .. } => {
                                    *prepared = if stage == 0 { None } else { saved_tag.clone() };
                                    if stage == 5 {
                                        prepared.as_mut().unwrap().endpoint_display =
                                            "ssh://host/long-path/".repeat(100);
                                    }
                                }
                                Kind::Remotes(editor) => {
                                    editor.snapshot = if stage == 0 {
                                        None
                                    } else {
                                        saved_config.clone()
                                    };
                                    editor.current = stage > 0;
                                    editor.confirm_remove = stage == 2;
                                    if stage == 5 {
                                        editor.url = "https://dummy-user:dummy-password@example.test/repo?private-query".into();
                                    }
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
                                    .memory(|memory| memory.area_rect(egui::Id::unique(id)))
                                    .unwrap();
                                assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                                    .contains_rect(rect));
                                assert!(rect.height() < 610.0);
                                if matches!(fixture, RefDialogFixture::Remotes) {
                                    assert!(
                                        rect.width() <= 624.0 && rect.height() < 470.0,
                                        "Remote manager is oversized: {rect:?}"
                                    );
                                }
                                if let Some(before) = baseline {
                                    assert_eq!(rect, before, "Dialog changed at stage {stage}: {fixture:?} {theme:?} {style:?}");
                                } else {
                                    baseline = Some(rect);
                                }
                                let texts = output
                                    .shapes
                                    .iter()
                                    .filter_map(|shape| {
                                        if let egui::Shape::Text(text) = &shape.shape {
                                            Some(text)
                                        } else {
                                            None
                                        }
                                    })
                                    .collect::<Vec<_>>();
                                if matches!(fixture, RefDialogFixture::Remotes) && stage == 1 {
                                    for label in ["Name:", "Fetch URL:", "Push URL:"] {
                                        let text = texts
                                            .iter()
                                            .find(|text| text.galley.job.text == label)
                                            .unwrap();
                                        let center_y = text.pos.y + text.galley.size().y / 2.0;
                                        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                                            egui::Shape::Rect(frame) if frame.rect.width() >= 400.0
                                                && frame.rect.height() <= 32.0
                                                && (frame.rect.center().y - center_y).abs() < 8.0
                                                && frame.rect.left() > text.pos.x
                                        )), "Narrow remote field: {label}");
                                    }
                                }
                                for secret in [
                                    "dummy-user",
                                    "dummy-password",
                                    "private-query",
                                    "private-fragment",
                                ] {
                                    assert!(
                                        !texts.iter().any(|text| text
                                            .galley
                                            .job
                                            .text
                                            .contains(secret)),
                                        "Credential rendered in {fixture:?}"
                                    );
                                }
                                let positions = texts
                                    .iter()
                                    .filter(|text| {
                                        [
                                            "Delete Branch",
                                            "Push Tag",
                                            "Close",
                                            "Cancel",
                                            "Reload details",
                                            "New Remote",
                                            "Name:",
                                            "Fetch URL:",
                                            "Push URL:",
                                            "Add Remote",
                                            "Save URLs",
                                            "Remove Remote",
                                        ]
                                        .contains(&text.galley.job.text.as_str())
                                    })
                                    .map(|text| (text.galley.job.text.clone(), text.pos))
                                    .collect::<Vec<_>>();
                                if let Some(before) = &buttons {
                                    assert_eq!(
                                        &positions, before,
                                        "Controls moved at stage {stage}: {fixture:?}"
                                    );
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
    fn cancelling_confirmed_branch_or_tag_dialog_does_not_run_a_write() {
        for fixture in [RefDialogFixture::Delete, RefDialogFixture::Tag] {
            let ctx = egui::Context::default();
            install_fonts(&ctx);
            egui_extras::install_image_loaders(&ctx);
            let (repo, mut dialog) = mock_data::ref_dialog(fixture);
            dialog.confirmed = true;
            let refs = repo.git(&["show-ref"]);
            let config = repo.git(&["config", "--local", "--list"]);
            let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
            app.state_current = true;
            app.repositories.push(crate::models::Repository {
                name: "Fixture".into(),
                path: dialog.root.clone(),
            });
            app.selected_repository = 0;
            app.ref_dialog = Some(dialog);
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
            let position = output
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape {
                        (text.galley.job.text == "Cancel")
                            .then(|| text.pos + text.galley.size() / 2.0)
                    } else {
                        None
                    }
                })
                .unwrap();
            for pressed in [true, false] {
                render(
                    &mut app,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert!(app.ref_dialog.is_none());
            assert!(!app.busy());
            assert_eq!(repo.git(&["show-ref"]), refs);
            assert_eq!(repo.git(&["config", "--local", "--list"]), config);
        }
    }
}
