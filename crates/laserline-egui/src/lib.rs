#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

mod callbacks;
mod files;
mod laser_ui;
mod layout;
mod slicer_ui;
mod window;
mod worker;

pub use callbacks::{LaserCallback, MeshViewCallback, install as install_renderers};
pub use files::{FileHost, FileKind, OpenedFile};
pub use laser_ui::{
    LaserUi, MAX_FRAME_DT, color_ui, deform_ui, grid, line_ui, motion_ui, noise_ui, plot, profile_ui, row, scan_params_ui, section,
};
pub use layout::{Action, DockState, Layout, ViewPlacement};
pub use slicer_ui::{handle_camera_input, profile_plot, project, scan_summary, slice_settings_ui};
pub use window::show_window;
pub use worker::SliceWorker;
pub use {egui, egui_wgpu, laserline_core as core};
