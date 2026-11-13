use super::*;
use eframe::egui::{epaint::ClippedShape, pos2, Event, Modifiers, PointerButton, RawInput, Rect};

#[test]
fn settings_position_centers_on_main_window() {
    let outer = Rect::from_min_size(pos2(-800.0, 100.0), Vec2::new(1200.0, 800.0));
    let inner = Rect::from_min_size(pos2(-792.0, 132.0), Vec2::new(1184.0, 760.0));
    let position = centered_position(outer, Some(inner), Vec2::new(740.0, 420.0));
    assert_eq!(position + Vec2::new(756.0, 460.0) / 2.0, outer.center());
    assert_eq!(
        centered_position(outer, None, Vec2::new(740.0, 420.0)),
        outer.center() - Vec2::new(740.0, 420.0) / 2.0
    );
}

#[test]
fn repository_overrides_can_be_enabled_saved_cancelled_and_reset_in_both_themes() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        for size in [Vec2::new(560.0, 420.0), Vec2::new(500.0, 380.0)] {
            let context = egui::Context::default();
            crate::theme::install_fonts(&context);
            egui_extras::install_image_loaders(&context);
            theme.apply(&context);
            let mut app = SannyGitApp::with_theme(theme);
            app.repositories = vec![crate::models::Repository {
                name: "Project".into(),
                path: "/projects/example".into(),
            }];
            app.selected_repository = 0;
            app.llm_config.store_api_key = false;
            app.llm_config.instructions = "Global instructions".into();
            let path = app.repositories[0].path.clone();
            let render = |app: &mut SannyGitApp, events| {
                context.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
                        events,
                        ..Default::default()
                    },
                    |ui| window_contents(ui, app),
                )
            };
            let click = |app: &mut SannyGitApp, label: &str| {
                let mut output = render(app, vec![]);
                let target = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("Missing {label}"));
                output.textures_delta.clear();
                for pressed in [true, false] {
                    let mut output = render(
                        app,
                        vec![
                            Event::PointerMoved(target),
                            Event::PointerButton {
                                pos: target,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                    );
                    output.textures_delta.clear();
                }
            };
            app.open_repository_llm_settings();
            assert_eq!(
                app.settings_window.llm_repository.as_deref(),
                Some(path.as_str())
            );
            let mut output = render(&mut app, vec![]);
            output.textures_delta.clear();
            for label in [
                "Project",
                "Use global settings",
                "Endpoint:",
                "OK",
                "Cancel",
            ] {
                assert!(visible_text(&output.shapes, label), "Missing {label}");
            }
            let text_rect = |label: &str| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => {
                            Some(text.galley.rect.translate(text.pos.to_vec2()))
                        }
                        _ => None,
                    })
                    .unwrap()
            };
            let name_rect = text_rect("Project");
            let header_text = name_rect.union(text_rect(&path));
            let icon_rect = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.rect.size() == Vec2::splat(20.0)
                            && rect.rect.right() < name_rect.left() =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("Repository icon must be visible");
            assert!(
                (icon_rect.center().y - header_text.center().y).abs() <= 1.0,
                "Repository icon must be centered beside both text lines"
            );
            assert!(
                !output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Rect(rect) if rect.fill == app.palette().selection
                        && rect.rect.contains(name_rect.center())
                )),
                "Repository header must not have a selection background"
            );
            for label in [
                "Repository LLM settings",
                "Main Window",
                "Settings for:",
                "Appearance",
                "Global (all repositories)",
            ] {
                assert!(
                    !visible_text(&output.shapes, label),
                    "Unexpected {label} in repository form"
                );
            }
            output.textures_delta.clear();
            click(&mut app, "Use global settings");
            let draft = app.settings_window.llm_repositories.get_mut(&path).unwrap();
            assert_eq!(draft.config.instructions, "Global instructions");
            draft.config.instructions = "Project instructions".into();
            draft.config.model = "project-model".into();
            assert_eq!(
                app.settings_window.llm.config.instructions,
                "Global instructions"
            );
            // Saving the repository form must not apply global preferences.
            app.settings_window.llm.config.instructions = "Unrelated global draft".into();
            app.settings_window.theme = if theme == ThemeChoice::Light {
                ThemeChoice::Dark
            } else {
                ThemeChoice::Light
            };
            click(&mut app, "OK");
            assert_eq!(
                app.effective_llm_config(&path).instructions,
                "Project instructions"
            );
            assert_eq!(app.effective_llm_config(&path).model, "project-model");
            assert_eq!(
                app.effective_llm_config("/another").instructions,
                "Global instructions"
            );
            assert_eq!(app.theme, theme);

            app.open_repository_llm_settings();
            app.settings_window
                .llm_repositories
                .get_mut(&path)
                .unwrap()
                .config
                .instructions = "Discard me".into();
            click(&mut app, "Cancel");
            assert_eq!(
                app.effective_llm_config(&path).instructions,
                "Project instructions"
            );

            app.open_repository_llm_settings();
            click(&mut app, "Use global settings");
            click(&mut app, "OK");
            assert!(!app.repository_llm_configs.contains_key(&path));
            assert_eq!(app.effective_llm_config(&path), &app.llm_config);
        }
    }
}

#[test]
fn global_preferences_leave_repository_overrides_unchanged() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.llm_config.store_api_key = false;
    let custom = crate::llm::Config {
        instructions: "Project instructions".into(),
        store_api_key: false,
        ..Default::default()
    };
    app.repository_llm_configs
        .insert("/project".into(), custom.clone());
    app.open_settings();
    assert!(app.settings_window.llm_repository.is_none());
    app.settings_window.llm.config.instructions = "New global instructions".into();
    // Even a modified repository draft cannot be applied from global preferences.
    app.settings_window.llm_repositories.clear();
    apply_settings(&mut app).unwrap();
    assert_eq!(app.llm_config.instructions, "New global instructions");
    assert_eq!(app.effective_llm_config("/project"), &custom);
}

#[test]
fn model_accepts_dropdown_selection_and_custom_text() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        for size in [Vec2::new(680.0, 380.0), Vec2::new(740.0, 420.0)] {
            let context = egui::Context::default();
            crate::theme::install_fonts(&context);
            egui_extras::install_image_loaders(&context);
            theme.apply(&context);
            let mut app = SannyGitApp::with_theme(theme);
            app.settings_window.page = SettingsPage::Llm;
            app.settings_window.llm.config.model = "custom-model".into();
            app.settings_window.llm.config.api_key = "keep-key".into();
            let models = app.llm_models.entry(&app.settings_window.llm.config, None);
            models.options = vec![crate::llm::DEFAULT_MODEL.into()];
            models.attempted = true;
            let render = |app: &mut SannyGitApp, events| {
                context.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
                        events,
                        ..Default::default()
                    },
                    |ui| window_contents(ui, app),
                )
            };
            let click = |app: &mut SannyGitApp, target| {
                for pressed in [true, false] {
                    let mut output = render(
                        app,
                        vec![
                            Event::PointerMoved(target),
                            Event::PointerButton {
                                pos: target,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                    );
                    output.textures_delta.clear();
                }
            };
            let text_center = |shapes: &[ClippedShape], label: &str| {
                shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .unwrap()
            };
            let mut output = render(&mut app, vec![]);
            output.textures_delta.clear();
            let mut output = render(&mut app, vec![]);
            let model_center = text_center(&output.shapes, "custom-model");
            let field = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.fill == app.palette().editor
                            && rect.rect.contains(model_center) =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .unwrap();
            output.textures_delta.clear();
            click(&mut app, pos2(field.right() - 10.0, field.center().y));
            let mut output = render(&mut app, vec![]);
            let option = text_center(&output.shapes, crate::llm::DEFAULT_MODEL);
            output.textures_delta.clear();
            click(&mut app, option);
            assert_eq!(
                app.settings_window.llm.config.model,
                crate::llm::DEFAULT_MODEL
            );
            let mut output = render(&mut app, vec![]);
            let editor = text_center(&output.shapes, crate::llm::DEFAULT_MODEL);
            output.textures_delta.clear();
            click(&mut app, editor);
            let mut output = render(&mut app, vec![Event::Text("-custom".into())]);
            output.textures_delta.clear();
            assert!(app.settings_window.llm.config.model.contains("-custom"));
            assert_eq!(app.settings_window.llm.config.api_key, "keep-key");
        }
    }
}

#[test]
fn endpoint_accepts_typed_text_and_clears_the_previous_providers_key() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let context = egui::Context::default();
        crate::theme::install_fonts(&context);
        theme.apply(&context);
        let mut app = SannyGitApp::with_theme(theme);
        app.settings_window.page = SettingsPage::Llm;
        app.settings_window.llm.config.api_key = "previous-key".into();
        let render = |app: &mut SannyGitApp, events| {
            context.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(740.0, 420.0))),
                    events,
                    ..Default::default()
                },
                |ui| window_contents(ui, app),
            )
        };
        let mut output = render(&mut app, vec![]);
        let target = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == crate::llm::DEFAULT_ENDPOINT => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .unwrap();
        output.textures_delta.clear();
        for pressed in [true, false] {
            let mut output = render(
                &mut app,
                vec![
                    Event::PointerMoved(target),
                    Event::PointerButton {
                        pos: target,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
            output.textures_delta.clear();
        }
        let mut output = render(&mut app, vec![Event::Text("custom".into())]);
        output.textures_delta.clear();
        assert!(app.settings_window.llm.config.endpoint.contains("custom"));
        assert!(app.settings_window.llm.config.api_key.is_empty());
    }
}

#[test]
fn clicking_tree_labels_and_row_backgrounds_selects_the_page() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let context = egui::Context::default();
        crate::theme::install_fonts(&context);
        egui_extras::install_image_loaders(&context);
        theme.apply(&context);
        let mut app = SannyGitApp::with_theme(theme);
        let render = |app: &mut SannyGitApp, events| {
            context.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(740.0, 420.0))),
                    events,
                    ..Default::default()
                },
                |ui| window_contents(ui, app),
            )
        };
        for background in [false, true] {
            for (page, label) in [
                (SettingsPage::General, "General"),
                (SettingsPage::Git, "Executable"),
                (SettingsPage::Remote, "Remote"),
                (SettingsPage::Appearance, "Appearance"),
                (SettingsPage::DateTime, "Date & Time"),
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

#[test]
fn thumbnails_load_and_support_image_and_radio_selection() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let context = egui::Context::default();
        crate::theme::install_fonts(&context);
        egui_extras::install_image_loaders(&context);
        theme.apply(&context);
        context.options_mut(|options| options.warn_on_id_clash = true);
        let mut app = SannyGitApp::with_theme(theme);
        let render = |app: &mut SannyGitApp, events| {
            let mut output = context.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(680.0, 380.0))),
                    events,
                    ..Default::default()
                },
                |ui| window_contents(ui, app),
            );
            output.textures_delta.clear();
            assert!(!output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains("widget ID"))
            }), "Interface choices have conflicting widget IDs");
            output
        };
        let mut output = render(&mut app, vec![]);
        output.textures_delta.clear();
        let mut thumbnails: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.rect.size() == Vec2::splat(112.0) => {
                    Some(rect.rect)
                }
                _ => None,
            })
            .collect();
        thumbnails.sort_by(|a, b| a.left().total_cmp(&b.left()));
        thumbnails.dedup();
        assert_eq!(thumbnails.len(), 2);
        assert!(thumbnails[1].right() - thumbnails[0].left() <= 380.0);
        assert!((thumbnails[1].center().x - thumbnails[0].center().x) < 220.0);
        let suffix = if theme == ThemeChoice::Light {
            "light"
        } else {
            "dark"
        };
        for style in ["classic", "modern"] {
            let uri = format!("bytes://previews/{style}-{suffix}.png");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            let image = loop {
                match context.try_load_image(&uri, Default::default()).unwrap() {
                    egui::load::ImagePoll::Ready { image } => break image,
                    egui::load::ImagePoll::Pending { .. } => {
                        assert!(std::time::Instant::now() < deadline, "Preview did not load");
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }
            };
            assert_eq!(image.size, [224, 224]);
        }
        let classic_radio = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Classic" => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .unwrap();
        output.textures_delta.clear();
        for (target, expected) in [
            (thumbnails[1].center(), InterfaceStyle::Modern),
            (thumbnails[0].center(), InterfaceStyle::Classic),
            (thumbnails[1].center(), InterfaceStyle::Modern),
            (classic_radio, InterfaceStyle::Classic),
        ] {
            for pressed in [true, false] {
                let mut output = render(
                    &mut app,
                    vec![
                        Event::PointerMoved(target),
                        Event::PointerButton {
                            pos: target,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
                output.textures_delta.clear();
            }
            assert_eq!(app.settings_window.interface_style, expected);
            assert_eq!(app.interface_style, InterfaceStyle::Classic);
        }
    }
}

#[test]
fn appearance_changes_apply_on_ok_and_cancel_discards_them() {
    for button in ["Cancel", "OK"] {
        let context = egui::Context::default();
        crate::theme::install_fonts(&context);
        egui_extras::install_image_loaders(&context);
        ThemeChoice::Light.apply(&context);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.open_settings();
        app.settings_window.interface_style = InterfaceStyle::Modern;
        app.settings_window.theme = ThemeChoice::Dark;
        app.settings_window.preferences.git_executable = "/custom Git/git".into();
        app.settings_window.preferences.show_welcome = false;
        app.settings_window.preferences.file_double_click =
            crate::preferences::FileDoubleClick::ToggleStage;
        app.settings_window.preferences.date_format = crate::preferences::DateFormat::DayMonthYear;
        let render = |app: &mut SannyGitApp, events| {
            context.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(680.0, 380.0))),
                    events,
                    ..Default::default()
                },
                |ui| window_contents(ui, app),
            )
        };
        let mut output = render(&mut app, vec![]);
        assert!(visible_text(&output.shapes, "Modern"));
        let target = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == button => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .unwrap();
        output.textures_delta.clear();
        for pressed in [true, false] {
            let mut output = render(
                &mut app,
                vec![
                    Event::PointerMoved(target),
                    Event::PointerButton {
                        pos: target,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
            output.textures_delta.clear();
        }
        assert!(!app.settings_window.open);
        let (style, theme) = if button == "OK" {
            (InterfaceStyle::Modern, ThemeChoice::Dark)
        } else {
            (InterfaceStyle::Classic, ThemeChoice::Light)
        };
        assert_eq!(app.interface_style, style);
        assert_eq!(app.theme, theme);
        assert_eq!(
            app.preferences.git_executable,
            if button == "OK" {
                "/custom Git/git"
            } else {
                ""
            }
        );
        assert_eq!(app.preferences.show_welcome, button != "OK");
        assert_eq!(
            app.preferences.file_double_click,
            if button == "OK" {
                crate::preferences::FileDoubleClick::ToggleStage
            } else {
                crate::preferences::FileDoubleClick::ShowDiff
            }
        );
        assert_eq!(
            app.preferences.date_format,
            if button == "OK" {
                crate::preferences::DateFormat::DayMonthYear
            } else {
                crate::preferences::DateFormat::Iso
            }
        );
        app.open_settings();
        assert_eq!(app.settings_window.interface_style, style);
        assert_eq!(app.settings_window.theme, theme);
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

fn filled_rect_count(shapes: &[ClippedShape], color: egui::Color32) -> usize {
    shapes
        .iter()
        .filter(|shape| {
            matches!(
                &shape.shape,
                egui::Shape::Rect(rect) if rect.fill == color
            )
        })
        .count()
}

#[test]
fn preferences_pages_fit_at_the_minimum_size_in_both_themes() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let context = egui::Context::default();
        crate::theme::install_fonts(&context);
        egui_extras::install_image_loaders(&context);
        theme.apply(&context);
        let mut app = SannyGitApp::with_theme(theme);
        for (page, title) in [
            (SettingsPage::General, "Confirmations"),
            (SettingsPage::Appearance, "Color theme:"),
            (SettingsPage::DateTime, "Time format:"),
            (SettingsPage::Git, "Path to Git executable:"),
            (SettingsPage::Remote, "Automatic fetch"),
            (SettingsPage::Llm, "Endpoint:"),
        ] {
            app.settings_window.page = page;
            let mut output = context.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(680.0, 380.0))),
                    ..Default::default()
                },
                |ui| window_contents(ui, &mut app),
            );
            output.textures_delta.clear();
            assert!(visible_text(&output.shapes, title));
            assert!(visible_text(&output.shapes, "Cancel"));
            assert!(visible_text(&output.shapes, "OK"));
            if page == SettingsPage::General {
                for label in ["Language:", "English", "Restore confirmation dialogs"] {
                    assert!(
                        visible_text(&output.shapes, label),
                        "Missing {label} at minimum size"
                    );
                }
            }
            if page == SettingsPage::DateTime {
                for label in ["Date format:", "Time format:"] {
                    assert_eq!(text_rows(&output.shapes, label), Some(1));
                }
            }
            if page == SettingsPage::Llm {
                assert!(visible_text(&output.shapes, crate::llm::DEFAULT_ENDPOINT));
                assert!(filled_rect_count(&output.shapes, app.palette().editor) >= 3);
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

#[test]
fn llm_error_shows_once_in_a_modal_in_both_themes() {
    let error = "Touch ID key storage requires a signed SannyGit application with Keychain entitlements. This build cannot access the data protection keychain. Use a properly signed build, or turn off secure storage and enter the API key for this session.";
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        for size in [Vec2::new(740.0, 420.0), Vec2::new(680.0, 380.0)] {
            let context = egui::Context::default();
            crate::theme::install_fonts(&context);
            theme.apply(&context);
            let mut app = SannyGitApp::with_theme(theme);
            app.settings_window.page = SettingsPage::Llm;
            app.settings_window.llm.secret_error = Some(error.into());
            let render = |app: &mut SannyGitApp, time, events| {
                let mut output = context.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ui| window_contents(ui, app),
                );
                output.textures_delta.clear();
                output
            };
            for time in [0.0, 0.1, 0.2] {
                render(&mut app, time, vec![]);
            }
            let output = render(&mut app, 0.3, vec![]);
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.job.text == "Attention required")
            }));
            let full_text_count = output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.job.text == error && !text.galley.elided)
                })
                .count();
            assert_eq!(
                full_text_count, 1,
                "Expected one complete modal error message ({theme:?}, {size:?})"
            );
        }
    }
}

#[test]
fn opening_and_applying_llm_settings_never_reads_saved_credentials() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        for repository in [false, true] {
            let context = egui::Context::default();
            crate::theme::install_fonts(&context);
            theme.apply(&context);
            let mut app = SannyGitApp::with_theme(theme);
            let path = "/projects/example".to_owned();
            app.repositories = vec![crate::models::Repository {
                name: "Project".into(),
                path: path.clone(),
            }];
            app.selected_repository = 0;
            app.repository_llm_configs
                .insert(path.clone(), app.llm_config.clone());
            let reads_before = crate::llm::CREDENTIAL_READS.with(|reads| reads.get());
            if repository {
                app.open_repository_llm_settings();
            } else {
                app.open_settings();
                app.settings_window.page = SettingsPage::Llm;
            }
            for _ in 0..2 {
                let mut output = context.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            pos2(0.0, 0.0),
                            Vec2::new(740.0, 420.0),
                        )),
                        ..Default::default()
                    },
                    |ui| window_contents(ui, &mut app),
                );
                output.textures_delta.clear();
            }
            let draft = if repository {
                app.settings_window.llm_repositories.get_mut(&path).unwrap()
            } else {
                &mut app.settings_window.llm
            };
            assert!(draft.config.api_key.is_empty());
            assert!(draft.secret_error.is_none());
            draft.config.model = "new-model".into();
            apply_settings(&mut app).unwrap();
            let config = if repository {
                app.effective_llm_config(&path)
            } else {
                &app.llm_config
            };
            assert_eq!(config.model, "new-model");
            assert_eq!(
                crate::llm::CREDENTIAL_READS.with(|reads| reads.get()),
                reads_before
            );
        }
    }
}
