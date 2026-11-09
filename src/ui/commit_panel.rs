use super::controls::tooltips::TooltipResponse;
use super::controls::EditableText;
use crate::{
    app::SannyGitApp,
    theme::{semibold_family, Palette},
};
use eframe::egui::{
    Align, Align2, Button, FontId, Frame, Layout, Margin, Rect, RichText, Sense, Shape, Stroke,
    TextEdit, Ui, Vec2,
};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    ui.spacing_mut().item_spacing.y = 0.0;
    super::header(ui, 28.0, palette, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(60.0, 28.0), Sense::hover());
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Commit",
            FontId::new(13.0, semibold_family()),
            palette.text,
        );
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            (3.0, palette.accent),
        );
        let staged = app.changes.iter().filter(|c| c.staged).count();
        ui.label(
            RichText::new(format!("{staged} staged"))
                .small()
                .color(palette.muted),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            eframe::egui::containers::menu::MenuButton::from_button(
                Button::image(super::controls::icons::image(
                    super::controls::icons::Icon::Chevron,
                    palette.muted,
                    14.0,
                ))
                .frame(false)
                .min_size(Vec2::new(20.0, 26.0)),
            )
            .ui(ui, |ui| {
                let blocker = app.generate_commit_message_blocker();
                if ui
                    .add_enabled(blocker.is_none(), Button::new("Generate commit message"))
                    .disabled_help_text(blocker.unwrap_or(""))
                    .clicked()
                {
                    app.generate_commit_message();
                    ui.close();
                }
                if ui.button("LLM settings…").clicked() {
                    app.open_settings();
                    app.settings_window.page = crate::app::SettingsPage::Llm;
                    ui.close();
                }
            })
            .0
            .help_text("Commit message generation options");

            let blocker = app.generate_commit_message_blocker();
            if ui
                .add_enabled(
                    blocker.is_none(),
                    Button::image(super::controls::icons::image(
                        super::controls::icons::Icon::Sparkles,
                        palette.muted,
                        20.0,
                    ))
                    .frame(false)
                    .min_size(Vec2::new(26.0, 26.0)),
                )
                .help_text("Generate a commit message from all staged changes")
                .disabled_help_text(blocker.unwrap_or(""))
                .clicked()
            {
                app.generate_commit_message();
            }
        });
    });
    Frame::new()
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 6,
            bottom: 16,
        })
        .show(ui, |ui| {
            let width = ui.available_width();
            let editor_height = (ui.available_height() - 38.0).max(32.0);
            let busy = app.busy();
            ui.add_enabled_ui(!busy, |ui| {
                let editor = ui.add_sized(
                    [width, editor_height],
                    TextEdit::multiline(&mut app.commit_message)
                        .editable()
                        .id(eframe::egui::Id::unique("commit.message"))
                        .desired_rows(1)
                        .desired_width(f32::INFINITY)
                        .margin(Vec2::new(6.0, 4.0)),
                );
                if app.generating_commit_message {
                    let time = ui.input(|input| input.time);
                    draw_generation_sparkles(ui, editor.rect, time, palette);
                }
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_enabled(
                    false,
                    eframe::egui::Checkbox::new(&mut app.amend, "Amend last commit"),
                )
                .disabled_help_text("Amending commits is not implemented yet");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let blocker = app.commit_blocker();
                    if ui
                        .add_enabled(
                            blocker.is_none(),
                            Button::new("Commit").min_size(Vec2::new(72.0, 24.0)),
                        )
                        .help_text("Commit staged changes and all tracked working-tree changes")
                        .disabled_help_text(blocker.unwrap_or(""))
                        .clicked()
                    {
                        app.create_commit();
                    }
                });
            });
        });
}

fn draw_generation_sparkles(ui: &Ui, rect: Rect, time: f64, palette: Palette) {
    let painter = ui.painter().with_clip_rect(rect);
    let colors = [palette.accent, palette.incoming, palette.outgoing];
    for index in 0..12 {
        let progress = (time * 0.16 + index as f64 / 12.0).fract() as f32;
        let x = rect.left() + progress * rect.width();
        let lane = ((index * 37) % 100) as f32 / 100.0;
        let wave = (time as f32 * 1.8 + index as f32 * 0.9).sin() * 4.0;
        let y = rect.top() + 6.0 + lane * (rect.height() - 12.0).max(0.0) + wave;
        let pulse = (time as f32 * 3.0 + index as f32 * 0.7).sin();
        let radius = 2.0 + (pulse + 1.0) * 1.1;
        let edge_fade = (progress * 8.0).min(1.0) * ((1.0 - progress) * 8.0).min(1.0);
        let opacity = edge_fade * (0.35 + (pulse + 1.0) * 0.225);
        let color = colors[index % colors.len()].gamma_multiply(opacity);
        let rotation = time as f32 * 0.35 + index as f32;
        let points = (0..8)
            .map(|point| {
                let radius = if point % 2 == 0 {
                    radius
                } else {
                    radius * 0.32
                };
                let angle = rotation + point as f32 * std::f32::consts::TAU / 8.0;
                eframe::egui::pos2(x, y) + Vec2::angled(angle) * radius
            })
            .collect();
        painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
    }
}
