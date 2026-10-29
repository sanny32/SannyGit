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
    ui.painter().rect_filled(rect, 4.0, fill);
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
        .corner_radius(2)
}

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let context = ui.ctx().clone();
    let palette = app.theme.palette();
    toolbar::show(ui, app);
    status_bar::show(ui, app);

    Panel::left("working_tree.v2.sidebar")
        .resizable(true)
        .default_size((context.content_rect().width() * 0.17).clamp(210.0, 270.0))
        .size_range(200.0..=350.0)
        .frame(Frame::new().fill(palette.background).inner_margin(Margin {
            left: 6,
            right: 6,
            top: 2,
            bottom: 0,
        }))
        .show(ui, |ui| {
            repositories::show(ui, app);
            ui.add_space(4.0);
            ui.separator();
            branches::show(ui, app);
        });

    let bottom_max = (ui.available_rect_before_wrap().height() * 0.36).max(180.0);
    Panel::bottom("working_tree.v2.lower")
        .resizable(true)
        .default_size((context.content_rect().height() * 0.21).clamp(180.0, 210.0))
        .size_range(180.0..=bottom_max)
        .frame(Frame::new().fill(palette.background).inner_margin(Margin {
            left: 0,
            right: 4,
            top: 4,
            bottom: 0,
        }))
        .show(ui, |ui| {
            Panel::right("working_tree.v2.journal")
                .resizable(true)
                .default_size(ui.available_width() * 0.575)
                .size_range(430.0..=(ui.available_width() - 350.0).max(430.0))
                .frame(panel_frame(palette).outer_margin(Margin {
                    left: 4,
                    ..Margin::ZERO
                }))
                .show(ui, |ui| journal::show(ui, app));
            CentralPanel::default()
                .frame(panel_frame(palette))
                .show(ui, |ui| commit_panel::show(ui, app));
        });

    CentralPanel::default()
        .frame(Frame::new().fill(palette.background).inner_margin(Margin {
            left: 0,
            right: 4,
            top: 0,
            bottom: 2,
        }))
        .show(ui, |ui| {
            Panel::right("working_tree.v2.diff")
                .resizable(true)
                .default_size(ui.available_width() * 0.52)
                .size_range(350.0..=(ui.available_width() - 360.0).max(350.0))
                .frame(panel_frame(palette).outer_margin(Margin {
                    left: 4,
                    ..Margin::ZERO
                }))
                .show(ui, |ui| diff_view::show(ui, app));
            CentralPanel::default()
                .frame(panel_frame(palette))
                .show(ui, |ui| changes::show(ui, app));
        });
}
