mod app;
mod files;
mod laser_ui;
mod mesh_view;
mod native_files;
mod renderer;
mod slicer_ui;
mod worker;

use eframe::egui;

pub const APP_ID: &str = "laser-line-emulator";

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Laser Line Emulator")
            .with_app_id(APP_ID)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([320.0, 240.0]),
        renderer: eframe::Renderer::Wgpu,
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "Laser Line Emulator",
        options,
        Box::new(|cc| {
            let mut app = app::LaserApp::new(cc)?;
            // Optional model or scan to open, e.g. from a file manager.
            if let Some(path) = std::env::args_os().nth(1) {
                app.open_path(std::path::Path::new(&path));
            }
            Ok(Box::new(app))
        }),
    )
}
