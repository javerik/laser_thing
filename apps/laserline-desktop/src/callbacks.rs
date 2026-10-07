//! egui paint callbacks drawing the laserline renderers inside egui rects.

use eframe::{egui, egui_wgpu, wgpu};
use laserline_core::gpu_types::{GpuMesh, Overlay, Uniforms, ViewUniforms};
use laserline_render::{LaserRenderer, MeshViewInput, MeshViewRenderer};
use std::sync::Arc;

/// MSAA samples of the 3D preview; every native backend supports 4.
const MESH_VIEW_SAMPLES: u32 = 4;

/// Registers the renderers with egui's wgpu renderer; needed once before the
/// callbacks can draw.
pub fn install(rs: &egui_wgpu::RenderState) {
    let mut renderer = rs.renderer.write();
    renderer.callback_resources.insert(LaserRenderer::new(&rs.device, rs.target_format));
    renderer.callback_resources.insert(MeshViewRenderer::new(&rs.device, rs.target_format, MESH_VIEW_SAMPLES));
}

/// Per-frame draw request for the laser line.
pub struct LaserCallback {
    pub uniforms: Uniforms,
    /// Fresh deformation samples, if any.
    pub deform: Option<Vec<[f32; 2]>>,
}

impl egui_wgpu::CallbackTrait for LaserCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(r) = resources.get::<LaserRenderer>() {
            r.prepare(queue, &self.uniforms, self.deform.as_deref());
        }
        Vec::new()
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, render_pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        if let Some(r) = resources.get::<LaserRenderer>() {
            r.paint(render_pass);
        }
    }
}

/// Per-frame draw request for the 3D preview.
pub struct MeshViewCallback {
    pub mesh: Option<Arc<GpuMesh>>,
    pub overlay: Arc<Overlay>,
    pub uniforms: ViewUniforms,
    /// Size of the view in physical pixels.
    pub size_px: [u32; 2],
    pub srgb_target: bool,
}

impl egui_wgpu::CallbackTrait for MeshViewCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(r) = resources.get_mut::<MeshViewRenderer>() {
            let input = MeshViewInput {
                mesh: self.mesh.as_deref(),
                overlay: &self.overlay,
                uniforms: self.uniforms,
                size_px: self.size_px,
                srgb_target: self.srgb_target,
            };
            r.prepare(device, queue, encoder, &input);
        }
        Vec::new()
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, render_pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        if let Some(r) = resources.get::<MeshViewRenderer>() {
            r.paint(render_pass);
        }
    }
}
