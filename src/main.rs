mod app;
mod model;
mod params;
mod presets;
mod renderer;
mod ui;

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
        Box::new(|cc| Ok(Box::new(app::LaserApp::new(cc)?))),
    )
}
