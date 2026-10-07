//! Built-in presets and JSON save/load.

use crate::params::{
    ColorMode, DeformKind, LaserParams, LengthUnit, MotionKind, Orientation, ProfileKind, SweepMode,
};

/// The built-in presets as `(name, parameters)`, in display order.
pub fn builtin() -> Vec<(&'static str, LaserParams)> {
    let base = LaserParams::default();
    // The coloured presets name their laser; the default colour is white.
    let mut red = base.clone();
    red.color.mode = ColorMode::Wavelength;

    let mut sweep = red.clone();
    sweep.motion.kind = MotionKind::Sweep;
    sweep.motion.sweep_mode = SweepMode::Wrap;
    sweep.motion.speed_px_s = 300.0;
    sweep.line.length = 100.0;

    let mut vertical_sine = red.clone();
    vertical_sine.line.orientation = Orientation::Vertical;
    vertical_sine.color.wavelength_nm = 520.0;
    vertical_sine.motion.kind = MotionKind::Sine;
    vertical_sine.motion.amplitude_px = 200.0;
    vertical_sine.motion.frequency_hz = 0.5;

    let mut step = red.clone();
    step.deform.kind = DeformKind::Box;
    step.deform.height_px = -80.0;
    step.deform.width_px = 300.0;
    step.deform.slope_px = 30.0;
    step.deform.scroll_px_s = 150.0;
    step.line.length = 100.0;
    step.profile.end_softness_px = 0.0;

    let mut realistic = red.clone();
    realistic.color.wavelength_nm = 405.0;
    realistic.line.intensity_pct = 85.0;
    realistic.line.width_px = 4.0;
    realistic.noise.ambient_pct = 4.0;
    realistic.noise.speckle_pct = 60.0;
    realistic.noise.grain_px = 1.5;
    realistic.noise.noise_pct = 1.5;
    realistic.deform.kind = DeformKind::Sine;
    realistic.deform.amplitude_px = 15.0;
    realistic.deform.wavelength_px = 400.0;

    let mut hard = base.clone();
    hard.color.custom_rgb = [1.0, 1.0, 1.0];
    hard.profile.kind = ProfileKind::Hard;
    hard.profile.end_softness_px = 0.0;
    hard.line.width_px = 1.0;
    hard.line.length = 1000.0;
    hard.line.length_unit = LengthUnit::Pixels;
    hard.color.mode = ColorMode::Custom;

    vec![
        ("Static red Gaussian", red),
        ("Sweep scan", sweep),
        ("Vertical green oscillation", vertical_sine),
        ("Object passing (box)", step),
        ("Realistic blue + speckle", realistic),
        ("1 px hard white line", hard),
    ]
}

/// Serializes parameters as a preset file (pretty JSON).
pub fn to_json(p: &LaserParams) -> Result<String, String> {
    serde_json::to_string_pretty(p).map_err(|e| e.to_string())
}

/// Parses a preset file; missing fields take their defaults.
pub fn from_json(s: &str) -> Result<LaserParams, String> {
    serde_json::from_str(s).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip() {
        for (_, p) in builtin() {
            assert_eq!(from_json(&to_json(&p).unwrap()).unwrap(), p);
        }
    }

    #[test]
    fn partial_json_uses_defaults() {
        let p = from_json(r#"{"line": {"width_px": 3.5}}"#).unwrap();
        assert_eq!(p.line.width_px, 3.5);
        assert_eq!(p.line.intensity_pct, 100.0);
        assert_eq!(p.motion, LaserParams::default().motion);
    }
}
