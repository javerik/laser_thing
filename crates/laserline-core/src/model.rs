//! Pure math of the laser model: motion, deformation, cross-section and color.
//!
//! Kept free of GUI/GPU types so it can be unit-tested. The shader mirrors
//! `profile_intensity`; deformation is sampled here and uploaded to the GPU.

use crate::params::{
    ColorMode, ColorParams, DeformKind, DeformParams, LineParams, MotionKind, MotionParams,
    Orientation, ProfileKind, ProfileParams, SweepMode,
};
use std::f32::consts::TAU;

/// Ratio between FWHM and standard deviation of a Gaussian.
pub const FWHM_PER_SIGMA: f32 = 2.354_82;

/// Line direction angle in radians (screen coordinates, y pointing down).
pub fn line_angle(line: &LineParams) -> f32 {
    let base: f32 = match line.orientation {
        Orientation::Horizontal => 0.0,
        Orientation::Vertical => 90.0,
    };
    (base + line.tilt_deg).to_radians()
}

/// Extent of a `w`×`h` rectangle projected onto the unit vector `axis`.
pub fn extent_along(size: [f32; 2], axis: [f32; 2]) -> f32 {
    size[0] * axis[0].abs() + size[1] * axis[1].abs()
}

/// Displacement of the line perpendicular to itself at time `t` (seconds).
/// `extent_px` is the view extent along that perpendicular direction.
pub fn motion_offset(m: &MotionParams, t: f64, extent_px: f32) -> f32 {
    match m.kind {
        MotionKind::Static => 0.0,
        MotionKind::Sweep => {
            let span = (extent_px * m.span_pct / 100.0) as f64;
            if span <= 0.0 {
                return 0.0;
            }
            let s = m.speed_px_s as f64 * t;
            let p = match m.sweep_mode {
                SweepMode::Wrap => s.rem_euclid(span),
                SweepMode::Bounce => {
                    let p = s.rem_euclid(2.0 * span);
                    if p < span { p } else { 2.0 * span - p }
                }
            };
            (p - span / 2.0) as f32
        }
        MotionKind::Sine => {
            let phase = (TAU as f64) * m.frequency_hz as f64 * t + (m.phase_deg as f64).to_radians();
            m.amplitude_px * phase.sin() as f32
        }
    }
}

/// Deformation (perpendicular displacement in px) at position `u` along the line.
pub fn deform_offset(d: &DeformParams, u: f32, t: f64) -> f32 {
    let x = u - (d.scroll_px_s as f64 * t) as f32 - d.position_px;
    match d.kind {
        DeformKind::None => 0.0,
        DeformKind::Step => {
            if x >= 0.0 { d.height_px } else { 0.0 }
        }
        DeformKind::Bump => {
            let sigma = (d.width_px / FWHM_PER_SIGMA).max(1e-3);
            d.height_px * (-0.5 * (x / sigma).powi(2)).exp()
        }
        DeformKind::Box => {
            let a = x.abs();
            let half = 0.5 * d.width_px.max(0.0);
            if a <= half {
                d.height_px
            } else if d.slope_px > 0.0 && a < half + d.slope_px {
                d.height_px * (1.0 - (a - half) / d.slope_px)
            } else {
                0.0
            }
        }
        DeformKind::Sine => {
            let wl = d.wavelength_px.max(1e-3);
            // Deliberately anchored at the line center, not at `position_px`.
            let x = u - (d.scroll_px_s as f64 * t) as f32;
            d.amplitude_px * (TAU * x / wl).sin()
        }
    }
}

/// Fill `out` with deformation samples at `u = start + i * step`.
pub fn sample_deformation(d: &DeformParams, t: f64, start: f32, step: f32, out: &mut [f32]) {
    for (i, v) in out.iter_mut().enumerate() {
        *v = deform_offset(d, start + i as f32 * step, t);
    }
}

/// Normalized cross-section intensity (0..1) at distance `v` px from the line center.
/// Must stay in sync with `profile()` in `laser.wgsl`.
pub fn profile_intensity(p: &ProfileParams, width_px: f32, v: f32) -> f32 {
    let a = v.abs();
    let hw = 0.5 * width_px;
    match p.kind {
        ProfileKind::Gaussian => {
            let sigma = (width_px / FWHM_PER_SIGMA).max(1e-3);
            (-0.5 * (a / sigma).powi(2)).exp()
        }
        ProfileKind::TopHat => {
            let s = p.edge_softness_px.max(1e-3);
            1.0 - smoothstep(hw - 0.5 * s, hw + 0.5 * s, a)
        }
        // Coverage of the pixel [a-0.5, a+0.5] by the band [-hw, hw].
        ProfileKind::Hard => ((a + 0.5).min(hw) - (a - 0.5).max(-hw)).clamp(0.0, 1.0),
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Approximate display color of a wavelength (380–780 nm), normalized so the
/// strongest channel is 1 — brightness is controlled by intensity alone.
pub fn wavelength_to_rgb(nm: f32) -> [f32; 3] {
    let (r, g, b) = match nm {
        n if (380.0..440.0).contains(&n) => ((440.0 - n) / 60.0, 0.0, 1.0),
        n if (440.0..490.0).contains(&n) => (0.0, (n - 440.0) / 50.0, 1.0),
        n if (490.0..510.0).contains(&n) => (0.0, 1.0, (510.0 - n) / 20.0),
        n if (510.0..580.0).contains(&n) => ((n - 510.0) / 70.0, 1.0, 0.0),
        n if (580.0..645.0).contains(&n) => (1.0, (645.0 - n) / 65.0, 0.0),
        n if (645.0..=780.0).contains(&n) => (1.0, 0.0, 0.0),
        _ => (0.0, 0.0, 0.0),
    };
    let max = r.max(g).max(b);
    if max > 0.0 { [r / max, g / max, b / max] } else { [0.0; 3] }
}

/// Linear RGB (0..1) of the line for `c`.
pub fn laser_rgb(c: &ColorParams) -> [f32; 3] {
    match c.mode {
        ColorMode::Wavelength => wavelength_to_rgb(c.wavelength_nm),
        ColorMode::Custom => c.custom_rgb,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn sweep(mode: SweepMode) -> MotionParams {
        MotionParams { kind: MotionKind::Sweep, speed_px_s: 100.0, span_pct: 100.0, sweep_mode: mode, ..Default::default() }
    }

    #[test]
    fn static_has_no_offset() {
        assert_eq!(motion_offset(&MotionParams::default(), 12.3, 1000.0), 0.0);
    }

    #[test]
    fn sweep_wrap_jumps_back() {
        let m = sweep(SweepMode::Wrap);
        assert!(approx(motion_offset(&m, 0.0, 1000.0), -500.0));
        assert!(approx(motion_offset(&m, 5.0, 1000.0), 0.0));
        assert!(approx(motion_offset(&m, 9.99, 1000.0), 499.0));
        assert!(approx(motion_offset(&m, 10.0, 1000.0), -500.0));
    }

    #[test]
    fn sweep_bounce_reverses() {
        let m = sweep(SweepMode::Bounce);
        assert!(approx(motion_offset(&m, 10.0, 1000.0), 500.0));
        assert!(approx(motion_offset(&m, 15.0, 1000.0), 0.0));
        assert!(approx(motion_offset(&m, 20.0, 1000.0), -500.0));
    }

    #[test]
    fn sweep_negative_speed_stays_in_span() {
        let mut m = sweep(SweepMode::Wrap);
        m.speed_px_s = -100.0;
        let o = motion_offset(&m, 3.0, 1000.0);
        assert!((-500.0..500.0).contains(&o));
        assert!(approx(o, 200.0));
    }

    #[test]
    fn sine_motion() {
        let m = MotionParams { kind: MotionKind::Sine, amplitude_px: 50.0, frequency_hz: 1.0, ..Default::default() };
        assert!(approx(motion_offset(&m, 0.0, 0.0), 0.0));
        assert!(approx(motion_offset(&m, 0.25, 0.0), 50.0));
        assert!(approx(motion_offset(&m, 0.75, 0.0), -50.0));
    }

    #[test]
    fn deformations() {
        let mut d = DeformParams { position_px: 10.0, height_px: 20.0, width_px: 100.0, slope_px: 50.0, ..Default::default() };
        d.kind = DeformKind::Step;
        assert_eq!(deform_offset(&d, 9.0, 0.0), 0.0);
        assert_eq!(deform_offset(&d, 10.0, 0.0), 20.0);
        d.kind = DeformKind::Bump;
        assert!(approx(deform_offset(&d, 10.0, 0.0), 20.0));
        assert!(approx(deform_offset(&d, 60.0, 0.0), 10.0)); // half max at FWHM/2
        d.kind = DeformKind::Box;
        assert_eq!(deform_offset(&d, 60.0, 0.0), 20.0);
        assert!(approx(deform_offset(&d, 85.0, 0.0), 10.0));
        assert_eq!(deform_offset(&d, 111.0, 0.0), 0.0);
        // Scrolling moves the feature along the line.
        d.scroll_px_s = 10.0;
        assert_eq!(deform_offset(&d, 60.0 + 20.0, 2.0), 20.0);
    }

    #[test]
    fn profiles() {
        let mut p = ProfileParams { kind: ProfileKind::Gaussian, ..Default::default() };
        assert!(approx(profile_intensity(&p, 10.0, 0.0), 1.0));
        assert!(approx(profile_intensity(&p, 10.0, 5.0), 0.5));
        p.kind = ProfileKind::Hard;
        assert_eq!(profile_intensity(&p, 4.0, 0.0), 1.0);
        assert!(approx(profile_intensity(&p, 4.0, 2.0), 0.5));
        assert!(approx(profile_intensity(&p, 0.4, 0.0), 0.4));
        p.kind = ProfileKind::TopHat;
        assert!(approx(profile_intensity(&p, 10.0, 5.0), 0.5));
        assert_eq!(profile_intensity(&p, 10.0, 0.0), 1.0);
    }

    #[test]
    fn wavelength_colors() {
        assert_eq!(wavelength_to_rgb(660.0), [1.0, 0.0, 0.0]);
        assert_eq!(wavelength_to_rgb(520.0)[1], 1.0);
        assert_eq!(wavelength_to_rgb(450.0)[2], 1.0);
    }
}
