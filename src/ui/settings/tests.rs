use super::*;
use eframe::egui::{epaint::ClippedShape, pos2, Event, Modifiers, PointerButton, RawInput, Rect};

#[test]
fn repository_instructions_are_editable_with_global_model_settings() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        theme.apply(&ctx);
        let mut app = SannyGitApp::with_theme(theme);
        app.repositories = vec![crate::models::Repository {
            name: "Project".into(),
            path: "/project".into(),
        }];
        app.selected_repository = 0;
        app.llm_config.instructions = "Global generation text".into();
        app.open_repository_llm_settings();
        let original = app.llm_config.clone();
        let reads = crate::llm::CREDENTIAL_READS.with(|reads| reads.get());
        let render = |app: &mut SannyGitApp, events| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(790.0, 456.0))),
                    events,
                    ..Default::default()
                },
                |ui| repository_window_contents(ui, app),
            );
            output.textures_delta.clear();
            output
        };
        render(&mut app, vec![]);
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
        click(&mut app, "Commit messages");
        let mut output = render(&mut app, vec![]);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Global generation text" => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .expect("Instructions editor is missing");
        output.textures_delta.clear();
        for pressed in [true, false] {
            render(
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
        }
        render(&mut app, vec![Event::Text("custom".into())]);
        apply_settings(&mut app).unwrap();
        assert!(app.repository_llm_configs.is_empty());
        assert!(app
            .effective_generation_config("/project")
            .instructions
            .contains("custom"));
        assert_eq!(app.llm_config, original);
        assert_eq!(
            crate::llm::CREDENTIAL_READS.with(|reads| reads.get()),
            reads
        );
        app.llm_config.model = "updated-global-model".into();
        assert_eq!(
            app.effective_generation_config("/project").model,
            "updated-global-model"
        );
        assert!(app
            .effective_generation_config("/project")
            .instructions
            .contains("custom"));
        app.open_repository_llm_settings();
        app.settings_window.llm_instructions.clear();
        apply_settings(&mut app).unwrap();
        assert_eq!(
            app.effective_generation_config("/project").instructions,
            "Global generation text"
        );
    }
}

#[test]
fn repository_prompt_tab_restores_global_instructions_without_touching_model_settings() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let context = egui::Context::default();
        crate::theme::install_fonts(&context);
        theme.apply(&context);
        let mut app = SannyGitApp::with_theme(theme);
        app.repositories = vec![crate::models::Repository {
            name: "Project".into(),
            path: "/project".into(),
        }];
        app.selected_repository = 0;
        app.llm_config.store_api_key = false;
        app.llm_config.instructions = "Global instructions".into();
        // A repository config saved without a separate instructions override
        // still surfaces its custom prompt on the Prompt tab.
        app.repository_llm_configs.insert(
            "/project".into(),
            crate::llm::Config {
                model: "project-model".into(),
                store_api_key: false,
                instructions: "Custom instructions".into(),
                ..Default::default()
            },
        );
        app.open_repository_llm_settings();
        assert!(
            app.settings_window
                .llm_instructions
                .contains_key("/project"),
            "Custom repository instructions must mark a prompt override"
        );
        let render = |app: &mut SannyGitApp, events| {
            let mut output = context.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(790.0, 456.0))),
                    events,
                    ..Default::default()
                },
                |ui| repository_window_contents(ui, app),
            );
            output.textures_delta.clear();
            output
        };
        render(&mut app, vec![]);
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
        click(&mut app, "Commit messages");
        let mut output = render(&mut app, vec![]);
        assert!(
            visible_text(&output.shapes, "Custom instructions"),
            "Missing custom instructions ({theme:?})"
        );
        output.textures_delta.clear();
        app.settings_window.llm_instructions.remove("/project");
        apply_settings(&mut app).unwrap();
        assert!(app.repository_llm_instructions.is_empty());
        let config = app.effective_generation_config("/project");
        assert_eq!(config.instructions, "Global instructions");
        assert_eq!(config.model, "project-model");
        assert!(app.repository_llm_configs.contains_key("/project"));
    }
}

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
fn reset_repository_settings_draft_disables_when_matching_defaults() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repositories = vec![crate::models::Repository {
        name: "Project".into(),
        path: "/projects/example".into(),
    }];
    app.selected_repository = 0;
    let path = app.repositories[0].path.clone();
    app.repository_llm_configs.insert(
        path.clone(),
        crate::llm::Config {
            model: "project-model".into(),
            ..Default::default()
        },
    );
    app.repository_llm_instructions
        .insert(path.clone(), "Project instructions".into());
    app.open_repository_llm_settings();
    assert!(repository_settings_differ_from_defaults(&app, &path));
    reset_repository_settings_draft(&mut app, &path);
    assert!(
        !repository_settings_differ_from_defaults(&app, &path),
        "Reset should disable while overrides remain on disk until OK"
    );
    assert!(app.repository_llm_configs.contains_key(&path));
}

#[test]
fn repository_overrides_can_be_enabled_saved_cancelled_and_reset_in_both_themes() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        for size in [Vec2::new(790.0, 456.0), Vec2::new(680.0, 380.0)] {
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
                    |ui| repository_window_contents(ui, app),
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
            for label in ["AI", "Commit messages", "Endpoint:", "OK", "Cancel"] {
                assert!(
                    visible_text(&output.shapes, label),
                    "Missing {label} at {size:?} {theme:?}"
                );
            }
            click(&mut app, "Commit messages");
            output = render(&mut app, vec![]);
            for label in ["Generation instructions:", "OK"] {
                assert!(
                    visible_text(&output.shapes, label),
                    "Missing {label} on commit messages page at {size:?} {theme:?}"
                );
            }
            for label in [
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
            render(&mut app, vec![]);
            let draft = app.settings_window.llm_repositories.get_mut(&path).unwrap();
            draft.config.model = "project-model".into();
            app.settings_window
                .llm_instructions
                .insert(path.clone(), "Project instructions".into());
            assert_eq!(
                app.settings_window.llm.config.instructions,
                "Global instructions"
            );
            // Saving the repository form must not apply unrelated preferences.
            app.settings_window.theme = if theme == ThemeChoice::Light {
                ThemeChoice::Dark
            } else {
                ThemeChoice::Light
            };
            apply_settings(&mut app).unwrap();
            assert_eq!(
                app.effective_generation_config(&path).instructions,
                "Project instructions"
            );
            assert_eq!(
                app.effective_generation_config(&path).model,
                "project-model"
            );
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
                app.effective_generation_config(&path).instructions,
                "Project instructions"
            );

            app.open_repository_llm_settings();
            let global = app.llm_config.clone();
            let draft = app.settings_window.llm_repositories.get_mut(&path).unwrap();
            draft.config.endpoint = global.endpoint;
            draft.config.model = global.model;
            draft.config.store_api_key = global.store_api_key;
            apply_settings(&mut app).unwrap();
            assert!(!app.repository_llm_configs.contains_key(&path));
            assert_eq!(app.effective_llm_config(&path), &app.llm_config);
        }
    }
}

#[test]
fn global_ai_connection_mode_persists_and_applies_to_repositories() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.repository_llm_configs.insert(
        "/project".into(),
        crate::llm::Config {
            endpoint: "http://localhost:11434/v1/chat/completions".into(),
            model: "local-model".into(),
            chatgpt_subscription: false,
            ..Default::default()
        },
    );
    app.open_settings();
    app.settings_window.llm.config.chatgpt_subscription = true;
    app.settings_window.llm.config.subscription_provider =
        crate::llm::SubscriptionProvider::Copilot;
    use crate::llm::chatgpt::PROVIDER_ID;
    app.settings_window.llm.config.subscription_models.insert(
        PROVIDER_ID.into(),
        crate::llm::SubscriptionModelSettings {
            model: "gpt-test".into(),
            reasoning_effort: "low".into(),
        },
    );
    apply_settings(&mut app).unwrap();
    assert!(app.llm_config.chatgpt_subscription);
    assert_eq!(
        app.llm_config.subscription_provider,
        crate::llm::SubscriptionProvider::Copilot
    );
    assert_eq!(
        app.llm_config.subscription_models[&PROVIDER_ID.to_owned()].model,
        "gpt-test"
    );

    let restored: crate::llm::Config =
        serde_json::from_str(&serde_json::to_string(&app.llm_config).unwrap()).unwrap();
    assert!(restored.chatgpt_subscription);
    assert_eq!(
        restored.subscription_provider,
        crate::llm::SubscriptionProvider::Copilot
    );
    assert_eq!(
        restored.subscription_models[&PROVIDER_ID.to_owned()].model,
        "gpt-test"
    );

    let generation = app.effective_generation_config("/project");
    assert!(generation.chatgpt_subscription);
    assert_eq!(
        generation.subscription_provider,
        crate::llm::SubscriptionProvider::Copilot
    );
    assert_eq!(
        generation.subscription_models[&PROVIDER_ID.to_owned()].model,
        "gpt-test"
    );
    assert_eq!(generation.model, "local-model");
}

#[test]
fn global_ai_provider_applies_without_ok_and_cancel_restores_it() {
    use crate::llm::SubscriptionProvider;
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.llm_config.chatgpt_subscription = true;
    app.llm_config.subscription_provider = SubscriptionProvider::ChatGpt;
    app.open_settings();
    app.settings_window.llm.config.subscription_provider = SubscriptionProvider::Copilot;
    app.sync_global_ai_connection_from_draft();
    assert_eq!(
        app.llm_config.subscription_provider,
        SubscriptionProvider::Copilot
    );
    assert_eq!(
        app.effective_generation_config("/any")
            .subscription_provider,
        SubscriptionProvider::Copilot
    );

    app.discard_unsaved_global_ai_connection();
    assert_eq!(
        app.llm_config.subscription_provider,
        SubscriptionProvider::ChatGpt
    );

    let restored: crate::llm::Config =
        serde_json::from_str(&serde_json::to_string(&app.llm_config).unwrap()).unwrap();
    assert_eq!(
        restored.subscription_provider,
        SubscriptionProvider::ChatGpt
    );
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
                (SettingsPage::General, "User Interface"),
                (SettingsPage::Appearance, "Appearance"),
                (SettingsPage::Git, "Git"),
                (SettingsPage::Remote, "Remote"),
                (SettingsPage::DateTime, "Date & Time"),
                (SettingsPage::Logging, "Logging"),
                (SettingsPage::Llm, "AI"),
                (SettingsPage::CommitMessages, "Commit messages"),
                (SettingsPage::CommitContext, "Context"),
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
fn interface_style_dropdown_updates_the_draft_in_both_themes() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let context = egui::Context::default();
        crate::theme::install_fonts(&context);
        theme.apply(&context);
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
            output
        };
        let click = |app: &mut SannyGitApp, label| {
            let output = render(app, vec![]);
            let position = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == label => {
                        Some(text.pos + text.galley.size() / 2.0)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing {label}"));
            for pressed in [true, false] {
                render(
                    app,
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
            }
        };

        click(&mut app, "Classic");
        click(&mut app, "Modern");
        assert_eq!(app.settings_window.interface_style, InterfaceStyle::Modern);
        click(&mut app, "Modern");
        click(&mut app, "Classic");
        assert_eq!(app.settings_window.interface_style, InterfaceStyle::Classic);
        assert_eq!(app.interface_style, InterfaceStyle::Classic);
    }
}

#[test]
fn preferences_changes_apply_on_ok_and_cancel_discards_them() {
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
        app.settings_window.preferences.log_level = Level::Off;
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
        let log_level = if button == "OK" {
            Level::Off
        } else {
            Level::Info
        };
        assert_eq!(app.preferences.log_level, log_level);
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
        assert_eq!(app.settings_window.preferences.log_level, log_level);
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
    text_position(shapes, expected).map(|position| position.x)
}

fn text_position(shapes: &[ClippedShape], expected: &str) -> Option<egui::Pos2> {
    shapes.iter().find_map(|shape| {
        let egui::Shape::Text(text) = &shape.shape else {
            return None;
        };
        (text.galley.job.text == expected).then_some(text.pos)
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
fn logging_path_shares_a_row_with_its_label_and_copy_button() {
    let Some(path) = logging::path() else {
        return;
    };
    let path = path.to_string_lossy();
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        for size in [Vec2::new(740.0, 420.0), Vec2::new(680.0, 380.0)] {
            let context = egui::Context::default();
            crate::theme::install_fonts(&context);
            theme.apply(&context);
            let mut app = SannyGitApp::with_theme(theme);
            app.settings_window.page = SettingsPage::Logging;
            let mut output = context.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
                    ..Default::default()
                },
                |ui| window_contents(ui, &mut app),
            );
            output.textures_delta.clear();
            let bounds = |label: &str| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => Some(
                            text.galley
                                .rect
                                .translate(text.pos.to_vec2())
                                .intersect(shape.clip_rect),
                        ),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("Missing {label} at {size:?} in {theme:?}"))
            };
            let level = bounds("Log level:");
            let file = bounds("Log file:");
            let path = bounds(&path);
            let copy = bounds("Copy path");
            assert_eq!(level.left(), file.left());
            let selection = bounds("Information");
            assert!(
                (path.left() - selection.left()).abs() < 10.0,
                "Path {path:?} and level {selection:?} should start in the same column"
            );
            assert!((file.center().y - path.center().y).abs() < 2.0);
            assert!((path.center().y - copy.center().y).abs() < 2.0);
            assert!(file.right() < path.left() && path.right() < copy.left());
            assert!(copy.right() < size.x);
        }
    }
}

#[test]
fn copilot_sign_in_controls_fit_in_both_themes() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let context = egui::Context::default();
        crate::theme::install_fonts(&context);
        egui_extras::install_image_loaders(&context);
        theme.apply(&context);
        let mut app = SannyGitApp::with_theme(theme);
        app.settings_window.page = SettingsPage::Llm;
        let config = &mut app.settings_window.llm.config;
        config.chatgpt_subscription = true;
        config.subscription_provider = crate::llm::SubscriptionProvider::Copilot;
        for size in [Vec2::new(680.0, 380.0), PREFERENCES_SIZE] {
            for (status, action, hidden_action) in [
                (AiConnectionStatus::SignedOut, "Sign in", "Sign out"),
                (AiConnectionStatus::Connected, "Sign out", "Sign in"),
            ] {
                app.settings_window.copilot_status = status;
                let mut output = context.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
                        ..Default::default()
                    },
                    |ui| window_contents(ui, &mut app),
                );
                output.textures_delta.clear();
                let provider = text_position(&output.shapes, "GitHub Copilot").unwrap();
                let button = text_position(&output.shapes, action).unwrap();
                assert!(button.x > provider.x && (button.y - provider.y).abs() < 7.0);
                assert!(!visible_text(&output.shapes, "Model:"));
                assert!(!visible_text(&output.shapes, "Provider default"));
                assert!(!visible_text(&output.shapes, hidden_action));
                assert!(visible_text(&output.shapes, "OK"));
                assert!(
                    filled_rect_count(&output.shapes, app.palette().selection.gamma_multiply(0.4))
                        > 0
                );
            }
        }
    }
}

#[test]
fn restore_all_confirmations_clears_skips_on_ok_and_reopens_unchecked() {
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.skip_push_confirmation = true;
    app.preferences.skip_sync_confirmation = true;
    app.open_settings();
    assert!(!app.settings_window.restore_all_confirmations);
    app.settings_window.restore_all_confirmations = true;
    apply_settings(&mut app).unwrap();
    assert!(!app.skip_push_confirmation);
    assert!(!app.preferences.skip_sync_confirmation);
    app.open_settings();
    assert!(!app.settings_window.restore_all_confirmations);
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
            (SettingsPage::Logging, "Log level:"),
            (SettingsPage::Git, "Path to Git executable:"),
            (SettingsPage::Remote, "Automatic fetch"),
            (SettingsPage::Llm, "Endpoint:"),
            (SettingsPage::CommitMessages, "Generation instructions:"),
            (SettingsPage::CommitContext, "Patch size:"),
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
                for label in [
                    "Language:",
                    "English",
                    "Use Staged only when opening or staging files",
                    "Restore all confirmation dialogs",
                ] {
                    assert!(
                        visible_text(&output.shapes, label),
                        "Missing {label} at minimum size"
                    );
                }
            }
            if page == SettingsPage::Appearance {
                for label in ["Interface style:", "Color theme:"] {
                    assert_eq!(text_rows(&output.shapes, label), Some(1));
                }
            }
            if page == SettingsPage::DateTime {
                for label in ["Date format:", "Time format:"] {
                    assert_eq!(text_rows(&output.shapes, label), Some(1));
                }
            }
            if page == SettingsPage::Llm {
                assert!(!visible_text(&output.shapes, "Generation instructions:"));
                assert!(!visible_text(&output.shapes, "Sign in"));
                assert!(!visible_text(&output.shapes, "Test connection"));
                assert!(!visible_text(&output.shapes, "Manage usage"));
                assert!(visible_text(&output.shapes, crate::llm::DEFAULT_ENDPOINT));
                assert!(filled_rect_count(&output.shapes, app.palette().editor) >= 3);
                for label in ["Endpoint:", "Model:", "API key:"] {
                    assert_eq!(text_rows(&output.shapes, label), Some(1));
                }
                let left = text_left(&output.shapes, "Endpoint:").unwrap();
                for label in ["Model:", "API key:"] {
                    assert!((text_left(&output.shapes, label).unwrap() - left).abs() < 0.5);
                }
                app.settings_window.llm.config.chatgpt_subscription = true;
                app.settings_window.chatgpt_models = vec![crate::llm::chatgpt::Model {
                    slug: "model-fixture".into(),
                    display_name: "Fixture Model".into(),
                    reasoning_efforts: vec!["low".into(), "high".into()],
                    input_cost_rank: u32::MAX,
                }];
                app.settings_window.chatgpt_models_attempted = true;
                for (status, action, hidden_action) in [
                    (AiConnectionStatus::SignedOut, "Sign in", "Sign out"),
                    (AiConnectionStatus::Connected, "Sign out", "Sign in"),
                ] {
                    app.settings_window.chatgpt_status = status;
                    let mut provider = context.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                pos2(0.0, 0.0),
                                Vec2::new(680.0, 380.0),
                            )),
                            ..Default::default()
                        },
                        |ui| window_contents(ui, &mut app),
                    );
                    provider.textures_delta.clear();
                    assert!(visible_text(&provider.shapes, action));
                    assert!(visible_text(&provider.shapes, "Model:"));
                    assert!(visible_text(&provider.shapes, "effort: Low"));
                    for label in [
                        hidden_action,
                        "Status:",
                        "Endpoint:",
                        "Test connection",
                        "Manage usage",
                    ] {
                        assert!(
                            !visible_text(&provider.shapes, label),
                            "Unexpected {label} in {theme:?}"
                        );
                    }
                }
            }
            if page == SettingsPage::CommitMessages {
                assert!(!visible_text(&output.shapes, "Endpoint:"));
                assert!(!visible_text(&output.shapes, "Patch size:"));
                assert!(visible_text(&output.shapes, "Language:"));
                assert!(visible_text(&output.shapes, "English"));
                assert!(visible_text(
                    &output.shapes,
                    "Show token counts after generation"
                ));
                assert!(visible_text(
                    &output.shapes,
                    &app.settings_window.llm.config.instructions
                ));
            }
            if page == SettingsPage::CommitContext {
                assert!(!visible_text(&output.shapes, "Endpoint:"));
                assert!(!visible_text(&output.shapes, "Generation instructions:"));
                for label in [
                    "Changes:",
                    "Same as commit panel",
                    "256 KiB",
                    "Include the current branch name",
                    "Excluded paths:",
                ] {
                    assert!(visible_text(&output.shapes, label), "Missing {label}");
                }
                for label in [
                    "New file content:",
                    "Total patch size:",
                    "When the patch is too large:",
                    "Diff context:",
                    "Ignore whitespace when building the diff",
                    "Changes to describe:",
                ] {
                    assert!(!visible_text(&output.shapes, label), "Unexpected {label}");
                }
                let left = text_left(&output.shapes, "Changes:").unwrap();
                assert!((text_left(&output.shapes, "Patch size:").unwrap() - left).abs() < 0.5);
                for label in ["Changes:", "Patch size:"] {
                    assert_eq!(text_rows(&output.shapes, label), Some(1));
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
                            Vec2::new(740.0, 500.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        if repository {
                            repository_window_contents(ui, &mut app);
                        } else {
                            window_contents(ui, &mut app);
                        }
                    },
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
