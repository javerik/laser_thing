//! The laser line: a full-viewport pass running `laser.wgsl`, which evaluates every
//! pixel analytically, so the line can be drawn into any viewport rectangle.

use laserline_core::DEFORM_SAMPLES;
use laserline_core::gpu_types::Uniforms;

/// Texels per row of the deformation texture; `DEFORM_SAMPLES` must be a multiple.
/// Must match `DEFORM_WIDTH` in `laser.wgsl`. WebGL2 guarantees 2048.
const DEFORM_WIDTH: u32 = 1024;
const DEFORM_ROWS: u32 = DEFORM_SAMPLES as u32 / DEFORM_WIDTH;
const _: () = assert!((DEFORM_SAMPLES as u32).is_multiple_of(DEFORM_WIDTH));

pub struct LaserRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    deform: wgpu::Texture,
}

impl LaserRenderer {
    /// `format` is the format of the target the line is painted into.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("laser.wgsl"));

        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("laser uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let deform = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("laser deformation"),
            size: wgpu::Extent3d { width: DEFORM_WIDTH, height: DEFORM_ROWS, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("laser bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let deform_view = deform.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("laser bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&deform_view) },
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

    /// Uploads the frame's uniforms and, if given, fresh deformation samples
    /// (`(displacement px, intensity)`, at most `DEFORM_SAMPLES`).
    pub fn prepare(&self, queue: &wgpu::Queue, uniforms: &Uniforms, deform: Option<&[[f32; 2]]>) {
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(uniforms));
        if let Some(d) = deform {
            let n = d.len().min(DEFORM_SAMPLES);
            let full_rows = (n / DEFORM_WIDTH as usize) as u32;
            let row_bytes = DEFORM_WIDTH * size_of::<[f32; 2]>() as u32;
            let write = |data: &[[f32; 2]], y: u32, width: u32, rows: u32| {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.deform,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: 0, y, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    bytemuck::cast_slice(data),
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row_bytes), rows_per_image: None },
                    wgpu::Extent3d { width, height: rows, depth_or_array_layers: 1 },
                );
            };
            let split = full_rows as usize * DEFORM_WIDTH as usize;
            if full_rows > 0 {
                write(&d[..split], 0, DEFORM_WIDTH, full_rows);
            }
            if split < n {
                write(&d[split..n], full_rows, (n - split) as u32, 1);
            }
        }
    }

    /// Draws the line into the pass's current viewport.
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}
