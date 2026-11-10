//! Compact desktop dialog chrome shared by repository dialogs.
use super::icons::{self, Icon};
use crate::theme::{semibold_family, Palette};
use eframe::egui::{self, Button, Frame, Id, Margin, Modal, RichText, Ui};

/// Keyboard activation shares the exact availability and action of a dialog button.
pub trait DialogResponse {
    fn dialog_accept(self) -> bool;
    fn dialog_cancel(self) -> bool;
    fn dialog_close(self) -> bool;
}

impl DialogResponse for egui::Response {
    fn dialog_accept(self) -> bool {
        keyboard_action(&self, &[egui::Key::Enter]) || self.clicked()
    }

    fn dialog_cancel(self) -> bool {
        keyboard_action(&self, &[egui::Key::Escape]) || self.clicked()
    }

    fn dialog_close(self) -> bool {
        keyboard_action(&self, &[egui::Key::Escape, egui::Key::Enter]) || self.clicked()
    }
}

fn keyboard_action(response: &egui::Response, keys: &[egui::Key]) -> bool {
    let ctx = &response.ctx;
    if !response.enabled()
        || ctx.memory(|memory| {
            memory
                .top_modal_layer()
                .is_some_and(|layer| layer != response.layer_id)
        })
        || ctx
            .data(|data| data.get_temp::<bool>(response.layer_id.id.with("dialog.popup")))
            .unwrap_or(false)
    {
        return false;
    }
    let enter_allowed = ctx.memory(|memory| memory.focused()).is_none_or(|id| {
        !ctx.data(|data| {
            data.get_temp::<bool>(id.with("dialog.multiline"))
                .unwrap_or(false)
        }) && (id == response.id || egui::TextEdit::load_state(ctx, id).is_some())
    });
    ctx.input_mut(|input| {
        let mut activated = false;
        input.events.retain(|event| {
            if let egui::Event::Key {
                key,
                pressed: true,
                repeat,
                modifiers,
                ..
            } = event
            {
                if keys.contains(key)
                    && modifiers.is_none()
                    && (*key != egui::Key::Enter || enter_allowed)
                {
                    activated |= !repeat;
                    return false;
                }
            }
            true
        });
        activated
    })
}

/// Multiline editors keep Enter for inserting a newline.
pub fn preserve_multiline_enter(response: &egui::Response) {
    response
        .ctx
        .data_mut(|data| data.insert_temp(response.id.with("dialog.multiline"), true));
}

pub fn frame(ctx: &egui::Context, palette: Palette) -> Frame {
    Frame::window(&ctx.global_style())
        .fill(palette.header)
        .stroke((1.0, palette.border))
        .corner_radius(8)
        .inner_margin(1)
}

pub fn modal(ctx: &egui::Context, palette: Palette, id: &'static str) -> Modal {
    Modal::new(Id::unique(id))
        .frame(frame(ctx, palette))
        .backdrop_color(egui::Color32::TRANSPARENT)
}

pub fn prepare_keyboard(ui: &mut Ui) {
    let popup_open = egui::Popup::is_any_open(ui.ctx());
    ui.data_mut(|data| data.insert_temp(ui.layer_id().id.with("dialog.popup"), popup_open));
}

pub fn setup(ui: &mut Ui) {
    prepare_keyboard(ui);
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
    ui.spacing_mut().button_padding = egui::vec2(12.0, 4.0);
    ui.spacing_mut().interact_size.y = 26.0;
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
}

pub fn section(ui: &mut Ui, fill: egui::Color32, margin: Margin, contents: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    Frame::new()
        .fill(fill)
        .corner_radius(7)
        .inner_margin(margin)
        .show(ui, |ui| {
            ui.set_width(width - margin.sum().x);
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), contents);
        });
}

pub fn header(ui: &mut Ui, palette: Palette, title: &str, description: &str) -> egui::Rect {
    header_with_icon(ui, palette, title, description, Icon::Repository)
}

pub fn header_with_icon(
    ui: &mut Ui,
    palette: Palette,
    title: &str,
    description: &str,
    icon: Icon,
) -> egui::Rect {
    let width = ui.available_width();
    Frame::new()
        .fill(palette.panel)
        .corner_radius(egui::CornerRadius {
            nw: 7,
            ne: 7,
            sw: 0,
            se: 0,
        })
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_width(width - 36.0);
            ui.spacing_mut().item_spacing = egui::vec2(16.0, 6.0);
            let origin = ui.next_widget_position();
            let content_width = ui.available_width();
            let mut text = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_min_size(
                        origin,
                        egui::vec2(content_width - 64.0, 0.0),
                    ))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            text.label(RichText::new(title).size(16.0).family(semibold_family()));
            text.add(egui::Label::new(description).wrap());
            let height = text.min_rect().height().max(48.0);
            let (content_rect, _) =
                ui.allocate_exact_size(egui::vec2(content_width, height), egui::Sense::hover());
            let icon_rect = egui::Rect::from_center_size(
                egui::pos2(content_rect.right() - 24.0, content_rect.center().y),
                egui::Vec2::splat(48.0),
            );
            icons::image(icon, palette.accent, 48.0).paint_at(ui, icon_rect);
        })
        .response
        .rect
}

pub fn button(label: &str) -> Button<'_> {
    Button::new(label).min_size(egui::vec2(82.0, 28.0))
}

pub fn primary(label: &str, palette: Palette) -> Button<'_> {
    Button::new(RichText::new(label).color(palette.on_accent))
        .min_size(egui::vec2(82.0, 28.0))
        .fill(palette.accent)
        .stroke((1.0, palette.accent))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{install_fonts, ThemeChoice};

    fn key_event(key: egui::Key, repeat: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        }
    }

    #[test]
    fn dialog_keys_respect_availability_text_editors_and_popups() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                for multiline in [false, true] {
                    let ctx = egui::Context::default();
                    install_fonts(&ctx);
                    theme.apply(&ctx);
                    let editor_id = Id::unique("keyboard.editor");
                    let mut text = String::new();
                    let mut render = |mut events: Vec<egui::Event>, enabled, popup| {
                        let releases = events
                            .iter()
                            .filter_map(|event| {
                                if let egui::Event::Key { key, modifiers, .. } = event {
                                    Some(egui::Event::Key {
                                        key: *key,
                                        physical_key: None,
                                        pressed: false,
                                        repeat: false,
                                        modifiers: *modifiers,
                                    })
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>();
                        events.extend(releases);
                        let mut actions = (false, false);
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                modal(ui.ctx(), theme.palette(), "keyboard.dialog").show(
                                    ui.ctx(),
                                    |ui| {
                                        if popup {
                                            egui::Popup::open_id(
                                                ui.ctx(),
                                                Id::unique("keyboard.popup"),
                                            );
                                        }
                                        setup(ui);
                                        let editor = if multiline {
                                            egui::TextEdit::multiline(&mut text)
                                        } else {
                                            egui::TextEdit::singleline(&mut text)
                                        };
                                        let response = ui.add(editor.id(editor_id));
                                        if multiline {
                                            preserve_multiline_enter(&response);
                                        }
                                        actions.0 =
                                            ui.add_enabled(enabled, button("OK")).dialog_accept();
                                        actions.1 = ui
                                            .add_enabled(enabled, button("Cancel"))
                                            .dialog_cancel();
                                    },
                                );
                            },
                        );
                        output.textures_delta.clear();
                        actions
                    };
                    for _ in 0..3 {
                        render(vec![], true, false);
                    }
                    for key in [egui::Key::Enter, egui::Key::Escape] {
                        let event = || key_event(key, false, egui::Modifiers::NONE);
                        assert_eq!(render(vec![event()], false, false), (false, false));
                        ctx.input_mut(|input| {
                            input.keys_down.insert(key);
                        });
                        assert_eq!(
                            render(
                                vec![key_event(key, true, egui::Modifiers::NONE)],
                                true,
                                false
                            ),
                            (false, false)
                        );
                        assert_eq!(
                            render(
                                vec![key_event(key, false, egui::Modifiers::COMMAND)],
                                true,
                                false
                            ),
                            (false, false)
                        );
                        ctx.memory_mut(|memory| memory.request_focus(editor_id));
                        let expected = if key == egui::Key::Escape {
                            (false, true)
                        } else {
                            (!multiline, false)
                        };
                        assert_eq!(render(vec![event()], true, false), expected);
                        assert_eq!(render(vec![event()], true, true), (false, false));
                        egui::Popup::close_all(&ctx);
                    }
                    drop(render);
                    assert_eq!(text.contains('\n'), multiline);
                }
            }
        }
    }

    #[test]
    fn only_top_modal_handles_keyboard_actions() {
        let ctx = egui::Context::default();
        let mut actions = [false; 2];
        for frame in 0..4 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events: if frame == 3 {
                        vec![key_event(egui::Key::Enter, false, egui::Modifiers::NONE)]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
                |ui| {
                    for (index, id) in ["keyboard.bottom", "keyboard.top"].into_iter().enumerate() {
                        modal(ui.ctx(), ThemeChoice::Light.palette(), id).show(ui.ctx(), |ui| {
                            setup(ui);
                            actions[index] = ui.add(button("OK")).dialog_close();
                        });
                    }
                },
            );
            output.textures_delta.clear();
        }
        assert_eq!(actions, [false, true]);
    }

    #[test]
    fn header_height_depends_on_content_not_available_space() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let ctx = egui::Context::default();
            install_fonts(&ctx);
            theme.apply(&ctx);
            let mut heights = Vec::new();
            for height in [300.0, 700.0, 1024.0] {
                let mut rect = egui::Rect::NOTHING;
                for _ in 0..3 {
                    let mut output = ctx.run_ui(egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, height))),
                        ..Default::default()
                    }, |ui| {
                        ui.set_width(560.0);
                        setup(ui);
                        rect = header(ui, theme.palette(), "Add existing or create new repository",
                            "Specify the local Git repository to open. To create a new repository, specify an empty directory.");
                    });
                    output.textures_delta.clear();
                }
                assert!(
                    rect.height() <= 112.0,
                    "Header has excess padding: {rect:?}"
                );
                heights.push(rect.height());
            }
            assert!(
                heights.windows(2).all(|h| (h[0] - h[1]).abs() < 1.0),
                "Header height changed: {heights:?}"
            );
        }
    }
}
