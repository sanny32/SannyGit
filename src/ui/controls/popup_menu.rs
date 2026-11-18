use super::icons::{self, Icon};
use crate::theme::{Palette, UI_FONT_SIZE};
use eframe::egui::{
    containers::menu::{self, MenuConfig, SubMenuButton},
    Atom, Button, CornerRadius, FontId, Popup, Response, Separator, Stroke, Style, TextWrapMode,
    Ui, UiKind, UiStackInfo, Vec2,
};

pub(crate) fn config(palette: Palette) -> MenuConfig {
    MenuConfig::new()
        .submenu_gap(0.0)
        .style(move |style: &mut Style| apply_style(style, palette))
}

pub(crate) fn context_menu(response: &Response, palette: Palette) -> Popup<'_> {
    let config = config(palette);
    Popup::context_menu(response)
        .style(config.style.clone())
        .info(UiStackInfo::new(UiKind::Menu).with_tag_value(MenuConfig::MENU_CONFIG_TAG, config))
}

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

pub(crate) fn separator(ui: &mut Ui) {
    ui.add(Separator::default().spacing(10.0));
}

fn apply_style(style: &mut Style, palette: Palette) {
    menu::menu_style(style);
    style.override_font_id = Some(FontId::proportional(UI_FONT_SIZE));
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
