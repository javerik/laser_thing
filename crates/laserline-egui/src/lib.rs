//! egui frontend of the laserline emulator for native applications.
//!
//! [`LaserUi`] holds the emulator, the slicer and their view state and draws the
//! panels: the live laser view ([`LaserUi::live_view`]), the control panel
//! ([`LaserUi::controls_ui`]) and the STL slicer with its 3D preview
//! ([`LaserUi::slicer_panels`]). Window layout, shortcuts and persistence stay with the
//! application; see the `laserline-desktop` app for a complete example.
//!
//! Requirements:
//! - egui must render with wgpu; call [`LaserUi::install_gpu`] once with the
//!   `egui_wgpu::RenderState` (eframe: `CreationContext::wgpu_render_state`).
//! - File dialogs are provided by the application through [`FileHost`].
//! - Slices run on a background thread, so this crate targets native platforms.
//!
//! The individual widgets (`line_ui`, `profile_ui`, …) are public as well, to build
//! custom layouts.

mod callbacks;
mod files;
mod laser_ui;
mod slicer_ui;
mod worker;

pub use callbacks::{LaserCallback, MeshViewCallback, install as install_renderers};
pub use files::{FileHost, FileKind, OpenedFile};
pub use laser_ui::{
    LaserUi, color_ui, deform_ui, grid, line_ui, motion_ui, noise_ui, plot, profile_ui, row, scan_params_ui, section,
};
pub use slicer_ui::{handle_camera_input, profile_plot, project, scan_summary, slice_settings_ui};
pub use worker::SliceWorker;
pub use {egui, egui_wgpu, laserline_core as core};
