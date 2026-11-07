mod branches;
mod changes;
mod commit_panel;
mod diff_view;
mod icons;
mod journal;
mod repositories;
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

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let context = ui.ctx().clone();
    let palette = app.theme.palette();
    toolbar::show(ui, app);
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
            Panel::bottom("working_tree.v3.branches")
                .resizable(true)
                .default_size(ui.available_height() * 0.36)
                .size_range(180.0..=450.0)
                .frame(panel_frame(palette))
                .show(ui, |ui| branches::show(ui, app));
            CentralPanel::default()
                .frame(panel_frame(palette))
                .show(ui, |ui| repositories::show(ui, app));
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
