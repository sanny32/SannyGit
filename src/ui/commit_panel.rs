use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::SannyGitApp,
    theme::{semibold_family, Palette},
};
use eframe::egui::{
    Align, Align2, Button, FontId, Frame, Layout, Margin, Rect, RichText, Sense, Shape, Stroke,
    TextEdit, Ui, Vec2,
};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
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
        let previous = app.commit_contents;
        ui.add_enabled_ui(!app.busy(), |ui| {
            eframe::egui::ComboBox::from_id_salt("commit.contents")
                .width(98.0)
                .selected_text(app.commit_contents.label())
                .show_ui(ui, |ui| {
                    for mode in [
                        crate::git::CommitContents::All,
                        crate::git::CommitContents::Staged,
                    ] {
                        ui.selectable_value(&mut app.commit_contents, mode, mode.label());
                    }
                });
        });
        if previous != app.commit_contents {
            app.contents_preview_open = false;
            app.contents_preview = None;
        }
        let staged = app.changes.iter().filter(|c| c.staged).count();
        ui.label(
            RichText::new(format!("{staged} staged"))
                .small()
                .color(palette.muted),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            icons::menu(
                ui,
                Icon::Chevron,
                "Commit contents and message options",
                |ui| {
                    if app.generating_commit_message && ui.button("Cancel generation").clicked() {
                        app.cancel_commit_message_generation();
                        ui.close();
                    }
                    let blocker = app.commit_contents_preview_blocker();
                    if ui
                        .add_enabled(blocker.is_none(), Button::new("Preview commit contents…"))
                        .disabled_help_text(blocker.unwrap_or(""))
                        .clicked()
                    {
                        app.preview_commit_contents();
                        ui.close();
                    }
                    let blocker = app.generate_commit_message_blocker();
                    if ui
                        .add_enabled(blocker.is_none(), Button::new("Generate commit message"))
                        .disabled_help_text(blocker.unwrap_or(""))
                        .clicked()
                    {
                        app.generate_commit_message();
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            app.repositories.get(app.selected_repository).is_some(),
                            Button::new("Repository LLM settings…"),
                        )
                        .clicked()
                    {
                        app.open_repository_llm_settings();
                        ui.close();
                    }
                },
            );

            let blocker = app.generate_commit_message_blocker();
            let generating = app.generating_commit_message;
            if icons::action(
                ui,
                if generating {
                    Icon::Stop
                } else {
                    Icon::Sparkles
                },
                generating || blocker.is_none(),
            )
            .help_text(if generating {
                "Cancel commit message generation"
            } else {
                if app.commit_contents == crate::git::CommitContents::All {
                    "Generate a commit message from all changes, including new files"
                } else {
                    "Generate a commit message from staged contents only"
                }
            })
            .disabled_help_text(blocker.unwrap_or(""))
            .clicked()
            {
                if generating {
                    app.cancel_commit_message_generation();
                } else {
                    app.generate_commit_message();
                }
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
                let blocker = app.amend_blocker();
                let mut amend = app.amend;
                if ui.add_enabled(
                    blocker.is_none(),
                    eframe::egui::Checkbox::new(&mut amend, "Amend last commit"),
                )
                .help_text("Replace the last commit using the selected contents mode. Rewrites commit history.")
                .disabled_help_text(blocker.unwrap_or(""))
                .changed() {
                    app.set_amend(amend);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let blocker = app.commit_blocker();
                    let label = if app.amend { "Amend" } else { "Commit" };
                    if ui
                        .add_enabled(
                            blocker.is_none(),
                            if app.interface_style == crate::theme::InterfaceStyle::Modern {
                                super::controls::dialog_style::primary(
                                    if app.amend { "✓  Amend" } else { "✓  Commit" },
                                    palette,
                                )
                            } else {
                                Button::new(label).min_size(Vec2::new(72.0, 24.0))
                            },
                        )
                        .help_text(match (app.amend, app.commit_contents) {
                            (false, crate::git::CommitContents::All) => "Commit all changes, including new files",
                            (false, crate::git::CommitContents::Staged) => "Commit staged changes only; preserve unstaged changes",
                            (true, crate::git::CommitContents::All) => "Replace the last commit with this message and all changes, including new files",
                            (true, crate::git::CommitContents::Staged) => "Replace the last commit with this message and staged changes only; preserve unstaged changes",
                        })
                        .disabled_help_text(blocker.unwrap_or(""))
                        .clicked()
                    {
                        app.create_commit();
                    }
                });
            });
        });
    let mut open = app.contents_preview_open;
    eframe::egui::Window::new("Commit contents preview")
        .id(eframe::egui::Id::unique("commit.contents.window"))
        .open(&mut open)
        .fixed_size(Vec2::new(720.0, 420.0))
        .collapsible(false)
        .show(ui.ctx(), |ui| {
            ui.label(format!(
                "{}{} — snapshot at preview time",
                app.commit_contents.label(),
                if app.amend { " · Amend" } else { "" }
            ));
            ui.label("Review again if repository contents change before committing.");
            ui.allocate_ui_with_layout(
                Vec2::new(700.0, 360.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_size(Vec2::new(700.0, 360.0));
                    match &app.contents_preview {
                        None => {
                            ui.label("Loading preview…");
                        }
                        Some(Err(_)) => {
                            ui.label("Preview unavailable.");
                        }
                        Some(Ok(preview)) if preview.lines.is_empty() => {
                            ui.label("No content changes.");
                        }
                        Some(Ok(preview)) => super::diff_view::render_preview(
                            ui,
                            preview,
                            palette,
                            "commit.contents.preview",
                        ),
                    }
                },
            );
        });
    app.contents_preview_open = open;
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
    fn commit_modes_and_generation_preserve_layout_and_draft_in_both_themes() {
        for size in [Vec2::new(1100.0, 700.0), Vec2::new(1536.0, 1024.0)] {
            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                for style in [
                    crate::theme::InterfaceStyle::Classic,
                    crate::theme::InterfaceStyle::Modern,
                ] {
                    let context = egui::Context::default();
                    crate::theme::install_fonts(&context);
                    egui_extras::install_image_loaders(&context);
                    theme.apply_style(&context, style);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.interface_style = style;
                    app.commit_message = "Existing draft\n\nKeep this text readable.".into();
                    let mut idle_layout = None;
                    for (frame, generating) in [false, true, true, false, false, false]
                        .into_iter()
                        .enumerate()
                    {
                        app.commit_contents = if frame % 2 == 0 {
                            crate::git::CommitContents::All
                        } else {
                            crate::git::CommitContents::Staged
                        };
                        app.generating_commit_message = generating;
                        app.amend = generating;
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
    #[test]
    fn contents_preview_keeps_window_size_during_loading_errors_and_success() {
        for size in [Vec2::new(1100.0, 700.0), Vec2::new(1536.0, 1024.0)] {
            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                for style in [
                    crate::theme::InterfaceStyle::Classic,
                    crate::theme::InterfaceStyle::Modern,
                ] {
                    let context = egui::Context::default();
                    crate::theme::install_fonts(&context);
                    egui_extras::install_image_loaders(&context);
                    theme.apply_style(&context, style);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.interface_style = style;
                    app.contents_preview_open = true;
                    let mut initial = None;
                    for frame in 0..6 {
                        app.contents_preview = match frame {
                            0 | 1 => None,
                            2 => Some(Err("Long diagnostic text ".repeat(100))),
                            3 => Some(Ok(crate::models::DiffPreview::new(""))),
                            _ => Some(Ok(crate::models::DiffPreview::new(
                                &"+content\n".repeat(200),
                            ))),
                        };
                        let mut output = context.run_ui(
                            egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| show(ui, &mut app),
                        );
                        output.textures_delta.clear();
                        let rect = context
                            .memory(|memory| {
                                memory.area_rect(egui::Id::unique("commit.contents.window"))
                            })
                            .unwrap();
                        if frame == 1 {
                            initial = Some(rect.size());
                        }
                        if frame > 1 {
                            assert_eq!(
                                Some(rect.size()),
                                initial,
                                "Preview resized: {theme:?}, {style:?}"
                            );
                        }
                        assert!(rect.width() <= size.x && rect.height() <= size.y);
                    }
                }
            }
        }
    }
}
