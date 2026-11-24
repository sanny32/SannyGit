use super::controls::dialog_style::DialogResponse;
use super::controls::icons::{self, Icon};
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::{AiConnectionStatus, SannyGitApp, SettingsPage},
    logging::{self, Level},
    theme::{semibold_family, InterfaceStyle, Palette, ThemeChoice},
};
use eframe::egui::{
    self, Align, CentralPanel, Frame, Key, Layout, Margin, Panel, Popup, Rect, RectAlign, RichText,
    ScrollArea, Sense, StrokeKind, TextWrapMode, Ui, UiBuilder, Vec2, ViewportCommand, ViewportId,
};

const PREFERENCES_SIZE: Vec2 = egui::vec2(790.0, 456.0);
/// Keeps both provider rows aligned with the Sign in / Sign out button.
const AI_ACTION_BUTTON_MIN_WIDTH: f32 = 86.0;

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    if !app.settings_window.open {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        if !app.about_dialog {
            crate::native_window::end_modal(ui.ctx());
        }
        return;
    }
    if app.settings_window.llm_repository.is_some() {
        repository_llm_window(ui.ctx(), app);
        return;
    }
    preferences_window(ui.ctx(), app);
}

fn preferences_window(ctx: &egui::Context, app: &mut SannyGitApp) {
    settings_viewport(
        ctx,
        app,
        ViewportId::from_hash_of("sannygit.preferences"),
        "SannyGit Preferences",
        false,
    );
}

fn repository_llm_window(ctx: &egui::Context, app: &mut SannyGitApp) {
    settings_viewport(
        ctx,
        app,
        ViewportId::from_hash_of("sannygit.repository.llm.settings"),
        "Repository AI Settings",
        true,
    );
}

fn settings_viewport(
    ctx: &egui::Context,
    app: &mut SannyGitApp,
    viewport_id: ViewportId,
    title: &'static str,
    repository: bool,
) {
    let min_size = [680.0, 380.0];
    if !app.settings_window.initial_position_set {
        app.settings_window.initial_position = ctx.input(|input| {
            let viewport = input.viewport();
            viewport
                .outer_rect
                .map(|outer| centered_position(outer, viewport.inner_rect, PREFERENCES_SIZE))
        });
        app.settings_window.initial_position_set = true;
    }
    let builder = crate::theme::native_viewport()
        .with_title(title)
        .with_inner_size(PREFERENCES_SIZE)
        .with_min_inner_size(min_size)
        .with_decorations(true)
        .with_taskbar(false)
        .with_minimize_button(false)
        .with_maximize_button(false);
    #[cfg(target_os = "linux")]
    let builder = builder.with_window_type(egui::X11WindowType::Dialog);
    let builder = match app.settings_window.initial_position {
        Some(position) => builder.with_position(position),
        None => builder,
    };
    ctx.show_viewport_immediate(viewport_id, builder, |ui, class| {
        #[cfg(target_os = "macos")]
        ui.input_mut(|input| input.events.append(&mut app.pending_native_edit_events));
        if escape_pressed(ui.ctx(), app) || ui.input(|input| input.viewport().close_requested()) {
            close_without_saving(ui.ctx(), app);
            return;
        }

        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        if app.settings_window.needs_reveal && class == egui::ViewportClass::Immediate {
            if let Err(error) = crate::native_window::begin_modal(ui.ctx(), title) {
                app.error = Some(format!(
                    "Cannot attach settings to the main window: {error}"
                ));
                close_without_saving(ui.ctx(), app);
                return;
            }
        }
        crate::theme::paint_native_background(ui, app.palette());
        #[cfg(target_os = "windows")]
        if let Err(error) = crate::native_window::finish_first_presentation(ui.ctx()) {
            app.error = Some(error.to_string());
        }
        if repository {
            repository_window_contents(ui, app);
        } else {
            window_contents(ui, app);
        }
        if app.settings_window.open
            && app.settings_window.needs_reveal
            && class == egui::ViewportClass::Immediate
        {
            // Immediate viewports process this command after painting and swapping the frame.
            #[cfg(target_os = "windows")]
            if let Err(error) = crate::native_window::prepare_child_presentation(ui.ctx(), title) {
                app.error = Some(error.to_string());
            }
            ui.ctx().send_viewport_cmd(ViewportCommand::Visible(true));
            app.settings_window.needs_reveal = false;
        }
    });
}

fn escape_pressed(ctx: &egui::Context, app: &SannyGitApp) -> bool {
    app.error_dialogs.pending.is_empty()
        && !egui::Popup::is_any_open(ctx)
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape))
}

fn close_after_save(ctx: &egui::Context, app: &mut SannyGitApp) {
    app.commit_global_ai_connection_session();
    finish_close(ctx, app);
}

fn close_without_saving(ctx: &egui::Context, app: &mut SannyGitApp) {
    app.discard_unsaved_global_ai_connection();
    finish_close(ctx, app);
}

fn finish_close(ctx: &egui::Context, app: &mut SannyGitApp) {
    #[cfg(target_os = "windows")]
    crate::native_window::cancel_first_presentation(ctx);
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    crate::native_window::end_modal(ctx);
    app.settings_window.open = false;
    if ctx.viewport_id() != ViewportId::ROOT {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}

fn centered_position(outer: egui::Rect, inner: Option<egui::Rect>, size: Vec2) -> egui::Pos2 {
    let decorations = inner.map_or(Vec2::ZERO, |inner| {
        (outer.size() - inner.size()).max(Vec2::ZERO)
    });
    outer.center() - (size + decorations) / 2.0
}

fn window_contents(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    super::controls::dialog_style::prepare_keyboard(ui);
    ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
    footer(ui, app, palette, false);
    navigation(ui, app, palette);
    let page_background = if app.settings_window.page == SettingsPage::Llm {
        palette.panel_alt
    } else {
        palette.header
    };
    CentralPanel::default()
        .frame(Frame::new().fill(page_background).inner_margin(12))
        .show(ui, |ui| {
            let page_height = ui.available_height();
            if app.settings_window.page == SettingsPage::Llm && page_height < 350.0 {
                ui.spacing_mut().item_spacing.y = 3.0;
            }
            let content_height = page_height;
            if app.settings_window.page == SettingsPage::CommitMessages {
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), content_height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_height(content_height);
                        ui.set_max_height(content_height);
                        page(ui, app, palette);
                    },
                );
            } else {
                ScrollArea::vertical()
                    .id_salt("preferences.page")
                    .auto_shrink([false, false])
                    .max_height(content_height)
                    .show(ui, |ui| page(ui, app, palette));
            }
        });
    super::show_errors(ui, app);
}

fn repository_window_contents(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    super::controls::dialog_style::prepare_keyboard(ui);
    ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
    footer(ui, app, palette, true);
    repository_navigation(ui, app, palette);
    CentralPanel::default()
        .frame(Frame::new().fill(palette.panel_alt).inner_margin(12))
        .show(ui, |ui| {
            let page_height = ui.available_height();
            let content_height = page_height;
            match app.settings_window.page {
                SettingsPage::CommitMessages => {
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), content_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_height(content_height);
                            ui.set_max_height(content_height);
                            repository_page(ui, app, palette);
                        },
                    );
                }
                _ => {
                    ScrollArea::vertical()
                        .id_salt("repository.preferences.page")
                        .auto_shrink([false, false])
                        .max_height(content_height)
                        .show(ui, |ui| repository_page(ui, app, palette));
                }
            }
        });
    super::show_errors(ui, app);
}

fn repository_navigation(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette) {
    Panel::left("repository.preferences.navigation")
        .resizable(false)
        .exact_size(206.0)
        .show_separator_line(false)
        .frame(Frame::new().fill(palette.header).inner_margin(Margin {
            left: 12,
            right: 0,
            top: 12,
            bottom: 12,
        }))
        .show(ui, |ui| {
            ui.take_available_space();
            let panel_size = ui.available_size();
            Frame::new()
                .fill(palette.panel)
                .stroke((1.0, palette.border))
                .show(ui, |ui| {
                    ui.set_min_size(panel_size - Vec2::splat(2.0));
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let scroll_height = ui.available_height();
                    ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .max_height(scroll_height)
                        .show(ui, |ui| {
                            section(ui, app, SettingsPage::Llm, "AI", |ui, app| {
                                entry(ui, app, SettingsPage::CommitContext, "Context");
                                entry(ui, app, SettingsPage::CommitMessages, "Commit messages");
                            });
                        });
                });
        });
}

fn repository_page(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    let Some(path) = app.settings_window.llm_repository.clone() else {
        return;
    };
    match app.settings_window.page {
        SettingsPage::Llm => repository_ai_page(ui, app, palette, &path),
        SettingsPage::CommitMessages => {
            heading(ui, "Commit messages");
            repository_llm_prompt_tab(ui, app, Some(&path), palette);
        }
        SettingsPage::CommitContext => {
            heading(ui, "Context");
            commit_context_page(
                ui,
                palette,
                &mut app.settings_window.llm.config,
                "repository.commit_context",
            );
        }
        _ => {}
    }
}

fn footer(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette, repository: bool) {
    Panel::bottom("preferences.footer")
        .exact_size(46.0)
        .frame(
            Frame::new()
                .fill(palette.header)
                .inner_margin(Margin::symmetric(12, 9)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if repository {
                    if let Some(path) = app.settings_window.llm_repository.clone() {
                        let enabled = repository_settings_differ_from_defaults(app, &path);
                        if ui
                            .add_enabled(
                                enabled,
                                super::controls::dialog_style::button("Reset"),
                            )
                            .help_text(
                                "Reset to defaults: discard repository overrides and use the settings from Preferences",
                            )
                            .disabled_help_text(
                                "This repository already uses the application defaults",
                            )
                            .clicked()
                        {
                            reset_repository_settings_draft(app, &path);
                        }
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.set_width(ui.available_width());
                    if ui
                        .add(super::controls::dialog_style::primary("OK", palette))
                        .dialog_accept()
                    {
                        match apply_settings(app) {
                            Ok(()) => close_after_save(ui.ctx(), app),
                            Err(error) => report_settings_error(app, error),
                        }
                    }
                    if ui
                        .add(super::controls::dialog_style::button("Cancel"))
                        .dialog_cancel()
                    {
                        close_without_saving(ui.ctx(), app);
                    }
                });
            });
        });
}

fn repository_settings_differ_from_defaults(app: &SannyGitApp, path: &str) -> bool {
    let window = &app.settings_window;
    let global = &app.llm_config;
    if window.llm_instructions.contains_key(path) {
        return true;
    }
    if let Some(draft) = window.llm_repositories.get(path) {
        let mut config = draft.config.clone();
        SannyGitApp::strip_repository_connection_mode(&mut config, global);
        if !SannyGitApp::repository_model_matches_global(&config, global) {
            return true;
        }
    }
    false
}

fn reset_repository_settings_draft(app: &mut SannyGitApp, path: &str) {
    let window = &mut app.settings_window;
    window.llm_instructions.remove(path);
    window.llm_repositories.remove(path);
    let mut config = app.llm_config.clone();
    SannyGitApp::strip_repository_connection_mode(&mut config, &app.llm_config);
    window.llm_repositories.insert(
        path.to_owned(),
        crate::app::LlmSettings {
            config,
            ..Default::default()
        },
    );
}

fn navigation(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette) {
    Panel::left("preferences.navigation")
        .resizable(false)
        .exact_size(206.0)
        .show_separator_line(false)
        .frame(Frame::new().fill(palette.header).inner_margin(Margin {
            left: 12,
            right: 0,
            top: 12,
            bottom: 12,
        }))
        .show(ui, |ui| {
            ui.take_available_space();
            let panel_size = ui.available_size();
            Frame::new()
                .fill(palette.panel)
                .stroke((1.0, palette.border))
                .show(ui, |ui| {
                    ui.set_min_size(panel_size - Vec2::splat(2.0));
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let scroll_height = ui.available_height();
                    ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .max_height(scroll_height)
                        .show(ui, |ui| {
                            section(
                                ui,
                                app,
                                SettingsPage::General,
                                "User Interface",
                                |ui, app| {
                                    entry(ui, app, SettingsPage::Appearance, "Appearance");
                                    entry(ui, app, SettingsPage::DateTime, "Date & Time");
                                    entry(ui, app, SettingsPage::Logging, "Logging");
                                },
                            );
                            section(ui, app, SettingsPage::Git, "Git", |ui, app| {
                                entry(ui, app, SettingsPage::Remote, "Remote");
                            });
                            section(ui, app, SettingsPage::Llm, "AI", |ui, app| {
                                entry(ui, app, SettingsPage::CommitContext, "Context");
                                entry(ui, app, SettingsPage::CommitMessages, "Commit messages");
                            });
                        });
                });
        });
}

fn section(
    ui: &mut Ui,
    app: &mut SannyGitApp,
    page: SettingsPage,
    title: &str,
    contents: impl FnOnce(&mut Ui, &mut SannyGitApp),
) {
    let id = ui.make_persistent_id(("preferences.section", title));
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
    let response =
        super::controls::selectable_row(ui, app.settings_window.page == page, 23.0, |ui| {
            state.show_toggle_button(ui, egui::collapsing_header::paint_default_icon);
            ui.label(title);
        });
    if response.clicked() {
        app.settings_window.page = page;
    }
    state.show_body_unindented(ui, |ui| contents(ui, app));
}

fn entry(ui: &mut Ui, app: &mut SannyGitApp, page: SettingsPage, label: &str) {
    let selected = app.settings_window.page == page;
    let response = super::controls::selectable_row(ui, selected, 23.0, |ui| {
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
            ui.spacing_mut().item_spacing.y = 4.0;
            heading(ui, "User Interface");
            ui.horizontal(|ui| {
                ui.label("Language:");
                egui::ComboBox::from_id_salt("preferences.language")
                    .selected_text(app.settings_window.preferences.language.label())
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut app.settings_window.preferences.language,
                            crate::preferences::Language::English,
                            "English",
                        );
                    })
                    .response
                    .help_text("English is currently the only available language");
            });
            // Keep the remaining checkbox and radio sections compact; the language
            // field above uses the application's shared input height.
            ui.spacing_mut().interact_size.y = 20.0;
            ui.add_space(4.0);
            group(ui, palette, "Behavior", |ui| {
                ui.checkbox(
                    &mut app.settings_window.preferences.show_welcome,
                    "Show welcome dialog if no repository is reopened",
                );
                ui.checkbox(
                    &mut app.settings_window.preferences.staged_only_after_staging,
                    "Use Staged only when opening or staging files",
                )
                .help_text("Use only staged changes for commits and message generation when opening a repository with staged files or after staging a file. Switch back to All changes when the index is empty.");
            });
            ui.add_space(4.0);
            group(ui, palette, "Double-clicking a local file", |ui| {
                ui.radio_value(
                    &mut app.settings_window.preferences.file_double_click,
                    crate::preferences::FileDoubleClick::ShowDiff,
                    "Show changes in the diff panel",
                );
                ui.radio_value(
                    &mut app.settings_window.preferences.file_double_click,
                    crate::preferences::FileDoubleClick::ToggleStage,
                    "Stage or unstage changes",
                );
            });
            ui.add_space(4.0);
            group(ui, palette, "Confirmations", |ui| {
                let restore_enabled = app.settings_window.skip_push_confirmation
                    || app.settings_window.preferences.skip_sync_confirmation;
                ui.add_enabled(
                    restore_enabled,
                    egui::Checkbox::new(
                        &mut app.settings_window.restore_all_confirmations,
                        "Restore all confirmation dialogs",
                    ),
                )
                .help_text("Re-enable upstream push and sync confirmations")
                .disabled_help_text("All confirmation dialogs are already enabled");
            });
        }
        SettingsPage::Appearance => {
            heading(ui, "Appearance");
            group(ui, palette, "Interface", |ui| {
                interface_choices(
                    ui,
                    &mut app.settings_window.interface_style,
                    if app.settings_window.preferences.follow_system_theme {
                        ThemeChoice::from_system(ui.ctx(), app.settings_window.theme)
                    } else {
                        app.settings_window.theme
                    },
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label("Color theme:");
                    egui::ComboBox::from_id_salt("preferences.theme")
                        .selected_text(if app.settings_window.preferences.follow_system_theme {
                            "Automatic (system)"
                        } else {
                            app.settings_window.theme.label()
                        })
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    app.settings_window.preferences.follow_system_theme,
                                    "Automatic (system)",
                                )
                                .clicked()
                            {
                                app.settings_window.preferences.follow_system_theme = true;
                            }
                            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                                if ui
                                    .selectable_label(
                                        !app.settings_window.preferences.follow_system_theme
                                            && app.settings_window.theme == theme,
                                        theme.label(),
                                    )
                                    .clicked()
                                {
                                    app.settings_window.preferences.follow_system_theme = false;
                                    app.settings_window.theme = theme;
                                }
                            }
                        });
                });
            });
        }
        SettingsPage::DateTime => {
            use crate::preferences::{DateFormat, TimeFormat};
            heading(ui, "Date & Time");
            group(ui, palette, "Commit dates", |ui| {
                let preferences = &mut app.settings_window.preferences;
                egui::Grid::new("preferences.date_time")
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        form_label(ui, "Date format:");
                        egui::ComboBox::from_id_salt("preferences.date_format")
                            .selected_text(preferences.date_format.label())
                            .width(150.0)
                            .show_ui(ui, |ui| {
                                for format in [
                                    DateFormat::Iso,
                                    DateFormat::DayMonthYear,
                                    DateFormat::MonthDayYear,
                                ] {
                                    ui.selectable_value(
                                        &mut preferences.date_format,
                                        format,
                                        format.label(),
                                    );
                                }
                            });
                        ui.end_row();
                        form_label(ui, "Example:");
                        ui.label(RichText::new(preferences.date_example()).color(palette.muted));
                        ui.end_row();
                        form_label(ui, "Time format:");
                        egui::ComboBox::from_id_salt("preferences.time_format")
                            .selected_text(preferences.time_format.label())
                            .width(150.0)
                            .show_ui(ui, |ui| {
                                for format in [TimeFormat::Hours24, TimeFormat::Hours12] {
                                    ui.selectable_value(
                                        &mut preferences.time_format,
                                        format,
                                        format.label(),
                                    );
                                }
                            });
                        ui.end_row();
                        form_label(ui, "Example:");
                        ui.label(RichText::new(preferences.time_example()).color(palette.muted));
                        ui.end_row();
                    });
                ui.add_space(8.0);
                ui.checkbox(
                    &mut preferences.verbose_dates,
                    "Show Today and Yesterday for recent dates",
                );
                ui.label(
                    RichText::new("Commit dates are shown in your local time zone.")
                        .small()
                        .color(palette.muted),
                );
            });
        }
        SettingsPage::Logging => {
            heading(ui, "Logging");
            group(ui, palette, "Application log", |ui| {
                let label = |ui: &mut Ui, text| {
                    ui.horizontal(|ui| {
                        ui.set_min_width(82.0);
                        ui.label(text);
                    });
                };
                let level = &mut app.settings_window.preferences.log_level;
                ui.horizontal(|ui| {
                    label(ui, "Log level:");
                    egui::ComboBox::from_id_salt("preferences.logging.level")
                        .selected_text(level.label())
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for option in Level::ALL {
                                ui.selectable_value(level, option, option.label());
                            }
                        })
                        .response
                        .help_text("Debug includes Git command timings and exit codes");
                });
                if let Some(path) = logging::path() {
                    let path = path.to_string_lossy();
                    egui::Sides::new().shrink_left().show(
                        ui,
                        |ui| {
                            label(ui, "Log file:");
                            ui.add(egui::Label::new(path.as_ref()).truncate())
                                .help_text(path.as_ref());
                        },
                        |ui| {
                            if ui.button("Copy path").clicked() {
                                ui.ctx().copy_text(path.to_string());
                            }
                        },
                    );
                }
            });
        }
        SettingsPage::Git => {
            heading(ui, "Git executable");
            group(ui, palette, "Command", |ui| {
                ui.label("Path to Git executable:");
                ui.horizontal(|ui| {
                    let value = &mut app.settings_window.preferences.git_executable;
                    ui.add_sized(
                        [ui.available_width() - 34.0, ui.spacing().interact_size.y],
                        crate::ui::controls::singleline(value),
                    );
                    if super::controls::icons::button(
                        ui,
                        super::controls::icons::Icon::Folder,
                        "Choose Git executable",
                    )
                    .clicked()
                    {
                        let mut picker = rfd::FileDialog::new().set_title("Choose Git executable");
                        if let Some(parent) =
                            std::path::Path::new(value).parent().filter(|p| p.is_dir())
                        {
                            picker = picker.set_directory(parent);
                        }
                        if let Some(path) = picker.pick_file() {
                            *value = path.to_string_lossy().into_owned();
                        }
                    }
                });
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
            ai_page(ui, app, palette, true, None);
        }
        SettingsPage::CommitMessages => {
            heading(ui, "Commit messages");
            instructions_form(ui, &mut app.settings_window.llm, palette, true);
        }
        SettingsPage::CommitContext => {
            heading(ui, "Context");
            commit_context_page(
                ui,
                palette,
                &mut app.settings_window.llm.config,
                "preferences.commit_context",
            );
        }
    }
}

#[derive(Debug)]
enum SettingsError {
    Credential(String),
    Logging(String),
}

fn apply_settings(app: &mut SannyGitApp) -> Result<(), SettingsError> {
    if let Some(path) = &app.settings_window.llm_repository {
        app.settings_window
            .llm
            .save_secret(&app.llm_config, None)
            .map_err(SettingsError::Credential)?;
        app.llm_config = app.settings_window.llm.config.clone();
        for config in app.repository_llm_configs.values_mut() {
            SannyGitApp::strip_repository_connection_mode(config, &app.llm_config);
        }
        let global = &app.llm_config;
        if let Some(draft) = app.settings_window.llm_repositories.get(path) {
            let previous = app.repository_llm_configs.get(path).unwrap_or(global);
            let mut config = draft.config.clone();
            SannyGitApp::strip_repository_connection_mode(&mut config, global);
            let store_override = !SannyGitApp::repository_model_matches_global(&config, global)
                || draft.secret_changed(previous);
            if store_override {
                draft
                    .save_secret(previous, Some(path))
                    .map_err(SettingsError::Credential)?;
                app.repository_llm_configs.insert(path.clone(), config);
            } else {
                app.repository_llm_configs.remove(path);
            }
        } else {
            app.repository_llm_configs.remove(path);
        }
        let global_instructions = &app.settings_window.llm.config.instructions;
        match app.settings_window.llm_instructions.get(path) {
            Some(instructions) if instructions != global_instructions => {
                app.repository_llm_instructions
                    .insert(path.clone(), instructions.clone());
            }
            _ => {
                app.repository_llm_instructions.remove(path);
            }
        }
    } else {
        app.settings_window
            .llm
            .save_secret(&app.llm_config, None)
            .map_err(SettingsError::Credential)?;
        let log_level = app.settings_window.preferences.log_level;
        if log_level != app.preferences.log_level {
            logging::configure(log_level).map_err(SettingsError::Logging)?;
            logging::record(
                Level::Info,
                format_args!("Log level set to {}", log_level.label()),
            );
        }
        app.theme = app.settings_window.theme;
        if app.settings_window.restore_all_confirmations {
            app.settings_window.skip_push_confirmation = false;
            app.settings_window.preferences.skip_sync_confirmation = false;
        }
        app.preferences = app.settings_window.preferences.clone();
        app.interface_style = app.settings_window.interface_style;
        app.skip_push_confirmation = app.settings_window.skip_push_confirmation;
        app.tracking.set_auto_fetch(app.settings_window.auto_fetch);
        app.llm_config = app.settings_window.llm.config.clone();
        for config in app.repository_llm_configs.values_mut() {
            SannyGitApp::strip_repository_connection_mode(config, &app.llm_config);
        }
    }
    Ok(())
}

fn report_settings_error(app: &mut SannyGitApp, error: SettingsError) {
    let error = match error {
        SettingsError::Credential(error) => error,
        SettingsError::Logging(error) => {
            app.settings_window.page = SettingsPage::Logging;
            app.error = Some(error);
            return;
        }
    };
    app.settings_window.page = SettingsPage::Llm;
    let draft = match app.settings_window.llm_repository.as_deref() {
        Some(path) => app.settings_window.llm_repositories.get_mut(path),
        None => Some(&mut app.settings_window.llm),
    };
    if let Some(draft) = draft {
        draft.secret_error = Some(error);
    }
}

fn ensure_repository_model_draft(app: &mut SannyGitApp, path: &str) {
    if app.settings_window.llm_repositories.contains_key(path) {
        return;
    }
    let mut config = app.effective_llm_config(path).clone();
    SannyGitApp::strip_repository_connection_mode(&mut config, &app.settings_window.llm.config);
    app.settings_window.llm_repositories.insert(
        path.to_owned(),
        crate::app::LlmSettings {
            config,
            ..Default::default()
        },
    );
}

fn repository_ai_page(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette, path: &str) {
    let name = app
        .repositories
        .iter()
        .find(|repository| repository.path == path)
        .map_or(path, |repository| repository.name.as_str());
    heading(ui, "AI");
    ui.label(
        RichText::new(format!(
            "Model settings used to generate commit messages for {name}. Matching Preferences uses the application defaults."
        ))
        .color(palette.muted),
    );
    ui.add_space(8.0);
    ensure_repository_model_draft(app, path);
    ai_page(ui, app, palette, false, Some(path));
}

fn repository_llm_prompt_tab(
    ui: &mut Ui,
    app: &mut SannyGitApp,
    path: Option<&str>,
    palette: Palette,
) {
    let window = &mut app.settings_window;
    let global = window.llm.config.instructions.clone();
    let Some(path) = path else {
        let mut instructions = global;
        instructions_editor(ui, &mut instructions, false);
        return;
    };
    let mut instructions = window
        .llm_instructions
        .get(path)
        .cloned()
        .or_else(|| {
            window
                .llm_repositories
                .get(path)
                .filter(|draft| draft.config.instructions != global)
                .map(|draft| draft.config.instructions.clone())
        })
        .unwrap_or_else(|| global.clone());
    commit_message_language_instructions_row(
        ui,
        palette,
        "repository.commit_message.language",
        &mut instructions,
    );
    ui.label("Generation instructions:");
    instructions_editor(ui, &mut instructions, true);
    if instructions == global {
        window.llm_instructions.remove(path);
    } else {
        window
            .llm_instructions
            .insert(path.to_owned(), instructions);
    }
    if let Some(draft) = window.llm_repositories.get_mut(path) {
        draft.config.instructions.clone_from(&global);
    }
}

fn ai_page(
    ui: &mut Ui,
    app: &mut SannyGitApp,
    palette: Palette,
    show_heading: bool,
    repository: Option<&str>,
) {
    if show_heading {
        heading(ui, "AI");
        ui.label(
            RichText::new("Connect a service to generate commit messages.").color(palette.muted),
        );
    }
    let section_gap = ui.spacing().item_spacing.y;
    ui.add_space(section_gap);

    let subscription_mode = app.settings_window.llm.config.chatgpt_subscription;
    ui.columns(2, |columns| {
        connection_choice(
            &mut columns[0],
            &mut app.settings_window.llm.config.chatgpt_subscription,
            true,
            Icon::Sparkles,
            ("AI Provider", "Sign in with your AI Provider account"),
            palette,
        );
        connection_choice(
            &mut columns[1],
            &mut app.settings_window.llm.config.chatgpt_subscription,
            false,
            Icon::Code,
            ("OpenAI API", "OpenAI-compatible API or local server"),
            palette,
        );
    });
    if app.settings_window.llm.config.chatgpt_subscription != subscription_mode {
        app.sync_global_ai_connection_from_draft();
    }
    let subscription = app.settings_window.llm.config.chatgpt_subscription;
    ui.separator();
    ui.add_space(section_gap);
    let idle = !app.busy();
    if subscription {
        use crate::llm::SubscriptionProvider;
        let provider = app.settings_window.llm.config.subscription_provider;
        let status = match provider {
            SubscriptionProvider::ChatGpt => {
                app.update_chatgpt_models(ui.ctx());
                app.settings_window.chatgpt_status
            }
            SubscriptionProvider::Copilot => {
                app.update_ai_connection_status(ui.ctx(), provider);
                app.settings_window.copilot_status
            }
        };
        let connected = status == AiConnectionStatus::Connected;
        let can_change_session =
            idle && !matches!(
                status,
                AiConnectionStatus::Unknown
                    | AiConnectionStatus::Checking
                    | AiConnectionStatus::ChangingSession
            ) && (provider == SubscriptionProvider::ChatGpt
                || connected
                || crate::llm::copilot::can_sign_in());
        let chatgpt_models = std::mem::take(&mut app.settings_window.chatgpt_models);
        let connection_clicked = subscription_provider(
            ui,
            app,
            palette,
            can_change_session,
            connected,
            &chatgpt_models,
        );
        app.settings_window.chatgpt_models = chatgpt_models;
        if connection_clicked {
            match provider {
                SubscriptionProvider::ChatGpt => {
                    app.change_chatgpt_connection(ui.ctx(), !connected)
                }
                SubscriptionProvider::Copilot => {
                    app.change_copilot_connection(ui.ctx(), !connected)
                }
            }
        }
        ui.add_space(section_gap);
        let instructions = match app.settings_window.llm.config.subscription_provider {
            SubscriptionProvider::ChatGpt => "Sign in to ChatGPT in your browser. SannyGit uses your account and sends the selected diff when you generate a message.",
            SubscriptionProvider::Copilot if crate::llm::copilot::can_sign_in() =>
                "Sign in to GitHub in your browser. SannyGit sends the selected diff to Copilot when you generate a message.",
            SubscriptionProvider::Copilot => "GitHub Copilot sign-in is unavailable in this build.",
        };
        ai_sign_in_banner(ui, palette, instructions);
        if let Some(path) = repository {
            use crate::llm::SubscriptionProvider;
            if provider == SubscriptionProvider::ChatGpt {
                ensure_repository_model_draft(app, path);
                let window = &mut app.settings_window;
                let draft = window.llm_repositories.get_mut(path).expect("draft");
                ui.add_space(section_gap);
                chatgpt_model_row(ui, &mut draft.config, &window.chatgpt_models, palette);
            }
        }
        return;
    }
    match repository {
        None => llm_api_form(
            ui,
            &mut app.settings_window.llm,
            palette,
            None,
            &mut app.llm_models,
            false,
        ),
        Some(path) => {
            ensure_repository_model_draft(app, path);
            let draft = app
                .settings_window
                .llm_repositories
                .get_mut(path)
                .expect("draft");
            llm_api_form(ui, draft, palette, Some(path), &mut app.llm_models, false);
        }
    }
}

fn subscription_provider(
    ui: &mut Ui,
    app: &mut SannyGitApp,
    palette: Palette,
    enabled: bool,
    connected: bool,
    models: &[crate::llm::chatgpt::Model],
) -> bool {
    let mut clicked = false;
    let config = &mut app.settings_window.llm.config;
    let previous_provider = config.subscription_provider;
    Frame::new().inner_margin(Margin::ZERO).show(ui, |ui| {
        use crate::llm::SubscriptionProvider::{ChatGpt, Copilot};
        let label = |provider| match provider {
            ChatGpt => (
                icons::image(Icon::OpenAi, palette.text, 20.0),
                "OpenAI (ChatGPT)",
            ),
            Copilot => (
                icons::image(Icon::GitHub, palette.text, 20.0),
                "GitHub Copilot",
            ),
        };
        egui::Grid::new("preferences.ai.provider")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                form_label(ui, "Provider:");
                ai_provider_field_row(
                    ui,
                    palette,
                    Some((enabled, connected)),
                    &mut clicked,
                    |ui| {
                        egui::ComboBox::from_id_salt("preferences.ai.provider")
                            .width(ui.available_width())
                            .selected_text(label(config.subscription_provider))
                            .show_ui(ui, |ui| {
                                for provider in [ChatGpt, Copilot] {
                                    if ui
                                        .selectable_value(
                                            &mut config.subscription_provider,
                                            provider,
                                            label(provider),
                                        )
                                        .clicked()
                                    {
                                        ui.close();
                                    }
                                }
                            });
                    },
                );
                ui.end_row();

                if config.subscription_provider != Copilot {
                    form_label(ui, "Model:");
                    ai_provider_field_row(ui, palette, None, &mut clicked, |ui| {
                        let settings = config
                            .subscription_models
                            .entry(crate::llm::chatgpt::PROVIDER_ID.into())
                            .or_default();
                        ui.add_enabled_ui(!models.is_empty(), |ui| {
                            generation_settings_field(ui, palette, settings, models);
                        });
                    });
                    ui.end_row();
                }
            });
    });
    if config.subscription_provider != previous_provider {
        app.sync_global_ai_connection_from_draft();
    }
    clicked
}

fn ai_provider_field_row(
    ui: &mut Ui,
    palette: Palette,
    action: Option<(bool, bool)>,
    clicked: &mut bool,
    field: impl FnOnce(&mut Ui),
) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if let Some((enabled, connected)) = action {
            let button = if connected {
                egui::Button::new("Sign out")
            } else {
                egui::Button::image_and_text(
                    icons::image(Icon::ExternalLink, palette.text, 18.0),
                    "Sign in",
                )
                .gap(6.0)
            };
            *clicked |= ui
                .add_enabled(
                    enabled,
                    button.corner_radius(6).min_size(Vec2::new(
                        AI_ACTION_BUTTON_MIN_WIDTH,
                        crate::theme::CONTROL_HEIGHT,
                    )),
                )
                .clicked();
        } else {
            ui.allocate_exact_size(
                Vec2::new(AI_ACTION_BUTTON_MIN_WIDTH, crate::theme::CONTROL_HEIGHT),
                Sense::hover(),
            );
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(ui.available_width());
            field(ui);
        });
    });
}

fn ai_sign_in_banner(ui: &mut Ui, palette: Palette, instructions: &str) {
    Frame::new()
        .fill(palette.selection.gamma_multiply(0.4))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(6, 6))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_top(|ui| {
                icons::paint(ui, Icon::Info, palette.accent, 17.0);
                ui.label(RichText::new(instructions).color(palette.muted));
            });
        });
}

fn connection_choice(
    ui: &mut Ui,
    subscription: &mut bool,
    value: bool,
    icon: Icon,
    (title, description): (&str, &str),
    palette: Palette,
) {
    let selected = *subscription == value;
    let icon_color = if selected {
        palette.accent
    } else {
        palette.text
    };
    let mut label = egui::text::LayoutJob::default();
    label.append(
        title,
        0.0,
        egui::TextFormat::simple(egui::FontId::new(14.0, semibold_family()), palette.text),
    );
    label.append(
        &format!("\n{description}"),
        0.0,
        egui::TextFormat::simple(egui::TextStyle::Body.resolve(ui.style()), palette.muted),
    );
    let compact = ui.spacing().item_spacing.y < 6.0;
    let heading_offset = egui::vec2(0.0, if compact { 6.0 } else { 10.0 });
    let indicator = egui::Atom::paint(Vec2::new(36.0, 18.0), move |ui, geometry| {
        let center = geometry.rect.center() - heading_offset;
        let outline = if selected {
            palette.accent
        } else {
            palette.border
        };
        ui.painter().circle_stroke(center, 6.5, (1.5, outline));
        if selected {
            ui.painter().circle_filled(center, 4.0, palette.accent);
        }
    });
    let image = icons::image(
        icon,
        icon_color,
        if icon == Icon::Sparkles { 26.0 } else { 22.0 },
    );
    let symbol = egui::Atom::paint(Vec2::splat(24.0), move |ui, geometry| {
        image.paint_at(ui, geometry.rect.translate(-heading_offset));
    });
    let background = ui.painter().add(egui::Shape::Noop);
    let height = if compact { 48.0 } else { 58.0 };
    let response = ui.add_sized(
        [ui.available_width(), height],
        egui::Button::new((indicator, symbol, label, egui::Atom::grow()))
            .gap(8.0)
            .selected(selected)
            .frame(false),
    );
    let (fill, border) = if selected {
        (
            palette.panel_alt.lerp_to_gamma(palette.selection, 0.7),
            palette.accent,
        )
    } else if response.has_focus() || response.hovered() {
        (palette.hover, palette.accent)
    } else {
        (palette.panel_alt, palette.border)
    };
    ui.painter()
        .set(background, egui::Shape::rect_filled(response.rect, 7, fill));
    ui.painter()
        .rect_stroke(response.rect, 7, (1.0, border), egui::StrokeKind::Inside);
    if response.clicked() {
        *subscription = value;
    }
}

fn chatgpt_model_row(
    ui: &mut Ui,
    config: &mut crate::llm::Config,
    models: &[crate::llm::chatgpt::Model],
    palette: Palette,
) {
    use crate::llm::chatgpt::PROVIDER_ID;

    let settings = config
        .subscription_models
        .entry(PROVIDER_ID.into())
        .or_default();
    ui.horizontal(|ui| {
        form_label(ui, "Model:");
        ui.add_enabled_ui(!models.is_empty(), |ui| {
            generation_settings_field(ui, palette, settings, models);
        });
    });
}

fn generation_menu_max_height(ui: &Ui, field_bottom: f32) -> f32 {
    const FOOTER_HEIGHT: f32 = 46.0;
    const MARGIN: f32 = 8.0;
    let limit = ui.ctx().content_rect().bottom() - field_bottom - FOOTER_HEIGHT - MARGIN;
    limit.clamp(100.0, ui.spacing().combo_height)
}

fn generation_menu_width(ui: &Ui, field_width: f32) -> f32 {
    field_width.min(ui.ctx().content_rect().width() - 12.0)
}

fn generation_settings_field(
    ui: &mut Ui,
    palette: Palette,
    settings: &mut crate::llm::SubscriptionModelSettings,
    models: &[crate::llm::chatgpt::Model],
) {
    let id = ui.make_persistent_id("chatgpt.generation");
    let width = ui.available_width();
    let height = ui
        .spacing()
        .interact_size
        .y
        .max(crate::theme::CONTROL_HEIGHT);
    let popup_open = Popup::is_id_open(ui.ctx(), id.with("popup"));
    use crate::llm::chatgpt::selected_model;
    let automatic = settings.model.is_empty();
    let selected = selected_model(models, &settings.model);
    let model_name = if automatic {
        selected.map_or_else(
            || "Automatic".to_owned(),
            |model| format!("Automatic ({})", model.display_name),
        )
    } else {
        selected.map_or_else(
            || settings.model.clone(),
            |model| model.display_name.clone(),
        )
    };
    let effort = effort_summary(settings, selected, models);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    ui.painter().rect(
        rect,
        4,
        palette.editor,
        egui::Stroke::NONE,
        StrokeKind::Inside,
    );

    let icon_width = ui.spacing().icon_width;
    let right_padding = ui.spacing().button_padding.x;
    let button_rect = Rect::from_min_max(
        egui::pos2(rect.right() - icon_width - 2.0 * right_padding, rect.top()),
        rect.max,
    );
    let label_rect = Rect::from_min_max(
        rect.min + Vec2::new(8.0, 0.0),
        egui::pos2(button_rect.left() - 4.0, rect.bottom()),
    );
    {
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(label_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        child.style_mut().interaction.selectable_labels = false;
        child.horizontal(|ui| {
            ui.set_width(label_rect.width().max(0.0));
            ui.label(
                RichText::new(model_name)
                    .family(semibold_family())
                    .color(palette.text),
            );
            if let Some(effort) = effort {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(effort).size(12.0).color(palette.muted));
                });
            }
        });
    }
    let button = ui.interact(button_rect, id.with("arrow"), Sense::click());
    let visuals = ui.style().interact(&button);
    let triangle_rect = Rect::from_center_size(
        egui::pos2(
            rect.right() - right_padding - icon_width / 2.0,
            rect.center().y,
        ),
        Vec2::new(icon_width * 0.7, icon_width * 0.45),
    )
    .expand(visuals.expansion);
    ui.painter().add(egui::Shape::rotated_triangle(
        triangle_rect,
        0.0,
        visuals.fg_stroke.color,
    ));

    let response = response.union(button);
    let border = if popup_open || response.hovered() {
        palette.accent
    } else {
        palette.border
    };
    ui.painter()
        .rect_stroke(rect, 4, (1.0, border), StrokeKind::Inside);

    let mut anchor = response.rect;
    if let Some(to_global) = ui.ctx().layer_transform_to_global(response.layer_id) {
        anchor = to_global * anchor;
    }
    let menu_height = generation_menu_max_height(ui, anchor.bottom());
    let menu_width = generation_menu_width(ui, width);

    Popup::menu(&response)
        .id(id.with("popup"))
        .width(menu_width)
        .anchor(anchor.left_bottom())
        .align(RectAlign::BOTTOM_START)
        .align_alternatives(&[])
        .gap(2.0)
        .show(|ui| {
            ui.set_width(menu_width);
            let scroll = ui.style().spacing.scroll;
            let scrollbar_pad = scroll.bar_width + scroll.bar_inner_margin;
            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(menu_height)
                .show(ui, |ui| {
                    Frame::new()
                        .inner_margin(Margin {
                            right: scrollbar_pad as i8,
                            ..Margin::ZERO
                        })
                        .show(ui, |ui| {
                            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                            generation_settings_menu(ui, palette, settings, models);
                            ui.add_space(4.0);
                        });
                });
        });
    ui.advance_cursor_after_rect(rect);
}

fn effort_summary(
    settings: &crate::llm::SubscriptionModelSettings,
    model: Option<&crate::llm::chatgpt::Model>,
    models: &[crate::llm::chatgpt::Model],
) -> Option<String> {
    let model = model.or_else(|| crate::llm::chatgpt::selected_model(models, &settings.model))?;
    if model.reasoning_efforts.is_empty() {
        return None;
    }
    let effort = if settings.model.is_empty() {
        crate::llm::chatgpt::cheapest_effort(model)?
    } else {
        settings.reasoning_effort.as_str()
    };
    Some(format!("effort: {}", reasoning_label(effort)))
}

fn generation_settings_menu(
    ui: &mut Ui,
    palette: Palette,
    settings: &mut crate::llm::SubscriptionModelSettings,
    models: &[crate::llm::chatgpt::Model],
) {
    let row_height = crate::theme::CONTROL_HEIGHT;
    let row_width = ui.available_width();

    ui.spacing_mut().item_spacing.y = 2.0;
    if ui
        .selectable_label(settings.model.is_empty(), "Automatic")
        .clicked()
    {
        settings.model.clear();
        settings.reasoning_effort.clear();
        ui.close();
    }

    for model in models {
        let selected = settings.model == model.slug;
        ui.allocate_ui_with_layout(
            Vec2::new(row_width, row_height),
            Layout::left_to_right(Align::Center),
            |ui| {
                if ui.selectable_label(selected, &model.display_name).clicked() {
                    settings.model.clone_from(&model.slug);
                    sync_effort_with_model(settings, model);
                }
                if !model.reasoning_efforts.is_empty() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let effort_width = ui.available_width().max(0.0);
                        ui.set_width(effort_width);
                        ui.set_min_width(effort_width);
                        generation_menu_effort_pills(ui, palette, settings, model, selected);
                    });
                }
            },
        );
    }
}

fn generation_menu_effort_pills(
    ui: &mut Ui,
    palette: Palette,
    settings: &mut crate::llm::SubscriptionModelSettings,
    model: &crate::llm::chatgpt::Model,
    selected: bool,
) {
    if model.reasoning_efforts.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for effort in &model.reasoning_efforts {
            let active = selected && settings.reasoning_effort == *effort;
            if effort_pill(
                ui,
                effort,
                active,
                palette,
                &format!("{}.{}", model.slug, effort),
            )
            .clicked()
            {
                settings.model.clone_from(&model.slug);
                settings.reasoning_effort.clone_from(effort);
                ui.close();
            }
        }
    });
}

fn sync_effort_with_model(
    settings: &mut crate::llm::SubscriptionModelSettings,
    model: &crate::llm::chatgpt::Model,
) {
    if !model
        .reasoning_efforts
        .iter()
        .any(|effort| effort == &settings.reasoning_effort)
    {
        settings.reasoning_effort = model.reasoning_efforts.first().cloned().unwrap_or_default();
    }
}

fn effort_pill(
    ui: &mut Ui,
    effort: &str,
    active: bool,
    palette: Palette,
    id_salt: &str,
) -> egui::Response {
    let label = reasoning_label(effort);
    ui.push_id(id_salt, |ui| {
        let fill = if active {
            palette.selection.gamma_multiply(0.55)
        } else {
            palette.panel_alt
        };
        let stroke = if active {
            (1.0, palette.accent)
        } else {
            (1.0, palette.border)
        };
        ui.add(
            egui::Button::new(RichText::new(label).size(12.0).color(palette.text))
                .fill(fill)
                .stroke(stroke)
                .corner_radius(6)
                .min_size(Vec2::new(0.0, crate::theme::CONTROL_HEIGHT - 4.0)),
        )
    })
    .inner
}

fn reasoning_label(effort: &str) -> &str {
    match effort {
        "" => "Default",
        "none" => "None",
        "minimal" => "Minimal",
        "low" => "Low",
        "medium" => "Medium",
        "high" => "High",
        "xhigh" => "Extra high",
        "max" => "Maximum",
        _ => effort,
    }
}

fn llm_api_form(
    ui: &mut Ui,
    draft: &mut crate::app::LlmSettings,
    palette: Palette,
    repository: Option<&str>,
    catalog: &mut crate::app::ModelCatalog,
    show_heading: bool,
) {
    catalog.update();
    ui.push_id(("llm.form", repository), |ui| {
        let title = if show_heading { "OpenAI-compatible API" } else { "" };
        group(ui, palette, title, |ui| {
            let key_left = egui::Grid::new("preferences.llm.connection")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    form_label(ui, "Endpoint:");
                    let mut endpoint = draft.config.endpoint.clone();
                    let endpoints: Vec<_> = crate::llm::PROVIDER_PRESETS
                        .iter()
                        .map(|provider| provider.endpoint.to_owned())
                        .collect();
                    let endpoint_response = super::controls::editable_combobox::show(
                        ui,
                        ui.make_persistent_id("preferences.llm.endpoint"),
                        &mut endpoint,
                        &endpoints,
                        ui.available_width(),
                        palette,
                        "No provider presets",
                    );
                    let preset_selected =
                        endpoint_response.changed() && !endpoint_response.has_focus();
                    let provider = crate::llm::PROVIDER_PRESETS.iter()
                        .find(|provider| provider.endpoint == endpoint)
                        .map_or("Custom endpoint", |provider| provider.name);
                    endpoint_response.clone().help_text(format!(
                        "{provider}. HTTPS is required outside loopback. HTTP is allowed for localhost and loopback IPs. Redirects are blocked. Use the API key field; URL credentials, queries and fragments are rejected."
                    ));
                    if endpoint != draft.config.endpoint {
                        draft.config.endpoint = endpoint;
                        draft.config.api_key.clear();
                        draft.api_key_dirty = false;
                        draft.secret_error = None;
                        if let Some(provider) = crate::llm::PROVIDER_PRESETS
                            .iter()
                            .find(|provider| provider.endpoint == draft.config.endpoint)
                        {
                            draft.config.model = provider.model.to_owned();
                        }
                        if draft.config.is_local() {
                            draft.config.store_api_key = false;
                        }
                    }
                    // The model dropdown can unlock saved credentials on demand.
                    if (preset_selected || endpoint_response.lost_focus())
                        && ui.is_enabled()
                        && (!draft.config.api_key.trim().is_empty()
                            || (draft.config.is_local() && !draft.config.store_api_key))
                    {
                        catalog.load(&draft.config, repository, ui.ctx());
                    }
                    ui.end_row();
                    form_label(ui, "Model:");
                    let id = ui.make_persistent_id("preferences.llm.model");
                    let popup = id.with("popup");
                    let was_open = egui::Popup::is_id_open(ui.ctx(), popup);
                    let models = catalog.entry(&draft.config, repository);
                    let should_load = !models.attempted || (models.error.is_some() && !was_open);
                    let empty_text = if models.loading() {
                        "Loading models…"
                    } else {
                        "Enter a model name"
                    };
                    super::controls::editable_combobox::show(
                        ui,
                        id,
                        &mut draft.config.model,
                        &models.options,
                        ui.available_width(),
                        palette,
                        empty_text,
                    )
                    .help_text("Select a model from the server or enter a custom model name");
                    if ui.is_enabled() && should_load && egui::Popup::is_id_open(ui.ctx(), popup) {
                        if draft.api_key_dirty {
                            catalog.load(&draft.config, repository, ui.ctx());
                        } else {
                            catalog.load_from_dropdown(&draft.config, repository, ui.ctx());
                        }
                        ui.ctx().request_repaint();
                    }
                    ui.end_row();
                    form_label(ui, "API key:");
                    let api_key_response = ui.add(
                        crate::ui::controls::singleline(&mut draft.config.api_key)
                            .hint_text(if draft.config.store_api_key {
                                "Leave unchanged to keep saved key"
                            } else {
                                "Optional for local servers"
                            })
                            .password(true)
                            .desired_width(f32::INFINITY),
                    );
                    if api_key_response.changed() {
                        draft.api_key_dirty = true;
                    }
                    if api_key_response.lost_focus()
                        && draft.api_key_dirty
                        && ui.is_enabled()
                        && (!draft.config.api_key.trim().is_empty()
                            || (draft.config.is_local() && !draft.config.store_api_key))
                    {
                        catalog.load(&draft.config, repository, ui.ctx());
                    }
                    ui.end_row();
                    api_key_response.rect.left()
                }).inner;
            if show_heading {
                store_api_key(ui, &mut draft.config.store_api_key);
            } else {
                ui.horizontal(|ui| {
                    ui.add_space((key_left - ui.cursor().min.x).max(0.0));
                    store_api_key(ui, &mut draft.config.store_api_key);
                });
            }
            let models = catalog.entry(&draft.config, repository);
            if models.loading() {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50));
            }
        });
    });
}

fn store_api_key(ui: &mut Ui, enabled: &mut bool) {
    ui.checkbox(enabled, "Store API key and model securely")
        .help_text(if cfg!(target_os = "macos") {
            "Builds with Keychain entitlements use Touch ID. Other builds use the login Keychain and may ask for your macOS password."
        } else {
            "Save the API key and model in the OS credential manager"
        });
}

fn instructions_form(
    ui: &mut Ui,
    draft: &mut crate::app::LlmSettings,
    palette: Palette,
    fill_to_bottom: bool,
) {
    commit_message_language_row(
        ui,
        palette,
        "preferences.commit_message.language",
        &mut draft.config,
    );
    ui.label("Generation instructions:");
    instructions_editor(ui, &mut draft.config.instructions, fill_to_bottom);
}

fn commit_message_language_row(
    ui: &mut Ui,
    palette: Palette,
    id: &str,
    config: &mut crate::llm::Config,
) {
    commit_message_language_selector(
        ui,
        palette,
        id,
        config.selected_commit_message_language(),
        |language| config.apply_commit_message_language(language),
    );
}

fn commit_context_page(
    ui: &mut Ui,
    palette: Palette,
    config: &mut crate::llm::Config,
    id_prefix: &str,
) {
    ui.label(
        RichText::new("What is sent when you generate a commit message.").color(palette.muted),
    );
    ui.add_space(8.0);
    egui::Grid::new(format!("{id_prefix}.fields"))
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            form_label(ui, "Changes:");
            egui::ComboBox::from_id_salt(format!("{id_prefix}.generation_scope"))
                .selected_text(config.generation_scope.label())
                .width(200.0)
                .show_ui(ui, |ui| {
                    for scope in [
                        crate::llm::CommitGenerationScope::FollowCommitPanel,
                        crate::llm::CommitGenerationScope::AllChanges,
                        crate::llm::CommitGenerationScope::StagedOnly,
                    ] {
                        ui.selectable_value(&mut config.generation_scope, scope, scope.label());
                    }
                })
                .response
                .help_text("Used only when generating a commit message.");
            ui.end_row();
            form_label(ui, "Patch size:");
            let mut limit = config.max_patch_bytes;
            egui::ComboBox::from_id_salt(format!("{id_prefix}.patch_limit"))
                .selected_text(crate::llm::patch_byte_limit_label(limit))
                .width(200.0)
                .show_ui(ui, |ui| {
                    for &(value, label) in crate::llm::PATCH_BYTE_LIMITS {
                        ui.selectable_value(&mut limit, value, label);
                    }
                })
                .response
                .help_text(
                    "Generation stops if the diff is larger. Each new file sends at most 64 KiB.",
                );
            config.max_patch_bytes = limit;
            ui.end_row();
        });
    ui.add_space(4.0);
    ui.checkbox(
        &mut config.include_branch_in_prompt,
        "Include the current branch name",
    );
    ui.add_space(6.0);
    ui.label("Excluded paths:");
    ui.label(
        RichText::new("One pattern per line. Matching files are left out.")
            .small()
            .color(palette.muted),
    );
    egui::TextEdit::multiline(&mut config.patch_path_excludes)
        .id_salt(format!("{id_prefix}.path_excludes"))
        .desired_width(f32::INFINITY)
        .desired_rows(4)
        .hint_text("*.lock\npackage-lock.json\ndist/")
        .show(ui);
}

fn commit_message_language_selector(
    ui: &mut Ui,
    _palette: Palette,
    id: &str,
    mut selected: crate::llm::CommitMessageLanguage,
    mut apply: impl FnMut(crate::llm::CommitMessageLanguage),
) {
    ui.horizontal(|ui| {
        ui.label("Language:");
        let previous = selected;
        egui::ComboBox::from_id_salt(id)
            .selected_text(selected.label())
            .width(200.0)
            .show_ui(ui, |ui| {
                for language in crate::llm::CommitMessageLanguage::ALL {
                    ui.selectable_value(&mut selected, language, language.label());
                }
            });
        if selected != previous {
            apply(selected);
        }
    });
    ui.add_space(6.0);
}

fn commit_message_language_instructions_row(
    ui: &mut Ui,
    palette: Palette,
    id: &str,
    instructions: &mut String,
) {
    let selected =
        crate::llm::CommitMessageLanguage::from_instructions(instructions).unwrap_or_default();
    commit_message_language_selector(ui, palette, id, selected, |language| {
        *instructions = language.instructions();
    });
}

fn instructions_editor(ui: &mut Ui, instructions: &mut String, fill_to_bottom: bool) {
    let editor_height = if fill_to_bottom {
        (ui.max_rect().max.y - ui.cursor().top()).max(24.0)
    } else {
        (ui.available_height() - ui.spacing().item_spacing.y).max(24.0)
    };
    let instructions = ui.add_sized(
        [ui.available_width(), editor_height],
        egui::TextEdit::multiline(instructions)
            .desired_rows(1)
            .margin(egui::Margin::symmetric(6, 4)),
    );
    super::controls::dialog_style::preserve_multiline_enter(&instructions);
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
    ui.label(
        RichText::new(title)
            .size(crate::theme::SETTINGS_TITLE_FONT_SIZE)
            .family(semibold_family()),
    );
    ui.add_space(8.0);
}

fn group(ui: &mut Ui, palette: Palette, title: &str, contents: impl FnOnce(&mut Ui)) {
    if title.is_empty() {
        contents(ui);
        return;
    }
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

fn interface_choices(ui: &mut Ui, selected: &mut InterfaceStyle, theme: ThemeChoice) {
    let width = ui.available_width().min(380.0);
    ui.allocate_ui_with_layout(Vec2::new(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        ui.columns(2, |columns| {
            for (column, style) in columns
                .iter_mut()
                .zip([InterfaceStyle::Classic, InterfaceStyle::Modern])
            {
                column.with_layout(Layout::top_down(Align::Center), |column| {
                    let (uri, bytes): (&str, &[u8]) = match (style, theme) {
                        (InterfaceStyle::Classic, ThemeChoice::Light) => (
                            "bytes://previews/classic-light.png",
                            include_bytes!("../../assets/previews/classic-light.png"),
                        ),
                        (InterfaceStyle::Classic, ThemeChoice::Dark) => (
                            "bytes://previews/classic-dark.png",
                            include_bytes!("../../assets/previews/classic-dark.png"),
                        ),
                        (InterfaceStyle::Modern, ThemeChoice::Light) => (
                            "bytes://previews/modern-light.png",
                            include_bytes!("../../assets/previews/modern-light.png"),
                        ),
                        (InterfaceStyle::Modern, ThemeChoice::Dark) => (
                            "bytes://previews/modern-dark.png",
                            include_bytes!("../../assets/previews/modern-dark.png"),
                        ),
                    };
                    let palette = style.palette(theme);
                    let (rect, _) =
                        column.allocate_exact_size(Vec2::splat(112.0), egui::Sense::hover());
                    egui::Image::from_bytes(uri, bytes)
                        .fit_to_exact_size(Vec2::splat(112.0))
                        .corner_radius(4)
                        .paint_at(column, rect);
                    let response = column.interact(
                        rect,
                        column.scope_id().with(("interface.preview", style.label())),
                        egui::Sense::click(),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Button,
                            true,
                            *selected == style,
                            format!("{} interface preview", style.label()),
                        )
                    });
                    column.painter().rect_stroke(
                        response.rect,
                        4,
                        (
                            if *selected == style { 2.0 } else { 1.0 },
                            if *selected == style || response.hovered() {
                                palette.accent
                            } else {
                                palette.border
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    if response.clicked() {
                        *selected = style;
                    }
                    column.radio_value(selected, style, style.label());
                });
            }
        });
    });
}

#[cfg(test)]
mod tests;
