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

pub fn button(ui: &mut Ui, icon: Icon, tooltip: &str, size: Vec2) -> Response {
    ui.add(
        Button::image(image(icon, ui.visuals().text_color(), 14.0))
            .frame(false)
            .min_size(size),
    )
    .help_text(tooltip)
}

pub fn unavailable(ui: &mut Ui, icon: Icon, tooltip: &str) {
    let response = ui.add_enabled(
        false,
        Button::image(image(icon, ui.visuals().text_color(), 14.0))
            .frame(false)
            .min_size(Vec2::splat(20.0)),
    );
    response.disabled_help_text(tooltip);
}

#[cfg(test)]
mod tests {
    use super::Icon;

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
