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
    let window_size = egui::vec2(1536.0, 1024.0);
    let viewport = egui::ViewportBuilder::default()
        .with_title("SannyGit")
        .with_decorations(true)
        .with_inner_size(window_size)
        .with_min_inner_size([1100.0, 700.0])
        .with_clamp_size_to_monitor_size(true);

    let options = eframe::NativeOptions {
        viewport,
        persist_window: false,
        // The event loop has initialized native DPI handling by this point.
        // Place the window before eframe renders and reveals its first frame.
        window_builder: Some(Box::new(move |viewport| {
            if let Some((position, monitor_size)) = cursor_monitor_window_geometry(window_size) {
                viewport
                    .with_inner_size(window_size.min(monitor_size))
                    .with_position(position)
            } else {
                viewport
            }
        })),
        ..Default::default()
    };

    eframe::run_native(
        "SannyGit",
        options,
        Box::new(|creation_context| Ok(Box::new(SannyGitApp::new(creation_context)))),
    )
}

fn cursor_monitor_window_geometry(window_size: egui::Vec2) -> Option<(egui::Pos2, egui::Vec2)> {
    use display_info::DisplayInfo;
    use mouse_position::mouse_position::Mouse;

    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() || std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return None;
    }

    let Mouse::Position { x, y } = Mouse::get_mouse_position() else {
        return None;
    };
    let display = DisplayInfo::all().ok()?.into_iter().find(|display| {
        #[cfg(target_os = "linux")]
        let (x, y) = (
            (x as f32 / display.scale_factor) as i32,
            (y as f32 / display.scale_factor) as i32,
        );

        x >= display.x
            && x < display.x + display.width as i32
            && y >= display.y
            && y < display.y + display.height as i32
    })?;

    #[cfg(target_os = "windows")]
    let scale = windows_monitor_scale_factor(&display);
    #[cfg(not(target_os = "windows"))]
    let scale = 1.0;
    let scale = if scale > 0.0 { scale } else { 1.0 };
    // Viewport positions, including the monitor origin, are in logical points.
    let display_x = display.x as f32 / scale;
    let display_y = display.y as f32 / scale;
    let display_width = display.width as f32 / scale;
    let display_height = display.height as f32 / scale;
    let width = window_size.x.min(display_width);
    let height = window_size.y.min(display_height);

    Some((
        egui::pos2(
            display_x + (display_width - width) / 2.0,
            display_y + (display_height - height) / 2.0,
        ),
        egui::vec2(display_width, display_height),
    ))
}

#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
#[link(name = "shcore")]
extern "system" {
    fn GetDpiForMonitor(
        monitor: *mut std::ffi::c_void,
        dpi_type: i32,
        dpi_x: *mut u32,
        dpi_y: *mut u32,
    ) -> i32;
}

#[cfg(target_os = "windows")]
fn windows_monitor_scale_factor(display: &display_info::DisplayInfo) -> f32 {
    let mut dpi_x = 0;
    let mut dpi_y = 0;
    let result = unsafe { GetDpiForMonitor(display.raw_handle.0, 0, &mut dpi_x, &mut dpi_y) };

    if result == 0 && dpi_x > 0 {
        dpi_x as f32 / 96.0
    } else if display.scale_factor > 0.0 {
        display.scale_factor
    } else {
        1.0
    }
}
