# laserline-render

wgpu renderers of [laserline](https://git.javerik.dev/opensource/laserthing), independent
of any UI toolkit:

- `LaserRenderer`: the emulated laser line, evaluated per pixel from the `Uniforms` of
  `laserline_core::emulator::Emulator::frame`.
- `MeshViewRenderer`: the slicer's 3D preview, rendered offscreen with MSAA and blitted
  into your pass (inputs from `laserline_core::slicer::Slicer::view_3d`).

Both split work into `prepare` (uploads, offscreen passes) and `paint` (draws into a
render pass you own), which fits egui paint callbacks as well as plain render loops. The
shaders use no storage buffers or compute, so they run on WebGL2 as well as WebGPU,
Vulkan, Metal and DX12. Backends are chosen by the application through wgpu's features.

```rust,no_run
use laserline_core::emulator::Emulator;
use laserline_render::{LaserRenderer, wgpu};

fn draw(
    renderer: &LaserRenderer, // LaserRenderer::new(&device, format), once
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    emulator: &Emulator,
    size_px: [f32; 2],
    srgb_target: bool, // format.is_srgb()
) {
    let frame = emulator.frame([0.0, 0.0], size_px, srgb_target);
    renderer.prepare(queue, &frame.uniforms, frame.deform.as_deref());
    renderer.paint(pass);
}
```

Licensed under either of Apache-2.0 or MIT, at your option.
