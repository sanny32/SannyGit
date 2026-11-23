use super::controls::dialog_style::DialogResponse;
use super::controls::{dialog_style, icons::Icon, tooltips::TooltipResponse};
use crate::{
    app::{
        ref_management::{BranchAction, Dialog, Kind, RemoteEditor},
        SannyGitApp,
    },
    branches::{ResetMode, ResetStrategy},
    remote_config,
};
use eframe::egui::{self, Button, ComboBox, RichText};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let busy = app.busy();
    let current = app.state_current;
    let Some(mut dialog) = app.branches.ref_dialog.take() else {
        return;
    };
    let palette = app.palette();
    let (id, title, description, icon, width) = match &dialog.kind {
        Kind::Branch { .. } => {
            show_branch_action(ctx, app, dialog);
            return;
        }
        Kind::Delete { .. } => ("ref.delete", "Delete Local Branch", "Delete a local branch only after it is fully merged into the current HEAD. No force deletion; current and worktree branches are protected.", Icon::Branch, 580.0),
        Kind::DeleteTag { .. } => {
            show_tag_delete(ctx, app, dialog);
            return;
        }
        Kind::Tag { .. } => ("ref.tag.push", "Push Selected Tag", "Push exactly one tag object to one reviewed push address. Other branches and tags are not sent. Existing remote tags are not replaced.", Icon::Tag, 580.0),
        Kind::AddTagRemote => ("ref.tag.add_remote", "Add a Remote?", "Configure a remote for this repository, then run Push again to send the tag.", Icon::Tag, 440.0),
        Kind::Reset { advanced: Some(_), choose_changes: false, .. } => {
            show_advanced_reset(ctx, app, dialog);
            return;
        }
        Kind::Reset { .. } => {
            show_tag_reset(ctx, app, dialog);
            return;
        }
        Kind::Remotes(_) => ("remote.manager", "Manage Remotes", "Edit fetch and push URLs for this repository.", Icon::Repository, 620.0),
    };
    let mut close = false;
    let mut reload = false;
    let mut submit = false;
    let mut add_remote = false;
    let mut config_change = None;
    let needs_remote = matches!(dialog.kind, Kind::AddTagRemote);
    dialog_style::modal(ctx, palette, id).show(ctx, |ui| {
        ui.set_width(width);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header_with_icon(ui, palette, title, description, icon);
        if needs_remote {
            dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
                dialog_style::button_row(ui, |ui| {
                    add_remote = ui
                        .add_enabled(
                            !busy && current,
                            dialog_style::primary("Add Remote", palette),
                        )
                        .dialog_accept();
                    let cancel = ui.add(dialog_style::button("Cancel")).dialog_cancel();
                    close = add_remote || cancel;
                });
            });
            return;
        }
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
                Kind::AddTagRemote
                | Kind::Reset { .. }
                | Kind::DeleteTag { .. }
                | Kind::Branch { .. } => {}
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
            dialog_style::button_row(ui, |ui| {
                let ready = match &dialog.kind {
                    Kind::Delete { prepared, .. } => prepared.is_some(),
                    Kind::Tag { prepared, .. } => prepared.is_some(),
                    Kind::AddTagRemote
                    | Kind::Reset { .. }
                    | Kind::Remotes(_)
                    | Kind::DeleteTag { .. }
                    | Kind::Branch { .. } => false,
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
        app.branches.ref_dialog = Some(dialog);
    }
    if add_remote {
        app.open_remote_manager();
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

fn show_branch_action(ctx: &egui::Context, app: &mut SannyGitApp, mut dialog: Dialog) {
    let Kind::Branch {
        source,
        prepared,
        action,
    } = &mut dialog.kind
    else {
        return;
    };
    let name = source
        .strip_prefix("refs/remotes/")
        .or_else(|| source.strip_prefix("refs/heads/"))
        .unwrap_or(source);
    let owner = prepared
        .as_ref()
        .map(|preview| preview.owner.as_str())
        .or_else(|| name.split_once('/').map(|(remote, _)| remote))
        .unwrap_or("");
    let (title, description, icon, button, dialog_id) = match &*action {
        BranchAction::Delete { .. } => (
            format!("Delete branch '{name}'?"),
            "If the branch remains on the remote, it may come back with the next fetch.",
            Icon::Warning,
            "Delete",
            "ref.remote_branch.delete",
        ),
        BranchAction::Push { .. } => (
            format!("Push '{name}' to remote '{owner}'"),
            "Select the destination ref for the push.",
            Icon::Push,
            "Push",
            "ref.remote_branch.push",
        ),
        BranchAction::PushLocal { .. } => (
            format!("Push branch '{name}'"),
            "Review the remote and destination ref for this branch.",
            Icon::Push,
            "Push",
            "ref.local_branch.push",
        ),
        BranchAction::SetTracking { .. } => (
            format!("Set tracked branch for '{name}'"),
            "Choose the remote branch to track.",
            Icon::Branch,
            "Set Tracked Branch",
            "ref.local_branch.track",
        ),
        BranchAction::StopTracking => (
            format!("Stop tracking for '{name}'?"),
            "Remove this local branch’s upstream configuration.",
            Icon::Branch,
            "Stop Tracking",
            "ref.local_branch.untrack",
        ),
    };
    let palette = app.palette();
    let busy = app.busy();
    let mut close = false;
    let mut submit = false;
    let mut reload = false;
    dialog_style::modal(ctx, palette, dialog_id).show(ctx, |ui| {
        ui.set_width(500.0);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header_with_icon(ui, palette, &title, description, icon);
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 8), |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            let mut ready = prepared.is_some();
            match action {
                BranchAction::Delete { also_remote } => {
                    let label = format!("Delete from remote repository '{owner}'");
                    let supported = prepared.as_ref().is_some_and(|preview| preview.can_delete_remote);
                    let response = ui.add_enabled(!busy && supported, egui::Checkbox::new(also_remote, &label));
                    response.help_text(if ready && !supported {
                        "Deleting on the server requires a configured remote with the standard branch fetch mapping."
                    } else {
                        &label
                    });
                }
                BranchAction::Push { custom, destination } => {
                    push_target(ui, busy, custom, destination, prepared.as_ref().map_or("", |preview| preview.branch.as_str()));
                    ready = prepared.as_ref().is_some_and(|preview| preview.has_owner)
                        && (!*custom || !destination.trim().is_empty());
                }
                BranchAction::PushLocal { remote, custom, destination, choose_destination } => {
                    let options = prepared.as_ref().map_or(&[][..], |preview| preview.destinations.as_slice());
                    ui.horizontal(|ui| {
                        ui.label("Remote");
                        ui.add_enabled_ui(!busy && *choose_destination && !options.is_empty(), |ui| {
                            ComboBox::from_id_salt("local_branch.push_remote").width(240.0)
                                .selected_text(remote.as_str()).show_ui(ui, |ui| {
                                    for name in options {
                                        ui.selectable_value(remote, name.clone(), name.as_str());
                                    }
                                });
                        });
                    });
                    let target = prepared.as_ref().filter(|preview| preview.owner == *remote)
                        .map_or(name, |preview| preview.branch.as_str());
                    if *choose_destination {
                        push_target(ui, busy, custom, destination, target);
                    } else {
                        line(ui, format!("Destination branch · {target}"), palette.text);
                    }
                    ready = options.contains(remote) && (!*custom || !destination.trim().is_empty());
                }
                BranchAction::SetTracking { upstream } => {
                    let options = prepared.as_ref().map_or(&[][..], |preview| preview.tracking_refs.as_slice());
                    let display = |reference: &str| reference.strip_prefix("refs/remotes/").unwrap_or(reference).to_owned();
                    ui.add_enabled_ui(!busy && !options.is_empty(), |ui| {
                        ComboBox::from_id_salt("local_branch.upstream").width(320.0).truncate()
                            .selected_text(if upstream.is_empty() { "Select a remote branch".into() } else { display(upstream) })
                            .show_ui(ui, |ui| {
                                for reference in options {
                                    ui.selectable_value(upstream, reference.clone(), display(reference));
                                }
                            });
                    });
                    ready = options.contains(upstream);
                }
                BranchAction::StopTracking => {}
            }
            let status = if busy {
                "Checking branch…"
            } else if matches!(action, BranchAction::Push { .. } | BranchAction::PushLocal { .. }) && prepared.as_ref().is_some_and(|preview| !preview.has_owner) {
                "The source remote is no longer configured"
            } else {
                ""
            };
            dialog_style::text_line(ui, RichText::new(status).color(palette.muted), 18.0);
            dialog_style::button_row(ui, |ui| {
                submit = ui.add_enabled(
                    !busy && app.state_current && ready,
                    dialog_style::primary(button, palette),
                ).clicked();
                close = ui.add_enabled(!busy, dialog_style::button("Cancel")).dialog_close();
                if !busy && prepared.is_none() && dialog.error.is_some() {
                    reload = ui.add(dialog_style::button("Reload Details")).clicked();
                }
            });
        });
    });
    if !close {
        dialog.confirmed = submit;
        app.branches.ref_dialog = Some(dialog);
    }
    if reload {
        app.reload_ref_dialog();
    }
    if submit && !close {
        app.run_ref_action();
    }
}

fn push_target(
    ui: &mut egui::Ui,
    busy: bool,
    custom: &mut bool,
    destination: &mut String,
    matching: &str,
) {
    egui::Grid::new("remote_branch.push_target")
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label("Push To");
            ui.add_enabled_ui(!busy, |ui| {
                ui.radio_value(custom, false, "Tracked or matching branch")
            })
            .inner
            .help_text(matching);
            ui.end_row();
            ui.label("");
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    ui.radio_value(custom, true, "Custom Ref");
                });
                ui.add_enabled(
                    !busy && *custom,
                    egui::TextEdit::singleline(destination)
                        .hint_text("refs/heads/branch")
                        .desired_width(240.0),
                );
            });
            ui.end_row();
        });
}

fn show_tag_delete(ctx: &egui::Context, app: &mut SannyGitApp, mut dialog: Dialog) {
    let Kind::DeleteTag {
        source,
        prepared,
        also_remote,
    } = &mut dialog.kind
    else {
        return;
    };
    let busy = app.busy();
    let palette = app.palette();
    let remotes = prepared
        .as_ref()
        .map_or(&[][..], |preview| preview.remotes.as_slice());
    let label = match remotes {
        [remote] => format!("Delete from remote '{remote}'"),
        _ => "Delete from all remotes".into(),
    };
    let name = source.strip_prefix("refs/tags/").unwrap_or(source);
    let mut close = false;
    let mut reload = false;
    let mut submit = false;
    dialog_style::modal(ctx, palette, "ref.tag.delete").show(ctx, |ui| {
        ui.set_width(430.0);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header_with_icon(
            ui,
            palette,
            "Delete Tag",
            "This will remove the selected tag from the local repository.",
            Icon::Warning,
        );
        dialog_style::section(ui, palette.panel, egui::Margin::symmetric(18, 8), |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            line(ui, format!("Tag: {name}"), palette.text);
            ui.add_enabled_ui(!busy, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.add_visible(
                    !remotes.is_empty(),
                    egui::Checkbox::new(also_remote, &label),
                )
                .help_text(label);
            });
            let status = if dialog.running {
                "Deleting tag…"
            } else if busy {
                "Checking tag…"
            } else {
                ""
            };
            dialog_style::text_line(ui, RichText::new(status).color(palette.muted), 18.0);
        });
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            dialog_style::button_row(ui, |ui| {
                close = ui
                    .add_enabled(!busy, dialog_style::button("Cancel"))
                    .dialog_close();
                if !busy && prepared.is_none() && dialog.error.is_some() {
                    reload = ui.add(dialog_style::button("Reload Details")).clicked();
                } else {
                    submit = ui
                        .add_enabled(
                            !busy && app.state_current && prepared.is_some(),
                            dialog_style::primary("Delete", palette),
                        )
                        .clicked();
                }
            });
        });
    });
    if !close {
        dialog.confirmed = submit;
        app.branches.ref_dialog = Some(dialog);
    }
    if reload {
        app.reload_ref_dialog();
    }
    if submit && !close {
        app.run_ref_action();
    }
}

fn show_tag_reset(ctx: &egui::Context, app: &mut SannyGitApp, dialog: Dialog) {
    let Kind::Reset {
        source,
        prepared,
        choose_changes,
        ..
    } = &dialog.kind
    else {
        return;
    };
    let palette = app.palette();
    let busy = app.busy();
    let enabled = !busy && app.state_current && prepared.is_some();
    let target = reset_target_text(source, prepared.as_ref());
    let mut close = false;
    let mut reload = false;
    let mut mode = None;
    dialog_style::modal(ctx, palette, "ref.tag.reset").show(ctx, |ui| {
        ui.set_width(490.0);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header_with_icon(
            ui,
            palette,
            reset_heading(source),
            "Move HEAD, the index and the working tree to the selected commit.",
            Icon::Warning,
        );
        dialog_style::section(ui, palette.panel, egui::Margin::symmetric(18, 10), |ui| {
            line(ui, format!("Target: {target}"), palette.text);
            let details = if dialog.running {
                "Resetting HEAD…"
            } else if *choose_changes {
                "Save the staged and unstaged changes to a stash, or discard them."
            } else if prepared.is_none() && busy {
                "Checking the target and working tree…"
            } else {
                "Reset will discard any staged and unstaged changes unless you save them."
            };
            line(ui, details.into(), palette.text);
            line(
                ui,
                "Untracked or ignored files in the way may be overwritten or removed.".into(),
                palette.muted,
            );
        });
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            dialog_style::button_row(ui, |ui| {
                let step = egui::Id::unique("ref.tag.reset.choose_changes");
                let entering_choices = ui.data_mut(|data| {
                    let was_choosing = data.get_temp::<bool>(step).unwrap_or_default();
                    data.insert_temp(step, *choose_changes);
                    *choose_changes && !was_choosing
                });
                close = ui
                    .add_enabled(!busy, dialog_style::button("Cancel"))
                    .dialog_cancel();
                if *choose_changes {
                    if ui
                        .push_id("reset.discard", |ui| {
                            ui.add_enabled(enabled, dialog_style::button("Discard"))
                        })
                        .inner
                        .clicked()
                    {
                        mode = Some(ResetMode::Discard);
                    }
                    let save = ui
                        .push_id("reset.save_stash", |ui| {
                            ui.add_enabled(enabled, dialog_style::primary("Save Stash", palette))
                        })
                        .inner;
                    if entering_choices && enabled {
                        save.request_focus();
                    }
                    if save.dialog_accept() {
                        mode = Some(ResetMode::SaveStash);
                    }
                } else if !busy && prepared.is_none() && dialog.error.is_some() {
                    reload = ui.add(dialog_style::button("Reload Details")).clicked();
                } else if ui
                    .push_id("reset.confirm", |ui| {
                        ui.add_enabled(enabled, dialog_style::primary("Reset", palette))
                    })
                    .inner
                    .dialog_accept()
                {
                    mode = Some(ResetMode::Clean);
                }
            });
        });
    });
    if !close {
        app.branches.ref_dialog = Some(dialog);
    }
    if reload {
        app.reload_ref_dialog();
    }
    if let Some(mode) = mode {
        app.confirm_tag_reset(mode);
    }
}

fn show_advanced_reset(ctx: &egui::Context, app: &mut SannyGitApp, mut dialog: Dialog) {
    let Kind::Reset {
        source,
        prepared,
        advanced: Some(options),
        ..
    } = &mut dialog.kind
    else {
        return;
    };
    ctx.data_mut(|data| {
        data.remove::<bool>(egui::Id::unique("ref.tag.reset.choose_changes"));
    });
    let palette = app.palette();
    let busy = app.busy();
    let enabled = !busy && app.state_current && prepared.is_some();
    let target = reset_target_text(source, prepared.as_ref());
    let mut close = false;
    let mut reload = false;
    let mut submit = false;
    dialog_style::modal(ctx, palette, "ref.tag.reset.advanced").show(ctx, |ui| {
        ui.set_width(560.0);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header_with_icon(
            ui,
            palette,
            "Reset Advanced",
            "Move HEAD to the selected commit. Choose what to update in the index and working tree.",
            Icon::Warning,
        );
        dialog_style::section(ui, palette.panel, egui::Margin::symmetric(18, 8), |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.spacing_mut().interact_size.y = 18.0;
            ui.spacing_mut().button_padding.y = 1.0;
            let target = format!("Target: {target}");
            dialog_style::text_line(ui, RichText::new(&target).color(palette.text), 20.0)
                .help_text(target);
            ui.add_space(3.0);
            ui.add_enabled_ui(!busy, |ui| {
                for (strategy, label, description) in [
                    (
                        ResetStrategy::Mixed,
                        "Mixed · Reset the index",
                        "Keep working files. Replace staged changes with the target commit.",
                    ),
                    (
                        ResetStrategy::Soft,
                        "Soft · Keep the index and working tree",
                        "Staged changes are shown relative to the new HEAD.",
                    ),
                    (
                        ResetStrategy::Hard,
                        "Hard · Reset the index and working tree",
                        "Save tracked changes to a stash or discard them before resetting.",
                    ),
                    (
                        ResetStrategy::Keep,
                        "Keep · Preserve local changes",
                        "Abort if updating a file would overwrite local changes.",
                    ),
                    (
                        ResetStrategy::Merge,
                        "Merge · Preserve unstaged changes",
                        "Abort if a file that needs updating has conflicting unstaged changes.",
                    ),
                ] {
                    ui.radio_value(&mut options.strategy, strategy, label);
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        dialog_style::text_line(
                            ui,
                            RichText::new(description).color(palette.muted),
                            17.0,
                        )
                        .help_text(description);
                    });
                    if strategy == ResetStrategy::Hard {
                        ui.horizontal(|ui| {
                            ui.add_space(20.0);
                            let hard = options.strategy == ResetStrategy::Hard;
                            ui.add_enabled(
                                hard,
                                egui::Checkbox::new(
                                    &mut options.fix_line_endings,
                                    "Refresh line endings using .gitattributes",
                                ),
                            );
                        });
                    }
                    ui.add_space(5.0);
                }
            });
            let notice = if dialog.running {
                "Resetting HEAD…"
            } else if prepared.is_none() && busy {
                "Checking the target and working tree…"
            } else if options.strategy == ResetStrategy::Hard {
                "Untracked or ignored files in the way may be overwritten or removed."
            } else {
                ""
            };
            dialog_style::text_line(ui, RichText::new(notice).color(palette.muted), 18.0)
                .help_text(notice);
        });
        dialog_style::section(ui, palette.header, egui::Margin::symmetric(18, 10), |ui| {
            dialog_style::button_row(ui, |ui| {
                close = ui
                    .add_enabled(!busy, dialog_style::button("Cancel"))
                    .dialog_cancel();
                if !busy && prepared.is_none() && dialog.error.is_some() {
                    reload = ui.add(dialog_style::button("Reload Details")).clicked();
                } else {
                    submit = ui
                        .add_enabled(enabled, dialog_style::primary("Reset", palette))
                        .dialog_accept();
                }
            });
        });
    });
    if !close {
        app.branches.ref_dialog = Some(dialog);
    }
    if reload {
        app.reload_ref_dialog();
    }
    if submit {
        app.confirm_tag_reset(ResetMode::Clean);
    }
}

fn reset_target_label(source: &str) -> String {
    if crate::branches::is_commit_id(source) {
        source.get(..12).unwrap_or(source).to_owned()
    } else {
        source
            .strip_prefix("refs/tags/")
            .or_else(|| source.strip_prefix("refs/remotes/"))
            .or_else(|| source.strip_prefix("refs/heads/"))
            .unwrap_or(source)
            .to_owned()
    }
}

fn reset_heading(source: &str) -> &'static str {
    if source.starts_with("refs/tags/") {
        "Reset to Tag"
    } else if crate::branches::is_commit_id(source) {
        "Reset to Commit"
    } else {
        "Reset to Branch"
    }
}

fn reset_target_text(source: &str, prepared: Option<&crate::branches::ResetPreview>) -> String {
    let name = reset_target_label(source);
    match prepared {
        Some(preview) if crate::branches::is_commit_id(source) => preview.target.clone(),
        Some(preview) => format!(
            "{name} · {}",
            preview.target.get(..8).unwrap_or(&preview.target)
        ),
        None => name,
    }
}

fn line(ui: &mut egui::Ui, text: String, color: egui::Color32) {
    dialog_style::text_line(ui, RichText::new(&text).color(color), 26.0).help_text(text);
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
                            crate::ui::controls::singleline(&mut editor.name).char_limit(4096),
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
                        crate::ui::controls::singleline(&mut editor.url)
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
                        crate::ui::controls::singleline(&mut editor.push_url)
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
    use crate::test_support::SYSTEM_GIT;
    use crate::{
        mock_data::{self, RefDialogFixture},
        theme::{install_fonts, InterfaceStyle, ThemeChoice},
    };

    #[test]
    fn tag_delete_keeps_its_layout_and_enter_defaults_to_cancel() {
        let repo = crate::test_support::TempRepo::new();
        repo.git(&["commit", "--allow-empty", "-m", "base"]);
        repo.git(&["tag", "release"]);
        repo.git(&["remote", "add", "origin", "https://example.test/repo"]);
        let source = "refs/tags/release";
        let prepared = crate::remotes::preview_tag_delete(&SYSTEM_GIT, &repo.0, source).unwrap();
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                let ctx = egui::Context::default();
                install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.state_current = true;
                app.branches.ref_dialog = Some(Dialog {
                    root: repo.0.to_string_lossy().into_owned(),
                    kind: Kind::DeleteTag {
                        source: source.into(),
                        prepared: None,
                        also_remote: false,
                    },
                    confirmed: false,
                    running: false,
                    reload_pending: false,
                    error: None,
                    status: String::new(),
                });
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                let render = |app: &mut SannyGitApp, events| {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            events,
                            ..Default::default()
                        },
                        |ui| show(ui.ctx(), app),
                    );
                    output.textures_delta.clear();
                    output
                };
                for _ in 0..3 {
                    render(&mut app, Vec::new());
                }
                let id = egui::Id::unique("ref.tag.delete");
                let loading = ctx.memory(|memory| memory.area_rect(id)).unwrap();
                if let Kind::DeleteTag { prepared: slot, .. } =
                    &mut app.branches.ref_dialog.as_mut().unwrap().kind
                {
                    *slot = Some(prepared.clone());
                }
                let ready = render(&mut app, Vec::new());
                assert_eq!(ctx.memory(|memory| memory.area_rect(id)).unwrap(), loading);
                assert!(screen.contains_rect(loading));
                assert!(ready.shapes.iter().any(|shape| matches!(
                    &shape.shape, egui::Shape::Text(text) if text.galley.text() == "Delete from remote 'origin'"
                )));
                render(
                    &mut app,
                    vec![egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
                assert!(app.branches.ref_dialog.is_none());
                assert!(!app.busy());
            }
        }
        assert_eq!(repo.git(&["tag", "--list"]).trim(), "release");
    }

    #[test]
    fn remote_branch_dialogs_use_the_same_geometry_after_loading() {
        let repo = crate::test_support::TempRepo::new();
        repo.git(&["commit", "--allow-empty", "-m", "base"]);
        repo.git(&["remote", "add", "origin", "https://example.test/repo"]);
        let source = "refs/remotes/origin/team/topic";
        repo.git(&["update-ref", source, "HEAD"]);
        let preview = crate::remotes::preview_branch(&SYSTEM_GIT, &repo.0, source).unwrap();
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                let ctx = egui::Context::default();
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                for (id, action) in [
                    (
                        "ref.remote_branch.delete",
                        BranchAction::Delete { also_remote: false },
                    ),
                    (
                        "ref.remote_branch.push",
                        BranchAction::Push {
                            custom: false,
                            destination: String::new(),
                        },
                    ),
                ] {
                    let mut previous = None;
                    for prepared in [None, Some(preview.clone())] {
                        app.branches.ref_dialog = Some(Dialog {
                            root: repo.0.to_string_lossy().into_owned(),
                            kind: Kind::Branch {
                                source: source.into(),
                                prepared,
                                action: action.clone(),
                            },
                            confirmed: false,
                            running: false,
                            reload_pending: false,
                            error: None,
                            status: String::new(),
                        });
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
                            .memory(|memory| memory.area_rect(egui::Id::unique(id)))
                            .unwrap();
                        assert!(screen.contains_rect(rect));
                        if let Some(previous) = previous {
                            assert_eq!(rect, previous);
                        }
                        previous = Some(rect);
                    }
                }
            }
        }
    }

    #[test]
    fn advanced_reset_stays_compact_when_switching_modes() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                let ctx = egui::Context::default();
                install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                let mut previous = None;
                for strategy in [ResetStrategy::Mixed, ResetStrategy::Hard] {
                    app.branches.ref_dialog = Some(Dialog {
                        root: "Fixture".into(),
                        kind: Kind::Reset {
                            source: "refs/tags/release".into(),
                            prepared: None,
                            choose_changes: false,
                            advanced: Some(crate::branches::AdvancedReset {
                                strategy,
                                fix_line_endings: false,
                            }),
                        },
                        confirmed: false,
                        running: false,
                        reload_pending: false,
                        error: None,
                        status: String::new(),
                    });
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
                        .memory(|memory| {
                            memory.area_rect(egui::Id::unique("ref.tag.reset.advanced"))
                        })
                        .unwrap();
                    assert!(screen.contains_rect(rect));
                    assert!(
                        rect.height() < 470.0,
                        "Reset Advanced is too tall: {rect:?}"
                    );
                    if let Some(previous) = previous {
                        assert_eq!(previous, rect);
                    }
                    previous = Some(rect);
                }
            }
        }
    }

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
                        let (_repo, dialog) = mock_data::ref_dialog(&SYSTEM_GIT, fixture);
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
                        app.branches.ref_dialog = Some(dialog);
                        let mut baseline = None;
                        let mut buttons = None;
                        for stage in 0..6 {
                            let dialog = app.branches.ref_dialog.as_mut().unwrap();
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
                                Kind::AddTagRemote
                                | Kind::Reset { .. }
                                | Kind::DeleteTag { .. }
                                | Kind::Branch { .. } => {}
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
            let (repo, mut dialog) = mock_data::ref_dialog(&SYSTEM_GIT, fixture);
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
            app.branches.ref_dialog = Some(dialog);
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
            assert!(app.branches.ref_dialog.is_none());
            assert!(!app.busy());
            assert_eq!(repo.git(&["show-ref"]), refs);
            assert_eq!(repo.git(&["config", "--local", "--list"]), config);
        }
    }
}
