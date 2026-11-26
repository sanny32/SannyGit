use super::tooltips::TooltipResponse;
use eframe::egui::{Button, Color32, Image, Rect, Response, Sense, Ui, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    GitHub,
    GitLab,
    Bitbucket,
    AzureDevOps,
    Pull,
    Push,
    Fetch,
    Branch,
    Merge,
    Rebase,
    Stash,
    Discard,
    Repository,
    Tag,
    File,
    Copy,
    CodeFile,
    Code,
    ExternalLink,
    Info,
    OpenAi,
    TextFile,
    Folder,
    ImageFile,
    Settings,
    Globe,
    Chevron,
    Plus,
    List,
    Check,
    Close,
    Cancel,
    Stop,
    Warning,
    Star,
    Sparkles,
}

impl Icon {
    pub fn for_provider(provider: crate::clone_source::Provider) -> Self {
        use crate::clone_source::Provider;
        match provider {
            Provider::Url => Self::Repository,
            Provider::GitHub | Provider::GitHubEnterprise => Self::GitHub,
            Provider::GitLab | Provider::GitLabSelfHosted => Self::GitLab,
            Provider::Bitbucket | Provider::BitbucketServer => Self::Bitbucket,
            Provider::Azure | Provider::AzureServer => Self::AzureDevOps,
        }
    }

    pub fn for_file(path: &str) -> Self {
        if path.ends_with(['/', '\\']) {
            return Self::Folder;
        }
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let extension = name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase());
        match extension.as_deref() {
            Some(
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tif" | "tiff"
                | "avif",
            ) => Self::ImageFile,
            Some(
                "rs" | "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "go" | "py" | "js" | "jsx" | "ts"
                | "tsx" | "java" | "kt" | "swift" | "rb" | "php" | "sh" | "bash" | "zsh" | "ps1"
                | "html" | "css" | "scss" | "sql",
            ) => Self::CodeFile,
            Some(
                "md" | "mdx" | "txt" | "rst" | "adoc" | "toml" | "yaml" | "yml" | "json" | "xml"
                | "ini" | "cfg" | "conf" | "csv" | "log" | "lock",
            ) => Self::TextFile,
            _ => Self::File,
        }
    }

    fn source(self) -> (&'static str, &'static [u8]) {
        macro_rules! svg {
            ($name:literal) => {
                (
                    $name,
                    include_bytes!(concat!("../../../assets/icons/", $name, ".svg")),
                )
            };
        }
        match self {
            Self::GitHub => svg!("github"),
            Self::GitLab => svg!("gitlab"),
            Self::Bitbucket => svg!("bitbucket"),
            Self::AzureDevOps => svg!("azure-devops"),
            Self::Pull => svg!("pull"),
            Self::Push => svg!("push"),
            Self::Fetch => svg!("fetch"),
            Self::Branch => svg!("branch"),
            Self::Merge => svg!("merge"),
            Self::Rebase => svg!("rebase"),
            Self::Stash => svg!("stash"),
            Self::Discard => svg!("discard"),
            Self::Repository => svg!("repository"),
            Self::Tag => svg!("tag"),
            Self::File => svg!("file"),
            Self::Copy => svg!("copy"),
            Self::CodeFile => svg!("code-file"),
            Self::Code => svg!("code"),
            Self::ExternalLink => svg!("external-link"),
            Self::Info => svg!("info"),
            Self::OpenAi => svg!("openai"),
            Self::TextFile => svg!("text-file"),
            Self::Folder => svg!("folder"),
            Self::ImageFile => svg!("image-file"),
            Self::Settings => svg!("settings"),
            Self::Globe => svg!("globe"),
            Self::Chevron => svg!("chevron"),
            Self::Plus => svg!("plus"),
            Self::List => svg!("list"),
            Self::Check => svg!("check"),
            Self::Close => svg!("close"),
            Self::Cancel => svg!("cancel"),
            Self::Stop => svg!("stop"),
            Self::Warning => svg!("warning"),
            Self::Star => svg!("star"),
            Self::Sparkles => svg!("sparkles"),
        }
    }
}

pub fn image(icon: Icon, color: Color32, size: f32) -> Image<'static> {
    let (name, bytes) = icon.source();
    Image::from_bytes(format!("bytes://icons/{name}.svg"), bytes)
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color)
}

pub fn paint(ui: &mut Ui, icon: Icon, color: Color32, size: f32) {
    ui.add(image(icon, color, size));
}

/// Same disclosure mark as the changes tree: down when open, right when closed.
pub fn collapsing_icon(ui: &mut Ui, openness: f32, response: &Response) {
    let color = ui.style().interact(response).fg_stroke.color;
    let rect = Rect::from_center_size(response.rect.center(), Vec2::splat(14.0));
    let angle = (openness - 1.0) * std::f32::consts::FRAC_PI_2;
    let painter = ui.painter().clone();
    paint_at(ui, &painter, Icon::Chevron, color, rect, angle);
}

pub fn paint_at(
    ui: &Ui,
    painter: &eframe::egui::Painter,
    icon: Icon,
    color: Color32,
    rect: Rect,
    angle: f32,
) {
    let image = image(icon, color, rect.width());
    let Ok(poll) = image.load_for_size(ui.ctx(), rect.size()) else {
        return;
    };
    let eframe::egui::load::TexturePoll::Ready { texture } = poll else {
        ui.ctx().request_repaint();
        return;
    };
    let uv = Rect::from_min_max(eframe::egui::pos2(0.0, 0.0), eframe::egui::pos2(1.0, 1.0));
    if angle == 0.0 {
        painter.image(texture.id, rect, uv, color);
        return;
    }
    let mut mesh = eframe::egui::Mesh::with_texture(texture.id);
    mesh.add_rect_with_uv(rect, uv, color);
    mesh.rotate(eframe::egui::emath::Rot2::from_angle(angle), rect.center());
    painter.add(eframe::egui::Shape::mesh(mesh));
}

// All icon-only actions use this geometry, including menu triggers and disabled
// controls. Local dialog padding must not change their hit area or hover frame.
pub const BUTTON_SIZE: f32 = 26.0;
const BUTTON_ICON_SIZE: f32 = 16.0;

fn button_widget(ui: &Ui, icon: Icon) -> Button<'static> {
    Button::new(image(icon, ui.visuals().text_color(), BUTTON_ICON_SIZE))
        .frame(true)
        .frame_when_inactive(false)
        .min_size(Vec2::splat(BUTTON_SIZE))
}

fn button_scope<R>(ui: &mut Ui, contents: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = Vec2::splat(4.0);
        ui.spacing_mut().interact_size = Vec2::splat(BUTTON_SIZE);
        contents(ui)
    })
    .inner
}

pub fn action(ui: &mut Ui, icon: Icon, enabled: bool) -> Response {
    button_scope(ui, |ui| {
        let background = ui.painter().add(eframe::egui::Shape::Noop);
        let response = ui.add_enabled(enabled, button_widget(ui, icon));
        // Keep disabled feedback subtle; its tooltip explains the unavailable action.
        if !response.enabled() && response.contains_pointer() {
            ui.painter().set(
                background,
                eframe::egui::Shape::rect_filled(
                    response.rect,
                    ui.visuals().widgets.inactive.corner_radius,
                    ui.visuals().widgets.hovered.weak_bg_fill,
                ),
            );
        }
        response
    })
}

pub fn button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let response = action(ui, icon, true);
    response.widget_info(|| {
        eframe::egui::WidgetInfo::labeled(eframe::egui::Role::Button, true, tooltip)
    });
    response.help_text(tooltip)
}

/// An X inside a faint disc, like the JetBrains status-bar progress stop icon.
/// The disc fills the shorter side of `size` and does not grow on hover.
pub fn circled_button(
    ui: &mut Ui,
    icon: Icon,
    color: Color32,
    tooltip: &str,
    size: Vec2,
) -> Response {
    button_scope(ui, |ui| {
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let diameter = rect.width().min(rect.height());
        // expui/progress/stop.svg uses a 10% disc, and 20% while hovered.
        let alpha = if response.hovered() || response.has_focus() {
            51
        } else {
            26
        };
        let [red, green, blue, _] = color.to_array();
        ui.painter().circle_filled(
            rect.center(),
            diameter * 0.5,
            Color32::from_rgba_unmultiplied(red, green, blue, alpha),
        );
        // The cancel glyph is thicker than close.svg. Keep its stroke caps inside the disc.
        let glyph = diameter * 0.75;
        image(icon, color, glyph).paint_at(
            ui,
            Rect::from_center_size(rect.center(), Vec2::splat(glyph)),
        );
        response.widget_info(|| {
            eframe::egui::WidgetInfo::labeled(eframe::egui::Role::Button, true, tooltip)
        });
        response.help_text(tooltip)
    })
}

#[cfg(any(not(target_os = "macos"), test))]
pub fn menu(ui: &mut Ui, icon: Icon, tooltip: &str, contents: impl FnOnce(&mut Ui)) -> Response {
    let spacing = ui.spacing().clone();
    button_scope(ui, |ui| {
        let response =
            eframe::egui::containers::menu::MenuButton::from_button(button_widget(ui, icon))
                .ui(ui, |ui| {
                    *ui.spacing_mut() = spacing;
                    contents(ui);
                })
                .0;
        response.widget_info(|| {
            eframe::egui::WidgetInfo::labeled(eframe::egui::Role::Button, true, tooltip)
        });
        response.help_text(tooltip)
    })
}

pub fn unavailable(ui: &mut Ui, icon: Icon, tooltip: &str) {
    let response = action(ui, icon, false);
    response.widget_info(|| {
        eframe::egui::WidgetInfo::labeled(eframe::egui::Role::Button, false, tooltip)
    });
    response.disabled_help_text(tooltip);
}

#[cfg(test)]
mod tests {
    use super::Icon;

    #[test]
    fn icon_actions_and_menus_share_geometry_across_styles_and_states() {
        use super::*;
        use crate::theme::{install_fonts, InterfaceStyle, ThemeChoice};
        use eframe::egui::{vec2, Context, Event, Pos2, RawInput, Rect};

        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for interface in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for padding in [Vec2::ZERO, vec2(12.0, 8.0)] {
                    let ctx = Context::default();
                    install_fonts(&ctx);
                    theme.apply_style(&ctx, interface);
                    let mut pointer = Pos2::ZERO;
                    for frame in 0..4 {
                        let mut rects = Vec::new();
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    Pos2::ZERO,
                                    vec2(1100.0, 700.0),
                                )),
                                events: if frame > 0 {
                                    vec![Event::PointerMoved(pointer)]
                                } else {
                                    vec![]
                                },
                                ..Default::default()
                            },
                            |ui| {
                                ui.spacing_mut().button_padding = padding;
                                ui.spacing_mut().interact_size = vec2(40.0, 40.0);
                                ui.horizontal(|ui| {
                                    for icon in [
                                        Icon::Fetch,
                                        Icon::Plus,
                                        Icon::Sparkles,
                                        Icon::Stop,
                                        Icon::Folder,
                                        Icon::Close,
                                    ] {
                                        rects.push(action(ui, icon, true).rect);
                                        rects.push(action(ui, icon, false).rect);
                                    }
                                    rects.push(menu(ui, Icon::Chevron, "Options", |_| {}).rect);
                                });
                                assert_eq!(ui.spacing().button_padding, padding);
                                assert_eq!(ui.spacing().interact_size, vec2(40.0, 40.0));
                            },
                        );
                        output.textures_delta.clear();
                        for rect in &rects {
                            assert_eq!(
                                rect.size(),
                                Vec2::splat(BUTTON_SIZE),
                                "{theme:?} {interface:?} {padding:?}: {rect:?}"
                            );
                        }
                        pointer = rects[frame % rects.len()].center();
                    }
                }
            }
        }
    }

    #[test]
    fn file_icons_handle_case_paths_and_unknown_extensions() {
        for (path, expected) in [
            ("src/main.rs", Icon::CodeFile),
            ("web/app.TSX", Icon::CodeFile),
            ("assets/logo.svg", Icon::ImageFile),
            (r"assets\photo.JPEG", Icon::ImageFile),
            ("README.md", Icon::TextFile),
            ("Cargo.toml", Icon::TextFile),
            ("folder.png/data", Icon::File),
            (r"folder.rs\data", Icon::File),
            ("archive.unknown", Icon::File),
            ("LICENSE", Icon::File),
        ] {
            assert_eq!(Icon::for_file(path), expected, "{path}");
        }
    }
}
