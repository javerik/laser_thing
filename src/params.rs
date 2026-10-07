//! All user-tunable parameters of the emulated laser line.
//!
//! Every section is a flat struct (rather than enums carrying data) so values
//! survive switching between modes, and `#[serde(default)]` keeps older preset
//! files loadable when new fields are added.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LengthUnit {
    /// Absolute length in physical pixels.
    Pixels,
    /// Percent of the live view's extent along the line direction.
    Percent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileKind {
    /// Gaussian cross-section; `width` is the FWHM.
    Gaussian,
    /// Flat core with soft (smoothstep) edges.
    TopHat,
    /// Uniform intensity, edges rendered by exact pixel coverage.
    Hard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorMode {
    Wavelength,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotionKind {
    Static,
    Sweep,
    Sine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SweepMode {
    /// Jump back to the start after reaching the end.
    Wrap,
    /// Reverse direction at both ends.
    Bounce,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeformKind {
    None,
    Step,
    Bump,
    Box,
    Sine,
}

/// How the scan's along-line millimetres are sized on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanScale {
    /// Stretch the scan width over the configured line length.
    FitLine,
    /// Fixed pixels per mm; the line length follows the scan width.
    Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateMode {
    /// One profile per rendered frame.
    PerFrame,
    /// Fixed profile rate, scaled by the simulation speed.
    PerSecond,
}

/// How object height turns into line displacement on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeightGain {
    /// Same px/mm as along the line: the line shows the true-scale profile.
    MatchScale,
    /// `px/mm · sin(triangulation angle)`, what a camera at that angle would see.
    FromAngle,
    /// Fixed px/mm, e.g. calibrated against a real camera.
    Manual,
}

/// What happens after the last profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndMode {
    Loop,
    Bounce,
    Once,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LineParams {
    /// Peak intensity in percent (100 % = full pixel value).
    pub intensity_pct: f32,
    /// Line width in pixels (FWHM for Gaussian).
    pub width_px: f32,
    pub length: f32,
    pub length_unit: LengthUnit,
    /// Line center in percent of the live view (x, y).
    pub position_pct: [f32; 2],
    pub orientation: Orientation,
    /// Additional rotation in degrees on top of the orientation.
    pub tilt_deg: f32,
}

impl Default for LineParams {
    fn default() -> Self {
        Self {
            intensity_pct: 100.0,
            width_px: 6.0,
            length: 80.0,
            length_unit: LengthUnit::Percent,
            position_pct: [50.0, 50.0],
            orientation: Orientation::Horizontal,
            tilt_deg: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileParams {
    pub kind: ProfileKind,
    /// Edge transition width of the top-hat profile in pixels.
    pub edge_softness_px: f32,
    /// Fade-out length at both line ends in pixels (0 = hard ends).
    pub end_softness_px: f32,
}

impl Default for ProfileParams {
    fn default() -> Self {
        Self {
            kind: ProfileKind::Gaussian,
            edge_softness_px: 2.0,
            end_softness_px: 20.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorParams {
    pub mode: ColorMode,
    pub wavelength_nm: f32,
    pub custom_rgb: [f32; 3],
}

impl Default for ColorParams {
    fn default() -> Self {
        Self {
            mode: ColorMode::Wavelength,
            wavelength_nm: 660.0,
            custom_rgb: [1.0, 1.0, 1.0],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionParams {
    pub kind: MotionKind,
    /// Sweep speed in px/s (negative reverses direction).
    pub speed_px_s: f32,
    /// Sweep span in percent of the view extent perpendicular to the line.
    pub span_pct: f32,
    pub sweep_mode: SweepMode,
    pub amplitude_px: f32,
    pub frequency_hz: f32,
    pub phase_deg: f32,
}

impl Default for MotionParams {
    fn default() -> Self {
        Self {
            kind: MotionKind::Static,
            speed_px_s: 200.0,
            span_pct: 80.0,
            sweep_mode: SweepMode::Bounce,
            amplitude_px: 100.0,
            frequency_hz: 0.5,
            phase_deg: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeformParams {
    pub kind: DeformKind,
    /// Feature position along the line in px, relative to the line center.
    pub position_px: f32,
    /// Displacement perpendicular to the line in px (sign = direction).
    pub height_px: f32,
    /// Bump FWHM / box top width in px.
    pub width_px: f32,
    /// Box flank length in px.
    pub slope_px: f32,
    pub amplitude_px: f32,
    pub wavelength_px: f32,
    /// Feature travel speed along the line in px/s (object passing through).
    pub scroll_px_s: f32,
}

impl Default for DeformParams {
    fn default() -> Self {
        Self {
            kind: DeformKind::None,
            position_px: 0.0,
            height_px: -60.0,
            width_px: 200.0,
            slope_px: 40.0,
            amplitude_px: 20.0,
            wavelength_px: 300.0,
            scroll_px_s: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NoiseParams {
    /// Ambient (background) light level in percent.
    pub ambient_pct: f32,
    /// Speckle contrast in percent (0 = clean line).
    pub speckle_pct: f32,
    /// Speckle grain size in px.
    pub grain_px: f32,
    /// Sensor-like additive noise, standard deviation in percent.
    pub noise_pct: f32,
    /// Re-randomize speckle and noise every frame.
    pub animated: bool,
}

impl Default for NoiseParams {
    fn default() -> Self {
        Self {
            ambient_pct: 0.0,
            speckle_pct: 0.0,
            grain_px: 2.0,
            noise_pct: 0.0,
            animated: true,
        }
    }
}

/// Display and playback of a sliced object scan (the scan itself is not a parameter).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanParams {
    /// Shape the line from the loaded scan instead of the synthetic deformation.
    pub enabled: bool,
    pub scale: ScanScale,
    /// Pixels per mm along the line in `Fixed` mode.
    pub px_per_mm: f32,
    pub height_gain: HeightGain,
    /// Line displacement per mm of height in `Manual` mode.
    pub height_px_per_mm: f32,
    /// Higher surfaces move the line down instead of up.
    pub invert_height: bool,
    pub rate: RateMode,
    pub profiles_per_s: f32,
    pub end: EndMode,
}

impl Default for ScanParams {
    fn default() -> Self {
        Self {
            enabled: true,
            scale: ScanScale::FitLine,
            px_per_mm: 5.0,
            height_gain: HeightGain::MatchScale,
            height_px_per_mm: 2.5,
            invert_height: false,
            rate: RateMode::PerFrame,
            profiles_per_s: 60.0,
            end: EndMode::Loop,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserParams {
    pub line: LineParams,
    pub profile: ProfileParams,
    pub color: ColorParams,
    pub motion: MotionParams,
    pub deform: DeformParams,
    pub noise: NoiseParams,
    pub scan: ScanParams,
    /// Simulation speed multiplier for all time-dependent effects.
    pub time_scale: f32,
}

impl Default for LaserParams {
    fn default() -> Self {
        Self {
            line: LineParams::default(),
            profile: ProfileParams::default(),
            color: ColorParams::default(),
            motion: MotionParams::default(),
            deform: DeformParams::default(),
            noise: NoiseParams::default(),
            scan: ScanParams::default(),
            time_scale: 1.0,
        }
    }
}
