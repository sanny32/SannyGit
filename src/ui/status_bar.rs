use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use crate::app::{AiConnectionStatus, SannyGitApp, SettingsPage};
use crate::llm::{Config, SubscriptionProvider};
use eframe::egui::{
    Align, CursorIcon, Frame, Id, Label, Layout, Margin, Panel, RichText, Sense, Ui, Vec2,
};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    track_provider_connection(app, ui.ctx());
    let palette = app.palette();
    let status = if app.tag_patch_pending() {
        "Exporting patch…".to_owned()
    } else {
        app.status.clone()
    };
    Panel::bottom("working_tree.v2.status")
        .exact_size(26.0)
        .frame(
            Frame::new()
                .fill(palette.background)
                .stroke((1.0, palette.border))
                .inner_margin(Margin::symmetric(8, 0)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                icons::paint(ui, Icon::Branch, palette.text, 14.0);
                ui.label(if app.branches.current_branch.is_empty() {
                    "No repository"
                } else {
                    &app.branches.current_branch
                });
                let badge = app.tracking.badge();
                if !badge.is_empty() {
                    ui.add(
                        eframe::egui::Label::new(super::sync_badge(&badge, palette))
                            .selectable(false),
                    )
                    .help_text(app.tracking.description());
                }
                ui.separator();
                ui.label(format!("{} changes", app.changes.len()));
                ui.separator();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some((icon, tooltip)) = connection_indicator(app) {
                        let (_, rect) = ui.allocate_space(Vec2::splat(14.0));
                        icons::image(icon, palette.text, 14.0).paint_at(ui, rect);
                        if ui
                            .interact(rect, Id::unique("status.provider"), Sense::CLICK)
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .help_text(tooltip)
                            .clicked()
                        {
                            open_ai_settings(app);
                        }
                        ui.separator();
                    }
                    if let Some(repository) = app.repositories.get(app.selected_repository) {
                        ui.scope(|ui| {
                            ui.set_max_width(ui.available_width() * 0.45);
                            ui.add(Label::new(&repository.path).truncate());
                        });
                        icons::paint(ui, Icon::Folder, palette.text, 14.0);
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(
                            Label::new(RichText::new(status.clone()).color(palette.muted))
                                .truncate()
                                .show_tooltip_when_elided(false),
                        )
                        .help_text(status);
                    });
                });
            });
        });
}

fn track_provider_connection(app: &mut SannyGitApp, ctx: &eframe::egui::Context) {
    if !app.llm_config.chatgpt_subscription
        || app.repositories.get(app.selected_repository).is_none()
    {
        return;
    }
    app.update_ai_connection_status(ctx, app.llm_config.subscription_provider);
}

fn open_ai_settings(app: &mut SannyGitApp) {
    let path = app
        .repositories
        .get(app.selected_repository)
        .map(|repository| repository.path.clone());
    let overrides = path.as_ref().is_some_and(|path| {
        app.repository_llm_configs.contains_key(path)
            || app.repository_llm_instructions.contains_key(path)
    });
    if overrides {
        app.open_repository_llm_settings();
    } else {
        app.open_settings();
        app.settings_window.page = SettingsPage::Llm;
    }
}

fn connection_indicator(app: &SannyGitApp) -> Option<(Icon, String)> {
    let path = &app.repositories.get(app.selected_repository)?.path;
    let config = app.effective_generation_config(path);
    if config.chatgpt_subscription {
        let connected = match config.subscription_provider {
            SubscriptionProvider::ChatGpt => app.settings_window.chatgpt_status,
            SubscriptionProvider::Copilot => app.settings_window.copilot_status,
        } == AiConnectionStatus::Connected;
        if !connected {
            return None;
        }
        return Some(match config.subscription_provider {
            SubscriptionProvider::ChatGpt => (Icon::OpenAi, "OpenAI (ChatGPT)".into()),
            SubscriptionProvider::Copilot => (Icon::GitHub, "GitHub Copilot".into()),
        });
    }
    // A cloud key stays in the OS credential store until generation unlocks it.
    // Reading it from the status bar can prompt for Touch ID on macOS.
    if config.is_local() || !config.api_key.trim().is_empty() {
        Some((Icon::Code, api_tooltip(&config)))
    } else {
        None
    }
}

fn api_tooltip(config: &Config) -> String {
    let name = crate::llm::PROVIDER_PRESETS
        .iter()
        .find(|preset| preset.endpoint == config.endpoint.trim())
        .map(|preset| preset.name)
        .unwrap_or("OpenAI-compatible API");
    let model = config.model.trim();
    if model.is_empty() {
        name.to_owned()
    } else {
        format!("{name} ({model})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Repository;
    use crate::theme::ThemeChoice;
    use eframe::egui::{
        self, epaint::ClippedShape, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect,
        Vec2,
    };

    fn repository(path: &str) -> Repository {
        Repository {
            name: "Project".into(),
            path: path.into(),
        }
    }

    fn open_repository(app: &mut SannyGitApp, path: &str) {
        app.repositories = vec![repository(path)];
        app.selected_repository = 0;
        app.branches.current_branch = "main".into();
    }

    #[test]
    fn indicator_follows_the_connected_provider_for_the_open_repository() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.llm_config.chatgpt_subscription = true;
        app.llm_config.subscription_provider = SubscriptionProvider::Copilot;
        app.settings_window.copilot_status = AiConnectionStatus::Connected;
        assert!(connection_indicator(&app).is_none());

        open_repository(&mut app, "/project");
        for status in [
            AiConnectionStatus::Unknown,
            AiConnectionStatus::Checking,
            AiConnectionStatus::SignedOut,
            AiConnectionStatus::ChangingSession,
            AiConnectionStatus::Failed,
        ] {
            app.settings_window.copilot_status = status;
            assert!(connection_indicator(&app).is_none());
        }
        app.settings_window.copilot_status = AiConnectionStatus::Connected;
        assert_eq!(
            connection_indicator(&app),
            Some((Icon::GitHub, "GitHub Copilot".into()))
        );

        app.llm_config.subscription_provider = SubscriptionProvider::ChatGpt;
        app.settings_window.chatgpt_status = AiConnectionStatus::SignedOut;
        assert!(connection_indicator(&app).is_none());
        app.settings_window.chatgpt_status = AiConnectionStatus::Connected;
        assert_eq!(
            connection_indicator(&app),
            Some((Icon::OpenAi, "OpenAI (ChatGPT)".into()))
        );

        app.repository_llm_configs.insert(
            "/project".into(),
            Config {
                endpoint: "http://localhost:11434/v1/chat/completions".into(),
                model: String::new(),
                api_key: "repo-key".into(),
                ..Config::default()
            },
        );
        assert_eq!(
            connection_indicator(&app),
            Some((Icon::OpenAi, "OpenAI (ChatGPT)".into()))
        );
    }

    #[test]
    fn indicator_shows_the_repository_api_only_when_it_can_connect() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        open_repository(&mut app, "/project");
        assert!(connection_indicator(&app).is_none());

        app.llm_config.api_key = "  ".into();
        assert!(connection_indicator(&app).is_none());

        app.llm_config.api_key = "test-key".into();
        assert_eq!(
            connection_indicator(&app),
            Some((Icon::Code, "OpenAI (gpt-4.1-mini)".into()))
        );

        app.llm_config.endpoint = "https://api.kimi.ai/coding/v1/chat/completions".into();
        app.llm_config.model = "kimi-for-coding".into();
        assert_eq!(
            connection_indicator(&app),
            Some((Icon::Code, "Kimi (kimi-for-coding)".into()))
        );

        app.llm_config.api_key.clear();
        app.llm_config.endpoint = "http://127.0.0.1:1234/v1/chat/completions".into();
        app.llm_config.model.clear();
        assert_eq!(
            connection_indicator(&app),
            Some((Icon::Code, "OpenAI-compatible API".into()))
        );

        app.llm_config.endpoint = "http://localhost:11434/v1/chat/completions".into();
        assert_eq!(
            connection_indicator(&app),
            Some((Icon::Code, "Ollama (local)".into()))
        );

        app.llm_config.api_key.clear();
        app.llm_config.endpoint = crate::llm::DEFAULT_ENDPOINT.into();
        app.llm_config.model = crate::llm::DEFAULT_MODEL.into();
        app.repository_llm_configs.insert(
            "/project".into(),
            Config {
                endpoint: "https://api.example.test/v1/chat/completions".into(),
                model: "custom-model".into(),
                api_key: "repo-key".into(),
                ..Config::default()
            },
        );
        assert_eq!(
            connection_indicator(&app),
            Some((Icon::Code, "OpenAI-compatible API (custom-model)".into()))
        );

        app.repositories.push(repository("/other"));
        app.selected_repository = 1;
        assert!(connection_indicator(&app).is_none());
    }

    fn text_rect(shapes: &[ClippedShape], expected: &str) -> Rect {
        shapes
            .iter()
            .find_map(|shape| {
                let egui::Shape::Text(text) = &shape.shape else {
                    return None;
                };
                (text.galley.job.text == expected).then(|| {
                    text.galley
                        .rect
                        .translate(text.pos.to_vec2())
                        .intersect(shape.clip_rect)
                })
            })
            .unwrap_or_else(|| panic!("Missing {expected}"))
    }

    #[test]
    fn connected_provider_icon_sits_to_the_right_of_the_path() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let mut path_right = [0.0, 0.0];
            for connected in [false, true] {
                let context = Context::default();
                crate::theme::install_fonts(&context);
                egui_extras::install_image_loaders(&context);
                theme.apply(&context);
                let mut app = SannyGitApp::with_theme(theme);
                let path = r"D:\repositories\example".to_owned();
                open_repository(&mut app, &path);
                app.llm_config.chatgpt_subscription = true;
                app.llm_config.subscription_provider = SubscriptionProvider::Copilot;
                app.settings_window.copilot_status = if connected {
                    AiConnectionStatus::Connected
                } else {
                    AiConnectionStatus::SignedOut
                };
                let size = Vec2::new(980.0, 220.0);
                let mut output = context.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| show(ui, &mut app),
                );
                let path_rect = text_rect(&output.shapes, &path);
                path_right[usize::from(connected)] = path_rect.right();
                let icon = context.read_response(egui::Id::unique("status.provider"));
                assert_eq!(icon.is_some(), connected, "{theme:?}");
                if let Some(icon) = &icon {
                    assert!(
                        icon.rect.left() + 1.0 >= path_rect.right(),
                        "{theme:?}: icon {:?} should follow path {path_rect:?}",
                        icon.rect
                    );
                    assert!(icon.rect.center().y > size.y - 26.0);
                }
                output.textures_delta.clear();
            }
            assert!(
                path_right[1] + 10.0 < path_right[0],
                "{theme:?}: connected path right {} should move left of signed-out {}",
                path_right[1],
                path_right[0]
            );
        }
    }

    #[test]
    fn provider_icon_opens_repository_settings_only_when_they_override_the_application() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        open_repository(&mut app, "/project");
        open_ai_settings(&mut app);
        assert!(app.settings_window.open);
        assert_eq!(app.settings_window.page, SettingsPage::Llm);
        assert!(app.settings_window.llm_repository.is_none());

        app.repository_llm_instructions
            .insert("/project".into(), "Project instructions".into());
        open_ai_settings(&mut app);
        assert_eq!(
            app.settings_window.llm_repository.as_deref(),
            Some("/project")
        );
        assert_eq!(app.settings_window.page, SettingsPage::Llm);

        app.repository_llm_instructions.clear();
        app.repository_llm_configs.insert(
            "/other".into(),
            Config {
                model: "repo-model".into(),
                ..Config::default()
            },
        );
        open_ai_settings(&mut app);
        assert!(app.settings_window.llm_repository.is_none());

        app.repositories.push(repository("/other"));
        app.selected_repository = 1;
        open_ai_settings(&mut app);
        assert_eq!(
            app.settings_window.llm_repository.as_deref(),
            Some("/other")
        );
    }

    #[test]
    fn clicking_the_provider_icon_opens_ai_settings() {
        let context = Context::default();
        crate::theme::install_fonts(&context);
        egui_extras::install_image_loaders(&context);
        ThemeChoice::Light.apply(&context);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        open_repository(&mut app, r"D:\repositories\example");
        app.llm_config.chatgpt_subscription = true;
        app.llm_config.subscription_provider = SubscriptionProvider::Copilot;
        app.settings_window.copilot_status = AiConnectionStatus::Connected;
        let size = Vec2::new(980.0, 220.0);
        let render = |app: &mut SannyGitApp, events| {
            let mut output = context.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| show(ui, app),
            );
            output.textures_delta.clear();
            output
        };
        render(&mut app, vec![]);
        let target = context
            .read_response(egui::Id::unique("status.provider"))
            .expect("Provider icon")
            .rect
            .center();
        for pressed in [true, false] {
            render(
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
        }
        assert!(app.settings_window.open);
        assert_eq!(app.settings_window.page, SettingsPage::Llm);
        assert!(app.settings_window.llm_repository.is_none());
    }
}
