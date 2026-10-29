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
    Align, CentralPanel, Context, Frame, InnerResponse, Layout, Margin, Response, Sense, SidePanel,
    TopBottomPanel, Ui, UiBuilder, Vec2,
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
            .max_rect(rect.shrink2(Vec2::new(12.0, 2.0)))
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
            .max_rect(rect.shrink2(Vec2::new(16.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    InnerResponse::new(contents(&mut child), response)
}

fn panel_frame(palette: Palette) -> Frame {
    Frame::new()
        .fill(palette.panel)
        .stroke((1.0, palette.border))
        .corner_radius(6)
}

fn compact(ui: &Ui) -> bool {
    ui.ctx().screen_rect().height() < 850.0
}

pub fn show(context: &Context, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    let compact = context.screen_rect().height() < 850.0;
    toolbar::show(context, app);
    status_bar::show(context, app);

    SidePanel::left("working_tree.v2.sidebar")
        .resizable(true)
        .default_width((context.screen_rect().width() * 0.195).clamp(240.0, 300.0))
        .width_range(230.0..=350.0)
        .frame(Frame::new().fill(palette.background).inner_margin(Margin {
            left: 20,
            right: 14,
            top: 6,
            bottom: 0,
        }))
        .show(context, |ui| {
            repositories::show(ui, app);
            ui.add_space(if compact { 4.0 } else { 10.0 });
            ui.separator();
            branches::show(ui, app);
        });

    let bottom_max = (context.available_rect().height() * 0.36).max(220.0);
    TopBottomPanel::bottom("working_tree.v2.lower")
        .resizable(true)
        .default_height((context.screen_rect().height() * 0.269).clamp(220.0, 274.0))
        .height_range(220.0..=bottom_max)
        .frame(Frame::new().fill(palette.background).inner_margin(Margin {
            left: 0,
            right: 12,
            top: 6,
            bottom: 0,
        }))
        .show(context, |ui| {
            SidePanel::right("working_tree.v2.journal")
                .resizable(true)
                .default_width(ui.available_width() * 0.575)
                .width_range(430.0..=(ui.available_width() - 350.0).max(430.0))
                .frame(panel_frame(palette).outer_margin(Margin {
                    left: 8,
                    ..Margin::ZERO
                }))
                .show_inside(ui, |ui| journal::show(ui, app));
            CentralPanel::default()
                .frame(panel_frame(palette))
                .show_inside(ui, |ui| commit_panel::show(ui, app));
        });

    CentralPanel::default()
        .frame(Frame::new().fill(palette.background).inner_margin(Margin {
            left: 0,
            right: 12,
            top: 0,
            bottom: 2,
        }))
        .show(context, |ui| {
            SidePanel::right("working_tree.v2.diff")
                .resizable(true)
                .default_width(ui.available_width() * 0.52)
                .width_range(350.0..=(ui.available_width() - 360.0).max(350.0))
                .frame(panel_frame(palette).outer_margin(Margin {
                    left: 10,
                    ..Margin::ZERO
                }))
                .show_inside(ui, |ui| diff_view::show(ui, app));
            CentralPanel::default()
                .frame(panel_frame(palette))
                .show_inside(ui, |ui| changes::show(ui, app));
        });
}
