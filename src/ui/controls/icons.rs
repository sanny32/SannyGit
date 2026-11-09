use super::tooltips::TooltipResponse;
use eframe::egui::{Button, Color32, Image, Response, Ui, Vec2};

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
    CodeFile,
    TextFile,
    Folder,
    ImageFile,
    Settings,
    Chevron,
    Plus,
    List,
    More,
    Check,
    Close,
    Stop,
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
            Self::CodeFile => svg!("code-file"),
            Self::TextFile => svg!("text-file"),
            Self::Folder => svg!("folder"),
            Self::ImageFile => svg!("image-file"),
            Self::Settings => svg!("settings"),
            Self::Chevron => svg!("chevron"),
            Self::Plus => svg!("plus"),
            Self::List => svg!("list"),
            Self::More => svg!("more"),
            Self::Check => svg!("check"),
            Self::Close => svg!("close"),
            Self::Stop => svg!("stop"),
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
    action(ui, icon, true).help_text(tooltip)
}

pub fn menu(ui: &mut Ui, icon: Icon, tooltip: &str, contents: impl FnOnce(&mut Ui)) -> Response {
    let spacing = ui.spacing().clone();
    button_scope(ui, |ui| {
        eframe::egui::containers::menu::MenuButton::from_button(button_widget(ui, icon))
            .ui(ui, |ui| {
                *ui.spacing_mut() = spacing;
                contents(ui);
            })
            .0
            .help_text(tooltip)
    })
}

pub fn unavailable(ui: &mut Ui, icon: Icon, tooltip: &str) {
    action(ui, icon, false).disabled_help_text(tooltip);
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
