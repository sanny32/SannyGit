use eframe::egui::{self, Color32, Context, FontFamily, FontId, Stroke, Style, TextStyle, Visuals};
use serde::{Deserialize, Serialize};

pub const THEME_STORAGE_KEY: &str = "sannygit.theme";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    Light,
    Dark,
}

impl ThemeChoice {
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Self::Light => Palette::light(),
            Self::Dark => Palette::dark(),
        }
    }

    pub fn apply(self, context: &Context) {
        let palette = self.palette();
        let mut visuals = match self {
            Self::Light => Visuals::light(),
            Self::Dark => Visuals::dark(),
        };

        visuals.panel_fill = palette.panel;
        visuals.window_fill = palette.panel;
        visuals.extreme_bg_color = palette.background;
        visuals.faint_bg_color = palette.panel_alt;
        visuals.hyperlink_color = palette.accent;
        visuals.selection.bg_fill = palette.selection;
        visuals.selection.stroke = Stroke::new(1.0_f32, palette.accent);
        visuals.widgets.noninteractive.bg_fill = palette.panel;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, palette.border);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, palette.text);
        visuals.widgets.inactive.bg_fill = palette.panel_alt;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.border);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, palette.text);
        visuals.widgets.hovered.bg_fill = palette.hover;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, palette.accent);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, palette.text);
        visuals.widgets.active.bg_fill = palette.selection;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, palette.accent);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, palette.text);
        context.set_visuals(visuals);

        context.style_mut(|style| {
            *style = Style {
                text_styles: text_styles(),
                ..style.clone()
            };
            style.spacing.item_spacing = egui::vec2(6.0, 5.0);
            style.spacing.button_padding = egui::vec2(8.0, 4.0);
            style.spacing.indent = 14.0;
            style.spacing.scroll.bar_width = 8.0;
            style.interaction.resize_grab_radius_side = 5.0;
        });
    }
}

fn text_styles() -> std::collections::BTreeMap<TextStyle, FontId> {
    [
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(12.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(17.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into()
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub selection: Color32,
    pub hover: Color32,
    pub added_background: Color32,
    pub added_text: Color32,
    pub removed_background: Color32,
    pub removed_text: Color32,
    pub modified: Color32,
    pub orange: Color32,
}

impl Palette {
    fn dark() -> Self {
        Self {
            background: Color32::from_rgb(10, 17, 29),
            panel: Color32::from_rgb(15, 25, 41),
            panel_alt: Color32::from_rgb(18, 30, 48),
            border: Color32::from_rgb(37, 52, 73),
            text: Color32::from_rgb(218, 229, 241),
            muted: Color32::from_rgb(128, 145, 166),
            accent: Color32::from_rgb(48, 139, 230),
            selection: Color32::from_rgb(24, 76, 125),
            hover: Color32::from_rgb(28, 45, 67),
            added_background: Color32::from_rgb(17, 57, 43),
            added_text: Color32::from_rgb(117, 211, 151),
            removed_background: Color32::from_rgb(67, 34, 44),
            removed_text: Color32::from_rgb(242, 139, 151),
            modified: Color32::from_rgb(236, 190, 82),
            orange: Color32::from_rgb(239, 126, 55),
        }
    }

    fn light() -> Self {
        Self {
            background: Color32::from_rgb(243, 247, 251),
            panel: Color32::from_rgb(255, 255, 255),
            panel_alt: Color32::from_rgb(247, 250, 253),
            border: Color32::from_rgb(215, 225, 235),
            text: Color32::from_rgb(38, 52, 70),
            muted: Color32::from_rgb(111, 128, 148),
            accent: Color32::from_rgb(27, 119, 205),
            selection: Color32::from_rgb(219, 235, 251),
            hover: Color32::from_rgb(235, 242, 249),
            added_background: Color32::from_rgb(225, 246, 231),
            added_text: Color32::from_rgb(31, 133, 71),
            removed_background: Color32::from_rgb(253, 232, 235),
            removed_text: Color32::from_rgb(190, 55, 72),
            modified: Color32::from_rgb(174, 119, 12),
            orange: Color32::from_rgb(226, 104, 42),
        }
    }
}
