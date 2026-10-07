//! 3D preview of the object being sliced. Rendered offscreen (MSAA + depth) during
//! `prepare`, then blitted into the egui pass, so it works inside any egui rect.

use eframe::egui;
use eframe::{egui_wgpu, wgpu};
use glam::{Mat4, Vec3};
use glam::camera::rh as camera;
use std::sync::Arc;

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;
const BACKGROUND: wgpu::Color = wgpu::Color { r: 0.07, g: 0.075, b: 0.085, a: 1.0 };

/// Must match `struct View` in `mesh.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ViewUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub eye: [f32; 4],
    pub z_range: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineVertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
}

/// Triangle mesh in scan-frame mm. `id` changes whenever the geometry does.
pub struct GpuMesh {
    pub id: u64,
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// Overlay geometry, rebuilt every frame.
#[derive(Default)]
pub struct Overlay {
    /// Translucent triangles (the laser plane), depth tested, not depth written.
    pub triangles: Vec<LineVertex>,
    /// Lines hidden by the object where it is in front of them.
    pub lines: Vec<LineVertex>,
    /// Lines always drawn on top (the current profile).
    pub lines_on_top: Vec<LineVertex>,
}

impl Overlay {
    pub fn line(&mut self, a: Vec3, b: Vec3, color: [f32; 4]) {
        self.lines.push(LineVertex { pos: a.into(), color });
        self.lines.push(LineVertex { pos: b.into(), color });
    }

    pub fn line_on_top(&mut self, a: Vec3, b: Vec3, color: [f32; 4]) {
        self.lines_on_top.push(LineVertex { pos: a.into(), color });
        self.lines_on_top.push(LineVertex { pos: b.into(), color });
    }

    pub fn quad(&mut self, c: [Vec3; 4], color: [f32; 4]) {
        for i in [0, 1, 2, 0, 2, 3] {
            self.triangles.push(LineVertex { pos: c[i].into(), color });
        }
    }
}

struct Targets {
    size: [u32; 2],
    msaa: wgpu::TextureView,
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

pub struct MeshViewResources {
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

impl MeshViewResources {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
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
                multisample: wgpu::MultisampleState { count: SAMPLES, ..Default::default() },
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
        let msaa = texture("mesh msaa", COLOR_FORMAT, SAMPLES, U::RENDER_ATTACHMENT);
        let resolved = texture("mesh resolved", COLOR_FORMAT, 1, U::RENDER_ATTACHMENT | U::TEXTURE_BINDING);
        let depth = texture("mesh depth", DEPTH_FORMAT, SAMPLES, U::RENDER_ATTACHMENT);
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
        let Some(r) = resources.get_mut::<MeshViewResources>() else {
            return Vec::new();
        };
        let size = [self.size_px[0].clamp(1, 8192), self.size_px[1].clamp(1, 8192)];
        r.ensure_targets(device, size);
        r.ensure_mesh(device, queue, self.mesh.as_deref());
        queue.write_buffer(&r.view_uniforms, 0, bytemuck::bytes_of(&self.uniforms));
        queue.write_buffer(&r.blit_uniforms, 0, bytemuck::bytes_of(&[self.srgb_target as u32, 0, 0, 0]));

        let o = &self.overlay;
        let all: Vec<LineVertex> = [&o.triangles, &o.lines, &o.lines_on_top].into_iter().flatten().copied().collect();
        if all.len() * size_of::<LineVertex>() > r.overlay.size() as usize {
            r.overlay = MeshViewResources::overlay_buffer(device, all.len().next_power_of_two());
        }
        if !all.is_empty() {
            queue.write_buffer(&r.overlay, 0, bytemuck::cast_slice(&all));
        }

        let t = r.targets.as_ref().unwrap();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mesh preview"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.msaa,
                depth_slice: None,
                resolve_target: Some(&t.resolved),
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(BACKGROUND), store: wgpu::StoreOp::Discard },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_bind_group(0, &r.view_bind_group, &[]);
        if let Some(m) = &r.mesh {
            pass.set_pipeline(&r.mesh_pipeline);
            pass.set_vertex_buffer(0, m.vertices.slice(..));
            pass.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..m.count, 0, 0..1);
        }
        pass.set_vertex_buffer(0, r.overlay.slice(..));
        // Buffer order: plane triangles, depth-tested lines, on-top lines. Lines are drawn
        // before the translucent plane so it blends over them; the profile goes last.
        let n_tri = o.triangles.len() as u32;
        let n_line = o.lines.len() as u32;
        let n_top = o.lines_on_top.len() as u32;
        for (pipeline, range) in [
            (&r.line_pipeline, n_tri..n_tri + n_line),
            (&r.tri_pipeline, 0..n_tri),
            (&r.line_top_pipeline, n_tri + n_line..n_tri + n_line + n_top),
        ] {
            if !range.is_empty() {
                pass.set_pipeline(pipeline);
                pass.draw(range, 0..1);
            }
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(r) = resources.get::<MeshViewResources>()
            && let Some(t) = &r.targets
        {
            render_pass.set_pipeline(&r.blit_pipeline);
            render_pass.set_bind_group(0, &t.blit_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}

/// Turntable camera around a target point, z up.
#[derive(Clone, Copy, Debug)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl OrbitCamera {
    /// Three-quarter view framing a box from `lo` to `hi`.
    pub fn framing(lo: Vec3, hi: Vec3) -> Self {
        let radius = (0.5 * (hi - lo).length()).max(1.0);
        Self { target: 0.5 * (lo + hi), yaw: -2.2, pitch: 0.55, distance: radius * 2.6 }
    }

    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + self.distance * Vec3::new(cp * cy, cp * sy, sp)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = camera::view::look_at_mat4(self.eye(), self.target, Vec3::Z);
        let near = (self.distance * 0.01).max(0.01);
        // wgpu clip space is y-up with depth 0..1, the DirectX convention.
        let proj = camera::proj::directx::perspective(40f32.to_radians(), aspect.max(1e-3), near, self.distance * 20.0 + 1.0);
        proj * view
    }

    pub fn uniforms(&self, aspect: f32, z_max: f32) -> ViewUniforms {
        ViewUniforms {
            view_proj: self.view_proj(aspect).to_cols_array_2d(),
            eye: self.eye().extend(1.0).into(),
            z_range: [z_max, 0.0, 0.0, 0.0],
        }
    }

    /// Screen position of `p` inside `rect`, or `None` behind the camera.
    pub fn project(&self, rect: egui::Rect, p: Vec3) -> Option<egui::Pos2> {
        let clip = self.view_proj(rect.aspect_ratio()) * p.extend(1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(egui::pos2(
            rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
            rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
        ))
    }

    /// Drag: orbit; secondary drag or shift-drag: pan; scroll / pinch: zoom.
    pub fn handle_input(&mut self, ui: &egui::Ui, response: &egui::Response) {
        let drag = response.drag_delta();
        let shift = ui.input(|i| i.modifiers.shift);
        if response.dragged_by(egui::PointerButton::Secondary) || (response.dragged_by(egui::PointerButton::Primary) && shift) {
            let forward = (self.target - self.eye()).normalize_or_zero();
            let right = forward.cross(Vec3::Z).normalize_or_zero();
            let up = right.cross(forward);
            let k = self.distance * 0.0015;
            self.target += (-drag.x * right + drag.y * up) * k;
        } else if response.dragged_by(egui::PointerButton::Primary) {
            self.yaw -= drag.x * 0.008;
            self.pitch = (self.pitch + drag.y * 0.008).clamp(-1.55, 1.55);
        }
        if response.hovered() {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            self.distance = (self.distance * (-scroll * 0.002).exp() / zoom).clamp(0.5, 1e6);
        }
    }
}
