//! GPU side: a full-viewport pass running `laser.wgsl`, hooked into egui as a
//! paint callback so the live view can be any egui rect.

use eframe::{egui_wgpu, wgpu};

/// Number of deformation samples uploaded per frame.
pub const DEFORM_SAMPLES: usize = 8192;

/// Must match `struct Uniforms` in `laser.wgsl` (16-byte groups).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub center: [f32; 2],
    pub dir: [f32; 2],
    pub length: f32,
    pub width: f32,
    pub intensity: f32,
    pub end_soft: f32,
    pub color: [f32; 4],
    pub profile: u32,
    pub edge_soft: f32,
    pub deform_start: f32,
    pub deform_step: f32,
    pub ambient: f32,
    pub speckle: f32,
    pub grain: f32,
    pub noise: f32,
    pub seed: u32,
    pub deform_count: u32,
    pub srgb_out: u32,
    pub _pad: u32,
}

pub struct LaserResources {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    deform: wgpu::Buffer,
}

impl LaserResources {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("laser.wgsl"));

        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("laser uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let deform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("laser deformation"),
            size: (DEFORM_SAMPLES * size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let buffer_entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("laser bind group layout"),
            entries: &[
                buffer_entry(0, wgpu::BufferBindingType::Uniform),
                buffer_entry(1, wgpu::BufferBindingType::Storage { read_only: true }),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("laser bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: deform.as_entire_binding() },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("laser pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("laser pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self { pipeline, bind_group, uniforms, deform }
    }
}

/// Per-frame draw request carrying the uniforms and (optionally) fresh deformation samples.
pub struct LaserCallback {
    pub uniforms: Uniforms,
    pub deform: Option<Vec<f32>>,
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
        if let Some(r) = resources.get::<LaserResources>() {
            queue.write_buffer(&r.uniforms, 0, bytemuck::bytes_of(&self.uniforms));
            if let Some(d) = &self.deform {
                queue.write_buffer(&r.deform, 0, bytemuck::cast_slice(d));
            }
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: eframe::egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(r) = resources.get::<LaserResources>() {
            render_pass.set_pipeline(&r.pipeline);
            render_pass.set_bind_group(0, &r.bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}
