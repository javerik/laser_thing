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

`files` implements `FileHost` (file dialogs are the application's business). See
`apps/laserline-desktop` in the repository for a complete application.

Licensed under either of Apache-2.0 or MIT, at your option.
