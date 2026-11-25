use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use crate::app::{AiConnectionStatus, SannyGitApp, SettingsPage};
use crate::llm::{Config, SubscriptionProvider};
use eframe::egui::{
    Align, CursorIcon, Frame, Id, Label, Layout, Margin, Panel, RichText, Sense, Ui, Vec2,
};

const HEIGHT: f32 = 26.0;
const BORDER_WIDTH: f32 = 1.0;
/// Clear background between the cancel disc and the status-bar stroke.
const CANCEL_EDGE_GAP: f32 = 4.0;
/// Two pixels under the largest disc that still keeps `CANCEL_EDGE_GAP`.
const CANCEL_DIAMETER: f32 = HEIGHT - 2.0 * BORDER_WIDTH - 2.0 * CANCEL_EDGE_GAP - 2.0;
const CANCEL_SIZE: Vec2 = Vec2::splat(CANCEL_DIAMETER);

pub(crate) fn operation_cancel_id() -> Id {
    Id::unique("status.operation.cancel")
}

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    let status = if app.tag_patch_pending() {
        "Exporting patch…".to_owned()
    } else {
        app.status.clone()
    };
    Panel::bottom("working_tree.v2.status")
        .exact_size(HEIGHT)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(palette.background)
                .stroke((BORDER_WIDTH, palette.border))
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
                        .help_text(&status);
                        show_operation_cancel(ui, app);
                    });
                });
            });
        });
}

fn show_operation_cancel(ui: &mut Ui, app: &mut SannyGitApp) {
    if !(app.background_progress_only && app.manager.cancel.is_some()) {
        return;
    }
    let palette = app.palette();
    let response = icons::circled_button(
        ui,
        Icon::Cancel,
        palette.muted,
        "Cancel the background operation",
        CANCEL_SIZE,
    );
    ui.ctx()
        .data_mut(|data| data.insert_temp(operation_cancel_id(), response.rect));
    if response.clicked() {
        app.cancel_repository_job();
    }
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
    fn rendering_status_bar_does_not_check_saved_provider_sessions() {
        for provider in [SubscriptionProvider::ChatGpt, SubscriptionProvider::Copilot] {
            let context = Context::default();
            let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
            open_repository(&mut app, "/project");
            app.llm_config.chatgpt_subscription = true;
            app.llm_config.subscription_provider = provider;

            context
                .run_ui(RawInput::default(), |ui| show(ui, &mut app))
                .textures_delta
                .clear();

            assert!(matches!(
                app.settings_window.chatgpt_status,
                AiConnectionStatus::Unknown
            ));
            assert!(matches!(
                app.settings_window.copilot_status,
                AiConnectionStatus::Unknown
            ));
        }
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

    fn text_rects(shapes: &[ClippedShape], expected: &str) -> Vec<Rect> {
        shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == expected => {
                    Some(Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn operation_cancel_follows_the_message_in_the_status_bar() {
        use crate::theme::{InterfaceStyle, ThemeChoice};
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };

        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for size in [Vec2::new(1100.0, 700.0), Vec2::new(1400.0, 900.0)] {
                    let ctx = Context::default();
                    crate::theme::install_fonts(&ctx);
                    theme.apply(&ctx);
                    theme.apply_style(&ctx, style);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.interface_style = style;
                    app.repositories.push(Repository {
                        name: "Test".into(),
                        path: "D:/repositories/example".into(),
                    });
                    app.selected_repository = 0;
                    app.branches.current_branch = "main".into();
                    app.status = "Syncing with remote…".into();
                    app.background_progress_only = true;
                    app.manager.cancel = Some(Arc::new(AtomicBool::new(false)));

                    let render = |app: &mut SannyGitApp, events: Vec<Event>| {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                events,
                                ..Default::default()
                            },
                            |ui| crate::ui::show(ui, app),
                        );
                        output.textures_delta.clear();
                        output
                    };

                    let output = render(&mut app, vec![]);
                    let messages = text_rects(&output.shapes, "Syncing with remote…");
                    assert_eq!(
                        messages.len(),
                        1,
                        "Status should appear only in the status bar in {theme:?} {style:?} at {size:?}"
                    );
                    let status_message = messages[0];
                    let cancel = ctx
                        .data(|data| data.get_temp::<Rect>(super::operation_cancel_id()))
                        .unwrap_or_else(|| {
                            panic!("Cancel cross missing in {theme:?} {style:?} at {size:?}")
                        });
                    let window = Rect::from_min_size(Pos2::ZERO, size).expand(2.0);
                    assert!(
                        window.contains_rect(cancel),
                        "Cancel cross {cancel:?} leaves the {size:?} window"
                    );
                    assert!(
                        status_message.right() <= cancel.left() + 1.0
                            && cancel.left() - status_message.right() < 16.0,
                        "Cross is not immediately after the message in {theme:?} {style:?} at {size:?}: message {status_message:?} cancel {cancel:?}"
                    );
                    assert!(
                        (cancel.center().y - status_message.center().y).abs() < 6.0,
                        "Cross left the status row"
                    );
                    assert!(status_message.top() > size.y - 40.0);
                    let bar_top = size.y - HEIGHT;
                    let bar_bottom = size.y;
                    let edge = BORDER_WIDTH + CANCEL_EDGE_GAP;
                    assert!(
                        cancel.top() >= bar_top + edge - 1.0,
                        "Cancel touches the top border in {theme:?} {style:?} at {size:?}: {cancel:?}"
                    );
                    assert!(
                        cancel.bottom() <= bar_bottom - edge + 1.0,
                        "Cancel touches the bottom border in {theme:?} {style:?} at {size:?}: {cancel:?}"
                    );
                    let disc = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Circle(circle)
                                if circle.center.distance(cancel.center()) < 1.0
                                    && (circle.radius - CANCEL_DIAMETER * 0.5).abs() < 0.5 =>
                            {
                                Some(*circle)
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| {
                            panic!("Cancel disc missing in {theme:?} {style:?} at {size:?}")
                        });
                    assert!(
                        disc.center.y - disc.radius >= bar_top + edge - 1.0
                            && disc.center.y + disc.radius <= bar_bottom - edge + 1.0,
                        "Cancel disc touches the status-bar border: {disc:?}"
                    );
                    let settings = text_rects(&output.shapes, "Settings")
                        .into_iter()
                        .min_by(|left, right| left.top().total_cmp(&right.top()))
                        .expect("Settings in toolbar");
                    assert!(
                        settings.bottom() + 20.0 < cancel.top(),
                        "Cancel should not sit beside Settings: settings {settings:?}, cancel {cancel:?}"
                    );

                    let pointer = cancel.center();
                    for pressed in [true, false] {
                        render(
                            &mut app,
                            vec![
                                Event::PointerMoved(pointer),
                                Event::PointerButton {
                                    pos: pointer,
                                    button: PointerButton::Primary,
                                    pressed,
                                    modifiers: Modifiers::NONE,
                                },
                            ],
                        );
                    }
                    assert!(
                        app.manager
                            .cancel
                            .as_ref()
                            .is_some_and(|cancel| cancel.load(Ordering::Relaxed)),
                        "Clicking the cross did not cancel"
                    );
                    assert_eq!(app.status, "Cancelling operation…");

                    app.manager.cancel = None;
                    app.background_progress_only = false;
                    app.status = "Idle".into();
                    let output = render(&mut app, vec![]);
                    assert!(text_rects(&output.shapes, "Syncing with remote…").is_empty());
                    let idle = text_rects(&output.shapes, "Idle");
                    assert_eq!(idle.len(), 1);
                    assert_eq!(idle[0].center().y, status_message.center().y);
                }
            }
        }
    }
}
