//! How the line's view pixels relate to the scanned object's millimetres, for a controller
//! that measures what a camera filming the line sees (see [`metric_mapping`]).
//!
//! Coordinates are view pixels (the canvas' drawing buffer, y down) and scan-frame mm: `u`
//! along the line, `v` along the travel, `z` the height (see [`crate::scan`]).

use crate::mesh;
use crate::model;
use crate::params::{EndMode, LaserParams, MotionKind, RateMode};
use crate::scan::{self, ObjectBounds, Scan};
use serde::{Deserialize, Serialize};

/// Height gains below this (px per mm) count as a flat line.
pub const FLAT_GAIN_PX_PER_MM: f32 = 1e-6;

/// The user's line at rest (no motion) and at height 0, in view px.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct BaseLine {
    /// Centre of the line: `u0_mm` lies here.
    pub center_px: [f32; 2],
    /// Unit vector along the line, the direction of +u.
    pub dir: [f32; 2],
    /// Unit vector across the line, `[-dir.y, dir.x]`: positive heights move the line along
    /// it by `height_px_per_mm` (signed).
    pub normal: [f32; 2],
}

/// How scan rows advance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum TravelMode {
    /// One row per rendered frame: the rate is the display's frame rate.
    PerFrame,
    /// A set number of rows per simulated second.
    PerSecond,
}

/// What the playback does after the last row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum TravelEnd {
    /// Starts over from the first row.
    Loop,
    /// Plays backwards, then forwards again.
    Bounce,
    /// Stops at the last row.
    Once,
}

impl From<EndMode> for TravelEnd {
    fn from(e: EndMode) -> Self {
        match e {
            EndMode::Loop => TravelEnd::Loop,
            EndMode::Bounce => TravelEnd::Bounce,
            EndMode::Once => TravelEnd::Once,
        }
    }
}

/// How fast the scan travels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct Travel {
    /// How rows advance.
    pub mode: TravelMode,
    /// Rows played per second: `profiles_per_s × time_scale` in `per_second` mode, the
    /// measured frame rate in `per_frame` mode.
    pub rows_per_s: f32,
    /// Travel speed in mm/s: `rows_per_s × dv_mm`.
    pub mm_per_s: f32,
    /// True when `rows_per_s` is a measured frame rate (`per_frame`), not a set rate.
    pub approximate: bool,
    /// The playback is running.
    pub playing: bool,
    /// What happens after the last row.
    pub end: TravelEnd,
}

/// What makes the mapping unreliable for measuring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum Disturbance {
    /// The line moves on its own (a motion other than static).
    Motion,
    /// The playback bounces: the return pass runs backwards in v.
    Bounce,
    /// The playback stops at the last row.
    Once,
    /// The playback is paused.
    Paused,
    /// Ambient light, speckle or sensor noise is on.
    Noise,
    /// The height gain is about 0: heights don't move the line.
    Flat,
}

/// How the user's line maps the loaded scan into view px, and how fast it travels.
///
/// A view point `p` shows `u = u0_mm + (p − center_px)·dir ÷ along_px_per_mm` and
/// `z = (p − center_px)·normal ÷ height_px_per_mm`; row `j` is `v = v0 + j·dv_mm` of travel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct MetricMapping {
    /// The line at rest, height 0.
    pub base: BaseLine,
    /// View px per mm of u, along `base.dir`.
    pub along_px_per_mm: f32,
    /// View px per mm of height along `base.normal`, signed: positive when higher surfaces
    /// move the line along +normal (`invert_height`), negative otherwise.
    pub height_px_per_mm: f32,
    /// u at `base.center_px`, in mm.
    pub u0_mm: f32,
    /// Travel per scan row, in mm.
    pub dv_mm: f32,
    /// Rows of the scan.
    pub rows: usize,
    /// How fast the scan travels.
    pub travel: Travel,
    /// The object's extent in scan-frame mm (u, v, z), without the margins; `None` when no
    /// cell rises above [`scan::OBJECT_MIN_HEIGHT_MM`] (`null` in JSON).
    #[cfg_attr(feature = "wasm", tsify(type = "ObjectBounds | null"))]
    pub object_mm: Option<ObjectBounds>,
    /// The scan frame is a mirror image of the model (see [`mesh::scan_frame_mirrored`]).
    pub model_mirrored: bool,
    /// What makes the mapping unreliable; empty when nothing does.
    pub disturbances: Vec<Disturbance>,
}

/// The metric mapping of the line drawn with `params` in a view of `view_px`, or `None`
/// without a scan or with the scan disabled. `scan` comes with its
/// [`Scan::object_bounds`], which callers cache (they walk every cell). `fps` is the
/// measured frame rate (the row rate in `per_frame` mode); `playing` whether playback runs.
///
/// It describes `params` as given: a caller showing calibration patterns passes the user's
/// parameters, not the pattern's.
pub fn metric_mapping(
    params: &LaserParams,
    view_px: [f32; 2],
    scan: Option<(&Scan, Option<&ObjectBounds>)>,
    fps: f32,
    playing: bool,
) -> Option<MetricMapping> {
    let p = params;
    let (scan, object) = scan.filter(|_| p.scan.enabled)?;
    let angle = model::line_angle(&p.line);
    let dir = [angle.cos(), angle.sin()];
    let normal = [-dir[1], dir[0]];
    let center_px = [p.line.position_pct[0] / 100.0 * view_px[0], p.line.position_pct[1] / 100.0 * view_px[1]];
    let (mapping, _) = scan::display_mapping(scan, &p.scan, model::line_length_px(&p.line, view_px));
    let height_px_per_mm = mapping.height_px_per_mm * if mapping.invert { 1.0 } else { -1.0 };

    let (mode, rows_per_s) = match p.scan.rate {
        RateMode::PerSecond => (TravelMode::PerSecond, p.scan.profiles_per_s.max(0.0) * p.time_scale),
        RateMode::PerFrame => (TravelMode::PerFrame, fps.max(0.0)),
    };
    let dv_mm = scan.meta.dv_mm;

    let mut disturbances = Vec::new();
    if p.motion.kind != MotionKind::Static {
        disturbances.push(Disturbance::Motion);
    }
    match p.scan.end {
        EndMode::Bounce => disturbances.push(Disturbance::Bounce),
        EndMode::Once => disturbances.push(Disturbance::Once),
        EndMode::Loop => {}
    }
    if !playing {
        disturbances.push(Disturbance::Paused);
    }
    if p.noise.ambient_pct > 0.0 || p.noise.speckle_pct > 0.0 || p.noise.noise_pct > 0.0 {
        disturbances.push(Disturbance::Noise);
    }
    if height_px_per_mm.abs() < FLAT_GAIN_PX_PER_MM {
        disturbances.push(Disturbance::Flat);
    }

    Some(MetricMapping {
        base: BaseLine { center_px, dir, normal },
        along_px_per_mm: mapping.px_per_mm,
        height_px_per_mm,
        u0_mm: scan.u_center_mm(),
        dv_mm,
        rows: scan.meta.rows,
        travel: Travel {
            mode,
            rows_per_s,
            mm_per_s: rows_per_s * dv_mm,
            approximate: mode == TravelMode::PerFrame,
            playing,
            end: p.scan.end.into(),
        },
        object_mm: object.copied(),
        model_mirrored: mesh::scan_frame_mirrored(&scan.meta.settings),
        disturbances,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::Emulator;
    use crate::mesh::box_mesh;
    use crate::params::{HeightGain, LengthUnit, Orientation, ScanParams, ScanScale};
    use crate::scan::{Axis, Progress, SliceSettings, slice};
    use std::sync::atomic::AtomicBool;

    const VIEW: [f32; 2] = [1000.0, 500.0];

    /// A 20 × 10 × 8 mm box with 5 mm margins on a 1 mm grid: 30 mm wide, u centre 0.
    fn box_scan(s: SliceSettings) -> Scan {
        let s = SliceSettings { lateral_res_mm: 1.0, travel_step_mm: 0.5, margin_lateral_mm: 5.0, margin_travel_mm: 5.0, occlusion: false, ..s };
        slice(&box_mesh([0.0; 3], [20.0, 10.0, 8.0]), &s, "box", &AtomicBool::new(false), &Progress::new(0)).unwrap()
    }

    fn map(p: &LaserParams, scan: &Scan) -> MetricMapping {
        let object = scan.object_bounds();
        metric_mapping(p, VIEW, Some((scan, object.as_ref())), 60.0, true).unwrap()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4 * b.abs().max(1.0)
    }

    #[test]
    fn none_without_an_active_scan() {
        let p = LaserParams::default();
        assert!(metric_mapping(&p, VIEW, None, 60.0, true).is_none());
        let scan = box_scan(SliceSettings::default());
        let off = LaserParams { scan: ScanParams { enabled: false, ..Default::default() }, ..Default::default() };
        assert!(metric_mapping(&off, VIEW, Some((&scan, None)), 60.0, true).is_none());
    }

    #[test]
    fn base_line_and_scan_geometry() {
        let mut p = LaserParams::default();
        p.line.position_pct = [40.0, 60.0];
        p.line.orientation = Orientation::Vertical;
        p.line.tilt_deg = 10.0;
        // Motion moves the drawn line, not the base.
        p.motion.kind = MotionKind::Sine;
        let scan = box_scan(SliceSettings::default());
        let m = map(&p, &scan);
        assert_eq!(m.base.center_px, [400.0, 300.0]);
        let a = 100f32.to_radians();
        assert!(close(m.base.dir[0], a.cos()) && close(m.base.dir[1], a.sin()));
        assert_eq!(m.base.normal, [-m.base.dir[1], m.base.dir[0]]);
        assert_eq!((m.u0_mm, m.dv_mm, m.rows), (scan.u_center_mm(), 0.5, scan.meta.rows));
        assert_eq!(m.object_mm, scan.object_bounds());
        assert_eq!(m.disturbances, [Disturbance::Motion]);
    }

    #[test]
    fn scale_along_the_line() {
        let scan = box_scan(SliceSettings::default());
        assert_eq!(scan.width_mm(), 30.0);
        // FitLine with a percent length: 80 % of the view's extent along the line.
        let mut p = LaserParams::default();
        assert!(close(map(&p, &scan).along_px_per_mm, 800.0 / 30.0));
        p.line.orientation = Orientation::Vertical;
        assert!(close(map(&p, &scan).along_px_per_mm, 400.0 / 30.0));
        p.line.tilt_deg = 30.0; // 120°: extent 1000·cos 60° + 500·sin 60°
        let extent = 1000.0 * 0.5 + 500.0 * 3f32.sqrt() / 2.0;
        assert!(close(map(&p, &scan).along_px_per_mm, 0.8 * extent / 30.0));
        // FitLine with a pixel length.
        let mut p = LaserParams::default();
        p.line.length_unit = LengthUnit::Pixels;
        p.line.length = 600.0;
        assert!(close(map(&p, &scan).along_px_per_mm, 20.0));
        // Fixed: the length follows.
        p.scan.scale = ScanScale::Fixed;
        p.scan.px_per_mm = 7.0;
        assert_eq!(map(&p, &scan).along_px_per_mm, 7.0);
    }

    #[test]
    fn height_gain_and_its_sign() {
        let scan = box_scan(SliceSettings { triangulation_deg: 30.0, ..Default::default() });
        let mut p = LaserParams::default();
        p.line.length_unit = LengthUnit::Pixels;
        p.line.length = 600.0; // 20 px/mm
        assert!(close(map(&p, &scan).height_px_per_mm, -20.0), "MatchScale: up = −normal");
        p.scan.invert_height = true;
        assert!(close(map(&p, &scan).height_px_per_mm, 20.0));
        p.scan.height_gain = HeightGain::FromAngle;
        assert!(close(map(&p, &scan).height_px_per_mm, 10.0));
        p.scan.height_gain = HeightGain::Manual;
        p.scan.height_px_per_mm = 3.0;
        p.scan.invert_height = false;
        assert!(close(map(&p, &scan).height_px_per_mm, -3.0));
        p.scan.height_px_per_mm = 0.0;
        assert_eq!(map(&p, &scan).disturbances, [Disturbance::Flat]);
    }

    /// The mapping predicts where `Emulator::frame` moves the line: a cell at along
    /// offset s and height h shows at s·dir + h·height_px_per_mm·normal from the centre.
    #[test]
    fn round_trip_with_the_drawn_deformation() {
        let scan = box_scan(SliceSettings::default());
        let mut e = Emulator::default();
        e.params.line.tilt_deg = 5.0;
        e.params.scan.end = EndMode::Once;
        e.set_scan(scan.clone());
        let row = 15; // across the box (v = 2.5 mm)
        e.seek_row(row);
        for invert in [false, true] {
            e.params.scan.invert_height = invert;
            let m = map(&e.params, &scan);
            let f = e.frame([0.0; 2], VIEW, false);
            assert_eq!(f.info.center_px, m.base.center_px);
            assert_eq!(f.info.scan.unwrap().px_per_mm, m.along_px_per_mm);
            let samples = f.deform.unwrap();
            let (start, step) = (f.uniforms.deform_start, f.uniforms.deform_step);
            let mut checked = 0;
            for (k, d) in samples.iter().enumerate() {
                let u = m.u0_mm + (start + k as f32 * step) / m.along_px_per_mm;
                // On the box top or the floor, clear of the edges where the line interpolates.
                let i = ((u - scan.meta.u0_mm) / scan.meta.du_mm).round();
                if (u.abs() - 10.0).abs() < 1.5 || !(0.0..scan.meta.cols as f32).contains(&i) {
                    continue;
                }
                let h = scan.seen(i as usize, row);
                assert_eq!(h, if u.abs() < 10.0 { 8.0 } else { 0.0 });
                assert!(close(d[0], h * m.height_px_per_mm), "u {u}: drawn {} vs {}", d[0], h * m.height_px_per_mm);
                checked += 1;
            }
            assert!(checked > 100, "{checked} samples checked");
        }
    }

    #[test]
    fn travel_per_mode() {
        let scan = box_scan(SliceSettings::default());
        let mut p = LaserParams::default();
        let t = map(&p, &scan).travel;
        assert_eq!((t.mode, t.rows_per_s, t.mm_per_s, t.approximate), (TravelMode::PerFrame, 60.0, 30.0, true));
        assert_eq!((t.playing, t.end), (true, TravelEnd::Loop));
        p.scan.rate = RateMode::PerSecond;
        p.scan.profiles_per_s = 100.0;
        p.time_scale = 0.5;
        let t = map(&p, &scan).travel;
        assert_eq!((t.mode, t.rows_per_s, t.mm_per_s, t.approximate), (TravelMode::PerSecond, 50.0, 25.0, false));
        p.scan.end = EndMode::Bounce;
        p.noise.speckle_pct = 5.0;
        let m = metric_mapping(&p, VIEW, Some((&scan, None)), 60.0, false).unwrap();
        assert_eq!(m.travel.end, TravelEnd::Bounce);
        assert!(!m.travel.playing);
        assert_eq!(m.disturbances, [Disturbance::Bounce, Disturbance::Paused, Disturbance::Noise]);
        assert_eq!(m.object_mm, None);
    }

    #[test]
    fn mirrored_models() {
        let mirrored = |s: SliceSettings| map(&LaserParams::default(), &box_scan(s)).model_mirrored;
        assert!(!mirrored(SliceSettings::default()), "up +Z, travel Y");
        assert!(mirrored(SliceSettings { travel_axis: Axis::X, ..Default::default() }), "up +Z, travel X");
        assert!(mirrored(SliceSettings { flip_along: true, ..Default::default() }));
    }

    #[test]
    fn serialises_as_the_protocol_says() {
        let scan = box_scan(SliceSettings::default());
        let mut p = LaserParams::default();
        p.scan.end = EndMode::Once;
        let v = serde_json::to_value(map(&p, &scan)).unwrap();
        assert_eq!(v["travel"]["mode"], "per_frame");
        assert_eq!(v["travel"]["end"], "once");
        assert_eq!(v["disturbances"], serde_json::json!(["once"]));
        assert_eq!(v["object_mm"]["max"], serde_json::json!([10.0, 10.0, 8.0]));
        assert!(v["base"]["center_px"].is_array());
    }
}
