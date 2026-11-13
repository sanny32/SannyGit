use super::controls::dialog_style::DialogResponse;
use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::SannyGitApp,
    models::{BranchRef, RefKind},
    theme::{semibold_family, Palette},
};
use eframe::egui::{
    Align, Button, CollapsingHeader, Layout, RichText, ScrollArea, TextEdit, Ui, Vec2,
};

#[derive(Debug, PartialEq, Eq)]
enum BranchRequest {
    Switch(String),
    Rename(String),
    Delete(String),
    PushTag(String),
}

#[derive(Default)]
struct BranchRequests {
    action: Option<BranchRequest>,
    rename_allowed: bool,
}

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    create_dialog(ui, app);
    rename_dialog(ui, app);
    tag_dialog(ui, app);
    remote_dialog(ui, app);
    let palette = app.palette();
    ui.horizontal(|ui| {
        ui.set_min_height(26.0);
        let heading = ui.heading(
            RichText::new("Branches").color(if app.refs_error.is_some() {
                palette.removed_text
            } else {
                palette.text
            }),
        );
        if app.refs_error.is_some() {
            heading.help_text("Reload branches to retry.");
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_enabled_ui(
                !app.busy() && app.repositories.get(app.selected_repository).is_some(),
                |ui| {
                    if icons::button(ui, Icon::Plus, "Create local branch from HEAD").clicked() {
                        app.branch_dialog_open = true;
                    }
                    if icons::button(ui, Icon::Fetch, "Reload branches and tags").clicked() {
                        app.reload_refs();
                    }
                },
            );
        });
    });
    let mut requested = BranchRequests {
        rename_allowed: !app.busy() && app.state_current && app.refs_current,
        ..Default::default()
    };
    let mut add_tag_requested = false;
    let height = ui.available_height();
    ScrollArea::vertical()
        .id_salt("branches")
        .max_height(height)
        .min_scrolled_height(0.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for kind in [RefKind::Local, RefKind::Remote, RefKind::Tag] {
                add_tag_requested |= branch_group(
                    ui,
                    &app.refs,
                    &mut app.selected_branch,
                    &mut app.selected_ref_group,
                    kind,
                    palette,
                    &mut requested,
                );
            }
        });
    if let Some(request) = requested.action {
        app.selected_ref_group = None;
        match request {
            BranchRequest::Switch(target) => {
                app.selected_branch = target;
                if let Some(reason) = app.switch_branch_blocker() {
                    app.error = Some(reason.into());
                } else {
                    app.switch_branch();
                }
            }
            BranchRequest::Delete(target) => {
                app.selected_branch = target;
                app.open_delete_branch();
            }
            BranchRequest::PushTag(target) => {
                app.selected_branch = target;
                app.open_push_tag();
            }
            BranchRequest::Rename(target) => {
                app.selected_branch = target;
                app.open_rename_branch();
            }
        }
    }
    if add_tag_requested {
        app.tag_dialog_open = true;
        app.error = None;
    }
}

fn branch_group(
    ui: &mut Ui,
    refs: &[BranchRef],
    selected: &mut String,
    selected_group: &mut Option<RefKind>,
    kind: RefKind,
    palette: Palette,
    requested: &mut BranchRequests,
) -> bool {
    let title = match kind {
        RefKind::Local => "Local Branches",
        RefKind::Remote => "Remote Branches",
        RefKind::Tag => "Tags",
    };
    let matching: Vec<_> = refs.iter().filter(|r| r.kind == kind).collect();
    if matching.is_empty() && kind != RefKind::Tag {
        return false;
    }
    let mut add_tag_requested = false;
    let previous_visuals = ui.visuals().clone();
    let fill = if *selected_group == Some(kind) {
        palette.selection
    } else {
        eframe::egui::Color32::TRANSPARENT
    };
    ui.visuals_mut().collapsing_header_frame = true;
    let widgets = &mut ui.visuals_mut().widgets;
    for visuals in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
    ] {
        visuals.weak_bg_fill = fill;
        visuals.bg_stroke = eframe::egui::Stroke::NONE;
        visuals.corner_radius = eframe::egui::CornerRadius::same(0);
    }
    let body_visuals = previous_visuals.clone();
    let group = CollapsingHeader::new(
        RichText::new(format!("{title} ({})", matching.len()))
            .text_style(eframe::egui::TextStyle::Body),
    )
    .id_salt(title)
    .default_open(kind == RefKind::Local)
    .show_unindented(ui, |ui| {
        *ui.visuals_mut() = body_visuals;
        for branch in matching {
            // Avoid text layout for rows outside the viewport in large ref lists.
            let size = Vec2::new(ui.available_width(), 22.0);
            let rect = eframe::egui::Rect::from_min_size(ui.next_widget_position(), size);
            if !ui.is_rect_visible(rect) {
                ui.allocate_space(size);
                continue;
            }
            let response = super::controls::selectable_row(
                ui,
                selected_group.is_none() && *selected == branch.full_name,
                22.0,
                |ui| {
                    ui.add_space(ui.spacing().indent);
                    icons::paint(
                        ui,
                        if kind == RefKind::Tag {
                            Icon::Tag
                        } else {
                            Icon::Branch
                        },
                        palette.text,
                        14.0,
                    );
                    ui.add(
                        eframe::egui::Label::new(if branch.current {
                            RichText::new(&branch.name)
                                .text_style(eframe::egui::TextStyle::Body)
                                .family(semibold_family())
                        } else {
                            RichText::new(&branch.name).text_style(eframe::egui::TextStyle::Body)
                        })
                        .show_tooltip_when_elided(false)
                        .sense(eframe::egui::Sense::hover()),
                    );
                    if branch.current {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            icons::paint(ui, Icon::Star, palette.accent, 14.0)
                        });
                    }
                },
            )
            .help_text(format!(
                "{}\nObject: {}{}{}",
                branch.full_name,
                branch.hash,
                branch
                    .upstream
                    .as_ref()
                    .map(|s| format!("\nUpstream: {s}"))
                    .unwrap_or_default(),
                match kind {
                    RefKind::Local => "\nDouble-click to switch",
                    RefKind::Remote =>
                        "\nDouble-click to switch to a local branch or choose checkout options",
                    RefKind::Tag => "\nDouble-click to check out a detached HEAD",
                }
            ));
            if kind == RefKind::Local {
                response.context_menu(|ui| {
                    if ui
                        .add_enabled(requested.rename_allowed, Button::new("Rename…"))
                        .disabled_help_text(
                            "Refresh the repository and wait for the current operation",
                        )
                        .clicked()
                    {
                        requested.action = Some(BranchRequest::Rename(branch.full_name.clone()));
                        ui.close();
                    }
                    if ui.add_enabled(requested.rename_allowed && !branch.current, Button::new("Delete…"))
                        .disabled_help_text("Select a non-current local branch after refreshing; worktree and merge safety are checked before deletion")
                        .clicked() { requested.action = Some(BranchRequest::Delete(branch.full_name.clone())); ui.close(); }
                });
            } else if kind == RefKind::Tag {
                response.context_menu(|ui| {
                    if ui.add_enabled(requested.rename_allowed, Button::new("Push Tag…"))
                        .disabled_help_text("Refresh branches and wait for the current operation")
                        .clicked() { requested.action = Some(BranchRequest::PushTag(branch.full_name.clone())); ui.close(); }
                });
            }
            if response.clicked() {
                *selected_group = None;
                *selected = branch.full_name.clone();
            }
            if response.double_clicked() {
                requested.action = Some(BranchRequest::Switch(branch.full_name.clone()));
            }
        }
    });
    *ui.visuals_mut() = previous_visuals;
    if group.header_response.clicked() || group.header_response.secondary_clicked() {
        *selected_group = Some(kind);
        selected.clear();
    }
    if kind == RefKind::Tag {
        group.header_response.context_menu(|ui| {
            if ui.button("Add Tag…").clicked() {
                add_tag_requested = true;
                ui.close();
            }
        });
    }
    add_tag_requested
}

fn tag_dialog(ui: &mut Ui, app: &mut SannyGitApp) {
    if !app.tag_dialog_open {
        return;
    }
    let ctx = ui.ctx().clone();
    let palette = app.palette();
    let tag_names: Vec<_> = app
        .refs
        .iter()
        .filter(|reference| reference.kind == RefKind::Tag)
        .map(|reference| reference.name.clone())
        .collect();
    let mut keep = true;
    let busy = app.busy();
    super::controls::dialog_style::modal(&ctx, palette, "branches.add.tag").show(&ctx, |ui| {
        ui.set_width(500.0);
        super::controls::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        super::controls::dialog_style::header(
            ui,
            palette,
            "Add tag to current branch (HEAD)",
            "Enter the name of the tag to create. If entering a message, an annotated tag is created.",
        );
        super::controls::dialog_style::section(
            ui,
            palette.header,
            eframe::egui::Margin::same(18),
            |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                        ui.label("Tag:");
                        let combo_width = ui.available_width();
                        super::controls::editable_combobox::show(
                            ui,
                            ui.make_persistent_id("tag.name"),
                            &mut app.new_tag_name,
                            &tag_names,
                            combo_width,
                            palette,
                            "No existing tags",
                        );
                    });
                ui.label("Annotated Tag Message:");
                let message = ui.add_sized(
                    [ui.available_width(), 120.0],
                    TextEdit::multiline(&mut app.new_tag_message),
                );
                super::controls::dialog_style::preserve_multiline_enter(&message);
                });
            },
        );
        super::controls::dialog_style::section(
            ui,
            palette.header,
            eframe::egui::Margin::symmetric(18, 12),
            |ui| {
                super::controls::dialog_style::button_row(ui, |ui| {
                    ui.add_enabled(false, super::controls::dialog_style::button("Add Tag & Push"))
                        .disabled_help_text("Pushing tags is not implemented yet");
                    let blocker = app.create_tag_blocker();
                    if ui.add_enabled(blocker.is_none(), super::controls::dialog_style::primary("Add Tag", palette))
                        .disabled_help_text(blocker.unwrap_or("")).dialog_accept() {
                        app.create_tag();
                    }
                    if ui.add_enabled(!busy, super::controls::dialog_style::button("Cancel")).dialog_cancel() {
                        keep = false;
                    }
                });
            },
        );
    });
    app.tag_dialog_open = keep;
    if !keep {
        app.new_tag_name.clear();
        app.new_tag_message.clear();
    }
}

fn remote_dialog(ui: &mut Ui, app: &mut SannyGitApp) {
    let Some(mut dialog) = app.remote_checkout.take() else {
        return;
    };
    let ctx = ui.ctx().clone();
    let palette = app.palette();
    let mut keep = true;
    super::controls::dialog_style::modal(&ctx, palette, "branch.remote.checkout").show(&ctx, |ui| {
        ui.set_width(600.0);
        super::controls::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        super::controls::dialog_style::header(ui, palette, "Check out commit", "Switch to a local branch, create a tracking branch, or check out without creating a branch.");
        super::controls::dialog_style::section(ui, palette.header, eframe::egui::Margin::same(18), |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(dialog.remote.strip_prefix("refs/remotes/").unwrap_or(&dialog.remote));
            ui.add_enabled_ui(!app.busy(), |ui| {
                eframe::egui::Grid::new("remote.checkout.options").min_col_width(155.0).spacing([12.0, 10.0]).show(ui, |ui| {
                    ui.radio_value(&mut dialog.mode, 0, "Checkout local branch:");
                    ui.add_enabled_ui(dialog.mode == 0, |ui| {
                        eframe::egui::ComboBox::from_id_salt("remote.local").width(280.0)
                            .selected_text(dialog.local.strip_prefix("refs/heads/").unwrap_or(&dialog.local))
                            .show_ui(ui, |ui| { for branch in app.refs.iter().filter(|r| r.kind == RefKind::Local) { ui.selectable_value(&mut dialog.local, branch.full_name.clone(), &branch.name); } });
                    });
                    ui.end_row();
                    ui.radio_value(&mut dialog.mode, 1, "Create local branch:");
                    ui.add_enabled(dialog.mode == 1, TextEdit::singleline(&mut dialog.new_name).desired_width(280.0));
                    ui.end_row();
                });
                ui.radio_value(&mut dialog.mode, 2, "Don't create local branch (detached HEAD)");
            });
        });
        super::controls::dialog_style::section(ui, palette.header, eframe::egui::Margin::symmetric(18, 12), |ui| {
            super::controls::dialog_style::button_row(ui, |ui| {
                let valid = match dialog.mode {
                    0 => app.refs.iter().any(|r| r.kind == RefKind::Local && r.full_name == dialog.local),
                    1 => crate::branches::name_hint(&dialog.new_name).is_none() && !app.refs.iter().any(|r| r.full_name == format!("refs/heads/{}", dialog.new_name)),
                    _ => true,
                };
                if ui.add_enabled(valid && !app.busy(), super::controls::dialog_style::primary("Checkout", palette)).dialog_accept() {
                    let choice = match dialog.mode { 0 => crate::branches::RemoteChoice::Local(dialog.local.clone()), 1 => crate::branches::RemoteChoice::Create(dialog.new_name.clone()), _ => crate::branches::RemoteChoice::Detached };
                    app.checkout_remote(dialog.remote.clone(), choice);
                }
                if ui.add_enabled(!app.busy(), super::controls::dialog_style::button("Cancel")).dialog_cancel() { keep = false; }
            });
        });
    });
    if keep {
        app.remote_checkout = Some(dialog);
    }
}

fn rename_dialog(ui: &mut Ui, app: &mut SannyGitApp) {
    if app.rename_branch_dialog.is_none() {
        return;
    }
    let ctx = ui.ctx().clone();
    let palette = app.palette();
    let mut cancel = false;
    super::controls::dialog_style::modal(&ctx, palette, "branches.rename").show(&ctx, |ui| {
        ui.set_width(440.0);
        super::controls::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        super::controls::dialog_style::header_with_icon(
            ui,
            palette,
            "Rename local branch",
            "Choose a new local name. The remote branch keeps its name.",
            Icon::Branch,
        );
        let busy = app.busy();
        super::controls::dialog_style::section(
            ui,
            palette.header,
            eframe::egui::Margin::same(18),
            |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                if let Some(dialog) = &mut app.rename_branch_dialog {
                    ui.add(
                        eframe::egui::Label::new(
                            dialog
                                .source
                                .strip_prefix("refs/heads/")
                                .unwrap_or(&dialog.source),
                        )
                        .truncate()
                        .show_tooltip_when_elided(false),
                    )
                    .help_text(&dialog.source);
                    ui.add_enabled(
                        !busy,
                        TextEdit::singleline(&mut dialog.name).desired_width(f32::INFINITY),
                    );
                }
                let blocker = app.rename_branch_blocker();
                let status = blocker.unwrap_or("Ready to rename");
                ui.add(
                    eframe::egui::Label::new(RichText::new(status).small().color(palette.muted))
                        .truncate()
                        .show_tooltip_when_elided(false),
                )
                .help_text(status);
            },
        );
        super::controls::dialog_style::section(
            ui,
            palette.header,
            eframe::egui::Margin::symmetric(18, 12),
            |ui| {
                super::controls::dialog_style::button_row(ui, |ui| {
                    let blocker = app.rename_branch_blocker();
                    if ui
                        .add_enabled(
                            blocker.is_none(),
                            super::controls::dialog_style::primary("Rename", palette),
                        )
                        .disabled_help_text(blocker.unwrap_or(""))
                        .dialog_accept()
                    {
                        app.rename_branch();
                    }
                    if ui
                        .add_enabled(!busy, super::controls::dialog_style::button("Cancel"))
                        .dialog_cancel()
                    {
                        cancel = true;
                    }
                });
            },
        );
    });
    if cancel {
        app.rename_branch_dialog = None;
    }
}

fn create_dialog(ui: &mut Ui, app: &mut SannyGitApp) {
    if !app.branch_dialog_open {
        return;
    }
    let ctx = ui.ctx().clone();
    let palette = app.palette();
    let busy = app.busy();
    let mut cancel = false;
    super::controls::dialog_style::modal(&ctx, palette, "branches.create").show(&ctx, |ui| {
        ui.set_width(440.0);
        super::controls::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        super::controls::dialog_style::header_with_icon(
            ui,
            palette,
            "Create branch",
            "Create a local branch from the current HEAD.",
            Icon::Branch,
        );
        super::controls::dialog_style::section(
            ui,
            palette.header,
            eframe::egui::Margin::same(18),
            |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let source = format!("Create from HEAD ({})", app.current_branch);
                ui.add(
                    eframe::egui::Label::new(&source)
                        .truncate()
                        .show_tooltip_when_elided(false),
                )
                .help_text(&source);
                ui.add_enabled_ui(!busy, |ui| {
                    ui.add(
                        TextEdit::singleline(&mut app.new_branch_name)
                            .hint_text("feature/my-change")
                            .desired_width(f32::INFINITY),
                    );
                    ui.checkbox(&mut app.switch_to_new_branch, "Switch to the new branch");
                });
                let blocker = app.create_branch_blocker();
                let status = blocker.unwrap_or("Ready to create");
                ui.add(
                    eframe::egui::Label::new(RichText::new(status).small().color(palette.muted))
                        .truncate()
                        .show_tooltip_when_elided(false),
                )
                .help_text(status);
            },
        );
        super::controls::dialog_style::section(
            ui,
            palette.header,
            eframe::egui::Margin::symmetric(18, 12),
            |ui| {
                super::controls::dialog_style::button_row(ui, |ui| {
                    let blocker = app.create_branch_blocker();
                    if ui
                        .add_enabled(
                            blocker.is_none(),
                            super::controls::dialog_style::primary("Create", palette),
                        )
                        .disabled_help_text(blocker.unwrap_or(""))
                        .dialog_accept()
                    {
                        app.create_branch();
                    }
                    if ui
                        .add_enabled(!busy, super::controls::dialog_style::button("Cancel"))
                        .dialog_cancel()
                    {
                        cancel = true;
                    }
                });
            },
        );
    });
    if cancel {
        app.branch_dialog_open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

    #[test]
    fn tag_dialog_keeps_geometry_during_validation_progress_and_errors() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1400.0, 900.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.repositories = crate::mock_data::repositories();
                app.selected_repository = 0;
                app.refs = crate::mock_data::refs();
                app.state_current = true;
                app.refs_current = true;
                app.branch_head_exists = true;
                app.tag_dialog_open = true;
                let mut geometry = Vec::new();
                for stage in 0..4 {
                    app.new_tag_name = if stage == 0 { "" } else { "feature/new" }.into();
                    app.refs_current = stage != 2;
                    app.error = (stage == 3).then(|| "Long error message. ".repeat(100));
                    for frame in 0..3 {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| tag_dialog(ui, &mut app),
                        );
                        output.textures_delta.clear();
                        if frame == 2 {
                            let rect = ctx
                                .memory(|memory| {
                                    memory.area_rect(eframe::egui::Id::unique("branches.add.tag"))
                                })
                                .unwrap();
                            assert!(
                                (rect.center() - (Pos2::ZERO + size * 0.5)).length() < 1.0,
                                "Tag dialog is off center: {rect:?}"
                            );
                            assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect));
                            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, eframe::egui::Shape::Text(text) if text.galley.text() == "Add tag to current branch (HEAD)")));
                            geometry.push(rect);
                        }
                    }
                }
                for pair in geometry.windows(2) {
                    assert_eq!(pair[0], pair[1]);
                }
            }
        }
    }

    #[test]
    fn create_dialog_stays_centered_with_header_and_stable_status_line() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1400.0, 900.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.repositories = crate::mock_data::repositories();
                app.selected_repository = 0;
                app.refs = crate::mock_data::refs();
                app.state_current = true;
                app.refs_current = true;
                app.branch_head_exists = true;
                app.branch_dialog_open = true;
                let mut geometry = Vec::new();
                for stage in 0..4 {
                    app.new_branch_name = if stage == 0 { "" } else { "feature/new" }.into();
                    app.refs_current = stage != 2;
                    app.error = (stage == 3).then(|| "Long error message. ".repeat(100));
                    for frame in 0..3 {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| create_dialog(ui, &mut app),
                        );
                        output.textures_delta.clear();
                        if frame == 2 {
                            let rect = ctx
                                .memory(|memory| {
                                    memory.area_rect(eframe::egui::Id::unique("branches.create"))
                                })
                                .unwrap();
                            assert!(
                                (rect.center() - (Pos2::ZERO + size * 0.5)).length() < 1.0,
                                "Create dialog is off center: {rect:?}"
                            );
                            assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect));
                            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, eframe::egui::Shape::Text(text) if text.galley.text() == "Create branch")));
                            geometry.push(rect);
                        }
                    }
                }
                for pair in geometry.windows(2) {
                    assert_eq!(pair[0], pair[1]);
                }
            }
        }
    }

    #[test]
    fn rename_dialog_keeps_geometry_with_validation_progress_and_long_errors() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1400.0, 900.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.repositories = crate::mock_data::repositories();
                app.selected_repository = 0;
                app.state_current = true;
                app.refs_current = true;
                app.refs = crate::mock_data::refs();
                app.selected_branch = "refs/heads/develop".into();
                app.open_rename_branch();
                let mut geometry = Vec::new();
                for stage in 0..4 {
                    app.rename_branch_dialog.as_mut().unwrap().name =
                        if stage == 0 { "develop" } else { "new-name" }.into();
                    app.refs_current = stage != 2;
                    app.error = (stage == 3).then(|| "A very long Git error. ".repeat(80));
                    for frame in 0..3 {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| rename_dialog(ui, &mut app),
                        );
                        output.textures_delta.clear();
                        if frame == 2 {
                            let rect = ctx
                                .memory(|memory| {
                                    memory.area_rect(eframe::egui::Id::unique("branches.rename"))
                                })
                                .unwrap();
                            assert!(
                                (rect.center() - (Pos2::ZERO + size * 0.5)).length() < 1.0,
                                "Rename dialog is off center: {rect:?}"
                            );
                            let buttons: Vec<_> = output
                                .shapes
                                .iter()
                                .filter_map(|shape| {
                                    if let eframe::egui::Shape::Text(text) = &shape.shape {
                                        if matches!(text.galley.text(), "Rename" | "Cancel") {
                                            return Some((text.pos, text.galley.size()));
                                        }
                                    }
                                    None
                                })
                                .collect();
                            assert_eq!(buttons.len(), 2);
                            for (pos, extent) in &buttons {
                                assert!(Rect::from_min_size(Pos2::ZERO, size)
                                    .contains_rect(Rect::from_min_size(*pos, *extent)));
                            }
                            geometry.push(buttons);
                        }
                    }
                }
                for pair in geometry.windows(2) {
                    assert_eq!(pair[0], pair[1]);
                }
            }
        }
    }

    #[test]
    fn tag_dialog_content_stays_within_header_width() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1536.0, 1024.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.tag_dialog_open = true;
                app.new_tag_name = "v1.0.0-b1".into();
                for frame in 0..3 {
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ui| tag_dialog(ui, &mut app),
                    );
                    output.textures_delta.clear();
                    if frame == 2 {
                        let header = output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                eframe::egui::Shape::Rect(rect)
                                    if (rect.rect.width() - 500.0).abs() < 1.0
                                        && (60.0..150.0).contains(&rect.rect.height()) =>
                                {
                                    Some(rect.rect)
                                }
                                _ => None,
                            })
                            .expect("Dialog header missing");
                        for shape in &output.shapes {
                            if let eframe::egui::Shape::Rect(rect) = &shape.shape {
                                if rect.rect.width() > 300.0 && rect.rect.width() < size.x {
                                    assert!(rect.rect.right() <= header.right() + 2.0,
                                        "Dialog content expanded beyond header: header={header:?}, rect={:?}", rect.rect);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn background_ref_updates_do_not_move_branch_rows() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1400.0, 900.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.refs = vec![BranchRef {
                    name: "feature".into(),
                    full_name: "refs/heads/feature".into(),
                    hash: "a".repeat(40),
                    upstream: None,
                    current: false,
                    kind: RefKind::Local,
                }];
                let mut positions = Vec::new();
                for stage in 0..3 {
                    app.refs_loading = stage == 1;
                    app.refs_error = (stage == 2).then(|| "Cannot load refs".into());
                    for frame in 0..3 {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| {
                                ui.set_width(260.0);
                                show(ui, &mut app);
                            },
                        );
                        if frame == 2 {
                            let position = output
                                .shapes
                                .iter()
                                .find_map(|shape| match &shape.shape {
                                    eframe::egui::Shape::Text(text)
                                        if text.galley.job.text == "feature" =>
                                    {
                                        Some(text.pos)
                                    }
                                    _ => None,
                                })
                                .expect("Branch row missing");
                            positions.push(position);
                            assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                                eframe::egui::Shape::Text(text) if text.galley.job.text == "Loading branches…")));
                        }
                        output.textures_delta.clear();
                    }
                }
                assert!(
                    positions
                        .windows(2)
                        .all(|pair| pair[0].distance(pair[1]) < 0.5),
                    "Branch rows moved during a background update: {positions:?}"
                );
            }
        }
    }

    #[test]
    fn branch_name_requires_double_click_to_switch() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            let ctx = Context::default();
            crate::theme::install_fonts(&ctx);
            theme.apply(&ctx);
            let refs = vec![BranchRef {
                name: "feature".into(),
                full_name: "refs/heads/feature".into(),
                hash: "a".repeat(40),
                upstream: None,
                current: false,
                kind: RefKind::Local,
            }];
            let mut selected = String::new();
            let mut selected_group = Some(RefKind::Remote);
            let mut requested = BranchRequests::default();
            let mut pointer = Pos2::ZERO;
            for (frame, pressed) in [None, None, Some(true), Some(false), Some(true), Some(false)]
                .into_iter()
                .enumerate()
            {
                let mut events = vec![Event::PointerMoved(pointer)];
                if let Some(pressed) = pressed {
                    events.push(Event::PointerButton {
                        pos: pointer,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0))),
                        time: Some(frame as f64 * 0.05),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        pointer = ui.next_widget_position() + Vec2::new(70.0, 34.0);
                        let _ = branch_group(
                            ui,
                            &refs,
                            &mut selected,
                            &mut selected_group,
                            RefKind::Local,
                            theme.palette(),
                            &mut requested,
                        );
                    },
                );
                output.textures_delta.clear();
                if frame == 3 {
                    assert_eq!(selected, "refs/heads/feature");
                    assert_eq!(selected_group, None);
                    assert!(requested.action.is_none());
                }
            }
            assert_eq!(
                requested.action,
                Some(BranchRequest::Switch("refs/heads/feature".into()))
            );
        }
    }

    #[test]
    fn group_mouse_click_selects_group_instead_of_branch() {
        for button in [PointerButton::Primary, PointerButton::Secondary] {
            let ctx = Context::default();
            crate::theme::install_fonts(&ctx);
            let refs = vec![BranchRef {
                name: "feature".into(),
                full_name: "refs/heads/feature".into(),
                hash: "a".repeat(40),
                upstream: None,
                current: false,
                kind: RefKind::Local,
            }];
            let mut selected = "refs/heads/feature".to_string();
            let mut selected_group = None;
            let mut requested = BranchRequests::default();
            let mut pointer = Pos2::ZERO;
            for (frame, pressed) in [None, None, Some(true), Some(false)]
                .into_iter()
                .enumerate()
            {
                let mut events = vec![Event::PointerMoved(pointer)];
                if let Some(pressed) = pressed {
                    events.push(Event::PointerButton {
                        pos: pointer,
                        button,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0))),
                        time: Some(frame as f64 * 0.05),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        pointer = ui.next_widget_position() + Vec2::new(70.0, 11.0);
                        let _ = branch_group(
                            ui,
                            &refs,
                            &mut selected,
                            &mut selected_group,
                            RefKind::Local,
                            crate::theme::ThemeChoice::Light.palette(),
                            &mut requested,
                        );
                    },
                );
                output.textures_delta.clear();
            }
            assert_eq!(selected_group, Some(RefKind::Local));
            assert!(selected.is_empty());
            assert!(requested.action.is_none());
        }
    }

    #[test]
    fn selected_group_keeps_standard_header_geometry() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            let ctx = Context::default();
            crate::theme::install_fonts(&ctx);
            theme.apply(&ctx);
            let refs = vec![BranchRef {
                name: "feature".into(),
                full_name: "refs/heads/feature".into(),
                hash: "a".repeat(40),
                upstream: None,
                current: false,
                kind: RefKind::Local,
            }];
            let render = |selected_header: bool| {
                let mut selected = String::new();
                let mut selected_group = selected_header.then_some(RefKind::Local);
                let mut requested = BranchRequests::default();
                let mut result = None;
                for frame in 0..3 {
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(400.0, 200.0),
                            )),
                            time: Some(frame as f64 * 0.05),
                            ..Default::default()
                        },
                        |ui| {
                            if selected_header {
                                let _ = branch_group(
                                    ui,
                                    &refs,
                                    &mut selected,
                                    &mut selected_group,
                                    RefKind::Local,
                                    theme.palette(),
                                    &mut requested,
                                );
                            } else {
                                let _ = CollapsingHeader::new(
                                    RichText::new("Local Branches (1)")
                                        .text_style(eframe::egui::TextStyle::Body),
                                )
                                .id_salt("Local Branches")
                                .default_open(true)
                                .show_unindented(ui, |ui| {
                                    let _ = crate::ui::controls::selectable_row(
                                        ui,
                                        false,
                                        22.0,
                                        |ui| {
                                            ui.label("feature");
                                        },
                                    );
                                });
                            }
                        },
                    );
                    output.textures_delta.clear();
                    result = Some(output);
                }
                result.unwrap()
            };
            let geometry = |output: &eframe::egui::FullOutput| {
                let mut title = None;
                let mut branch_y = None;
                let mut arrow = None;
                for shape in &output.shapes {
                    match &shape.shape {
                        eframe::egui::Shape::Text(text)
                            if text.galley.job.text == "Local Branches (1)" =>
                        {
                            title = Some(text.galley.rect.translate(text.pos.to_vec2()));
                        }
                        eframe::egui::Shape::Text(text) if text.galley.job.text == "feature" => {
                            branch_y = Some(text.pos.y);
                        }
                        eframe::egui::Shape::Path(path) if path.points.len() == 3 => {
                            arrow = Some(path.points.clone());
                        }
                        _ => {}
                    }
                }
                (
                    title.expect("Group title missing"),
                    branch_y.expect("Branch row missing"),
                    arrow.expect("Group arrow missing"),
                )
            };
            assert_eq!(geometry(&render(false)), geometry(&render(true)));
        }
    }
}
