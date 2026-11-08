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
                );
            }
        });
}

fn branch_group(
    ui: &mut Ui,
    refs: &[BranchRef],
    selected: &mut String,
    kind: RefKind,
    title: &str,
    palette: Palette,
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
