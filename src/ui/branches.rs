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
            if icons::button(
                ui,
                Icon::Search,
                "Filter branches and tags",
                Vec2::splat(20.0),
            )
            .clicked()
            {
                ui.memory_mut(|m| m.request_focus(Id::unique("branch_filter")));
            }
        });
    });
    if !app.current_branch.is_empty() {
        ui.label(RichText::new(&app.current_branch).family(semibold_family()));
    }
    ui.add(
        TextEdit::singleline(&mut app.branch_filter)
            .id(Id::unique("branch_filter"))
            .hint_text("Filter branches and tags…")
            .margin(Vec2::new(5.0, 3.0))
            .desired_width(f32::INFINITY),
    );
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
    let filter = app.branch_filter.trim().to_lowercase();
    let height = (ui.available_height() - 30.0).max(0.0);
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
                    &filter,
                    palette,
                );
            }
        });
    ui.horizontal(|ui| {
        let blocker = app.switch_branch_blocker();
        if ui
            .add_enabled(blocker.is_none(), Button::new("Switch"))
            .on_hover_text("Switch to the selected local branch")
            .on_disabled_hover_text(blocker.unwrap_or(""))
            .clicked()
        {
            app.switch_branch();
        }
        if let Some(branch) = app.refs.iter().find(|r| r.full_name == app.selected_branch) {
            ui.label(RichText::new(&branch.name).small().color(palette.muted));
        }
    });
}

fn matches(branch: &BranchRef, filter: &str) -> bool {
    branch.name.to_lowercase().contains(filter)
        || branch
            .upstream
            .as_ref()
            .is_some_and(|u| u.to_lowercase().contains(filter))
}

fn branch_group(
    ui: &mut Ui,
    refs: &[BranchRef],
    selected: &mut String,
    kind: RefKind,
    title: &str,
    filter: &str,
    palette: Palette,
) {
    let matching: Vec<_> = refs
        .iter()
        .filter(|r| r.kind == kind && matches(r, filter))
        .collect();
    if matching.is_empty() {
        return;
    }
    CollapsingHeader::new(format!("{title} ({})", matching.len()))
        .id_salt(title)
        .default_open(kind == RefKind::Local || !filter.is_empty())
        .open(if filter.is_empty() { None } else { Some(true) })
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
                        ui.label(if branch.current {
                            RichText::new(&branch.name).family(semibold_family())
                        } else {
                            RichText::new(&branch.name)
                        });
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
                            ""
                        } else {
                            "\nRead-only in this stage"
                        }
                    ));
                if response.clicked() {
                    *selected = branch.full_name.clone();
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
        .show(ui.ctx(), |ui| {
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
