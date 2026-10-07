//! Plain data shared with the shaders and handed to renderers. No GPU API types here,
//! so any frontend can fill them.

use glam::Vec3;

/// Number of deformation samples uploaded per frame.
pub const DEFORM_SAMPLES: usize = 8192;

/// Must match `struct Uniforms` in `laser.wgsl` (16-byte groups).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
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

/// Must match `struct View` in `mesh.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ViewUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub eye: [f32; 4],
    pub z_range: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineVertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
}

/// Triangle mesh in scan-frame mm. `id` changes whenever the geometry does.
#[derive(Clone, Debug, Default)]
pub struct GpuMesh {
    pub id: u64,
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// Overlay geometry, rebuilt every frame.
#[derive(Clone, Debug, Default)]
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
