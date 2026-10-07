//! Browser bindings of laserline (`@javerik/laserline` on npm).
//!
//! - Pure functions and handles: [`Mesh`], [`Scan`], [`slice`], presets, plot curves.
//! - [`Emulator`] and [`Slicer`]: the core state machines, driven from JavaScript.
//! - [`LaserView`] and [`MeshView`]: wgpu renderers bound to a `<canvas>`, using
//!   WebGPU where available and WebGL2 otherwise (wasm32 only).
//!
//! Plain data (parameters, settings, scan metadata) crosses the boundary as JS objects
//! whose TypeScript types are generated from the Rust structs.

mod data;
mod state;
#[cfg(target_arch = "wasm32")]
mod view;

pub use data::{Mesh, Scan, slice};
pub use state::{Emulator, Slicer};
#[cfg(target_arch = "wasm32")]
pub use view::{LaserView, MeshView};

use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

pub(crate) fn to_js<T: Serialize + ?Sized>(value: &T) -> JsValue {
    value.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).unwrap_throw()
}

pub(crate) fn from_js<T: DeserializeOwned>(value: JsValue) -> Result<T, JsError> {
    serde_wasm_bindgen::from_value(value).map_err(|e| JsError::new(&e.to_string()))
}

pub(crate) fn js_err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen(typescript_custom_section)]
const TS_TYPES: &str = r#"
/** A built-in parameter set. */
export interface Preset { name: string; params: LaserParams }
/** Axis-aligned bounds in model units or mm. */
export interface Bounds { lo: [number, number, number]; hi: [number, number, number] }
/** A slice the host should run (in a worker), see `Slicer`. */
export interface SliceRequest { id: number; modelId: number; settings: SliceSettings; name: string }
/** The model loaded into a `Slicer`. */
export interface ModelInfo { id: number; name: string; location: string; triangles: number; bounds: Bounds }
/** Grid a slice would produce, or why the settings are unusable. */
export type GridInfo = { cols: number; rows: number; bytes: number } | { error: string };
/** Run of profile samples with the same visibility; `points` holds (column, height mm) pairs. */
export interface ProfileRun { visible: boolean; points: Float32Array }
/** Text to draw over the 3D view at (x, y), fractions of the view size from the top left. */
export interface ViewLabel { text: string; color: [number, number, number]; x: number; y: number }
"#;

/// Library version.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

#[wasm_bindgen(js_name = defaultParams, unchecked_return_type = "LaserParams")]
pub fn default_params() -> JsValue {
    to_js(&laserline_core::params::LaserParams::default())
}

#[wasm_bindgen(js_name = defaultSliceSettings, unchecked_return_type = "SliceSettings")]
pub fn default_slice_settings() -> JsValue {
    to_js(&laserline_core::scan::SliceSettings::default())
}

#[derive(Serialize)]
struct Preset {
    name: &'static str,
    params: laserline_core::params::LaserParams,
}

#[wasm_bindgen(js_name = builtinPresets, unchecked_return_type = "Preset[]")]
pub fn builtin_presets() -> JsValue {
    let presets: Vec<_> = laserline_core::presets::builtin().into_iter().map(|(name, params)| Preset { name, params }).collect();
    to_js(&presets)
}

#[wasm_bindgen(js_name = presetToJson)]
pub fn preset_to_json(#[wasm_bindgen(unchecked_param_type = "LaserParams")] params: JsValue) -> Result<String, JsError> {
    laserline_core::presets::to_json(&from_js(params)?).map_err(js_err)
}

/// Parses a preset file; missing fields take their defaults.
#[wasm_bindgen(js_name = presetFromJson, unchecked_return_type = "LaserParams")]
pub fn preset_from_json(text: &str) -> Result<JsValue, JsError> {
    laserline_core::presets::from_json(text).map(|p| to_js(&p)).map_err(js_err)
}

/// Cross-section intensity (0..1) at `n` points from `x0` to `x1` px across the line.
#[wasm_bindgen(js_name = profileCurve)]
pub fn profile_curve(
    #[wasm_bindgen(unchecked_param_type = "ProfileParams")] profile: JsValue,
    width_px: f32,
    x0: f32,
    x1: f32,
    n: usize,
) -> Result<Vec<f32>, JsError> {
    let p = from_js(profile)?;
    Ok(sample(x0, x1, n, |x| laserline_core::model::profile_intensity(&p, width_px, x)))
}

/// Deformation (px across the line) at `n` points from `x0` to `x1` px along the line,
/// at simulation time `t` seconds.
#[wasm_bindgen(js_name = deformCurve)]
pub fn deform_curve(
    #[wasm_bindgen(unchecked_param_type = "DeformParams")] deform: JsValue,
    t: f64,
    x0: f32,
    x1: f32,
    n: usize,
) -> Result<Vec<f32>, JsError> {
    let d = from_js(deform)?;
    Ok(sample(x0, x1, n, |x| laserline_core::model::deform_offset(&d, x, t)))
}

fn sample(x0: f32, x1: f32, n: usize, f: impl Fn(f32) -> f32) -> Vec<f32> {
    let n = n.max(2);
    (0..n).map(|i| f(x0 + (x1 - x0) * i as f32 / (n - 1) as f32)).collect()
}

/// Linear RGB (0..1) the laser is drawn in.
#[wasm_bindgen(js_name = laserRgb)]
pub fn laser_rgb(#[wasm_bindgen(unchecked_param_type = "ColorParams")] color: JsValue) -> Result<Vec<f32>, JsError> {
    Ok(laserline_core::model::laser_rgb(&from_js(color)?).to_vec())
}

/// Approximate display colour (RGB 0..1) of light with the given wavelength.
#[wasm_bindgen(js_name = wavelengthToRgb)]
pub fn wavelength_to_rgb(nm: f32) -> Vec<f32> {
    laserline_core::model::wavelength_to_rgb(nm).to_vec()
}
