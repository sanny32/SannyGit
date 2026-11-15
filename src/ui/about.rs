use super::controls::{
    dialog_style::{self, DialogResponse},
    icons::{self, Icon},
    tooltips::TooltipResponse,
};
use crate::{app::SannyGitApp, theme::Palette};
use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, Ui, Vec2, ViewportCommand};

const TITLE: &str = "About SannyGit";
const SIZE: Vec2 = egui::vec2(660.0, 330.0);

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let state_id = egui::Id::unique("sannygit.about.presentation");
    if !app.about_dialog {
        ctx.data_mut(|data| data.remove::<Option<egui::Pos2>>(state_id));
        return;
    }
    let saved_position = ctx.data(|data| data.get_temp::<Option<egui::Pos2>>(state_id));
    let first_frame = saved_position.is_none();
    let position = saved_position.unwrap_or_else(|| {
        let position = ctx.input(|input| {
            input
                .viewport()
                .outer_rect
                .map(|rect| rect.center() - SIZE / 2.0)
        });
        ctx.data_mut(|data| data.insert_temp(state_id, position));
        position
    });
    let mut builder = crate::theme::native_viewport()
        .with_title(TITLE)
        .with_inner_size(SIZE)
        .with_resizable(false)
        .with_decorations(true)
        .with_taskbar(false)
        .with_minimize_button(false)
        .with_maximize_button(false);
    if let Some(position) = position {
        builder = builder.with_position(position);
    }
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("sannygit.about"),
        builder,
        |ui, class| {
            if ui.input(|input| input.viewport().close_requested()) {
                close(ui.ctx(), app);
                return;
            }
            #[cfg(target_os = "windows")]
            if first_frame && class == egui::ViewportClass::Immediate {
                if let Err(error) = crate::native_window::begin_modal(ui.ctx(), TITLE) {
                    app.error = Some(format!("Cannot attach About to the main window: {error}"));
                    close(ui.ctx(), app);
                    return;
                }
            }
            crate::theme::paint_native_background(ui, app.palette());
            #[cfg(target_os = "windows")]
            if let Err(error) = crate::native_window::finish_first_presentation(ui.ctx()) {
                app.error = Some(error.to_string());
            }
            let mut dismiss = false;
            if class == egui::ViewportClass::EmbeddedWindow {
                ui.set_width(SIZE.x - 48.0);
                dismiss = contents(ui, app.palette());
            } else {
                egui::CentralPanel::default()
                    .frame(Frame::new().fill(app.palette().panel).inner_margin(24))
                    .show(ui, |ui| dismiss = contents(ui, app.palette()));
                super::show_errors(ui, app);
            }
            if dismiss {
                close(ui.ctx(), app);
            } else if first_frame && class == egui::ViewportClass::Immediate {
                #[cfg(target_os = "windows")]
                if let Err(error) =
                    crate::native_window::prepare_child_presentation(ui.ctx(), TITLE)
                {
                    app.error = Some(error.to_string());
                }
                ui.ctx().send_viewport_cmd(ViewportCommand::Visible(true));
            }
        },
    );
}

fn close(ctx: &egui::Context, app: &mut SannyGitApp) {
    #[cfg(target_os = "windows")]
    {
        crate::native_window::cancel_first_presentation(ctx);
        crate::native_window::end_modal(ctx);
    }
    app.about_dialog = false;
    if ctx.viewport_id() != egui::ViewportId::ROOT {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}

fn contents(ui: &mut Ui, palette: Palette) -> bool {
    dialog_style::setup(ui);
    let mut dismiss = false;
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(214.0, SIZE.y - 48.0),
            Layout::top_down(Align::Center),
            |ui| {
                ui.add_space(36.0);
                ui.add(
                    egui::Image::from_bytes(
                        "bytes://app/sannygit.svg",
                        include_bytes!("../../assets/app/sannygit.svg"),
                    )
                    .fit_to_exact_size(Vec2::splat(128.0)),
                );
                ui.add_space(4.0);
                brand_caption(ui, palette);
            },
        );
        let (separator, _) =
            ui.allocate_exact_size(egui::vec2(1.0, SIZE.y - 48.0), egui::Sense::hover());
        ui.painter().line_segment(
            [separator.center_top(), separator.center_bottom()],
            (1.0, palette.border),
        );
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new("SannyGit").size(32.0).strong());
            ui.add_space(8.0);
            Frame::new()
                .fill(palette.header)
                .corner_radius(12)
                .inner_margin(Margin::symmetric(12, 3))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(16.0),
                    );
                });
            ui.add_space(12.0);
            ui.label(RichText::new(
                "A desktop Git client built with Rust and egui.",
            ));
            ui.add_space(16.0);
            detail_row(ui, palette, Icon::TextFile, "License:", |ui| {
                ui.label("MIT");
            });
            detail_row(ui, palette, Icon::CodeFile, "Source code:", |ui| {
                ui.hyperlink_to(
                    "github.com/sanny32/sannygit",
                    "https://github.com/sanny32/sannygit",
                );
                external_link(ui, palette, "https://github.com/sanny32/sannygit");
            });
            detail_row(ui, palette, Icon::Globe, "Website:", |ui| {
                ui.hyperlink_to("sanny32.github.io/SannyGit", "https://sanny32.github.io/SannyGit");
                external_link(ui, palette, "https://sanny32.github.io/SannyGit");
            });
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);
            dialog_style::button_row(ui, |ui| {
                dismiss = ui.add(dialog_style::primary("OK", palette)).dialog_close();
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.label(
                        RichText::new("© 2026 SaNNy. All rights reserved.").color(palette.muted),
                    );
                });
            });
        });
    });
    dismiss
}

fn brand_caption(ui: &mut Ui, palette: Palette) {
    use eframe::egui::{text::LayoutJob, TextFormat};

    let mut title = LayoutJob::default();
    for (text, color) in [("Sanny", palette.text), ("Git", palette.brand)] {
        title.append(
            text,
            0.0,
            TextFormat {
                font_id: egui::FontId::new(32.0, crate::theme::semibold_family()),
                color,
                ..Default::default()
            },
        );
    }
    ui.label(title);
    ui.label(
        RichText::new("A simple Git client")
            .size(16.0)
            .color(palette.muted),
    );
}

fn external_link(ui: &mut Ui, palette: Palette, url: &str) {
    ui.add_space(2.0);
    let response = ui.add(
        egui::Button::image(icons::image(Icon::ExternalLink, palette.muted, 14.0)).frame(false),
    );
    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
    response.help_text("Open in browser");
}

fn detail_row(ui: &mut Ui, palette: Palette, icon: Icon, label: &str, value: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 32.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.spacing_mut().item_spacing.x = 12.0;
            icons::paint(ui, icon, palette.muted, 18.0);
            let column = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(92.0, 24.0));
            ui.allocate_ui_with_layout(column.size(), Layout::left_to_right(Align::Center), |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.label(label);
            });
            ui.advance_cursor_after_rect(column);
            value(ui);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{InterfaceStyle, ThemeChoice};

    #[test]
    fn about_contents_fit_fixed_window_in_both_themes_and_styles() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                let ctx = egui::Context::default();
                crate::theme::install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply_style(&ctx, style);
                for _ in 0..3 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
                            ..Default::default()
                        },
                        |ui| {
                            egui::CentralPanel::default()
                                .frame(Frame::new().inner_margin(24))
                                .show(ui, |ui| {
                                    let available = ui.max_rect();
                                    assert!(!contents(ui, style.palette(theme)));
                                    assert!(
                                        available.expand(0.1).contains_rect(ui.min_rect()),
                                        "About content overflows: {:?} outside {available:?}",
                                        ui.min_rect()
                                    );
                                });
                        },
                    );
                    let mut details = 0;
                    for shape in &output.shapes {
                        if let egui::epaint::Shape::Text(text) = &shape.shape {
                            if [
                                "License:",
                                "Source code:",
                                "Website:",
                                "github.com/sanny32/sannygit",
                                "sanny.git",
                            ]
                            .contains(&text.galley.job.text.as_str())
                            {
                                assert_eq!(
                                    text.galley.rows.len(),
                                    1,
                                    "Wrapped About detail: {}",
                                    text.galley.job.text
                                );
                                details += 1;
                            }
                        }
                    }
                    assert_eq!(details, 5);
                    output.textures_delta.clear();
                }
            }
        }
    }
}
