//! wgpu renderers for the laserline emulator, independent of any UI toolkit.
//!
//! - [`LaserRenderer`] draws the emulated laser line: a full-viewport pass evaluating
//!   every pixel analytically from [`laserline_core::gpu_types::Uniforms`].
//! - [`MeshViewRenderer`] draws the 3D slicer preview offscreen (MSAA + depth) and
//!   blits it into the caller's pass.
//!
//! Both split their work into `prepare` (uploads, offscreen passes) and `paint`
//! (draws into a render pass the caller owns), so they fit into egui paint callbacks
//! as well as plain render loops. The shaders avoid storage buffers and compute, so
//! they run on WebGL2 as well as on WebGPU and the native backends.

mod laser;
mod mesh_view;

pub use laser::LaserRenderer;
pub use mesh_view::{MeshViewInput, MeshViewRenderer};
pub use wgpu;
