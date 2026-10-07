//! Renderers bound to a `<canvas>` (or an `OffscreenCanvas`): WebGPU where available,
//! WebGL2 otherwise.

use crate::js_err;
use crate::state::{Emulator, Slicer};
use crate::to_js;
use laserline_render::{LaserRenderer, MeshViewInput, MeshViewRenderer};
use serde::Serialize;
use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, OffscreenCanvas};

/// Background of the 3D view where there is nothing to show (matches its clear colour).
const EMPTY_3D: wgpu::Color = wgpu::Color { r: 0.07, g: 0.075, b: 0.085, a: 1.0 };

/// A canvas surface with its own device (WebGL2 contexts cannot be shared).
struct Target {
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

impl Target {
    /// `backend`: "webgpu", "webgl" or anything else for automatic choice. `size` is the
    /// canvas' drawing buffer size.
    async fn new(
        canvas: wgpu::SurfaceTarget<'static>,
        (width, height): (u32, u32),
        backend: Option<String>,
    ) -> Result<Self, JsError> {
        let backends = match backend.as_deref() {
            Some("webgpu") => wgpu::Backends::BROWSER_WEBGPU,
            Some("webgl") => wgpu::Backends::GL,
            _ => wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
        };
        let (width, height) = (width.max(1), height.max(1));
        let desc = wgpu::InstanceDescriptor { backends, ..Default::default() };
        let instance = wgpu::util::new_instance_with_webgpu_detection(&desc).await;
        let surface = instance.create_surface(canvas).map_err(js_err)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })
            .await
            .map_err(js_err)?;
        let required_limits = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor { label: Some("laserline"), required_limits, ..Default::default() })
            .await
            .map_err(js_err)?;
        let caps = surface.get_capabilities(&adapter);
        // A plain (non-sRGB) target keeps pixel values exactly as computed.
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).or(caps.formats.first().copied()).ok_or_else(|| JsError::new("canvas offers no surface format"))?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        Ok(Self { surface, adapter, device, queue, config })
    }

    fn backend(&self) -> String {
        match self.adapter.get_info().backend {
            wgpu::Backend::BrowserWebGpu => "WebGPU".into(),
            wgpu::Backend::Gl => "WebGL2".into(),
            b => format!("{b:?}"),
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        let max = self.device.limits().max_texture_dimension_2d;
        let (w, h) = (width.clamp(1, max), height.clamp(1, max));
        if (w, h) != (self.config.width, self.config.height) {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn size(&self) -> [u32; 2] {
        [self.config.width, self.config.height]
    }

    /// Records `draw` into a pass on the canvas cleared to `clear` and presents. Skips
    /// the frame if the surface is unavailable.
    fn frame(&self, clear: wgpu::Color, draw: impl FnOnce(&mut wgpu::RenderPass<'_>)) {
        let texture = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(_) => return,
        };
        let view = texture.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            draw(&mut pass);
        }
        self.queue.submit([encoder.finish()]);
        texture.present();
    }
}

/// Draws an `Emulator`'s laser line over the whole canvas.
#[wasm_bindgen]
pub struct LaserView {
    target: Target,
    renderer: LaserRenderer,
}

#[wasm_bindgen]
impl LaserView {
    /// Binds to `canvas`. `backend`: `"webgpu"`, `"webgl"` or omitted for automatic.
    pub async fn create(canvas: HtmlCanvasElement, backend: Option<String>) -> Result<LaserView, JsError> {
        let size = (canvas.width(), canvas.height());
        Ok(Self::with_target(Target::new(wgpu::SurfaceTarget::Canvas(canvas), size, backend).await?))
    }

    /// Binds to an `OffscreenCanvas` created in this window, e.g. to show the frames in
    /// another window via `transferToImageBitmap()`. `backend` as for `create`.
    #[wasm_bindgen(js_name = createOffscreen)]
    pub async fn create_offscreen(canvas: OffscreenCanvas, backend: Option<String>) -> Result<LaserView, JsError> {
        let size = (canvas.width(), canvas.height());
        Ok(Self::with_target(Target::new(wgpu::SurfaceTarget::OffscreenCanvas(canvas), size, backend).await?))
    }

    fn with_target(target: Target) -> LaserView {
        let renderer = LaserRenderer::new(&target.device, target.config.format);
        LaserView { target, renderer }
    }

    /// `"WebGPU"` or `"WebGL2"`.
    #[wasm_bindgen(getter)]
    pub fn backend(&self) -> String {
        self.target.backend()
    }

    /// Resizes the drawing buffer (device pixels).
    pub fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    /// Draws the current frame of `emulator`; returns its measurements.
    #[wasm_bindgen(unchecked_return_type = "LiveInfo")]
    pub fn render(&mut self, emulator: &Emulator) -> JsValue {
        let [w, h] = self.target.size();
        let frame = emulator.inner.frame([0.0, 0.0], [w as f32, h as f32], self.target.config.format.is_srgb());
        self.renderer.prepare(&self.target.queue, &frame.uniforms, frame.deform.as_deref());
        self.target.frame(wgpu::Color::BLACK, |pass| self.renderer.paint(pass));
        to_js(&frame.info)
    }
}

#[derive(Serialize)]
struct ViewLabel {
    text: &'static str,
    color: [u8; 3],
    x: f32,
    y: f32,
}

/// Draws a `Slicer`'s 3D view: the model (or the scan surface), conveyor, laser plane,
/// camera and the emulator's current profile.
#[wasm_bindgen]
pub struct MeshView {
    target: Target,
    renderer: MeshViewRenderer,
}

#[wasm_bindgen]
impl MeshView {
    /// Binds to `canvas`. `backend`: `"webgpu"`, `"webgl"` or omitted for automatic.
    pub async fn create(canvas: HtmlCanvasElement, backend: Option<String>) -> Result<MeshView, JsError> {
        let size = (canvas.width(), canvas.height());
        Ok(Self::with_target(Target::new(wgpu::SurfaceTarget::Canvas(canvas), size, backend).await?))
    }

    /// Binds to an `OffscreenCanvas` created in this window, e.g. to show the frames in
    /// another window via `transferToImageBitmap()`. `backend` as for `create`.
    #[wasm_bindgen(js_name = createOffscreen)]
    pub async fn create_offscreen(canvas: OffscreenCanvas, backend: Option<String>) -> Result<MeshView, JsError> {
        let size = (canvas.width(), canvas.height());
        Ok(Self::with_target(Target::new(wgpu::SurfaceTarget::OffscreenCanvas(canvas), size, backend).await?))
    }

    fn with_target(target: Target) -> MeshView {
        let samples = MeshViewRenderer::supported_samples(&target.adapter);
        let renderer = MeshViewRenderer::new(&target.device, target.config.format, samples);
        MeshView { target, renderer }
    }

    #[wasm_bindgen(getter)]
    pub fn backend(&self) -> String {
        self.target.backend()
    }

    /// Resizes the drawing buffer (device pixels).
    pub fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    /// Draws the slicer's scene with the emulator's scan and current profile. Returns
    /// the labels to place over the view, or an empty list when there is nothing to
    /// show yet.
    #[wasm_bindgen(unchecked_return_type = "ViewLabel[]")]
    pub fn render(&mut self, slicer: &mut Slicer, emulator: &Emulator) -> JsValue {
        let scan = emulator.inner.scan.clone();
        let row = emulator.inner.current_row();
        slicer.inner.update_preview(scan.as_ref());
        let Some(view) = slicer.inner.view_3d(scan.as_deref(), row, false) else {
            self.target.frame(EMPTY_3D, |_| {});
            return to_js(&Vec::<ViewLabel>::new());
        };
        let [w, h] = self.target.size();
        let aspect = w as f32 / h as f32;
        let input = MeshViewInput {
            mesh: view.mesh.as_deref(),
            overlay: &view.overlay.overlay,
            uniforms: view.camera.uniforms(aspect, view.z_max),
            size_px: [w, h],
            srgb_target: self.target.config.format.is_srgb(),
        };
        let t = &self.target;
        let mut encoder = t.device.create_command_encoder(&Default::default());
        self.renderer.prepare(&t.device, &t.queue, &mut encoder, &input);
        t.queue.submit([encoder.finish()]);
        t.frame(EMPTY_3D, |pass| self.renderer.paint(pass));
        let labels: Vec<_> = view
            .overlay
            .labels
            .iter()
            .filter_map(|l| view.camera.project(aspect, l.pos).map(|[x, y]| ViewLabel { text: l.text, color: l.color, x, y }))
            .collect();
        to_js(&labels)
    }
}
