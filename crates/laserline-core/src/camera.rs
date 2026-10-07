//! Turntable camera for the 3D preview. Input handling is reduced to plain deltas so
//! every frontend (egui, DOM pointer events) drives it the same way.

use crate::gpu_types::ViewUniforms;
use glam::camera::rh as camera;
use glam::{Mat4, Vec3};

/// Turntable camera around a target point, z up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    /// Point the camera looks at and orbits around.
    pub target: Vec3,
    /// Angle around z in radians.
    pub yaw: f32,
    /// Elevation above the xy plane in radians.
    pub pitch: f32,
    /// Distance from the target.
    pub distance: f32,
}

impl OrbitCamera {
    /// Three-quarter view framing a box from `lo` to `hi`.
    pub fn framing(lo: Vec3, hi: Vec3) -> Self {
        let radius = (0.5 * (hi - lo).length()).max(1.0);
        Self { target: 0.5 * (lo + hi), yaw: -2.2, pitch: 0.55, distance: radius * 2.6 }
    }

    /// Camera position.
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + self.distance * Vec3::new(cp * cy, cp * sy, sp)
    }

    /// View-projection matrix for a viewport of `aspect` = width / height (wgpu clip space).
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = camera::view::look_at_mat4(self.eye(), self.target, Vec3::Z);
        let near = (self.distance * 0.01).max(0.01);
        // wgpu clip space is y-up with depth 0..1, the DirectX convention.
        let proj = camera::proj::directx::perspective(40f32.to_radians(), aspect.max(1e-3), near, self.distance * 20.0 + 1.0);
        proj * view
    }

    /// Shader uniforms; `z_max` is the height mapped to the top of the height tint.
    pub fn uniforms(&self, aspect: f32, z_max: f32) -> ViewUniforms {
        ViewUniforms {
            view_proj: self.view_proj(aspect).to_cols_array_2d(),
            eye: self.eye().extend(1.0).into(),
            z_range: [z_max, 0.0, 0.0, 0.0],
        }
    }

    /// Position of `p` in a view of the given aspect ratio as fractions of its size
    /// (`[0, 0]` top left, `[1, 1]` bottom right), or `None` behind the camera.
    pub fn project(&self, aspect: f32, p: Vec3) -> Option<[f32; 2]> {
        let clip = self.view_proj(aspect) * p.extend(1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some([(ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5])
    }

    /// Rotates by a pointer drag of `dx`, `dy` logical pixels.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.008;
        self.pitch = (self.pitch + dy * 0.008).clamp(-1.55, 1.55);
    }

    /// Moves the target by a pointer drag of `dx`, `dy` logical pixels.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let forward = (self.target - self.eye()).normalize_or_zero();
        let right = forward.cross(Vec3::Z).normalize_or_zero();
        let up = right.cross(forward);
        let k = self.distance * 0.0015;
        self.target += (-dx * right + dy * up) * k;
    }

    /// Zooms by `scroll` logical pixels of wheel movement (positive = towards the
    /// target) and a pinch factor (`> 1` = towards the target; 1 when not pinching).
    pub fn zoom(&mut self, scroll: f32, pinch: f32) {
        self.distance = (self.distance * (-scroll * 0.002).exp() / pinch).clamp(0.5, 1e6);
    }
}
