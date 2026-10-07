//! Plain data shared with the shaders and handed to renderers. No GPU API types here,
//! so any frontend can fill them.

use glam::Vec3;

/// Number of deformation samples uploaded per frame.
pub const DEFORM_SAMPLES: usize = 8192;

/// Must match `struct Uniforms` in `laser.wgsl` (16-byte groups).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    /// Line center in framebuffer px.
    pub center: [f32; 2],
    /// Unit direction along the line.
    pub dir: [f32; 2],
    /// Line length in px.
    pub length: f32,
    /// Line width in px.
    pub width: f32,
    /// Peak intensity, 0..1.
    pub intensity: f32,
    /// Fade-out length at the line ends in px.
    pub end_soft: f32,
    /// Linear RGBA, 0..1.
    pub color: [f32; 4],
    /// [`crate::params::ProfileKind`] as its index.
    pub profile: u32,
    /// Top-hat edge width in px.
    pub edge_soft: f32,
    /// Position along the line of the first deformation sample in px.
    pub deform_start: f32,
    /// Spacing of the deformation samples in px.
    pub deform_step: f32,
    /// Ambient light level, 0..1.
    pub ambient: f32,
    /// Speckle contrast, 0..1.
    pub speckle: f32,
    /// Speckle grain size in px.
    pub grain: f32,
    /// Additive noise standard deviation, 0..1.
    pub noise: f32,
    /// Noise seed.
    pub seed: u32,
    /// Number of deformation samples uploaded (0 = straight line).
    pub deform_count: u32,
    /// 1 if the target encodes sRGB itself, else 0.
    pub srgb_out: u32,
    /// Padding to 16 bytes.
    pub _pad: u32,
}

/// Must match `struct View` in `mesh.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ViewUniforms {
    /// Column-major view-projection matrix.
    pub view_proj: [[f32; 4]; 4],
    /// Camera position (w = 1).
    pub eye: [f32; 4],
    /// x: height mapped to the top of the height tint; rest unused.
    pub z_range: [f32; 4],
}

/// Vertex of overlay lines and triangles.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineVertex {
    /// Position in scan-frame mm.
    pub pos: [f32; 3],
    /// Linear RGBA, 0..1.
    pub color: [f32; 4],
}

/// Triangle mesh in scan-frame mm. `id` changes whenever the geometry does.
#[derive(Clone, Debug, Default)]
pub struct GpuMesh {
    /// Geometry version.
    pub id: u64,
    /// Vertex positions.
    pub positions: Vec<[f32; 3]>,
    /// Triangle list of indices into `positions`.
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
    /// Adds a depth-tested line from `a` to `b`.
    pub fn line(&mut self, a: Vec3, b: Vec3, color: [f32; 4]) {
        self.lines.push(LineVertex { pos: a.into(), color });
        self.lines.push(LineVertex { pos: b.into(), color });
    }

    /// Adds a line drawn on top of everything.
    pub fn line_on_top(&mut self, a: Vec3, b: Vec3, color: [f32; 4]) {
        self.lines_on_top.push(LineVertex { pos: a.into(), color });
        self.lines_on_top.push(LineVertex { pos: b.into(), color });
    }

    /// Adds a translucent quad with corners `c` in order.
    pub fn quad(&mut self, c: [Vec3; 4], color: [f32; 4]) {
        for i in [0, 1, 2, 0, 2, 3] {
            self.triangles.push(LineVertex { pos: c[i].into(), color });
        }
    }
}
