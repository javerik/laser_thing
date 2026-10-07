//! The live emulator: simulation clock, scan playback and the per-frame shader input.
//! Every frontend drives it the same way, `tick(dt)` once per displayed frame and
//! `frame(..)` for the view it draws into, so they animate identically.

use crate::gpu_types::{DEFORM_SAMPLES, Uniforms};
use crate::model;
use crate::params::{DeformKind, LaserParams, LengthUnit, RateMode};
use crate::scan::{self, DisplayMapping, Scan};
use serde::Serialize;
use std::sync::Arc;

/// Values measured while computing a frame, shown as readouts in the UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct LiveInfo {
    pub view_px: [f32; 2],
    /// Line center relative to the view origin.
    pub center_px: [f32; 2],
    pub offset_px: f32,
    pub length_px: f32,
    /// View extent along the line direction (basis for % lengths).
    pub extent_along_px: f32,
    /// Effective scan mapping while a scan drives the line.
    pub scan: Option<DisplayMapping>,
}

/// Everything a renderer needs to draw one frame of the line.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub uniforms: Uniforms,
    /// Fresh deformation samples as `(displacement px, intensity)`, `DEFORM_SAMPLES`
    /// long, or `None` when the line is undeformed.
    pub deform: Option<Vec<[f32; 2]>>,
    pub info: LiveInfo,
}

#[derive(Clone, Debug)]
pub struct Emulator {
    pub params: LaserParams,
    pub playing: bool,
    /// Simulated seconds, advanced by `tick` scaled with `params.time_scale`.
    pub sim_time: f64,
    /// Smoothed display frame rate.
    pub fps: f32,
    /// Loaded object scan, shaping the line while `params.scan.enabled`.
    pub scan: Option<Arc<Scan>>,
    /// Profiles played since the start; mapped to a row by the end mode.
    pub pos: f64,
    /// Seeds animated noise.
    pub frame_counter: u32,
}

impl Default for Emulator {
    fn default() -> Self {
        Self::new(LaserParams::default())
    }
}

impl Emulator {
    pub fn new(params: LaserParams) -> Self {
        Self { params, playing: true, sim_time: 0.0, fps: 0.0, scan: None, pos: 0.0, frame_counter: 0 }
    }

    /// Advances by one displayed frame that took `dt` seconds.
    pub fn tick(&mut self, dt: f64) {
        if self.playing {
            self.sim_time += dt * self.params.time_scale as f64;
        }
        if dt > 0.0 {
            self.fps = 0.9 * self.fps + 0.1 * (1.0 / dt) as f32;
        }
        self.frame_counter = self.frame_counter.wrapping_add(1);
        self.advance_scan(dt);
    }

    /// True when the loaded scan shapes the live line.
    pub fn scan_drives_line(&self) -> bool {
        self.params.scan.enabled && self.scan.is_some()
    }

    /// Profile currently shown (0 without a scan).
    pub fn current_row(&self) -> usize {
        self.scan.as_ref().map_or(0, |s| scan::row_index(self.pos, s.meta.rows, self.params.scan.end))
    }

    /// Shows the profile `delta` rows away from the current one.
    pub fn step_scan(&mut self, delta: isize) {
        if let Some(s) = &self.scan {
            self.pos = scan::step(self.pos, s.meta.rows, self.params.scan.end, delta);
        }
    }

    /// Jumps to profile `row`.
    pub fn seek_row(&mut self, row: usize) {
        self.pos = row as f64;
    }

    pub fn reset_time(&mut self) {
        self.sim_time = 0.0;
        self.pos = 0.0;
    }

    /// Replaces the scan, keeping the playback position.
    pub fn set_scan(&mut self, scan: impl Into<Arc<Scan>>) {
        self.scan = Some(scan.into());
    }

    /// Advances scan playback by one rendered frame of `dt` seconds.
    fn advance_scan(&mut self, dt: f64) {
        if !self.playing || !self.scan_drives_line() {
            return;
        }
        let p = &self.params;
        self.pos += match p.scan.rate {
            RateMode::PerFrame => 1.0,
            RateMode::PerSecond => dt * p.time_scale as f64 * p.scan.profiles_per_s.max(0.0) as f64,
        };
    }

    /// Shader input for a view at `origin` (framebuffer px) of `size` px. `srgb_out`
    /// tells the shader that the target encodes sRGB itself.
    pub fn frame(&self, origin: [f32; 2], size: [f32; 2], srgb_out: bool) -> Frame {
        let p = &self.params;
        let angle = model::line_angle(&p.line);
        let dir = [angle.cos(), angle.sin()];
        let normal = [-dir[1], dir[0]];

        let extent_along = model::extent_along(size, dir);
        let mut length_px = match p.line.length_unit {
            LengthUnit::Pixels => p.line.length,
            LengthUnit::Percent => p.line.length / 100.0 * extent_along,
        }
        .max(0.0);

        // A driving scan sets the scale along the line; in fixed mode also the length.
        let active_scan = self.scan.as_ref().filter(|_| p.scan.enabled);
        let mapping = active_scan.map(|s| {
            let (mapping, length) = scan::display_mapping(s, &p.scan, length_px);
            length_px = length;
            mapping
        });
        let offset = model::motion_offset(&p.motion, self.sim_time, model::extent_along(size, normal));
        let center = [
            origin[0] + p.line.position_pct[0] / 100.0 * size[0] + normal[0] * offset,
            origin[1] + p.line.position_pct[1] / 100.0 * size[1] + normal[1] * offset,
        ];

        // Sample the deformation over the visible line plus the soft end caps.
        let span = length_px + p.profile.end_softness_px + 4.0;
        let start = -0.5 * span;
        let step = (span / (DEFORM_SAMPLES - 1) as f32).max(1e-3);
        let (deform, deform_start, deform_step) = if let (Some(s), Some(map)) = (active_scan, &mapping) {
            let mut samples = vec![[0.0, 1.0]; DEFORM_SAMPLES];
            let row = scan::row_index(self.pos, s.meta.rows, p.scan.end);
            scan::fill_deform(s, row, map, start, step, &mut samples);
            (Some(samples), start, step)
        } else if p.deform.kind == DeformKind::None {
            (None, 0.0, 1.0)
        } else {
            let mut samples = vec![0.0; DEFORM_SAMPLES];
            model::sample_deformation(&p.deform, self.sim_time, start, step, &mut samples);
            (Some(samples.into_iter().map(|d| [d, 1.0]).collect()), start, step)
        };

        let rgb = model::laser_rgb(&p.color);
        let seed = if p.noise.animated { self.frame_counter } else { 1 };
        let uniforms = Uniforms {
            center,
            dir,
            length: length_px,
            width: p.line.width_px.max(0.0),
            intensity: p.line.intensity_pct / 100.0,
            end_soft: p.profile.end_softness_px.max(0.0),
            color: [rgb[0], rgb[1], rgb[2], 1.0],
            profile: p.profile.kind as u32,
            edge_soft: p.profile.edge_softness_px,
            deform_start,
            deform_step,
            ambient: p.noise.ambient_pct / 100.0,
            speckle: p.noise.speckle_pct / 100.0,
            grain: p.noise.grain_px,
            noise: p.noise.noise_pct / 100.0,
            seed,
            deform_count: if deform.is_some() { DEFORM_SAMPLES as u32 } else { 0 },
            srgb_out: srgb_out as u32,
            _pad: 0,
        };
        let info = LiveInfo {
            view_px: size,
            center_px: [center[0] - origin[0], center[1] - origin[1]],
            offset_px: offset,
            length_px,
            extent_along_px: extent_along,
            scan: mapping,
        };
        Frame { uniforms, deform, info }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::box_mesh;
    use crate::params::{EndMode, ScanParams};
    use crate::scan::{Progress, SliceSettings, slice};
    use std::sync::atomic::AtomicBool;

    #[test]
    fn default_frame_is_centered() {
        let e = Emulator::default();
        let f = e.frame([10.0, 20.0], [800.0, 600.0], false);
        assert_eq!(f.uniforms.center, [410.0, 320.0]);
        assert_eq!(f.info.center_px, [400.0, 300.0]);
        assert_eq!(f.uniforms.deform_count, 0);
        assert!(f.deform.is_none());
    }

    #[test]
    fn clock_and_playback() {
        let mut e = Emulator::default();
        e.params.time_scale = 2.0;
        e.tick(0.5);
        assert_eq!(e.sim_time, 1.0);
        assert_eq!(e.frame_counter, 1);
        e.playing = false;
        e.tick(0.5);
        assert_eq!(e.sim_time, 1.0);

        let m = box_mesh([0.0; 3], [10.0, 5.0, 3.0]);
        let s = SliceSettings { lateral_res_mm: 1.0, travel_step_mm: 1.0, ..Default::default() };
        e.set_scan(slice(&m, &s, "", &AtomicBool::new(false), &Progress::new(0)).unwrap());
        e.params.scan = ScanParams { enabled: true, end: EndMode::Once, ..Default::default() };
        e.playing = true;
        e.tick(0.1);
        e.tick(0.1);
        assert_eq!(e.current_row(), 2, "one profile per frame by default");
        let f = e.frame([0.0; 2], [800.0, 600.0], true);
        assert_eq!(f.deform.as_ref().map(Vec::len), Some(DEFORM_SAMPLES));
        assert_eq!(f.uniforms.srgb_out, 1);
        assert!(f.info.scan.is_some());
        e.reset_time();
        assert_eq!((e.sim_time, e.current_row()), (0.0, 0));
    }
}
