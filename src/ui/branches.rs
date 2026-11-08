use super::icons::{self, Icon};
use crate::{
    app::SannyGitApp,
    models::{BranchRef, RefKind},
    theme::{semibold_family, Palette},
};
use eframe::egui::{
    Align, Button, CollapsingHeader, Id, Layout, RichText, ScrollArea, TextEdit, Ui, Vec2, Window,
};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    create_dialog(ui, app);
    let palette = app.theme.palette();
    ui.horizontal(|ui| {
        ui.set_min_height(26.0);
        ui.heading("Branches");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_enabled_ui(
                !app.busy() && app.repositories.get(app.selected_repository).is_some(),
                |ui| {
                    if icons::button(
                        ui,
                        Icon::Plus,
                        "Create local branch from HEAD",
                        Vec2::splat(20.0),
                    )
                    .clicked()
                    {
                        app.branch_dialog_open = true;
                    }
                    if icons::button(
                        ui,
                        Icon::Fetch,
                        "Reload branches and tags",
                        Vec2::splat(20.0),
                    )
                    .clicked()
                    {
                        app.refresh();
                    }
                },
            );
        });
    });
    if app.refs_loading {
        ui.label(
            RichText::new("Loading branches…")
                .small()
                .color(palette.muted),
        );
    }
    if let Some(error) = &app.refs_error {
        ui.label(
            RichText::new("Branches unavailable; reload to retry")
                .small()
                .color(palette.removed_text),
        )
        .on_hover_text(error);
    }
    let mut requested = None;
    let height = ui.available_height();
    ScrollArea::vertical()
        .id_salt("branches")
        .max_height(height)
        .min_scrolled_height(0.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (kind, title) in [
                (RefKind::Local, "Local Branches"),
                (RefKind::Remote, "Remote Branches"),
                (RefKind::Tag, "Tags"),
            ] {
                branch_group(
                    ui,
                    &app.refs,
                    &mut app.selected_branch,
                    kind,
                    title,
                    palette,
                    &mut requested,
                );
            }
        });
    if let Some(target) = requested {
        app.selected_branch = target;
        if let Some(reason) = app.switch_branch_blocker() {
            app.error = Some(reason.into());
        } else {
            app.switch_branch();
        }
    }
}

fn branch_group(
    ui: &mut Ui,
    refs: &[BranchRef],
    selected: &mut String,
    kind: RefKind,
    title: &str,
    palette: Palette,
    requested: &mut Option<String>,
) {
    let matching: Vec<_> = refs.iter().filter(|r| r.kind == kind).collect();
    if matching.is_empty() {
        return;
    }
    CollapsingHeader::new(format!("{title} ({})", matching.len()))
        .id_salt(title)
        .default_open(kind == RefKind::Local)
        .show_unindented(ui, |ui| {
            for branch in matching {
                // Avoid text layout for rows outside the viewport in large ref lists.
                let size = Vec2::new(ui.available_width(), 22.0);
                let rect = eframe::egui::Rect::from_min_size(ui.next_widget_position(), size);
                if !ui.is_rect_visible(rect) {
                    ui.allocate_space(size);
                    continue;
                }
                let response =
                    super::selectable_row(ui, *selected == branch.full_name, 22.0, |ui| {
                        ui.add_space(6.0);
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
                                RichText::new(&branch.name).family(semibold_family())
                            } else {
                                RichText::new(&branch.name)
                            })
                            .selectable(false)
                            .sense(eframe::egui::Sense::hover()),
                        );
                        if branch.current {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                icons::paint(ui, Icon::Star, palette.accent, 14.0)
                            });
                        }
                    })
                    .on_hover_text(format!(
                        "{}\nObject: {}{}{}",
                        branch.full_name,
                        branch.hash,
                        branch
                            .upstream
                            .as_ref()
                            .map(|s| format!("\nUpstream: {s}"))
                            .unwrap_or_default(),
                        if kind == RefKind::Local {
                            "\nDouble-click to switch"
                        } else {
                            "\nDouble-click to check out a detached HEAD"
                        }
                    ));
                if response.clicked() {
                    *selected = branch.full_name.clone();
                }
                if response.double_clicked() {
                    *requested = Some(branch.full_name.clone());
                }
            }
        });
}

fn create_dialog(ui: &mut Ui, app: &mut SannyGitApp) {
    if !app.branch_dialog_open {
        return;
    }
    let mut open = true;
    Window::new("Create branch")
        .id(Id::unique("branches.create"))
        .open(&mut open)
        .resizable(false)
        .default_width(400.0)
        .collapsible(false)
        .frame(super::dialog_style::frame(ui.ctx(), app.theme.palette()).inner_margin(18))
        .show(ui.ctx(), |ui| {
            super::dialog_style::setup(ui);
            ui.label(format!("Create from HEAD ({})", app.current_branch));
            ui.add_enabled_ui(!app.busy(), |ui| {
                ui.add(
                    TextEdit::singleline(&mut app.new_branch_name)
                        .hint_text("feature/my-change")
                        .desired_width(f32::INFINITY),
                );
                ui.checkbox(&mut app.switch_to_new_branch, "Switch to the new branch");
            });
            let blocker = app.create_branch_blocker();
            if let Some(reason) = blocker {
                ui.label(
                    RichText::new(reason)
                        .small()
                        .color(app.theme.palette().muted),
                );
            }
            if ui
                .add_enabled(blocker.is_none(), Button::new("Create"))
                .on_disabled_hover_text(blocker.unwrap_or(""))
                .clicked()
            {
                app.create_branch();
            }
        });
    app.branch_dialog_open = open && app.branch_dialog_open;
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

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
            let mut requested = None;
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
                        branch_group(
                            ui,
                            &refs,
                            &mut selected,
                            RefKind::Local,
                            "Local Branches",
                            theme.palette(),
                            &mut requested,
                        );
                    },
                );
                output.textures_delta.clear();
                if frame == 3 {
                    assert_eq!(selected, "refs/heads/feature");
                    assert!(requested.is_none());
                }
            }
            assert_eq!(requested.as_deref(), Some("refs/heads/feature"));
        }
    }
}
