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
                .help_text("Generate a commit message from all changes, including new files")
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
                    draw_generation_animation(ui, editor.rect, time, palette);
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
                        .help_text("Commit all changes, including new files")
                        .disabled_help_text(blocker.unwrap_or(""))
                        .clicked()
                    {
                        app.create_commit();
                    }
                });
            });
        });
}

fn draw_generation_animation(ui: &Ui, rect: Rect, time: f64, palette: Palette) {
    if !ui.is_rect_visible(rect) || rect.width() < 8.0 || rect.height() < 8.0 {
        return;
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(16));
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let outline = rect.shrink(1.5);
    let radius = 3.0_f32.min(outline.height() / 2.0);
    let phase = (time * 0.22).rem_euclid(1.0) as f32;

    // Sample the rounded outline once, then shade by distance so the light moves
    // at the same speed on the long edges and around the corners.
    let mut points = Vec::with_capacity(37);
    for (center, start) in [
        (outline.right_top() + Vec2::new(-radius, radius), -90.0_f32),
        (outline.right_bottom() - Vec2::splat(radius), 0.0),
        (outline.left_bottom() + Vec2::new(radius, -radius), 90.0),
        (outline.left_top() + Vec2::splat(radius), 180.0),
    ] {
        for step in 0..=8 {
            let angle = (start + step as f32 * 90.0 / 8.0).to_radians();
            points.push(center + Vec2::angled(angle) * radius);
        }
    }
    points.push(points[0]);
    let perimeter: f32 = points
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .sum();
    let mut distance = 0.0;
    for pair in points.windows(2) {
        let length = pair[0].distance(pair[1]);
        let steps = (length / 6.0).ceil().max(1.0) as usize;
        for step in 0..steps {
            let from = pair[0].lerp(pair[1], step as f32 / steps as f32);
            let to = pair[0].lerp(pair[1], (step + 1) as f32 / steps as f32);
            let position = (distance + length * (step as f32 + 0.5) / steps as f32) / perimeter;
            let trail = (phase - position).rem_euclid(0.5) / 0.5;
            let intensity = (1.0 - trail / 0.38).max(0.0).powi(2);
            let color = palette
                .accent
                .lerp_to_gamma(palette.incoming, intensity * 0.65);
            for (width, opacity) in [
                (6.0, intensity * 0.06),
                (3.0, intensity * 0.14),
                (1.2, 0.16 + intensity * 0.75),
            ] {
                painter.line_segment(
                    [from, to],
                    Stroke::new(width, color.gamma_multiply(opacity)),
                );
            }
        }
        distance += length;
    }

    // Keep the glints near the lower edge so an existing draft stays readable.
    for index in 0..5 {
        let cycle = (time * 0.24 + index as f64 * 0.217).rem_euclid(1.0) as f32;
        let envelope = (cycle * std::f32::consts::PI).sin().powi(2);
        let center = eframe::egui::pos2(
            rect.left() + rect.width() * (0.1 + index as f32 * 0.19),
            rect.bottom() - 7.0 - cycle * (rect.height() * 0.2).min(12.0),
        );
        let color = palette
            .accent
            .lerp_to_gamma(palette.incoming, index as f32 / 5.0);
        painter.circle_filled(
            center,
            4.0 * envelope,
            color.gamma_multiply(envelope * 0.06),
        );
        let radius = 2.6 * envelope;
        // Each arm is a convex triangle; a whole four-point star is concave.
        for arm in 0..4 {
            let direction = Vec2::angled(arm as f32 * std::f32::consts::FRAC_PI_2);
            let side = Vec2::new(-direction.y, direction.x) * radius * 0.23;
            painter.add(Shape::convex_polygon(
                vec![center + direction * radius, center + side, center - side],
                color.gamma_multiply(envelope * 0.65),
                Stroke::NONE,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeChoice;
    use eframe::egui::{self, Pos2};

    #[test]
    fn generation_preserves_layout_and_draft_in_both_themes() {
        for size in [Vec2::new(1100.0, 700.0), Vec2::new(1536.0, 1024.0)] {
            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                let context = egui::Context::default();
                crate::theme::install_fonts(&context);
                egui_extras::install_image_loaders(&context);
                theme.apply(&context);
                let mut app = SannyGitApp::with_theme(theme);
                app.commit_message = "Existing draft\n\nKeep this text readable.".into();
                let mut idle_layout = None;
                for (frame, generating) in [false, true, true, false].into_iter().enumerate() {
                    app.generating_commit_message = generating;
                    let input = egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        time: Some(frame as f64 * 0.75),
                        ..Default::default()
                    };
                    let mut output = context.run_ui(input, |ui| {
                        ui.set_width(size.x * 0.4);
                        ui.set_height(size.y * 0.25);
                        show(ui, &mut app);
                        let layout = ui.min_rect();
                        if let Some(idle) = idle_layout {
                            assert_eq!(layout, idle, "Generation changed layout in {theme:?}");
                        } else {
                            idle_layout = Some(layout);
                        }
                    });
                    output.textures_delta.clear();
                    assert_eq!(
                        app.commit_message,
                        "Existing draft\n\nKeep this text readable."
                    );
                }
            }
        }
    }
}
