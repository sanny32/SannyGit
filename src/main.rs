#![windows_subsystem = "windows"]

mod app;
mod branches;
mod branding;
mod cherry_pick;
mod clone_source;
mod conflict;
mod diff;
mod diff_edit;
mod discard;
mod git;
mod history;
mod hunk;
mod llm;
mod logging;
mod menu;
mod merge;
#[cfg(test)]
mod mock_data;
mod models;
#[cfg(target_os = "macos")]
mod native_menu;
#[cfg(target_os = "linux")]
#[path = "native_window_linux.rs"]
mod native_window;
#[cfg(target_os = "macos")]
#[path = "native_window_macos.rs"]
mod native_window;
#[cfg(target_os = "windows")]
mod native_window;
mod persistence;
mod preferences;
mod rebase;
mod remote_config;
mod remotes;
mod repository;
mod revert;
mod shortcuts;
mod stash;
#[cfg(test)]
mod test_support;
mod theme;
mod tracking;
mod ui;

use app::SannyGitApp;
use eframe::egui;

/// Default client size for a new main window (logical points).
const DEFAULT_WINDOW_SIZE: egui::Vec2 = egui::Vec2::new(1536.0, 900.0);
const MIN_WINDOW_SIZE: egui::Vec2 = egui::Vec2::new(1100.0, 700.0);
/// Native frame around the egui viewport (`inner_size` is the client area only).
const WINDOW_VERTICAL_CHROME: f32 = 48.0;
const WINDOW_HORIZONTAL_CHROME: f32 = 16.0;

fn main() -> eframe::Result<()> {
    #[cfg(target_os = "windows")]
    let icon_bytes = include_bytes!("../assets/app/sannygit-windows.png").as_slice();
    #[cfg(not(target_os = "windows"))]
    let icon_bytes = include_bytes!("../assets/app/sannygit.png").as_slice();
    let icon = eframe::icon_data::from_png_bytes(icon_bytes)
        .map_err(|error| eframe::Error::AppCreation(Box::new(error)))?;
    let window_size = DEFAULT_WINDOW_SIZE;
    let viewport = theme::native_viewport()
        .with_title(branding::main_window_title(None))
        .with_app_id("io.github.sanny32.SannyGit")
        .with_icon(std::sync::Arc::new(icon))
        .with_decorations(true)
        .with_inner_size(window_size)
        .with_min_inner_size([MIN_WINDOW_SIZE.x, MIN_WINDOW_SIZE.y])
        .with_clamp_size_to_monitor_size(true);

    let options = eframe::NativeOptions {
        viewport,
        #[cfg(target_os = "macos")]
        renderer: eframe::Renderer::Wgpu,
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        renderer: eframe::Renderer::Glow,
        persist_window: true,
        persistence_path: if std::env::var_os("SANNYGIT_SECONDARY_WINDOW").is_some() {
            Some(std::env::temp_dir().join(format!(
                "sannygit-window-{}-{}.ron",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            )))
        } else {
            // Keep existing preferences when changing the native application ID.
            crate::persistence::frame_storage_path()
        },
        // The event loop has initialized native DPI handling by this point.
        // Place the window before eframe renders and reveals its first frame.
        window_builder: Some(Box::new(move |viewport| {
            initial_window_viewport(viewport, window_size)
        })),
        ..Default::default()
    };

    let result = eframe::run_native(
        "SannyGit",
        options,
        Box::new(|creation_context| {
            #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
            native_window::remember_owner(creation_context)?;
            #[allow(unused_mut)]
            let mut app = SannyGitApp::new(creation_context);
            #[cfg(target_os = "windows")]
            if let Err(error) = native_window::prepare_root_presentation(&creation_context.egui_ctx)
            {
                app.error = Some(error.to_string());
            }
            Ok(Box::new(app))
        }),
    );
    if result.is_ok() {
        logging::record(logging::Level::Info, format_args!("Application closed"));
    } else {
        logging::record(
            logging::Level::Error,
            format_args!("Application terminated with an error"),
        );
    }
    logging::shutdown();
    result
}

fn initial_window_viewport(
    viewport: egui::ViewportBuilder,
    window_size: egui::Vec2,
) -> egui::ViewportBuilder {
    // eframe inserts the persisted position before applying this hook.
    if viewport.position.is_some() {
        return viewport;
    }
    if let Some((position, fitted_size)) = cursor_monitor_window_geometry(window_size) {
        viewport
            .with_inner_size(fitted_size)
            .with_position(position)
    } else {
        viewport
    }
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
    let (area_x, area_y, area_width, area_height) = usable_display_area(&display);
    // Viewport positions, including the monitor origin, are in logical points.
    let area_x = area_x as f32 / scale;
    let area_y = area_y as f32 / scale;
    let area_width = area_width as f32 / scale;
    let area_height = area_height as f32 / scale;
    let min_size = MIN_WINDOW_SIZE;
    let (position, size) = centered_window_geometry(
        window_size,
        area_x,
        area_y,
        area_width,
        area_height,
        min_size,
    );

    Some((position, size))
}

fn centered_window_geometry(
    window_size: egui::Vec2,
    area_x: f32,
    area_y: f32,
    area_width: f32,
    area_height: f32,
    min_size: egui::Vec2,
) -> (egui::Pos2, egui::Vec2) {
    let available_width = (area_width - WINDOW_HORIZONTAL_CHROME).max(0.0);
    let available_height = (area_height - WINDOW_VERTICAL_CHROME).max(0.0);
    let width = window_size.x.min(available_width).max(min_size.x);
    let height = window_size.y.min(available_height).max(min_size.y);
    let outer_width = width + WINDOW_HORIZONTAL_CHROME;
    let outer_height = height + WINDOW_VERTICAL_CHROME;
    (
        egui::pos2(
            area_x + (area_width - outer_width) / 2.0,
            area_y + (area_height - outer_height) / 2.0,
        ),
        egui::vec2(width, height),
    )
}

fn usable_display_area(display: &display_info::DisplayInfo) -> (i32, i32, u32, u32) {
    #[cfg(target_os = "windows")]
    {
        windows_work_area(display)
            .unwrap_or_else(|| (display.x, display.y, display.width, display.height))
    }
    #[cfg(not(target_os = "windows"))]
    {
        (display.x, display.y, display.width, display.height)
    }
}

#[cfg(target_os = "windows")]
fn windows_work_area(display: &display_info::DisplayInfo) -> Option<(i32, i32, u32, u32)> {
    let mut info = MonitorInfo {
        size: std::mem::size_of::<MonitorInfo>() as u32,
        monitor: Rect::default(),
        work: Rect::default(),
        flags: 0,
    };
    let monitor = display.raw_handle.0 as isize;
    let ok = unsafe { GetMonitorInfoW(monitor, &raw mut info) };
    if ok == 0 {
        return None;
    }
    let work = info.work;
    let width = work.right.saturating_sub(work.left) as u32;
    let height = work.bottom.saturating_sub(work.top) as u32;
    if width == 0 || height == 0 {
        return None;
    }
    Some((work.left, work.top, width, height))
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct MonitorInfo {
    size: u32,
    monitor: Rect,
    work: Rect,
    flags: u32,
}

#[cfg(target_os = "windows")]
#[allow(clashing_extern_declarations)]
#[link(name = "user32")]
extern "system" {
    fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfo) -> i32;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_stays_inside_work_area_without_taskbar() {
        let (position, size) = centered_window_geometry(
            DEFAULT_WINDOW_SIZE,
            0.0,
            0.0,
            1920.0,
            1040.0,
            MIN_WINDOW_SIZE,
        );
        assert_eq!(size, DEFAULT_WINDOW_SIZE);
        let outer_bottom = position.y + size.y + WINDOW_VERTICAL_CHROME;
        assert!(outer_bottom <= 1040.0 + f32::EPSILON);
    }

    #[test]
    fn restored_window_geometry_is_not_recentered() {
        let position = egui::pos2(120.0, 80.0);
        let size = egui::vec2(1280.0, 800.0);
        let viewport = egui::ViewportBuilder::default()
            .with_position(position)
            .with_inner_size(size)
            .with_maximized(true);

        let restored = initial_window_viewport(viewport, DEFAULT_WINDOW_SIZE);

        assert_eq!(restored.position, Some(position));
        assert_eq!(restored.inner_size, Some(size));
        assert_eq!(restored.maximized, Some(true));
    }
}
