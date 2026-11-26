#[cfg(any(not(target_os = "macos"), test))]
use super::icons::{self, Icon};
#[cfg(any(not(target_os = "macos"), test))]
use crate::theme::ui_font_size;
use crate::theme::Palette;
use eframe::egui::Response;
#[cfg(any(not(target_os = "macos"), test))]
use eframe::egui::{
    containers::menu::{self, MenuConfig, SubMenuButton},
    Atom, Button, CornerRadius, FontId, Popup, Separator, Stroke, Style, TextWrapMode, Ui, UiKind,
    UiStackInfo, Vec2,
};

#[cfg(any(not(target_os = "macos"), test))]
pub(crate) fn config(palette: Palette) -> MenuConfig {
    MenuConfig::new()
        .submenu_gap(0.0)
        .style(move |style: &mut Style| apply_style(style, palette))
}

/// A shared description keeps native and egui menus on the same command path.
pub(crate) enum Entry<T> {
    Item {
        label: String,
        enabled: bool,
        action: T,
        help: String,
        shortcut: Option<eframe::egui::KeyboardShortcut>,
    },
    Submenu {
        label: String,
        entries: Vec<Entry<T>>,
    },
    Separator,
}

impl<T> Entry<T> {
    pub(crate) fn item(label: impl Into<String>, enabled: bool, action: T) -> Self {
        Self::Item {
            label: label.into(),
            enabled,
            action,
            help: String::new(),
            shortcut: None,
        }
    }
    pub(crate) fn help(mut self, text: impl Into<String>) -> Self {
        if let Self::Item { help, .. } = &mut self {
            *help = text.into();
        }
        self
    }
    pub(crate) fn shortcut(mut self, shortcut: eframe::egui::KeyboardShortcut) -> Self {
        if let Self::Item {
            shortcut: value, ..
        } = &mut self
        {
            *value = Some(shortcut);
        }
        self
    }
    pub(crate) fn maybe_shortcut(
        mut self,
        shortcut: Option<eframe::egui::KeyboardShortcut>,
    ) -> Self {
        if let Some(shortcut) = shortcut {
            self = self.shortcut(shortcut);
        }
        self
    }
    pub(crate) fn submenu(label: impl Into<String>, entries: Vec<Self>) -> Self {
        Self::Submenu {
            label: label.into(),
            entries,
        }
    }
}

/// Close every context menu when its window no longer has focus.
pub(crate) fn dismiss_on_focus_loss(ctx: &eframe::egui::Context) {
    if !ctx.input(|input| input.focused) && eframe::egui::Popup::is_any_open(ctx) {
        eframe::egui::Popup::close_all(ctx);
    }
}

pub(crate) fn context<T: Clone>(
    response: &Response,
    palette: Palette,
    entries: impl FnOnce() -> Vec<Entry<T>>,
) -> Option<T> {
    // An unfocused window must not keep or open a menu. This covers every
    // caller: branches, changes, and repositories.
    dismiss_on_focus_loss(&response.ctx);
    if !response.ctx.input(|input| input.focused) {
        return None;
    }
    #[cfg(all(target_os = "macos", not(test)))]
    if response.secondary_clicked() {
        return crate::native_menu::popup(&entries());
    }
    #[cfg(all(target_os = "macos", not(test)))]
    {
        let _ = palette;
        None
    }
    #[cfg(any(not(target_os = "macos"), test))]
    {
        let config = config(palette);
        let mut selected = None;
        Popup::context_menu(response)
            .style(config.style.clone())
            .info(
                UiStackInfo::new(UiKind::Menu).with_tag_value(MenuConfig::MENU_CONFIG_TAG, config),
            )
            .show(|ui| {
                selected = show_entries(ui, &entries());
            });
        selected
    }
}

#[cfg(any(not(target_os = "macos"), test))]
fn show_entries<T: Clone>(ui: &mut Ui, entries: &[Entry<T>]) -> Option<T> {
    let mut selected = None;
    for entry in entries {
        match entry {
            Entry::Item {
                label,
                enabled,
                action,
                help,
                shortcut,
            } => {
                let mut button = Button::new(label);
                if let Some(shortcut) = shortcut {
                    button = button.shortcut_text(ui.ctx().format_shortcut(shortcut));
                }
                if ui
                    .add_enabled(*enabled, button)
                    .on_disabled_hover_text(help)
                    .clicked()
                {
                    selected = Some(action.clone());
                    ui.close();
                }
            }
            Entry::Submenu { label, entries } => submenu(ui, label, |ui| {
                if let Some(action) = show_entries(ui, entries) {
                    selected = Some(action);
                }
            }),
            Entry::Separator => separator(ui),
        }
    }
    selected
}

#[cfg(any(not(target_os = "macos"), test))]
pub(crate) fn submenu(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui)) {
    let arrow_size = Vec2::splat(12.0);
    let arrow = Atom::paint(arrow_size, move |ui, args| {
        // Keep the shared text inset while bringing the arrow toward the right edge.
        let offset = ui.spacing().button_padding.x - 6.0;
        let rect = args.rect.translate(Vec2::new(offset, 0.0));
        icons::image(Icon::Chevron, args.fallback_text_color, arrow_size.x)
            .rotate(-std::f32::consts::FRAC_PI_2, Vec2::splat(0.5))
            .paint_at(ui, rect);
    });
    SubMenuButton::from_button(Button::new(label).right_text(arrow)).ui(ui, content);
}

#[cfg(any(not(target_os = "macos"), test))]
pub(crate) fn separator(ui: &mut Ui) {
    ui.add(Separator::default().spacing(10.0));
}

#[cfg(any(not(target_os = "macos"), test))]
fn apply_style(style: &mut Style, palette: Palette) {
    menu::menu_style(style);
    style.override_font_id = Some(FontId::proportional(ui_font_size()));
    style.wrap_mode = Some(TextWrapMode::Extend);
    style.spacing.item_spacing = Vec2::ZERO;
    style.spacing.button_padding = Vec2::new(28.0, 2.0);
    style.spacing.interact_size.y = 24.0;
    style.visuals.window_fill = palette.menu_background;
    style.visuals.window_stroke = Stroke::new(1.0, palette.border);
    style.visuals.menu_corner_radius = CornerRadius::same(4);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::ZERO;
        widget.expansion = 0.0;
    }
    style.visuals.widgets.hovered.weak_bg_fill = palette.hover;
    style.visuals.widgets.open.weak_bg_fill = palette.hover;
}

/// Toolbar popup buttons use the same native renderer as context menus.
pub(crate) fn button<T: Clone>(
    ui: &mut eframe::egui::Ui,
    icon: super::icons::Icon,
    label: &str,
    entries: impl FnOnce() -> Vec<Entry<T>>,
) -> Option<T> {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        let response = super::icons::button(ui, icon, label);
        if response.clicked() {
            crate::native_menu::popup(&entries())
        } else {
            None
        }
    }
    #[cfg(any(not(target_os = "macos"), test))]
    {
        let mut selected = None;
        super::icons::menu(ui, icon, label, |ui| {
            selected = show_entries(ui, &entries());
        });
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput};

    #[test]
    fn disabled_items_never_dispatch_an_action() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            let ctx = Context::default();
            theme.apply(&ctx);
            let entries = [
                Entry::item("Disabled", false, 1),
                Entry::item("Enabled", true, 2),
            ];
            let mut positions = [Pos2::ZERO; 2];
            let mut render = |events| {
                let mut selected = None;
                let mut output = ctx.run_ui(
                    RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        selected = show_entries(ui, &entries);
                    },
                );
                for shape in &output.shapes {
                    if let eframe::egui::Shape::Text(text) = &shape.shape {
                        let index = match text.galley.job.text.as_str() {
                            "Disabled" => 0,
                            "Enabled" => 1,
                            _ => continue,
                        };
                        positions[index] = text.pos + eframe::egui::vec2(3.0, 3.0);
                    }
                }
                output.textures_delta.clear();
                (selected, positions)
            };
            let (_, positions) = render(Vec::new());
            for (index, expected) in [None, Some(2)].into_iter().enumerate() {
                let pointer = positions[index];
                for pressed in [true, false] {
                    let (selected, _) = render(vec![
                        Event::PointerMoved(pointer),
                        Event::PointerButton {
                            pos: pointer,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ]);
                    assert_eq!(selected, if pressed { None } else { expected });
                }
            }
        }
    }

    #[test]
    fn every_context_menu_closes_when_its_window_loses_focus() {
        let ctx = Context::default();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let response = ui.button("row");
            eframe::egui::Popup::open_id(
                ui.ctx(),
                eframe::egui::Popup::default_response_id(&response),
            );
            dismiss_on_focus_loss(ui.ctx());
            assert!(eframe::egui::Popup::is_any_open(ui.ctx()));
            let selected =
                context::<()>(&response, crate::theme::ThemeChoice::Dark.palette(), || {
                    vec![Entry::item("Format Patch", true, ())]
                });
            assert!(selected.is_none());
            assert!(eframe::egui::Popup::is_any_open(ui.ctx()));
        });
        output.textures_delta.clear();

        let mut output = ctx.run_ui(
            RawInput {
                focused: false,
                ..Default::default()
            },
            |ui| {
                let response = ui.button("row");
                let selected =
                    context::<()>(&response, crate::theme::ThemeChoice::Dark.palette(), || {
                        panic!("A context menu must not stay open after its window loses focus");
                    });
                assert!(selected.is_none());
                assert!(!eframe::egui::Popup::is_any_open(ui.ctx()));
                dismiss_on_focus_loss(ui.ctx());
                assert!(!eframe::egui::Popup::is_any_open(ui.ctx()));
            },
        );
        output.textures_delta.clear();
    }

    #[test]
    fn unopened_context_menu_does_not_build_its_contents() {
        let ctx = Context::default();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let response = ui.button("Target");
            let selected =
                context::<()>(&response, crate::theme::ThemeChoice::Dark.palette(), || {
                    panic!("An unopened menu must not build or copy repository contents");
                });
            assert!(selected.is_none());
        });
        output.textures_delta.clear();
    }
}
