use eframe::egui::{self, Color32, Context, FontFamily, FontId, Stroke, Style, TextStyle, Visuals};
use serde::{Deserialize, Serialize};

pub const THEME_STORAGE_KEY: &str = "sannygit.theme";

pub fn semibold_family() -> FontFamily {
    FontFamily::Name("UI Semibold".into())
}

pub fn install_fonts(context: &Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Source Sans 3".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/SourceSans3-Regular.ttf"))
            .into(),
    );
    fonts.font_data.insert(
        "Source Sans 3 Semibold".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/SourceSans3-Semibold.ttf"))
            .into(),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Source Sans 3".into());
    let mut semibold = vec!["Source Sans 3 Semibold".into()];
    semibold.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    fonts.families.insert(semibold_family(), semibold);
    #[cfg(target_os = "windows")]
    prefer_windows_ui_fonts(&mut fonts);
    // Fit UI glyphs horizontally as well as vertically for small desktop text.
    for name in [
        "Source Sans 3",
        "Source Sans 3 Semibold",
        "Segoe UI",
        "Segoe UI Semibold",
    ] {
        if let Some(font) = fonts.font_data.get_mut(name) {
            std::sync::Arc::make_mut(font).tweak.hinting_target =
                egui::epaint::text::HintingTarget::Smooth(egui::epaint::text::SmoothHinting {
                    light: false,
                    symmetric_rendering: false,
                    preserve_linear_metrics: false,
                });
        }
    }
    context.set_fonts(fonts);
}

#[cfg(target_os = "windows")]
fn prefer_windows_ui_fonts(fonts: &mut egui::FontDefinitions) {
    let Some(windows_dir) = std::env::var_os("WINDIR") else {
        return;
    };
    let font_dir = std::path::PathBuf::from(windows_dir).join("Fonts");
    // System fonts are optional and are never bundled with the application.
    // Keep the embedded fonts when either face is unavailable.
    let (Ok(regular), Ok(semibold)) = (
        std::fs::read(font_dir.join("segoeui.ttf")),
        std::fs::read(font_dir.join("seguisb.ttf")),
    ) else {
        return;
    };
    for (name, data, family) in [
        ("Segoe UI", regular, FontFamily::Proportional),
        ("Segoe UI Semibold", semibold, semibold_family()),
    ] {
        fonts
            .font_data
            .insert(name.into(), egui::FontData::from_owned(data).into());
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.into());
    }
}

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
        let theme = match self {
            Self::Light => egui::Theme::Light,
            Self::Dark => egui::Theme::Dark,
        };
        context.set_theme(theme);
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
        visuals.override_text_color = Some(palette.text);
        visuals.text_options.font_hinting = true;
        // Fractional glyph positions favor even spacing over sharp pixel edges.
        visuals.text_options.subpixel_binning = false;
        visuals.widgets.noninteractive.weak_bg_fill = palette.panel_alt;
        visuals.widgets.inactive.weak_bg_fill = palette.panel_alt;
        visuals.widgets.hovered.weak_bg_fill = palette.hover;
        visuals.widgets.active.weak_bg_fill = palette.selection;
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
        context.set_visuals_of(theme, visuals);

        context.style_mut_of(theme, |style| {
            *style = Style {
                text_styles: text_styles(),
                ..style.clone()
            };
            style.spacing.item_spacing = egui::vec2(4.0, 2.0);
            style.spacing.button_padding = egui::vec2(5.0, 2.0);
            style.spacing.icon_width = 14.0;
            style.spacing.icon_spacing = 4.0;
            style.spacing.text_edit_width = 180.0;
            style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(4);
            style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(4);
            style.spacing.indent = 14.0;
            style.spacing.scroll.bar_width = 8.0;
            style.spacing.interact_size.y = 20.0;
            style.wrap_mode = Some(egui::TextWrapMode::Truncate);
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
        (TextStyle::Heading, FontId::new(13.0, semibold_family())),
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
    pub header: Color32,
    pub editor: Color32,
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
    pub status_added: Color32,
    pub status_removed: Color32,
    pub modified: Color32,
    pub modified_background: Color32,
    pub hunk_background: Color32,
    pub orange: Color32,
    pub graph_main: Color32,
    pub graph_branch: Color32,
    pub graph_root: Color32,
}

impl Palette {
    fn dark() -> Self {
        Self {
            background: Color32::from_rgb(16, 23, 30),
            panel: Color32::from_rgb(22, 29, 37),
            panel_alt: Color32::from_rgb(30, 39, 50),
            header: Color32::from_rgb(25, 33, 42),
            editor: Color32::from_rgb(30, 39, 50),
            border: Color32::from_rgb(46, 58, 72),
            text: Color32::from_rgb(218, 229, 241),
            muted: Color32::from_rgb(128, 145, 166),
            accent: Color32::from_rgb(28, 131, 242),
            selection: Color32::from_rgb(23, 63, 111),
            hover: Color32::from_rgb(35, 48, 64),
            added_background: Color32::from_rgb(23, 55, 34),
            added_text: Color32::from_rgb(151, 234, 139),
            removed_background: Color32::from_rgb(62, 30, 36),
            removed_text: Color32::from_rgb(244, 106, 111),
            status_added: Color32::from_rgb(46, 103, 58),
            status_removed: Color32::from_rgb(147, 65, 75),
            modified: Color32::from_rgb(236, 190, 82),
            modified_background: Color32::from_rgb(113, 87, 41),
            hunk_background: Color32::from_rgb(28, 47, 67),
            orange: Color32::from_rgb(247, 77, 43),
            graph_main: Color32::from_rgb(39, 190, 226),
            graph_branch: Color32::from_rgb(248, 82, 76),
            graph_root: Color32::from_rgb(79, 196, 72),
        }
    }

    fn light() -> Self {
        Self {
            background: Color32::from_rgb(244, 245, 248),
            panel: Color32::from_rgb(251, 251, 253),
            panel_alt: Color32::from_rgb(242, 244, 248),
            header: Color32::from_rgb(247, 248, 251),
            editor: Color32::from_rgb(255, 255, 255),
            border: Color32::from_rgb(220, 225, 234),
            text: Color32::from_rgb(18, 24, 39),
            muted: Color32::from_rgb(110, 119, 139),
            accent: Color32::from_rgb(28, 126, 235),
            selection: Color32::from_rgb(226, 237, 253),
            hover: Color32::from_rgb(234, 240, 249),
            added_background: Color32::from_rgb(232, 252, 230),
            added_text: Color32::from_rgb(0, 128, 24),
            removed_background: Color32::from_rgb(254, 235, 238),
            removed_text: Color32::from_rgb(215, 40, 53),
            status_added: Color32::from_rgb(189, 238, 182),
            status_removed: Color32::from_rgb(255, 194, 194),
            modified: Color32::from_rgb(174, 119, 12),
            modified_background: Color32::from_rgb(255, 225, 161),
            hunk_background: Color32::from_rgb(232, 240, 254),
            orange: Color32::from_rgb(244, 78, 40),
            graph_main: Color32::from_rgb(0, 130, 167),
            graph_branch: Color32::from_rgb(230, 63, 54),
            graph_root: Color32::from_rgb(16, 153, 46),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_themes_preserves_palette_and_typography_across_frames() {
        let context = Context::default();
        install_fonts(&context);
        for choice in [ThemeChoice::Dark, ThemeChoice::Light, ThemeChoice::Dark] {
            choice.apply(&context);
            for _ in 0..2 {
                let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                    let style = ui.style();
                    assert_eq!(style.visuals.dark_mode, choice == ThemeChoice::Dark);
                    assert_eq!(style.visuals.panel_fill, choice.palette().panel);
                    assert_eq!(style.text_styles[&TextStyle::Body].size, 13.0);
                    assert!(style.visuals.text_options.font_hinting);
                    assert!(!style.visuals.text_options.subpixel_binning);
                });
                output.textures_delta.clear();
            }
        }
    }
}
