//! Detachable panels: content shown in a window of its own.

use egui::{Context, Frame, Ui, ViewportBuilder, ViewportClass, ViewportId};

/// Shows `add` in its own OS window (an immediate viewport), or in a floating
/// `egui::Window` where the backend cannot open more windows. Call it every frame while
/// the window should be open; returns `false` once the user asked to close it, e.g. to
/// dock the content again.
///
/// The window's title and size come from `builder`; `frame` styles the OS window's
/// central panel. Inside `add`, `ui.ctx()` reports the input (keys, fullscreen state)
/// of this window.
pub fn show_window(
    ctx: &Context,
    id: ViewportId,
    builder: ViewportBuilder,
    frame: Frame,
    mut add: impl FnMut(&mut Ui),
) -> bool {
    let title = builder.title.clone().unwrap_or_default();
    let size = builder.inner_size.unwrap_or(egui::vec2(640.0, 480.0));
    ctx.show_viewport_immediate(id, builder, |ctx, class| {
        if class == ViewportClass::Embedded {
            let mut open = true;
            egui::Window::new(&title)
                .id(egui::Id::new(id))
                .open(&mut open)
                .default_size(size)
                .show(ctx, |ui| add(ui));
            return open;
        }
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| add(ui));
        !ctx.input(|i| i.viewport().close_requested())
    })
}
