mod branches;
mod changes;
mod clone_dialog;
mod commit_panel;
mod diff_view;
mod icons;
mod journal;
mod repositories;
mod repository_dialogs;
mod status_bar;
mod toolbar;

use crate::{app::SannyGitApp, theme::Palette};
use eframe::egui::{
    Align, CentralPanel, Frame, InnerResponse, Layout, Margin, Panel, Response, Sense, Ui,
    UiBuilder, Vec2,
};

fn selectable_row(
    ui: &mut Ui,
    selected: bool,
    height: f32,
    contents: impl FnOnce(&mut Ui),
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    let fill = if selected {
        ui.visuals().selection.bg_fill
    } else if response.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        eframe::egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 0.0, fill);
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(6.0, 1.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    contents(&mut child);
    response
}

fn header<R>(
    ui: &mut Ui,
    height: f32,
    palette: Palette,
    contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, palette.header);
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        (1.0, palette.border),
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(8.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    InnerResponse::new(contents(&mut child), response)
}

fn panel_frame(palette: Palette) -> Frame {
    Frame::new()
        .fill(palette.panel)
        .stroke((1.0, palette.border))
        .corner_radius(0)
}

fn sidebar_contents(ui: &mut Ui, app: &mut SannyGitApp) -> eframe::egui::Rect {
    let palette = app.theme.palette();
    let branches = Panel::bottom("working_tree.v3.branches")
        .resizable(true)
        .default_size(ui.available_height() * 0.36)
        .size_range(130.0..=(ui.available_height() - 120.0).max(130.0))
        .frame(panel_frame(palette).inner_margin(8).outer_margin(Margin {
            top: 4,
            ..Margin::ZERO
        }))
        .show(ui, |ui| branches::show(ui, app));
    CentralPanel::default()
        .frame(panel_frame(palette).inner_margin(8))
        .show(ui, |ui| repositories::show(ui, app));
    branches.response.rect
}

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let context = ui.ctx().clone();
    let palette = app.theme.palette();
    toolbar::show(ui, app);
    repository_dialogs::show(ui, app);
    eframe::egui::Window::new("About SannyGit")
        .open(&mut app.about_dialog)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.heading("SannyGit");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.label("A desktop Git client built with Rust and egui.");
            ui.label("MIT License");
        });
    if let Some(error) = app.error.clone() {
        Panel::top("repository.error")
            .frame(
                Frame::new()
                    .fill(palette.removed_background)
                    .inner_margin(6),
            )
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(eframe::egui::RichText::new(error).color(palette.removed_text));
                    if ui.small_button("Dismiss").clicked() {
                        app.error = None;
                    }
                });
            });
    }
    status_bar::show(ui, app);

    Panel::left("working_tree.v3.sidebar")
        .resizable(true)
        .default_size((context.content_rect().width() * 0.17).clamp(200.0, 270.0))
        .size_range(190.0..=350.0)
        .frame(Frame::new().fill(palette.background).inner_margin(4))
        .show(ui, |ui| {
            sidebar_contents(ui, app);
        });

    CentralPanel::default()
        .frame(Frame::new().fill(palette.background).inner_margin(Margin {
            left: 0,
            right: 4,
            top: 4,
            bottom: 4,
        }))
        .show(ui, |ui| {
            Panel::bottom("working_tree.v3.journal")
                .resizable(true)
                .default_size(180.0)
                .size_range(150.0..=300.0)
                .frame(panel_frame(palette))
                .show(ui, |ui| journal::show(ui, app));
            Panel::top("working_tree.v3.files")
                .resizable(true)
                .default_size((context.content_rect().height() * 0.38).max(300.0))
                .size_range(260.0..=550.0)
                .frame(Frame::new().fill(palette.background).inner_margin(Margin {
                    bottom: 4,
                    ..Margin::ZERO
                }))
                .show(ui, |ui| {
                    Panel::right("working_tree.v3.commit")
                        .resizable(true)
                        .default_size(ui.available_width() * 0.29)
                        .size_range(260.0..=450.0)
                        .frame(panel_frame(palette).outer_margin(Margin {
                            left: 4,
                            ..Margin::ZERO
                        }))
                        .show(ui, |ui| commit_panel::show(ui, app));
                    CentralPanel::default()
                        .frame(panel_frame(palette))
                        .show(ui, |ui| changes::show(ui, app));
                });
            CentralPanel::default()
                .frame(panel_frame(palette).outer_margin(Margin {
                    bottom: 4,
                    ..Margin::ZERO
                }))
                .show(ui, |ui| diff_view::show(ui, app));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeChoice;
    use eframe::egui::{pos2, vec2, Context, Event, Modifiers, PointerButton, RawInput, Rect};

    #[test]
    fn branch_panel_can_be_resized_by_dragging_divider() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [vec2(1100.0, 700.0), vec2(1536.0, 1024.0)] {
                let ctx = Context::default();
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                let mut rect = Rect::NOTHING;
                let mut frame = |events| {
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            Panel::left("test.sidebar")
                                .exact_size(260.0)
                                .frame(Frame::new().inner_margin(4))
                                .show(ui, |ui| {
                                    rect = sidebar_contents(ui, &mut app);
                                });
                        },
                    );
                    output.textures_delta.clear();
                    rect
                };
                for _ in 0..3 {
                    frame(vec![]);
                }
                let before = frame(vec![]);
                let pointer = pos2(before.center().x, before.top());
                frame(vec![Event::PointerMoved(pointer)]);
                frame(vec![Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }]);
                let moved = pointer - vec2(0.0, 70.0);
                for _ in 0..3 {
                    frame(vec![Event::PointerMoved(moved)]);
                }
                frame(vec![Event::PointerButton {
                    pos: moved,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }]);
                let after = frame(vec![]);
                assert!(
                    after.height() > before.height() + 50.0,
                    "Divider failed to grow {theme:?} {size:?}: {before:?} -> {after:?}"
                );
                let pointer = pos2(after.center().x, after.top());
                frame(vec![Event::PointerMoved(pointer)]);
                frame(vec![Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }]);
                let moved = pos2(pointer.x, after.bottom() - 140.0);
                for _ in 0..3 {
                    frame(vec![Event::PointerMoved(moved)]);
                }
                frame(vec![Event::PointerButton {
                    pos: moved,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }]);
                let shrunk = frame(vec![]);
                assert!(
                    shrunk.height() < 160.0,
                    "Divider failed to shrink: {shrunk:?}"
                );
            }
        }
    }
}
