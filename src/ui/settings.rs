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
    let palette = app.palette();
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
                    .clicked()
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
                    .clicked()
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
                ui.checkbox(
                    &mut app.settings_window.skip_push_confirmation,
                    "Skip confirmation when pushing to upstream",
                );
                ui.label(
                    RichText::new("Push results and errors appear in the status bar without opening a dialog.")
                        .small()
                        .color(palette.muted),
                );
                if ui
                    .add_enabled(
                        app.settings_window.skip_push_confirmation,
                        egui::Button::new("Restore confirmation dialogs"),
                    )
                    .help_text("Re-enable the upstream push confirmation")
                    .disabled_help_text("All confirmation dialogs are already enabled")
                    .clicked()
                {
                    app.settings_window.skip_push_confirmation = false;
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
                                        .truncate(),
                                    )
                                    .help_text(name);
                                let path_response = ui
                                    .add(
                                        egui::Label::new(
                                            RichText::new(path).small().color(palette.muted),
                                        )
                                        .truncate(),
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
                        api_key_loaded: window.llm.api_key_loaded,
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
    if !draft.api_key_loaded {
        draft.api_key_loaded = true;
        draft.secret_error =
            crate::llm::load_api_key_if_needed(&mut draft.config, repository).err();
    }
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
                    // Read only after editing finishes, rather than prompting
                    // Keychain for every character typed into a custom URL.
                    if preset_selected || endpoint_response.lost_focus() {
                        draft.api_key_loaded = true;
                        draft.secret_error =
                            crate::llm::load_api_key_if_needed(&mut draft.config, repository).err();
                        if ui.is_enabled() {
                            catalog.load(&draft.config, repository, ui.ctx());
                        }
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
                            .hint_text("Optional for local servers")
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
            );
            let models = catalog.entry(&draft.config, repository);
            let error = draft.secret_error.as_deref().or(models.error.as_deref());
            let status = error.unwrap_or(if models.loading() {
                "Loading models…"
            } else {
                ""
            });
            if models.loading() {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50));
            }
            ui.add_sized(
                [ui.available_width(), 20.0],
                egui::Label::new(RichText::new(status).small().color(if error.is_some() {
                    palette.removed_text
                } else {
                    palette.muted
                }))
                .truncate(),
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
                    ui.add_sized(
                        [ui.available_width(), editor_height],
                        egui::TextEdit::multiline(&mut draft.config.instructions).desired_rows(1),
                    );
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
mod tests {
    use super::*;
    use eframe::egui::{
        epaint::ClippedShape, pos2, Event, Modifiers, PointerButton, RawInput, Rect,
    };

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
                app.settings_window.llm.api_key_loaded = true;
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
            app.settings_window.llm.api_key_loaded = true;
            app.settings_window.llm.config.api_key = "previous-key".into();
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
            let mut output = render(&mut app, vec![]);
            let target = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.job.text == crate::llm::DEFAULT_ENDPOINT =>
                    {
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
            app.settings_window.llm.api_key_loaded = true;
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
                        screen_rect: Some(Rect::from_min_size(
                            pos2(0.0, 0.0),
                            Vec2::new(680.0, 380.0),
                        )),
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
            app.settings_window.preferences.date_format =
                crate::preferences::DateFormat::DayMonthYear;
            let render = |app: &mut SannyGitApp, events| {
                context.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            pos2(0.0, 0.0),
                            Vec2::new(680.0, 380.0),
                        )),
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
            app.settings_window.llm.api_key_loaded = true;
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
}
