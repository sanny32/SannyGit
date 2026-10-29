use eframe::egui::{Button, Color32, Image, Response, Ui, Vec2};

#[derive(Clone, Copy)]
pub enum Icon {
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
    Crab,
    Folder,
    ImageFile,
    Search,
    Settings,
    Chevron,
    Plus,
    List,
    More,
    Check,
    Close,
    Star,
    Cloud,
}

impl Icon {
    fn source(self) -> (&'static str, &'static [u8]) {
        macro_rules! svg {
            ($name:literal) => {
                (
                    $name,
                    include_bytes!(concat!("../../assets/icons/", $name, ".svg")),
                )
            };
        }
        match self {
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
            Self::Crab => svg!("crab"),
            Self::Folder => svg!("folder"),
            Self::ImageFile => svg!("image-file"),
            Self::Search => svg!("search"),
            Self::Settings => svg!("settings"),
            Self::Chevron => svg!("chevron"),
            Self::Plus => svg!("plus"),
            Self::List => svg!("list"),
            Self::More => svg!("more"),
            Self::Check => svg!("check"),
            Self::Close => svg!("close"),
            Self::Star => svg!("star"),
            Self::Cloud => svg!("cloud"),
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
        Button::image(image(icon, ui.visuals().text_color(), 19.0))
            .frame(false)
            .min_size(size),
    )
    .on_hover_text(tooltip)
}

pub fn unavailable(ui: &mut Ui, icon: Icon, tooltip: &str) {
    let response = ui.add_enabled(
        false,
        Button::image(image(icon, ui.visuals().text_color(), 18.0))
            .frame(false)
            .min_size(Vec2::splat(28.0)),
    );
    response.on_disabled_hover_text(tooltip);
}
