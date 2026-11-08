//! Render the editable app icon using the application's existing SVG renderer.
use eframe::{egui::IconData, icon_data::IconDataExt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let image = egui_extras::image::load_svg_bytes(
        include_bytes!("../assets/app/sannygit.svg"),
        &Default::default(),
    )?;
    let icon = IconData {
        width: image.size[0] as u32,
        height: image.size[1] as u32,
        rgba: image
            .pixels
            .iter()
            .flat_map(|pixel| pixel.to_srgba_unmultiplied())
            .collect(),
    };
    std::fs::write("assets/app/sannygit-1024.png", icon.to_png_bytes()?)?;
    Ok(())
}
