#![windows_subsystem = "windows"]

mod app;
mod diff;
mod mock_data;
mod models;
mod theme;
mod ui;

use app::SannyGitApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SannyGit")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1100.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "SannyGit",
        options,
        Box::new(|creation_context| Ok(Box::new(SannyGitApp::new(creation_context)))),
    )
}
