use super::tooltips::TooltipResponse;
use super::EditableText;
use crate::{
    app::{SannyGitApp, SettingsPage},
    theme::{semibold_family, Palette, ThemeChoice},
};
use eframe::egui::{
    self, Align, CentralPanel, Frame, Key, Layout, Margin, Panel, RichText, ScrollArea, Ui, Vec2,
    ViewportBuilder, ViewportCommand, ViewportId,
};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    if !app.settings_window.open {
        return;
    }

    let builder = ViewportBuilder::default()
        .with_title("SannyGit Preferences")
        .with_inner_size([740.0, 420.0])
        .with_min_inner_size([680.0, 380.0])
        .with_maximize_button(false);
    let ctx = ctx.clone();
    let viewport_id = ViewportId::from_hash_of("sannygit.preferences");
    ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
        #[cfg(target_os = "macos")]
        ui.input_mut(|input| input.events.append(&mut app.pending_native_edit_events));
        let close_requested = ui.ctx().input(|input| input.viewport().close_requested());
        let escape = ui.input(|input| input.key_pressed(Key::Escape));
        if close_requested || escape {
            app.settings_window.open = false;
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        window_contents(ui, app);
    });
}

fn window_contents(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.theme.palette();
    ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
    footer(ui, app, palette);
    navigation(ui, app, palette);
    CentralPanel::default()
        .frame(Frame::new().fill(palette.header).inner_margin(12))
        .show(ui, |ui| page(ui, app, palette));
}

fn footer(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette) {
    Panel::bottom("preferences.footer")
        .exact_size(46.0)
        .frame(
            Frame::new()
                .fill(palette.header)
                .inner_margin(Margin::symmetric(12, 9)),
        )
        .show(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(super::dialog_style::primary("OK", palette))
                    .clicked()
                {
                    let next_llm = app.settings_window.llm.clone();
                    let secret_changed = next_llm.store_api_key != app.llm_config.store_api_key
                        || next_llm.api_key != app.llm_config.api_key;
                    let saved = if secret_changed {
                        crate::llm::save_api_key(&next_llm)
                    } else {
                        Ok(())
                    };
                    match saved {
                        Ok(()) => {
                            app.theme = app.settings_window.theme;
                            app.skip_push_confirmation = app.settings_window.skip_push_confirmation;
                            app.tracking.set_auto_fetch(app.settings_window.auto_fetch);
                            app.llm_config = next_llm;
                            app.settings_window.llm_secret_error = None;
                            app.settings_window.open = false;
                            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                        }
                        Err(error) => app.settings_window.llm_secret_error = Some(error),
                    }
                }
                if ui.add(super::dialog_style::button("Cancel")).clicked() {
                    app.settings_window.open = false;
                    ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                }
            });
        });
}

fn navigation(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette) {
    Panel::left("preferences.navigation")
        .exact_size(206.0)
        .frame(Frame::new().fill(palette.header).inner_margin(Margin {
            left: 12,
            right: 0,
            top: 12,
            bottom: 0,
        }))
        .show(ui, |ui| {
            Frame::new()
                .fill(palette.panel)
                .stroke((1.0, palette.border))
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size() - Vec2::splat(2.0));
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ScrollArea::vertical().show(ui, |ui| {
                        section(ui, "Main Window", |ui| {
                            entry(ui, app, SettingsPage::General, "General")
                        });
                        section(ui, "User Interface", |ui| {
                            entry(ui, app, SettingsPage::Appearance, "Appearance")
                        });
                        section(ui, "Git", |ui| {
                            entry(ui, app, SettingsPage::Remote, "Remote")
                        });
                        section(ui, "AI", |ui| {
                            entry(ui, app, SettingsPage::Llm, "Commit messages")
                        });
                    });
                });
        });
}

fn section(ui: &mut Ui, title: &str, contents: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(title)
        .default_open(true)
        .show_unindented(ui, contents);
}

fn entry(ui: &mut Ui, app: &mut SannyGitApp, page: SettingsPage, label: &str) {
    let selected = app.settings_window.page == page;
    let response = super::selectable_row(ui, selected, 23.0, |ui| {
        ui.add_space(24.0);
        ui.label(label);
    });
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));
    if response.clicked() {
        app.settings_window.page = page;
    }
}

fn page(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    match app.settings_window.page {
        SettingsPage::General => {
            heading(ui, "General");
            group(ui, palette, "Confirmations", |ui| {
                ui.checkbox(
                    &mut app.settings_window.skip_push_confirmation,
                    "Skip confirmation when pushing to upstream",
                );
                ui.label(
                    RichText::new("SannyGit will still report Git errors and rejected pushes.")
                        .small()
                        .color(palette.muted),
                );
            });
        }
        SettingsPage::Appearance => {
            heading(ui, "Appearance");
            group(ui, palette, "Interface", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Color theme:");
                    egui::ComboBox::from_id_salt("preferences.theme")
                        .selected_text(app.settings_window.theme.label())
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                                ui.selectable_value(
                                    &mut app.settings_window.theme,
                                    theme,
                                    theme.label(),
                                );
                            }
                        });
                });
                ui.add_space(6.0);
                ui.label("Preview:");
                theme_preview(ui, app.settings_window.theme);
            });
        }
        SettingsPage::Remote => {
            heading(ui, "Remote");
            group(ui, palette, "Automatic fetch", |ui| {
                ui.checkbox(
                    &mut app.settings_window.auto_fetch,
                    "Fetch the upstream repository every 60 seconds",
                );
                ui.label(
                    RichText::new(
                        "Automatic fetch runs only while a repository is open and the application is idle.",
                    )
                    .small()
                    .color(palette.muted),
                );
            });
        }
        SettingsPage::Llm => {
            heading(ui, "LLM commit messages");
            group(ui, palette, "OpenAI-compatible API", |ui| {
                egui::Grid::new("preferences.llm.connection")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        form_label(ui, "Endpoint:");
                        ui.add(
                            egui::TextEdit::singleline(&mut app.settings_window.llm.endpoint)
                                .editable()
                                .desired_width(f32::INFINITY),
                        )
                        .help_text("Base URL or full /chat/completions endpoint");
                        ui.end_row();
                        form_label(ui, "Model:");
                        ui.add(
                            egui::TextEdit::singleline(&mut app.settings_window.llm.model)
                                .editable()
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();
                        form_label(ui, "API key:");
                        ui.add(
                            egui::TextEdit::singleline(&mut app.settings_window.llm.api_key)
                                .editable()
                                .password(true)
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();
                    });
                ui.checkbox(
                    &mut app.settings_window.llm.store_api_key,
                    format!("Store API key in {}", crate::llm::credential_store_label()),
                );
                ui.label(
                    RichText::new(
                        "The key is never written to SannyGit settings. SANNYGIT_LLM_API_KEY overrides the saved key at startup.",
                    )
                    .small()
                    .color(palette.muted),
                );
                let secret_error = app
                    .settings_window
                    .llm_secret_error
                    .as_deref()
                    .unwrap_or("");
                ui.add_sized(
                    [ui.available_width(), 20.0],
                    egui::Label::new(
                        RichText::new(secret_error)
                            .small()
                            .color(palette.removed_text),
                    )
                    .truncate(),
                )
                .help_text(secret_error);
                ui.add_space(6.0);
                ui.label("Generation instructions:");
                ui.add_sized(
                    [ui.available_width(), 76.0],
                    egui::TextEdit::multiline(&mut app.settings_window.llm.instructions).editable(),
                );
            });
        }
    }
}

fn form_label(ui: &mut Ui, text: &str) {
    ui.allocate_ui_with_layout(
        Vec2::new(82.0, ui.spacing().interact_size.y),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Extend));
        },
    );
}

fn heading(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(18.0).family(semibold_family()));
    ui.add_space(8.0);
}

fn group(ui: &mut Ui, palette: Palette, title: &str, contents: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).family(semibold_family()));
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter()
            .hline(rect.x_range(), rect.center().y, (1.0, palette.border));
    });
    ui.add_space(6.0);
    Frame::new()
        .inner_margin(Margin::symmetric(6, 0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui);
        });
}

fn theme_preview(ui: &mut Ui, theme: ThemeChoice) {
    let palette = theme.palette();
    Frame::new()
        .fill(palette.editor)
        .stroke((1.0, palette.border))
        .inner_margin(0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            Frame::new()
                .fill(palette.header)
                .inner_margin(Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Working Tree  /  src / main.rs").color(palette.text));
                });
            for (text, fill, color) in [
                ("  1    fn main() {", palette.editor, palette.text),
                (
                    "  2  -     start();",
                    palette.removed_background,
                    palette.removed_text,
                ),
                (
                    "  2  +     run();",
                    palette.added_background,
                    palette.added_text,
                ),
                ("  3    }", palette.editor, palette.text),
            ] {
                Frame::new()
                    .fill(fill)
                    .inner_margin(Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(text).monospace().color(color));
                    });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{
        epaint::ClippedShape, pos2, Event, Modifiers, PointerButton, RawInput, Rect,
    };

    #[test]
    fn clicking_tree_labels_and_row_backgrounds_selects_the_page() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let context = egui::Context::default();
            crate::theme::install_fonts(&context);
            theme.apply(&context);
            let mut app = SannyGitApp::with_theme(theme);
            let render = |app: &mut SannyGitApp, events| {
                context.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            pos2(0.0, 0.0),
                            Vec2::new(740.0, 420.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| window_contents(ui, app),
                )
            };
            for background in [false, true] {
                for (page, label) in [
                    (SettingsPage::General, "General"),
                    (SettingsPage::Remote, "Remote"),
                    (SettingsPage::Appearance, "Appearance"),
                    (SettingsPage::Llm, "Commit messages"),
                ] {
                    let mut output = render(&mut app, Vec::new());
                    let rect = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.job.text == label => {
                                Some(text.galley.rect.translate(text.pos.to_vec2()))
                            }
                            _ => None,
                        })
                        .expect("Tree label is missing");
                    let mut position = rect.center();
                    if background {
                        position.x = 190.0;
                    }
                    output.textures_delta.clear();
                    for pressed in [true, false] {
                        let mut output = render(
                            &mut app,
                            vec![
                                Event::PointerMoved(position),
                                Event::PointerButton {
                                    pos: position,
                                    button: PointerButton::Primary,
                                    pressed,
                                    modifiers: Modifiers::NONE,
                                },
                            ],
                        );
                        output.textures_delta.clear();
                    }
                    assert_eq!(
                        app.settings_window.page, page,
                        "Click on {label}, background={background}, theme={theme:?}"
                    );
                }
            }
        }
    }

    fn visible_text(shapes: &[ClippedShape], expected: &str) -> bool {
        shapes.iter().any(|shape| {
            let egui::Shape::Text(text) = &shape.shape else {
                return false;
            };
            text.galley.job.text == expected
                && shape
                    .clip_rect
                    .intersect(text.galley.rect.translate(text.pos.to_vec2()))
                    .height()
                    > 8.0
        })
    }

    fn text_rows(shapes: &[ClippedShape], expected: &str) -> Option<usize> {
        shapes.iter().find_map(|shape| {
            let egui::Shape::Text(text) = &shape.shape else {
                return None;
            };
            (text.galley.job.text == expected).then_some(text.galley.rows.len())
        })
    }

    fn text_left(shapes: &[ClippedShape], expected: &str) -> Option<f32> {
        shapes.iter().find_map(|shape| {
            let egui::Shape::Text(text) = &shape.shape else {
                return None;
            };
            (text.galley.job.text == expected).then_some(text.pos.x)
        })
    }

    fn white_rect_count(shapes: &[ClippedShape]) -> usize {
        shapes
            .iter()
            .filter(|shape| {
                matches!(
                    &shape.shape,
                    egui::Shape::Rect(rect) if rect.fill == egui::Color32::WHITE
                )
            })
            .count()
    }

    #[test]
    fn preferences_pages_fit_at_the_minimum_size_in_both_themes() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let context = egui::Context::default();
            crate::theme::install_fonts(&context);
            theme.apply(&context);
            let mut app = SannyGitApp::with_theme(theme);
            for (page, title) in [
                (SettingsPage::General, "Confirmations"),
                (SettingsPage::Appearance, "Color theme:"),
                (SettingsPage::Remote, "Automatic fetch"),
                (SettingsPage::Llm, "Endpoint:"),
            ] {
                app.settings_window.page = page;
                let mut output = context.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            pos2(0.0, 0.0),
                            Vec2::new(680.0, 380.0),
                        )),
                        ..Default::default()
                    },
                    |ui| window_contents(ui, &mut app),
                );
                output.textures_delta.clear();
                assert!(visible_text(&output.shapes, title));
                assert!(visible_text(&output.shapes, "Cancel"));
                assert!(visible_text(&output.shapes, "OK"));
                if page == SettingsPage::Llm {
                    assert!(white_rect_count(&output.shapes) >= 4);
                    for label in ["Endpoint:", "Model:", "API key:"] {
                        assert_eq!(text_rows(&output.shapes, label), Some(1));
                    }
                    let left = text_left(&output.shapes, "Endpoint:").unwrap();
                    for label in ["Model:", "API key:"] {
                        assert!((text_left(&output.shapes, label).unwrap() - left).abs() < 0.5);
                    }
                }
            }
        }
    }
}
