//! Platform-independent core of the laser line emulator.
//!
//! Everything here is pure computation without GUI, GPU or file-system access, so it
//! runs natively and on `wasm32-unknown-unknown` alike:
//!
//! - [`params`]: every user-tunable parameter of the emulated line (serde, presets).
//! - [`model`]: motion, deformation, cross-section and colour of the line.
//! - [`emulator`]: simulation clock and per-frame [`gpu_types::Uniforms`] for a renderer.
//! - [`mesh`], [`scan`], [`scan_file`]: STL parsing, slicing a mesh into laser
//!   profiles as a triangulation sensor sees them, and the `.lscan` format.
//! - [`slicer`]: job state of an interactive slicer (debounced re-slicing, previews).
//! - [`camera`], [`viz`]: orbit camera and preview geometry for 3D views.
//!
//! Time is always passed in by the host, so no clock is read here.

pub mod camera;
pub mod emulator;
pub mod gpu_types;
pub mod mesh;
pub mod model;
pub mod params;
pub mod presets;
pub mod scan;
pub mod scan_file;
pub mod slicer;
pub mod viz;

pub use gpu_types::DEFORM_SAMPLES;
/// Re-exported so users of the camera and overlay types get the matching version.
pub use glam;
