use super::controls::dialog_style::DialogResponse;
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::{SannyGitApp, SettingsPage},
    theme::{semibold_family, InterfaceStyle, Palette, ThemeChoice},
};
use eframe::egui::{
    self, Align, CentralPanel, Frame, Key, Layout, Margin, Panel, RichText, ScrollArea, Ui, Vec2,
    ViewportBuilder, ViewportCommand, ViewportId,
};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    if !app.settings_window.open {
        return;
    }

    let repository = app.settings_window.llm_repository.as_deref();
    let (title, size, min_size) = if repository.is_some() {
        ("Repository LLM Settings", [560.0, 420.0], [500.0, 380.0])
    } else {
        ("SannyGit Preferences", [740.0, 420.0], [680.0, 380.0])
    };
    let builder = ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size(min_size)
        .with_maximize_button(false);
    let viewport_id = match repository {
        Some(path) => ViewportId::from_hash_of(("sannygit.repository_llm", path)),
        None => ViewportId::from_hash_of("sannygit.preferences"),
    };
    let ctx = ctx.clone();
    ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
        #[cfg(target_os = "macos")]
        ui.input_mut(|input| input.events.append(&mut app.pending_native_edit_events));
        let close_requested = ui.ctx().input(|input| input.viewport().close_requested());
        let escape = app.error_dialogs.pending.is_empty()
            && !egui::Popup::is_any_open(ui.ctx())
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape));
        if close_requested || escape {
            app.settings_window.open = false;
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        window_contents(ui, app);
    });
}

fn window_contents(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    super::controls::dialog_style::prepare_keyboard(ui);
    ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
    footer(ui, app, palette);
    if app.settings_window.llm_repository.is_none() {
        navigation(ui, app, palette);
    }
    CentralPanel::default()
        .frame(Frame::new().fill(palette.header).inner_margin(12))
        .show(ui, |ui| {
            if app.settings_window.llm_repository.is_some() {
                repository_llm_page(ui, app, palette);
            } else {
                let page_height = ui.available_height();
                ScrollArea::vertical()
                    .id_salt("preferences.page")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if app.settings_window.page == SettingsPage::Llm {
                            ui.set_max_height(page_height);
                        }
                        page(ui, app, palette);
                    });
            }
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
                            app.settings_window.open = false;
                            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                        }
                        Err(error) => {
                            app.settings_window.page = SettingsPage::Llm;
                            let draft = match app.settings_window.llm_repository.as_deref() {
                                Some(path) => app.settings_window.llm_repositories.get_mut(path),
                                None => Some(&mut app.settings_window.llm),
                            };
                            if let Some(draft) = draft {
                                draft.secret_error = Some(error);
                            }
                        }
                    }
                }
                if ui
                    .add(super::controls::dialog_style::button("Cancel"))
                    .dialog_cancel()
                {
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
                            entry(ui, app, SettingsPage::Appearance, "Appearance");
                            entry(ui, app, SettingsPage::DateTime, "Date & Time");
                        });
                        section(ui, "Git", |ui| {
                            entry(ui, app, SettingsPage::Git, "Executable");
                            entry(ui, app, SettingsPage::Remote, "Remote");
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
            heading(ui, "General");
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
            ui.add_space(4.0);
            group(ui, palette, "Startup", |ui| {
                ui.checkbox(
                    &mut app.settings_window.preferences.show_welcome,
                    "Show welcome dialog if no repository is reopened",
                );
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
                    );
                });
                ui.label(
                    RichText::new("Skipped operations report results in the status bar.")
                        .small()
                        .color(palette.muted),
                );
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
                        egui::TextEdit::singleline(value),
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
            heading(ui, "LLM commit messages");
            llm_form(
                ui,
                &mut app.settings_window.llm,
                palette,
                None,
                &mut app.llm_models,
            );
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

fn repository_llm_page(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette) {
    let window = &mut app.settings_window;
    let path = window.llm_repository.clone();
    if let Some(path) = &path {
        let name = app
            .repositories
            .iter()
            .find(|repository| repository.path == *path)
            .map_or(path.as_str(), |repository| repository.name.as_str());
        Frame::new()
            .inner_margin(Margin::symmetric(6, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::new(20.0, 38.0), egui::Sense::hover());
                    let text_rect = ui
                        .allocate_ui_with_layout(
                            Vec2::new(ui.available_width(), 38.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                let name_response = ui
                                    .add(
                                        egui::Label::new(
                                            RichText::new(name).family(semibold_family()),
                                        )
                                        .truncate()
                                        .show_tooltip_when_elided(false),
                                    )
                                    .help_text(name);
                                let path_response = ui
                                    .add(
                                        egui::Label::new(
                                            RichText::new(path).small().color(palette.muted),
                                        )
                                        .truncate()
                                        .show_tooltip_when_elided(false),
                                    )
                                    .help_text(path);
                                name_response.rect.union(path_response.rect)
                            },
                        )
                        .inner;
                    super::controls::icons::image(
                        app.manager.hosting.get(path).copied().map_or(
                            super::controls::icons::Icon::Repository,
                            super::controls::icons::Icon::for_provider,
                        ),
                        palette.text,
                        20.0,
                    )
                    .paint_at(
                        ui,
                        egui::Rect::from_center_size(
                            egui::pos2(icon_rect.center().x, text_rect.center().y),
                            Vec2::splat(20.0),
                        ),
                    );
                });
            });
    }
    let mut inherit = path
        .as_ref()
        .is_none_or(|path| !window.llm_repositories.contains_key(path));
    let response = ui.add_enabled(
        path.is_some(),
        egui::Checkbox::new(&mut inherit, "Use global settings"),
    );
    if response.changed() {
        if let Some(path) = &path {
            if inherit {
                window.llm_repositories.remove(path);
            } else {
                window.llm_repositories.insert(
                    path.clone(),
                    crate::app::LlmSettings {
                        config: window.llm.config.clone(),
                        api_key_dirty: !window.llm.config.api_key.is_empty(),
                        ..Default::default()
                    },
                );
            }
        }
    }
    if let Some(path) = &path {
        if let Some(draft) = window.llm_repositories.get_mut(path) {
            llm_form(ui, draft, palette, Some(path), &mut app.llm_models);
        } else {
            ui.add_enabled_ui(false, |ui| {
                llm_form(ui, &mut window.llm, palette, None, &mut app.llm_models)
            });
        }
    } else {
        llm_form(ui, &mut window.llm, palette, None, &mut app.llm_models);
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
                    endpoint_response.clone().help_text(
                        crate::llm::PROVIDER_PRESETS
                            .iter()
                            .find(|provider| provider.endpoint == endpoint)
                            .map_or("Custom endpoint", |provider| provider.name),
                    );
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
                        egui::TextEdit::singleline(&mut draft.config.api_key)
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
            ui.add_sized(
                [ui.available_width(), 20.0],
                egui::Label::new(RichText::new(status).small().color(palette.muted))
                .truncate()
                .show_tooltip_when_elided(false),
            )
            .help_text(status);
            ui.add_space(6.0);
            ui.label("Generation instructions:");
            let editor_height = (ui.available_height() - ui.spacing().item_spacing.y).max(24.0);
            ScrollArea::vertical()
                .id_salt("llm.instructions")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, false])
                .min_scrolled_height(0.0)
                .max_height(editor_height)
                .show(ui, |ui| {
                    let instructions = ui.add_sized(
                        [ui.available_width(), editor_height],
                        egui::TextEdit::multiline(&mut draft.config.instructions).desired_rows(1),
                    );
                    super::controls::dialog_style::preserve_multiline_enter(&instructions);
                });
        });
    });
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
