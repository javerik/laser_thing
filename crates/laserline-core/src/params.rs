//! All user-tunable parameters of the emulated laser line.
//!
//! Every section is a flat struct (rather than enums carrying data) so values
//! survive switching between modes, and `#[serde(default)]` keeps older preset
//! files loadable when new fields are added.

use serde::{Deserialize, Serialize};

/// Base direction of the line on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum Orientation {
    /// Along the x axis.
    Horizontal,
    /// Along the y axis.
    Vertical,
}

/// Unit of [`LineParams::length`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum LengthUnit {
    /// Absolute length in physical pixels.
    Pixels,
    /// Percent of the live view's extent along the line direction.
    Percent,
}

/// Shape of the line's cross-section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum ProfileKind {
    /// Gaussian cross-section; `width` is the FWHM.
    Gaussian,
    /// Flat core with soft (smoothstep) edges.
    TopHat,
    /// Uniform intensity, edges rendered by exact pixel coverage.
    Hard,
}

/// How the line colour is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum ColorMode {
    /// From [`ColorParams::wavelength_nm`].
    Wavelength,
    /// From [`ColorParams::custom_rgb`].
    Custom,
}

/// Movement of the line perpendicular to itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum MotionKind {
    /// No movement.
    Static,
    /// Constant speed across a span, see [`SweepMode`].
    Sweep,
    /// Sinusoidal oscillation.
    Sine,
}

/// What a sweep does at the end of its span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum SweepMode {
    /// Jump back to the start after reaching the end.
    Wrap,
    /// Reverse direction at both ends.
    Bounce,
}

/// Synthetic shape the line is deformed by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum DeformKind {
    /// Straight line.
    None,
    /// Offset by `height_px` from `position_px` on.
    Step,
    /// Gaussian bump of FWHM `width_px`.
    Bump,
    /// Trapezoid: top `width_px` wide, flanks `slope_px` long.
    Box,
    /// Sine wave of `amplitude_px` and `wavelength_px`.
    Sine,
}

/// How the scan's along-line millimetres are sized on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum ScanScale {
    /// Stretch the scan width over the configured line length.
    FitLine,
    /// Fixed pixels per mm; the line length follows the scan width.
    Fixed,
}

/// How fast scan profiles advance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum RateMode {
    /// One profile per rendered frame.
    PerFrame,
    /// Fixed profile rate, scaled by the simulation speed.
    PerSecond,
}

/// How object height turns into line displacement on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
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
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum EndMode {
    /// Start over from the first profile.
    Loop,
    /// Play backwards, then forwards again.
    Bounce,
    /// Stop at the last profile.
    Once,
}

/// Geometry and intensity of the line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(default)]
pub struct LineParams {
    /// Peak intensity in percent (100 % = full pixel value).
    pub intensity_pct: f32,
    /// Line width in pixels (FWHM for Gaussian).
    pub width_px: f32,
    /// Line length, in [`LineParams::length_unit`].
    pub length: f32,
    /// Unit of `length`.
    pub length_unit: LengthUnit,
    /// Line center in percent of the live view (x, y).
    pub position_pct: [f32; 2],
    /// Base direction of the line.
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

/// Cross-section of the line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(default)]
pub struct ProfileParams {
    /// Cross-section shape.
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

/// Colour of the line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(default)]
pub struct ColorParams {
    /// Which of the two fields below sets the colour.
    pub mode: ColorMode,
    /// Laser wavelength in nm (visible: 380..780).
    pub wavelength_nm: f32,
    /// Linear RGB, each 0..1.
    pub custom_rgb: [f32; 3],
}

impl Default for ColorParams {
    /// White: brightest on any display, and seen through a camera's laser filter too.
    fn default() -> Self {
        Self {
            mode: ColorMode::Custom,
            wavelength_nm: 660.0,
            custom_rgb: [1.0, 1.0, 1.0],
        }
    }
}

/// Movement of the line perpendicular to itself; fields apply to the selected kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(default)]
pub struct MotionParams {
    /// Kind of movement.
    pub kind: MotionKind,
    /// Sweep speed in px/s (negative reverses direction).
    pub speed_px_s: f32,
    /// Sweep span in percent of the view extent perpendicular to the line.
    pub span_pct: f32,
    /// Behaviour at the ends of the sweep span.
    pub sweep_mode: SweepMode,
    /// Sine amplitude in px.
    pub amplitude_px: f32,
    /// Sine frequency in Hz.
    pub frequency_hz: f32,
    /// Sine phase at t = 0 in degrees.
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

/// Synthetic deformation of the line; fields apply to the selected kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(default)]
pub struct DeformParams {
    /// Shape of the deformation.
    pub kind: DeformKind,
    /// Feature position along the line in px, relative to the line center.
    pub position_px: f32,
    /// Displacement perpendicular to the line in px (sign = direction).
    pub height_px: f32,
    /// Bump FWHM / box top width in px.
    pub width_px: f32,
    /// Box flank length in px.
    pub slope_px: f32,
    /// Sine amplitude in px.
    pub amplitude_px: f32,
    /// Sine wavelength in px along the line.
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

/// Ambient light, speckle and sensor noise.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
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
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(default)]
pub struct ScanParams {
    /// Shape the line from the loaded scan instead of the synthetic deformation.
    pub enabled: bool,
    /// How the scan width maps to screen pixels.
    pub scale: ScanScale,
    /// Pixels per mm along the line in `Fixed` mode.
    pub px_per_mm: f32,
    /// How heights map to line displacement.
    pub height_gain: HeightGain,
    /// Line displacement per mm of height in `Manual` mode.
    pub height_px_per_mm: f32,
    /// Higher surfaces move the line down instead of up.
    pub invert_height: bool,
    /// How fast profiles advance.
    pub rate: RateMode,
    /// Profiles per simulated second in `PerSecond` mode.
    pub profiles_per_s: f32,
    /// What happens after the last profile.
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

/// Every parameter of the emulated line; the format of preset files.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(default)]
pub struct LaserParams {
    /// Geometry and intensity of the line.
    pub line: LineParams,
    /// Cross-section.
    pub profile: ProfileParams,
    /// Colour.
    pub color: ColorParams,
    /// Movement perpendicular to the line.
    pub motion: MotionParams,
    /// Synthetic deformation (when no scan drives the line).
    pub deform: DeformParams,
    /// Ambient light, speckle and sensor noise.
    pub noise: NoiseParams,
    /// Playback of a loaded scan.
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
