//! Support for calibrating a camera that films the emulated line.
//!
//! - [`line_envelope`]: the area the line can cover over its whole motion, deformation
//!   and scan playback, so a camera region can be chosen that always holds it.
//! - [`calibration_params`] and [`pattern_params`]: a clean, static line (or nothing) at
//!   known positions, for measuring how view pixels map onto the camera's sensor.
//! - [`fit_into`]: shrinks and recentres the line so its envelope lies inside the part
//!   of the view a camera sees.
//!
//! All coordinates are view pixels (the canvas' drawing buffer, y down).

use crate::model;
use crate::params::{
    DeformKind, DeformParams, LaserParams, LengthUnit, MotionKind, MotionParams, Orientation,
    ProfileKind, ScanScale,
};
use crate::scan::{self, Scan};
use serde::{Deserialize, Serialize};

/// Narrowest line drawn by [`calibration_params`], in px.
pub const MIN_CALIBRATION_WIDTH_PX: f32 = 4.0;

/// The area a line can cover: a rectangle in the line's own frame (along its direction,
/// across it), anchored at its resting center.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct Envelope {
    /// Corners in view px, in order: (along min, across min), (along max, across min),
    /// (along max, across max), (along min, across max).
    pub corners: [[f32; 2]; 4],
    /// Line center without motion, in view px.
    pub center_px: [f32; 2],
    /// Unit vector along the line.
    pub dir: [f32; 2],
    /// Unit vector across the line (the direction motion and deformation move it in).
    pub normal: [f32; 2],
    /// Extent along `dir` relative to `center_px`, including the soft end caps.
    pub along_px: [f32; 2],
    /// Extent along `normal` relative to `center_px`, including half the line width.
    pub across_px: [f32; 2],
}

impl Envelope {
    /// Whether every corner lies inside the convex `polygon`, at least `margin` px from
    /// its edges. The polygon may wind either way.
    pub fn inside(&self, polygon: &[[f32; 2]], margin: f32) -> bool {
        self.corners.iter().all(|&p| inset_distance(polygon, p) >= margin)
    }
}

/// The envelope of the line drawn with `params` in a view of `view_px`. `scan` is the
/// loaded scan; it shapes the line only while `params.scan.enabled`.
pub fn line_envelope(params: &LaserParams, view_px: [f32; 2], scan: Option<&Scan>) -> Envelope {
    let p = params;
    let angle = model::line_angle(&p.line);
    let dir = [angle.cos(), angle.sin()];
    let normal = [-dir[1], dir[0]];

    let mut length = match p.line.length_unit {
        LengthUnit::Pixels => p.line.length,
        LengthUnit::Percent => p.line.length / 100.0 * model::extent_along(view_px, dir),
    }
    .max(0.0);
    let active_scan = scan.filter(|_| p.scan.enabled);
    let shape = match active_scan {
        Some(s) => {
            let (mapping, l) = scan::display_mapping(s, &p.scan, length);
            length = l;
            scan_range(s, mapping.height_px_per_mm * if mapping.invert { 1.0 } else { -1.0 })
        }
        None => deform_range(&p.deform),
    };
    let motion = motion_range(&p.motion, model::extent_along(view_px, normal));
    let half_width = 0.5 * p.line.width_px.max(0.0);
    let across = [motion[0] + shape[0] - half_width, motion[1] + shape[1] + half_width];
    let half_length = 0.5 * (length + p.profile.end_softness_px.max(0.0));
    let along = [-half_length, half_length];

    let center = [p.line.position_pct[0] / 100.0 * view_px[0], p.line.position_pct[1] / 100.0 * view_px[1]];
    let at = |a: f32, c: f32| [center[0] + dir[0] * a + normal[0] * c, center[1] + dir[1] * a + normal[1] * c];
    Envelope {
        corners: [at(along[0], across[0]), at(along[1], across[0]), at(along[1], across[1]), at(along[0], across[1])],
        center_px: center,
        dir,
        normal,
        along_px: along,
        across_px: across,
    }
}

/// Range of [`model::motion_offset`] over all times.
fn motion_range(m: &MotionParams, extent_px: f32) -> [f32; 2] {
    match m.kind {
        MotionKind::Static => [0.0, 0.0],
        MotionKind::Sweep => {
            let half = 0.5 * (extent_px * m.span_pct / 100.0).max(0.0);
            [-half, half]
        }
        MotionKind::Sine => [-m.amplitude_px.abs(), m.amplitude_px.abs()],
    }
}

/// Range of [`model::deform_offset`] over all positions and times. Conservative: a
/// feature scrolling along the line is assumed to pass every point of it.
fn deform_range(d: &DeformParams) -> [f32; 2] {
    match d.kind {
        DeformKind::None => [0.0, 0.0],
        DeformKind::Step | DeformKind::Bump | DeformKind::Box => [d.height_px.min(0.0), d.height_px.max(0.0)],
        DeformKind::Sine => [-d.amplitude_px.abs(), d.amplitude_px.abs()],
    }
}

/// Range of the line displacement a scan produces: every seen height times `gain` (px per
/// mm, signed), and 0 where a profile starts without a seen height.
fn scan_range(s: &Scan, gain: f32) -> [f32; 2] {
    let mut range = [0.0f32, 0.0f32];
    for (h, v) in s.height.iter().zip(&s.visible) {
        if *v != 0 && h.is_finite() {
            let d = h * gain;
            range = [range[0].min(d), range[1].max(d)];
        }
    }
    range
}

/// What [`pattern_params`] draws.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Pattern {
    /// Nothing: a black view, for the camera's background.
    Blank,
    /// A line (or, with `width_pct`, a band) across the whole view.
    Line {
        /// `Horizontal` lies along x at `position_pct` of the height; `Vertical` along y
        /// at `position_pct` of the width.
        orientation: Orientation,
        /// Position of its centre across the line, in percent of the view.
        position_pct: f32,
        /// Width in percent of the view across the line, for a band (100 at 50 % lights
        /// the whole view); the calibration line's width when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width_pct: Option<f32>,
    },
}

/// `params` turned into a clean calibration line: static, undeformed, no scan or noise,
/// a hard profile without end caps at full intensity and at least
/// [`MIN_CALIBRATION_WIDTH_PX`] wide. The colour is kept, since the camera may only see
/// the user's laser colour through its filter.
pub fn calibration_params(params: &LaserParams) -> LaserParams {
    let mut p = params.clone();
    p.line.intensity_pct = 100.0;
    p.line.width_px = p.line.width_px.max(MIN_CALIBRATION_WIDTH_PX);
    p.line.tilt_deg = 0.0;
    p.profile.kind = ProfileKind::Hard;
    p.profile.end_softness_px = 0.0;
    p.motion.kind = MotionKind::Static;
    p.deform.kind = DeformKind::None;
    p.scan.enabled = false;
    p.noise.ambient_pct = 0.0;
    p.noise.speckle_pct = 0.0;
    p.noise.noise_pct = 0.0;
    p
}

/// `base` (see [`calibration_params`]) showing `pattern` in a view of `view_px`.
pub fn pattern_params(base: &LaserParams, pattern: Pattern, view_px: [f32; 2]) -> LaserParams {
    let mut p = base.clone();
    match pattern {
        Pattern::Blank => p.line.intensity_pct = 0.0,
        Pattern::Line { orientation, position_pct, width_pct } => {
            if let Some(w) = width_pct {
                let across = match orientation {
                    Orientation::Horizontal => view_px[1],
                    Orientation::Vertical => view_px[0],
                };
                p.line.width_px = (w / 100.0 * across).max(1.0);
            }
            p.line.orientation = orientation;
            p.line.tilt_deg = 0.0;
            p.line.length = 100.0;
            p.line.length_unit = LengthUnit::Percent;
            p.line.position_pct = match orientation {
                Orientation::Horizontal => [50.0, position_pct],
                Orientation::Vertical => [position_pct, 50.0],
            };
        }
    }
    p
}

/// Result of [`fit_into`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct Fit {
    /// The adjusted parameters.
    pub params: LaserParams,
    /// The envelope now lies inside the field. False when the deformation or scan heights
    /// alone are too large for it, or the field is empty.
    pub fits: bool,
}

/// `params` with the line moved to the middle of the convex `field` (view px) and, if
/// needed, its length and motion range shrunk so the whole envelope stays `margin` px
/// inside it. Nothing grows; deformation and scan heights are left alone.
pub fn fit_into(params: &LaserParams, field: &[[f32; 2]], view_px: [f32; 2], margin: f32, scan: Option<&Scan>) -> Fit {
    if field.len() < 3 || view_px[0] <= 0.0 || view_px[1] <= 0.0 {
        return Fit { params: params.clone(), fits: false };
    }
    let target = centroid(field);
    let at_scale = |s: f32| {
        let mut p = scaled(params, s);
        // Put the envelope's middle (not the resting center) on the field's middle.
        let e = line_envelope(&p, view_px, scan);
        let mid_along = 0.5 * (e.along_px[0] + e.along_px[1]);
        let mid_across = 0.5 * (e.across_px[0] + e.across_px[1]);
        let center = [
            target[0] - e.dir[0] * mid_along - e.normal[0] * mid_across,
            target[1] - e.dir[1] * mid_along - e.normal[1] * mid_across,
        ];
        p.line.position_pct = [center[0] / view_px[0] * 100.0, center[1] / view_px[1] * 100.0];
        let fits = line_envelope(&p, view_px, scan).inside(field, margin);
        (p, fits)
    };
    let (p, fits) = at_scale(1.0);
    if fits {
        return Fit { params: p, fits: true };
    }
    // The envelope shrinks monotonically with the scale: bisect for the largest that fits.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    if !at_scale(0.0).1 {
        return Fit { params: at_scale(0.0).0, fits: false };
    }
    for _ in 0..30 {
        let mid = 0.5 * (lo + hi);
        if at_scale(mid).1 { lo = mid } else { hi = mid }
    }
    Fit { params: at_scale(lo).0, fits: true }
}

/// `p` with the line length and motion range times `s`.
fn scaled(p: &LaserParams, s: f32) -> LaserParams {
    let mut p = p.clone();
    p.line.length *= s;
    p.motion.span_pct *= s;
    p.motion.amplitude_px *= s;
    if p.scan.scale == ScanScale::Fixed {
        p.scan.px_per_mm *= s;
    }
    p
}

fn centroid(polygon: &[[f32; 2]]) -> [f32; 2] {
    let n = polygon.len() as f32;
    let sum = polygon.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0], a[1] + p[1]]);
    [sum[0] / n, sum[1] / n]
}

/// Distance of `p` from the nearest edge of the convex `polygon`, negative outside.
fn inset_distance(polygon: &[[f32; 2]], p: [f32; 2]) -> f32 {
    // Orientation from the signed area, so either winding works.
    let area: f32 = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|(a, b)| {
            let e = [b[0] - a[0], b[1] - a[1]];
            let len = (e[0] * e[0] + e[1] * e[1]).sqrt().max(1e-6);
            sign * (e[0] * (p[1] - a[1]) - e[1] * (p[0] - a[0])) / len
        })
        .fold(f32::INFINITY, f32::min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{ScanParams, SweepMode};

    const VIEW: [f32; 2] = [1000.0, 500.0];

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-2
    }

    fn approx2(a: [f32; 2], b: [f32; 2]) -> bool {
        approx(a[0], b[0]) && approx(a[1], b[1])
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<[f32; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    #[test]
    fn static_line_is_its_length_by_its_width() {
        let mut p = LaserParams::default();
        p.profile.end_softness_px = 0.0;
        // 80 % of 1000 px, 6 px wide, centered.
        let e = line_envelope(&p, VIEW, None);
        assert!(approx2(e.center_px, [500.0, 250.0]));
        assert!(approx2(e.along_px, [-400.0, 400.0]));
        assert!(approx2(e.across_px, [-3.0, 3.0]));
        assert!(approx2(e.corners[0], [100.0, 247.0]));
        assert!(approx2(e.corners[2], [900.0, 253.0]));
    }

    #[test]
    fn soft_ends_extend_the_line_by_half_their_length() {
        let mut p = LaserParams::default();
        p.profile.end_softness_px = 20.0;
        assert!(approx2(line_envelope(&p, VIEW, None).along_px, [-410.0, 410.0]));
    }

    #[test]
    fn motion_widens_across() {
        let mut p = LaserParams::default();
        p.line.width_px = 0.0;
        p.motion = MotionParams { kind: MotionKind::Sweep, span_pct: 50.0, sweep_mode: SweepMode::Wrap, ..Default::default() };
        // Horizontal line: the sweep runs over 50 % of the 500 px height.
        assert!(approx2(line_envelope(&p, VIEW, None).across_px, [-125.0, 125.0]));
        p.motion = MotionParams { kind: MotionKind::Sine, amplitude_px: -40.0, ..Default::default() };
        assert!(approx2(line_envelope(&p, VIEW, None).across_px, [-40.0, 40.0]));
    }

    #[test]
    fn motion_range_covers_sampled_offsets() {
        let m = MotionParams { kind: MotionKind::Sweep, speed_px_s: 333.0, span_pct: 70.0, sweep_mode: SweepMode::Bounce, ..Default::default() };
        let r = motion_range(&m, 500.0);
        for i in 0..1000 {
            let o = model::motion_offset(&m, i as f64 * 0.013, 500.0);
            assert!(o >= r[0] - 1e-3 && o <= r[1] + 1e-3, "{o} outside {r:?}");
        }
    }

    #[test]
    fn deformation_adds_to_motion_on_its_side() {
        let mut p = LaserParams::default();
        p.line.width_px = 2.0;
        p.deform = DeformParams { kind: DeformKind::Step, height_px: -60.0, ..Default::default() };
        p.motion = MotionParams { kind: MotionKind::Sine, amplitude_px: 10.0, ..Default::default() };
        assert!(approx2(line_envelope(&p, VIEW, None).across_px, [-71.0, 11.0]));
        p.deform = DeformParams { kind: DeformKind::Sine, amplitude_px: 5.0, ..Default::default() };
        assert!(approx2(line_envelope(&p, VIEW, None).across_px, [-16.0, 16.0]));
    }

    #[test]
    fn deform_range_covers_sampled_offsets() {
        for kind in [DeformKind::Step, DeformKind::Bump, DeformKind::Box, DeformKind::Sine] {
            let d = DeformParams { kind, height_px: 35.0, amplitude_px: 12.0, ..Default::default() };
            let r = deform_range(&d);
            for i in 0..2000 {
                let o = model::deform_offset(&d, i as f32 - 1000.0, 0.0);
                assert!(o >= r[0] - 1e-3 && o <= r[1] + 1e-3, "{kind:?}: {o} outside {r:?}");
            }
        }
    }

    #[test]
    fn vertical_and_tilted_lines_rotate_the_envelope() {
        let mut p = LaserParams::default();
        p.profile.end_softness_px = 0.0;
        p.line.orientation = Orientation::Vertical;
        p.line.length = 100.0;
        let e = line_envelope(&p, VIEW, None);
        // Along y now: 100 % of the 500 px height.
        assert!(approx2(e.dir, [0.0, 1.0]));
        assert!(approx2(e.along_px, [-250.0, 250.0]));
        assert!(e.corners.iter().all(|c| approx((c[0] - 500.0).abs(), 3.0)), "{:?}", e.corners);

        p.line.orientation = Orientation::Horizontal;
        p.line.tilt_deg = 30.0;
        let e = line_envelope(&p, VIEW, None);
        assert!(approx2(e.dir, [30f32.to_radians().cos(), 30f32.to_radians().sin()]));
    }

    #[test]
    fn scan_heights_set_the_across_range() {
        use crate::mesh::box_mesh;
        use crate::scan::{Progress, SliceSettings, slice};
        use std::sync::atomic::AtomicBool;

        let m = box_mesh([0.0; 3], [10.0, 5.0, 3.0]);
        let s = SliceSettings { lateral_res_mm: 1.0, travel_step_mm: 1.0, ..Default::default() };
        let scan = slice(&m, &s, "", &AtomicBool::new(false), &Progress::new(0)).unwrap();
        let mut p = LaserParams::default();
        p.line.width_px = 0.0;
        p.scan = ScanParams { enabled: true, scale: ScanScale::Fixed, px_per_mm: 10.0, ..Default::default() };
        let e = line_envelope(&p, VIEW, Some(&scan));
        // Higher surfaces move the line up (negative y): 3 mm × 10 px/mm.
        assert!(approx(e.across_px[0], -30.0), "{:?}", e.across_px);
        assert!(approx(e.across_px[1], 0.0));
        // In fixed scale the scan width sets the length.
        assert!(approx(e.along_px[1] - e.along_px[0], scan.width_mm() * 10.0 + p.profile.end_softness_px));
        // A disabled scan does not shape the line.
        p.scan.enabled = false;
        assert!(approx2(line_envelope(&p, VIEW, Some(&scan)).across_px, [0.0, 0.0]));
    }

    #[test]
    fn calibration_line_is_clean_and_keeps_the_colour() {
        let mut p = LaserParams::default();
        p.line.width_px = 1.0;
        p.line.intensity_pct = 30.0;
        p.color.wavelength_nm = 520.0;
        p.motion.kind = MotionKind::Sine;
        p.deform.kind = DeformKind::Bump;
        p.noise.noise_pct = 5.0;
        let c = calibration_params(&p);
        assert_eq!(c.line.width_px, MIN_CALIBRATION_WIDTH_PX);
        assert_eq!(c.line.intensity_pct, 100.0);
        assert_eq!(c.profile.kind, ProfileKind::Hard);
        assert_eq!(c.profile.end_softness_px, 0.0);
        assert_eq!((c.motion.kind, c.deform.kind, c.scan.enabled), (MotionKind::Static, DeformKind::None, false));
        assert_eq!(c.noise.noise_pct, 0.0);
        assert_eq!(c.color, p.color);
    }

    #[test]
    fn patterns_place_full_lines() {
        let base = calibration_params(&LaserParams::default());
        let h = pattern_params(&base, Pattern::Line { orientation: Orientation::Horizontal, position_pct: 20.0, width_pct: None }, VIEW);
        let e = line_envelope(&h, VIEW, None);
        assert!(approx2(e.center_px, [500.0, 100.0]));
        assert!(approx2(e.along_px, [-500.0, 500.0]));
        let v = pattern_params(&base, Pattern::Line { orientation: Orientation::Vertical, position_pct: 75.0, width_pct: None }, VIEW);
        let e = line_envelope(&v, VIEW, None);
        assert!(approx2(e.center_px, [750.0, 250.0]));
        assert!(approx2(e.along_px, [-250.0, 250.0]));
        assert_eq!(pattern_params(&base, Pattern::Blank, VIEW).line.intensity_pct, 0.0);
        // A band: 10 % of the 1000 px width, and of the 500 px height.
        let band = |orientation, width_pct| pattern_params(&base, Pattern::Line { orientation, position_pct: 50.0, width_pct: Some(width_pct) }, VIEW);
        assert_eq!(band(Orientation::Vertical, 10.0).line.width_px, 100.0);
        assert_eq!(band(Orientation::Horizontal, 10.0).line.width_px, 50.0);
        assert_eq!(band(Orientation::Horizontal, 100.0).line.width_px, 500.0);
    }

    #[test]
    fn pattern_json_is_tagged() {
        let p: Pattern = serde_json::from_str(r#"{"kind":"line","orientation":"Vertical","position_pct":30}"#).unwrap();
        assert_eq!(p, Pattern::Line { orientation: Orientation::Vertical, position_pct: 30.0, width_pct: None });
        let b: Pattern = serde_json::from_str(r#"{"kind":"line","orientation":"Horizontal","position_pct":5,"width_pct":2.5}"#).unwrap();
        assert_eq!(b, Pattern::Line { orientation: Orientation::Horizontal, position_pct: 5.0, width_pct: Some(2.5) });
        let b: Pattern = serde_json::from_str(r#"{"kind":"blank"}"#).unwrap();
        assert_eq!(b, Pattern::Blank);
    }

    #[test]
    fn inside_respects_margin_and_winding() {
        let mut p = LaserParams::default();
        p.profile.end_softness_px = 0.0;
        let e = line_envelope(&p, VIEW, None); // x 100..900, y 247..253
        let field = rect(90.0, 200.0, 910.0, 300.0);
        assert!(e.inside(&field, 5.0));
        assert!(!e.inside(&field, 15.0));
        let reversed: Vec<_> = field.iter().rev().copied().collect();
        assert!(e.inside(&reversed, 5.0));
        assert!(!e.inside(&rect(200.0, 0.0, 1000.0, 500.0), 0.0));
    }

    #[test]
    fn fit_recentres_and_shrinks_into_the_field() {
        let mut p = LaserParams::default();
        p.profile.end_softness_px = 0.0;
        p.motion = MotionParams { kind: MotionKind::Sine, amplitude_px: 100.0, ..Default::default() };
        let field = rect(400.0, 100.0, 700.0, 300.0);
        let fit = fit_into(&p, &field, VIEW, 10.0, None);
        assert!(fit.fits);
        let e = line_envelope(&fit.params, VIEW, None);
        assert!(e.inside(&field, 10.0 - 1e-2));
        // Centered in the field, and as large as the field allows (within the bisection).
        assert!(approx2(e.center_px, [550.0, 200.0]));
        let length = e.along_px[1] - e.along_px[0];
        assert!((279.0..=280.01).contains(&length), "{length}");
        assert!(fit.params.motion.amplitude_px < 100.0);
    }

    #[test]
    fn fit_keeps_a_line_that_already_fits() {
        let mut p = LaserParams::default();
        p.profile.end_softness_px = 0.0;
        p.line.length = 10.0;
        let fit = fit_into(&p, &rect(0.0, 0.0, 1000.0, 500.0), VIEW, 0.0, None);
        assert!(fit.fits);
        assert_eq!(fit.params.line.length, 10.0);
    }

    #[test]
    fn fit_reports_a_deformation_too_large_for_the_field() {
        let mut p = LaserParams::default();
        p.deform = DeformParams { kind: DeformKind::Step, height_px: -300.0, ..Default::default() };
        let fit = fit_into(&p, &rect(0.0, 100.0, 1000.0, 300.0), VIEW, 0.0, None);
        assert!(!fit.fits);
        assert!(!fit_into(&p, &[[0.0, 0.0], [1.0, 1.0]], VIEW, 0.0, None).fits);
    }
}
