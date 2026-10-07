//! 3D preview of the object being sliced. Rendered offscreen (MSAA + depth) during
//! `prepare`, then blitted into the caller's pass, so it works inside any viewport.

use laserline_core::gpu_types::{GpuMesh, LineVertex, Overlay, ViewUniforms};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const BACKGROUND: wgpu::Color = wgpu::Color { r: 0.07, g: 0.075, b: 0.085, a: 1.0 };

struct Targets {
    size: [u32; 2],
    /// `None` without multisampling: then the scene renders straight into `resolved`.
    msaa: Option<wgpu::TextureView>,
    resolved: wgpu::TextureView,
    depth: wgpu::TextureView,
    blit_bind_group: wgpu::BindGroup,
}

struct MeshBuffers {
    id: u64,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

/// Everything drawn in one frame of the 3D preview.
pub struct MeshViewInput<'a> {
    /// The object; uploaded again only when its `id` changes.
    pub mesh: Option<&'a GpuMesh>,
    pub overlay: &'a Overlay,
    pub uniforms: ViewUniforms,
    /// Size of the view in physical pixels.
    pub size_px: [u32; 2],
    /// The paint target encodes sRGB itself.
    pub srgb_target: bool,
}

pub struct MeshViewRenderer {
    samples: u32,
    mesh_pipeline: wgpu::RenderPipeline,
    tri_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    line_top_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    view_bind_group: wgpu::BindGroup,
    blit_layout: wgpu::BindGroupLayout,
    view_uniforms: wgpu::Buffer,
    blit_uniforms: wgpu::Buffer,
    sampler: wgpu::Sampler,
    overlay: wgpu::Buffer,
    mesh: Option<MeshBuffers>,
    targets: Option<Targets>,
}

impl MeshViewRenderer {
    /// Highest MSAA sample count up to 4 that the adapter supports for the offscreen
    /// targets (WebGL2 may offer less).
    pub fn supported_samples(adapter: &wgpu::Adapter) -> u32 {
        [4, 2, 1]
            .into_iter()
            .find(|&n| {
                [COLOR_FORMAT, DEPTH_FORMAT]
                    .iter()
                    .all(|&f| adapter.get_texture_format_features(f).flags.sample_count_supported(n))
            })
            .unwrap_or(1)
    }

    /// `target_format` is the format of the pass `paint` draws into; `samples` the
    /// MSAA sample count of the offscreen render (see [`Self::supported_samples`]).
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat, samples: u32) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("mesh.wgsl"));

        let view_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh view uniforms"),
            size: size_of::<ViewUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let blit_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh blit uniforms"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_entry = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh view layout"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let view_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh view bind group"),
            layout: &view_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: view_uniforms.as_entire_binding() }],
        });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh blit layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                uniform_entry(3, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mesh blit sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let view_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh pipeline layout"),
            bind_group_layouts: &[&view_layout],
            push_constant_ranges: &[],
        });
        let line_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
        let line_layout = wgpu::VertexBufferLayout {
            array_stride: size_of::<LineVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &line_attrs,
        };
        let mesh_attrs = wgpu::vertex_attr_array![0 => Float32x3];
        let mesh_layout = wgpu::VertexBufferLayout {
            array_stride: size_of::<[f32; 3]>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &mesh_attrs,
        };

        let offscreen = |label: &str,
                         vs: &str,
                         fs: &str,
                         buffers: &[wgpu::VertexBufferLayout],
                         topology: wgpu::PrimitiveTopology,
                         blend: Option<wgpu::BlendState>,
                         depth_write: bool,
                         depth_compare: wgpu::CompareFunction| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&view_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState { topology, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: depth_write,
                    depth_compare,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
                multiview: None,
                cache: None,
            })
        };
        use wgpu::{CompareFunction as Cmp, PrimitiveTopology as Topo};
        let alpha = Some(wgpu::BlendState::ALPHA_BLENDING);
        let mesh_pipeline = offscreen("mesh", "vs_mesh", "fs_mesh", &[mesh_layout], Topo::TriangleList, None, true, Cmp::Less);
        let tri_pipeline = offscreen("mesh overlay tris", "vs_line", "fs_line", std::slice::from_ref(&line_layout), Topo::TriangleList, alpha, false, Cmp::Less);
        let line_pipeline = offscreen("mesh overlay lines", "vs_line", "fs_line", std::slice::from_ref(&line_layout), Topo::LineList, alpha, false, Cmp::LessEqual);
        let line_top_pipeline = offscreen("mesh overlay top lines", "vs_line", "fs_line", &[line_layout], Topo::LineList, alpha, false, Cmp::Always);

        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh blit pipeline layout"),
            bind_group_layouts: &[&blit_layout],
            push_constant_ranges: &[],
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh blit"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_blit"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_blit"),
                compilation_options: Default::default(),
                targets: &[Some(target_format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let overlay = Self::overlay_buffer(device, 1024);
        Self {
            samples,
            mesh_pipeline,
            tri_pipeline,
            line_pipeline,
            line_top_pipeline,
            blit_pipeline,
            view_bind_group,
            blit_layout,
            view_uniforms,
            blit_uniforms,
            sampler,
            overlay,
            mesh: None,
            targets: None,
        }
    }

    fn overlay_buffer(device: &wgpu::Device, vertices: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh overlay"),
            size: (vertices * size_of::<LineVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn ensure_targets(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = |label, format, samples, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        use wgpu::TextureUsages as U;
        let msaa = (self.samples > 1).then(|| texture("mesh msaa", COLOR_FORMAT, self.samples, U::RENDER_ATTACHMENT));
        let resolved = texture("mesh resolved", COLOR_FORMAT, 1, U::RENDER_ATTACHMENT | U::TEXTURE_BINDING);
        let depth = texture("mesh depth", DEPTH_FORMAT, self.samples, U::RENDER_ATTACHMENT);
        let blit_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh blit bind group"),
            layout: &self.blit_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&resolved) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: self.blit_uniforms.as_entire_binding() },
            ],
        });
        self.targets = Some(Targets { size, msaa, resolved, depth, blit_bind_group });
    }

    fn ensure_mesh(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, mesh: Option<&GpuMesh>) {
        let Some(mesh) = mesh else {
            self.mesh = None;
            return;
        };
        if self.mesh.as_ref().is_some_and(|m| m.id == mesh.id) {
            return;
        }
        let buffer = |label, bytes: &[u8], usage| {
            let b = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (bytes.len() as u64).max(4).next_multiple_of(4),
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&b, 0, bytes);
            b
        };
        self.mesh = Some(MeshBuffers {
            id: mesh.id,
            vertices: buffer("mesh vertices", bytemuck::cast_slice(&mesh.positions), wgpu::BufferUsages::VERTEX),
            indices: buffer("mesh indices", bytemuck::cast_slice(&mesh.indices), wgpu::BufferUsages::INDEX),
            count: mesh.indices.len() as u32,
        });
    }

    /// Uploads the frame and renders the scene offscreen, recording into `encoder`.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, input: &MeshViewInput<'_>) {
        let max = device.limits().max_texture_dimension_2d;
        let size = [input.size_px[0].clamp(1, max), input.size_px[1].clamp(1, max)];
        self.ensure_targets(device, size);
        self.ensure_mesh(device, queue, input.mesh);
        queue.write_buffer(&self.view_uniforms, 0, bytemuck::bytes_of(&input.uniforms));
        queue.write_buffer(&self.blit_uniforms, 0, bytemuck::bytes_of(&[input.srgb_target as u32, 0, 0, 0]));

        let o = input.overlay;
        let all: Vec<LineVertex> = [&o.triangles, &o.lines, &o.lines_on_top].into_iter().flatten().copied().collect();
        if all.len() * size_of::<LineVertex>() > self.overlay.size() as usize {
            self.overlay = Self::overlay_buffer(device, all.len().next_power_of_two());
        }
        if !all.is_empty() {
            queue.write_buffer(&self.overlay, 0, bytemuck::cast_slice(&all));
        }

        let t = self.targets.as_ref().unwrap();
        let (view, resolve_target, store) = match &t.msaa {
            Some(msaa) => (msaa, Some(&t.resolved), wgpu::StoreOp::Discard),
            None => (&t.resolved, None, wgpu::StoreOp::Store),
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mesh preview"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(BACKGROUND), store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_bind_group(0, &self.view_bind_group, &[]);
        if let Some(m) = &self.mesh {
            pass.set_pipeline(&self.mesh_pipeline);
            pass.set_vertex_buffer(0, m.vertices.slice(..));
            pass.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..m.count, 0, 0..1);
        }
        pass.set_vertex_buffer(0, self.overlay.slice(..));
        // Buffer order: plane triangles, depth-tested lines, on-top lines. Lines are drawn
        // before the translucent plane so it blends over them; the profile goes last.
        let n_tri = o.triangles.len() as u32;
        let n_line = o.lines.len() as u32;
        let n_top = o.lines_on_top.len() as u32;
        for (pipeline, range) in [
            (&self.line_pipeline, n_tri..n_tri + n_line),
            (&self.tri_pipeline, 0..n_tri),
            (&self.line_top_pipeline, n_tri + n_line..n_tri + n_line + n_top),
        ] {
            if !range.is_empty() {
                pass.set_pipeline(pipeline);
                pass.draw(range, 0..1);
            }
        }
    }

    /// Blits the scene rendered by the last `prepare` into the pass's current viewport.
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        if let Some(t) = &self.targets {
            render_pass.set_pipeline(&self.blit_pipeline);
            render_pass.set_bind_group(0, &t.blit_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}
