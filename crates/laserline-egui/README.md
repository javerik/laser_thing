# laserline-egui

egui frontend of [laserline](https://git.javerik.dev/opensource/laserthing) for native
applications: the live laser view, the control panel and the STL slicer with 3D preview,
range image and profile plot.

```rust
// Once, with eframe's wgpu render state:
laserline_egui::LaserUi::install_gpu(cc.wgpu_render_state.as_ref().unwrap());
let mut ui = laserline_egui::LaserUi::new(Default::default(), Default::default(), true, srgb_target);

// Every frame:
ui.begin_frame(ctx, dt);
egui::SidePanel::right("controls").show(ctx, |p| ui.controls_ui(p, &mut files));
egui::CentralPanel::default().show(ctx, |p| {
    let rect = p.max_rect();
    ui.live_view(p, rect);
});
```

The live view can be detached into a window of its own and docked again; keep a flag and
use `show_window` (it falls back to a floating egui window where the backend has no
multi-viewport support):

```rust
if view_detached {
    let builder = egui::ViewportBuilder::default().with_title("Live view").with_inner_size([1280.0, 720.0]);
    view_detached = laserline_egui::show_window(ctx, egui::ViewportId::from_hash_of("live"), builder,
        egui::Frame::NONE, |p| {
            let rect = p.max_rect();
            ui.live_view(p, rect);
        });
}
```

`files` implements `FileHost` (file dialogs are the application's business). See
`apps/laserline-desktop` in the repository for a complete application.

Licensed under either of Apache-2.0 or MIT, at your option.
