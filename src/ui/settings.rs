use super::controls::dialog_style::DialogResponse;
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::{RepositoryLlmTab, SannyGitApp, SettingsPage},
    theme::{semibold_family, InterfaceStyle, Palette, ThemeChoice},
};
use eframe::egui::{
    self, Align, CentralPanel, Frame, Key, Layout, Margin, Panel, RichText, ScrollArea, Ui, Vec2,
    ViewportBuilder, ViewportCommand, ViewportId,
};

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    if !app.settings_window.open {
        #[cfg(target_os = "windows")]
        crate::native_window::end_modal(ui.ctx());
        return;
    }
    if app.settings_window.llm_repository.is_some() {
        repository_llm_dialog(ui, app);
        return;
    }
    preferences_window(ui.ctx(), app);
}

fn preferences_window(ctx: &egui::Context, app: &mut SannyGitApp) {
    let (title, size, min_size) = ("SannyGit Preferences", [740.0, 420.0], [680.0, 380.0]);
    if !app.settings_window.initial_position_set {
        app.settings_window.initial_position = ctx.input(|input| {
            let viewport = input.viewport();
            viewport
                .outer_rect
                .map(|outer| centered_position(outer, viewport.inner_rect, Vec2::from(size)))
        });
        app.settings_window.initial_position_set = true;
    }
    let mut builder = ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size(min_size)
        .with_decorations(true)
        .with_taskbar(false)
        .with_visible(false)
        .with_minimize_button(false)
        .with_maximize_button(false);
    if let Some(position) = app.settings_window.initial_position {
        builder = builder.with_position(position);
    }
    let viewport_id = ViewportId::from_hash_of("sannygit.preferences");
    ctx.show_viewport_immediate(viewport_id, builder, |ui, class| {
        #[cfg(target_os = "macos")]
        ui.input_mut(|input| input.events.append(&mut app.pending_native_edit_events));
        let escape = app.error_dialogs.pending.is_empty()
            && !egui::Popup::is_any_open(ui.ctx())
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape));
        if escape || ui.input(|input| input.viewport().close_requested()) {
            close(ui.ctx(), app);
            return;
        }

        #[cfg(target_os = "windows")]
        if app.settings_window.needs_reveal && class == egui::ViewportClass::Immediate {
            if let Err(error) = crate::native_window::begin_modal(ui.ctx(), title) {
                app.error = Some(format!(
                    "Cannot attach settings to the main window: {error}"
                ));
                close(ui.ctx(), app);
                return;
            }
        }
        window_contents(ui, app);
        if app.settings_window.open
            && app.settings_window.needs_reveal
            && class == egui::ViewportClass::Immediate
        {
            // Immediate viewports process this command after painting and swapping the frame.
            ui.ctx().send_viewport_cmd(ViewportCommand::Visible(true));
            app.settings_window.needs_reveal = false;
        }
    });
}

fn close(ctx: &egui::Context, app: &mut SannyGitApp) {
    #[cfg(target_os = "windows")]
    crate::native_window::end_modal(ctx);
    app.settings_window.open = false;
    ctx.send_viewport_cmd(ViewportCommand::Close);
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
    footer(ui, app, palette);
    navigation(ui, app, palette);
    CentralPanel::default()
        .frame(Frame::new().fill(palette.header).inner_margin(12))
        .show(ui, |ui| {
            let page_height = ui.available_height();
            ScrollArea::vertical()
                .id_salt("preferences.page")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if app.settings_window.page == SettingsPage::CommitMessages {
                        ui.set_max_height(page_height);
                    }
                    page(ui, app, palette);
                });
        });
    super::show_errors(ui, app);
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
                    .add(super::controls::dialog_style::primary("OK", palette))
                    .dialog_accept()
                {
                    match apply_settings(app) {
                        Ok(()) => {
                            close(ui.ctx(), app);
                        }
                        Err(error) => report_settings_error(app, error),
                    }
                }
                if ui
                    .add(super::controls::dialog_style::button("Cancel"))
                    .dialog_cancel()
                {
                    close(ui.ctx(), app);
                }
            });
        });
}

fn navigation(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette) {
    Panel::left("preferences.navigation")
        .resizable(false)
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
                        section(
                            ui,
                            app,
                            SettingsPage::General,
                            "User Interface",
                            |ui, app| {
                                entry(ui, app, SettingsPage::Appearance, "Appearance");
                                entry(ui, app, SettingsPage::DateTime, "Date & Time");
                            },
                        );
                        section(ui, app, SettingsPage::Git, "Git", |ui, app| {
                            entry(ui, app, SettingsPage::Remote, "Remote");
                        });
                        section(ui, app, SettingsPage::Llm, "AI", |ui, app| {
                            entry(ui, app, SettingsPage::CommitMessages, "Commit messages")
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
                ui.horizontal(|ui| {
                    ui.label("Skip upstream confirmations:");
                    ui.checkbox(&mut app.settings_window.skip_push_confirmation, "Push");
                    ui.checkbox(
                        &mut app.settings_window.preferences.skip_sync_confirmation,
                        "Sync",
                    )
                    .help_text("Skipped operations report results in the status bar.");
                });
                if ui
                    .add_enabled(
                        app.settings_window.skip_push_confirmation
                            || app.settings_window.preferences.skip_sync_confirmation,
                        egui::Button::new("Restore confirmation dialogs"),
                    )
                    .help_text("Re-enable upstream push and sync confirmations")
                    .disabled_help_text("All confirmation dialogs are already enabled")
                    .clicked()
                {
                    app.settings_window.skip_push_confirmation = false;
                    app.settings_window.preferences.skip_sync_confirmation = false;
                }
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
            heading(ui, "AI");
            llm_form(
                ui,
                &mut app.settings_window.llm,
                palette,
                None,
                &mut app.llm_models,
            );
        }
        SettingsPage::CommitMessages => {
            heading(ui, "Commit messages");
            instructions_form(ui, &mut app.settings_window.llm);
        }
    }
}

fn apply_settings(app: &mut SannyGitApp) -> Result<(), String> {
    if let Some(path) = &app.settings_window.llm_repository {
        if let Some(draft) = app.settings_window.llm_repositories.get(path) {
            let previous = app
                .repository_llm_configs
                .get(path)
                .unwrap_or(&app.llm_config);
            draft.save_secret(previous, Some(path))?;
            app.repository_llm_configs
                .insert(path.clone(), draft.config.clone());
        } else {
            app.repository_llm_configs.remove(path);
        }
        if let Some(instructions) = app.settings_window.llm_instructions.get(path) {
            app.repository_llm_instructions
                .insert(path.clone(), instructions.clone());
        } else {
            app.repository_llm_instructions.remove(path);
        }
    } else {
        app.settings_window.llm.save_secret(&app.llm_config, None)?;
        app.theme = app.settings_window.theme;
        app.preferences = app.settings_window.preferences.clone();
        app.interface_style = app.settings_window.interface_style;
        app.skip_push_confirmation = app.settings_window.skip_push_confirmation;
        app.tracking.set_auto_fetch(app.settings_window.auto_fetch);
        app.llm_config = app.settings_window.llm.config.clone();
    }
    Ok(())
}

fn report_settings_error(app: &mut SannyGitApp, error: String) {
    app.settings_window.page = SettingsPage::Llm;
    app.settings_window.llm_repository_tab = RepositoryLlmTab::Model;
    let draft = match app.settings_window.llm_repository.as_deref() {
        Some(path) => app.settings_window.llm_repositories.get_mut(path),
        None => Some(&mut app.settings_window.llm),
    };
    if let Some(draft) = draft {
        draft.secret_error = Some(error);
    }
}

fn repository_llm_dialog(ui: &mut Ui, app: &mut SannyGitApp) {
    let Some(path) = app.settings_window.llm_repository.clone() else {
        app.settings_window.open = false;
        return;
    };
    let palette = app.palette();
    let ctx = ui.ctx().clone();
    let name = app
        .repositories
        .iter()
        .find(|repository| repository.path == path)
        .map_or(path.as_str(), |repository| repository.name.as_str())
        .to_owned();
    let mut accept = false;
    let mut close = false;
    super::controls::dialog_style::modal(&ctx, palette, "repository.llm.settings").show(&ctx, |ui| {
        ui.set_width(560.0_f32.min(ctx.content_rect().width() - 48.0));
        super::controls::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        super::controls::dialog_style::header(
            ui,
            palette,
            "Repository LLM Settings",
            &format!("Model and prompt used to generate commit messages for {name}. The default settings can be found in the Preferences."),
        );
        super::controls::dialog_style::section(
            ui,
            palette.header,
            egui::Margin::symmetric(12, 10),
            |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                // Keep checkboxes and label rows compact so the editors remain
                // accessible; input fields retain their shared height.
                ui.spacing_mut().interact_size.y = 20.0;
                let mut tab = app.settings_window.llm_repository_tab;
                ui.horizontal(|ui| {
                    // Size the tab buttons like the repository settings dialog.
                    ui.spacing_mut().button_padding = egui::vec2(12.0, 4.0);
                    ui.spacing_mut().interact_size.y = crate::theme::CONTROL_HEIGHT;
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (candidate, label) in [
                        (RepositoryLlmTab::Model, "Model"),
                        (RepositoryLlmTab::Prompt, "Prompt"),
                    ] {
                        if ui.selectable_label(tab == candidate, label).clicked() {
                            tab = candidate;
                        }
                    }
                });
                app.settings_window.llm_repository_tab = tab;
                // A fixed content height keeps the dialog size stable when
                // switching tabs; the prompt editor fills the remaining space.
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 212.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| match tab {
                        RepositoryLlmTab::Model => {
                            repository_llm_model_tab(ui, app, palette, Some(&path));
                        }
                        RepositoryLlmTab::Prompt => repository_llm_prompt_tab(ui, app, Some(&path)),
                    },
                );
            },
        );
        super::controls::dialog_style::section(
            ui,
            palette.header,
            egui::Margin::symmetric(12, 8),
            |ui| {
                super::controls::dialog_style::button_row(ui, |ui| {
                    accept = ui
                        .add(super::controls::dialog_style::primary("OK", palette))
                        .dialog_accept();
                    if ui
                        .add(super::controls::dialog_style::button("Cancel"))
                        .dialog_cancel()
                    {
                        close = true;
                    }
                });
            },
        );
    });
    if close {
        app.settings_window.open = false;
    } else if accept {
        match apply_settings(app) {
            Ok(()) => app.settings_window.open = false,
            Err(error) => report_settings_error(app, error),
        }
    }
    super::show_errors(ui, app);
}

fn repository_llm_model_tab(
    ui: &mut Ui,
    app: &mut SannyGitApp,
    palette: Palette,
    path: Option<&str>,
) {
    let window = &mut app.settings_window;
    let mut inherit = path.is_none_or(|path| !window.llm_repositories.contains_key(path));
    let response = ui.add_enabled(
        path.is_some(),
        egui::Checkbox::new(&mut inherit, "Use global model settings"),
    );
    if response.changed() {
        if let Some(path) = path {
            if inherit {
                if let Some(draft) = window.llm_repositories.get(path) {
                    if draft.config.instructions != window.llm.config.instructions {
                        window
                            .llm_instructions
                            .insert(path.to_owned(), draft.config.instructions.clone());
                    }
                }
                window.llm_repositories.remove(path);
            } else {
                window.llm_repositories.insert(
                    path.to_owned(),
                    crate::app::LlmSettings {
                        config: window.llm.config.clone(),
                        api_key_dirty: !window.llm.config.api_key.is_empty(),
                        ..Default::default()
                    },
                );
            }
        }
    }
    if let Some(path) = path {
        if let Some(draft) = window.llm_repositories.get_mut(path) {
            llm_form(ui, draft, palette, Some(path), &mut app.llm_models);
        } else {
            ui.add_enabled_ui(false, |ui| {
                llm_form(ui, &mut window.llm, palette, None, &mut app.llm_models);
            });
        }
    } else {
        llm_form(ui, &mut window.llm, palette, None, &mut app.llm_models);
    }
}

fn repository_llm_prompt_tab(ui: &mut Ui, app: &mut SannyGitApp, path: Option<&str>) {
    let window = &mut app.settings_window;
    let global = window.llm.config.instructions.clone();
    let Some(path) = path else {
        let mut instructions = global;
        instructions_editor(ui, &mut instructions);
        return;
    };
    // An entry in the instructions map marks the override, so the checkbox
    // stays unchecked even before the prompt starts to differ from the global
    // text; a repository config only mirrors the effective instructions.
    let mut inherit = !window.llm_instructions.contains_key(path);
    if ui
        .checkbox(&mut inherit, "Use global generation instructions")
        .changed()
    {
        if inherit {
            window.llm_instructions.remove(path);
            if let Some(draft) = window.llm_repositories.get_mut(path) {
                draft.config.instructions.clone_from(&global);
            }
        } else {
            let current = window
                .llm_instructions
                .get(path)
                .cloned()
                .or_else(|| {
                    window
                        .llm_repositories
                        .get(path)
                        .map(|draft| draft.config.instructions.clone())
                })
                .unwrap_or_else(|| global.clone());
            window.llm_instructions.insert(path.to_owned(), current);
        }
    }
    let mut instructions = window
        .llm_instructions
        .get(path)
        .cloned()
        .unwrap_or_else(|| global.clone());
    if inherit {
        ui.add_enabled_ui(false, |ui| instructions_editor(ui, &mut instructions));
        return;
    }
    instructions_editor(ui, &mut instructions);
    window
        .llm_instructions
        .insert(path.to_owned(), instructions.clone());
    if let Some(draft) = window.llm_repositories.get_mut(path) {
        draft.config.instructions = instructions;
    }
}

fn llm_form(
    ui: &mut Ui,
    draft: &mut crate::app::LlmSettings,
    palette: Palette,
    repository: Option<&str>,
    catalog: &mut crate::app::ModelCatalog,
) {
    catalog.update();
    ui.push_id(("llm.form", repository), |ui| {
        group(ui, palette, "OpenAI-compatible API", |ui| {
            egui::Grid::new("preferences.llm.connection")
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
                    // Refresh with the key already in memory; editing settings
                    // must never read the OS credential store.
                    if (preset_selected || endpoint_response.lost_focus()) && ui.is_enabled() {
                        catalog.load(&draft.config, repository, ui.ctx());
                    }
                    ui.end_row();
                    form_label(ui, "Model:");
                    let id = ui.make_persistent_id("preferences.llm.model");
                    let models = catalog.entry(&draft.config, repository);
                    let empty_text = if models.loading() {
                        "Loading models…"
                    } else if models.error.is_some() {
                        "Cannot load models; enter a model name"
                    } else {
                        "No models returned; enter a model name"
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
                    let first_open =
                        !models.attempted && egui::Popup::is_id_open(ui.ctx(), id.with("popup"));
                    if ui.is_enabled() && first_open {
                        catalog.load(&draft.config, repository, ui.ctx());
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
                    if api_key_response.lost_focus() && draft.api_key_dirty && ui.is_enabled() {
                        catalog.load(&draft.config, repository, ui.ctx());
                    }
                    ui.end_row();
                });
            ui.checkbox(
                &mut draft.config.store_api_key,
                "Store API key and model securely",
            )
            .help_text(if cfg!(target_os = "macos") {
                "Builds with Keychain entitlements use Touch ID. Other builds use the login Keychain and may ask for your macOS password."
            } else {
                "Save the API key and model in the OS credential manager"
            });
            let models = catalog.entry(&draft.config, repository);
            let status = if models.loading() {
                "Loading models…"
            } else {
                ""
            };
            if models.loading() {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50));
            }
            super::controls::dialog_style::text_line(
                ui,
                RichText::new(status).small().color(palette.muted),
                20.0,
            )
            .help_text(status);
        });
    });
}

fn instructions_form(ui: &mut Ui, draft: &mut crate::app::LlmSettings) {
    ui.label("Generation instructions:");
    instructions_editor(ui, &mut draft.config.instructions);
}

fn instructions_editor(ui: &mut Ui, instructions: &mut String) {
    let editor_height = (ui.available_height() - ui.spacing().item_spacing.y).max(24.0);
    let instructions = ui.add_sized(
        [ui.available_width(), editor_height],
        egui::TextEdit::multiline(instructions).desired_rows(1),
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
