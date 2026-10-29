use eframe::egui::{Color32, Image, Ui, Vec2};

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
}

impl Icon {
    fn source(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Pull => ("pull", include_bytes!("../../assets/icons/pull.svg")),
            Self::Push => ("push", include_bytes!("../../assets/icons/push.svg")),
            Self::Fetch => ("fetch", include_bytes!("../../assets/icons/fetch.svg")),
            Self::Branch => ("branch", include_bytes!("../../assets/icons/branch.svg")),
            Self::Merge => ("merge", include_bytes!("../../assets/icons/merge.svg")),
            Self::Rebase => ("rebase", include_bytes!("../../assets/icons/rebase.svg")),
            Self::Stash => ("stash", include_bytes!("../../assets/icons/stash.svg")),
            Self::Discard => ("discard", include_bytes!("../../assets/icons/discard.svg")),
            Self::Repository => (
                "repository",
                include_bytes!("../../assets/icons/repository.svg"),
            ),
            Self::Tag => ("tag", include_bytes!("../../assets/icons/tag.svg")),
            Self::File => ("file", include_bytes!("../../assets/icons/file.svg")),
            Self::Crab => ("crab", include_bytes!("../../assets/icons/crab.svg")),
        }
    }
}

pub fn paint(ui: &mut Ui, icon: Icon, color: Color32, size: f32) {
    let (name, bytes) = icon.source();
    let uri = format!("bytes://icons/{name}.svg");
    ui.add(
        Image::from_bytes(uri, bytes)
            .fit_to_exact_size(Vec2::splat(size))
            .tint(color),
    );
}
