# laserline-render

wgpu renderers of [laserline](https://git.javerik.dev/opensource/laserthing), independent
of any UI toolkit:

- `LaserRenderer`: the emulated laser line, evaluated per pixel from the
  `Uniforms` of `laserline_core::emulator::Emulator::frame`.
- `MeshViewRenderer`: the slicer's 3D preview, rendered offscreen with MSAA and blitted
  into your pass.

Both split work into `prepare` (uploads, offscreen passes) and `paint` (draws into a
render pass you own), which fits egui paint callbacks as well as plain render loops. The
shaders use no storage buffers or compute, so they run on WebGL2 as well as WebGPU,
Vulkan, Metal and DX12. Backends are chosen by the application through wgpu's features.

Licensed under either of Apache-2.0 or MIT, at your option.
