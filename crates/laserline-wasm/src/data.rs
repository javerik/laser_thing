//! Meshes, scans and slicing.

use crate::{from_js, js_err, to_js};
use laserline_core::scan::{self, SliceObserver};
use laserline_core::{mesh, scan_file, viz};
use serde::Serialize;
use std::cell::Cell;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// A parsed STL model.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Mesh {
    pub(crate) inner: Arc<mesh::Mesh>,
}

#[derive(Serialize)]
pub(crate) struct Bounds {
    pub lo: [f32; 3],
    pub hi: [f32; 3],
}

#[wasm_bindgen]
impl Mesh {
    /// Parses a binary or ASCII STL file.
    #[wasm_bindgen(js_name = parseStl)]
    pub fn parse_stl(bytes: &[u8]) -> Result<Mesh, JsError> {
        mesh::parse_stl(bytes).map(|m| Mesh { inner: Arc::new(m) }).map_err(js_err)
    }

    #[wasm_bindgen(getter, js_name = triangleCount)]
    pub fn triangle_count(&self) -> usize {
        self.inner.triangles.len()
    }

    #[wasm_bindgen(getter, js_name = vertexCount)]
    pub fn vertex_count(&self) -> usize {
        self.inner.vertices.len()
    }

    /// Bounds in model units.
    #[wasm_bindgen(unchecked_return_type = "Bounds")]
    pub fn bounds(&self) -> JsValue {
        let (lo, hi) = self.inner.bounds();
        to_js(&Bounds { lo, hi })
    }
}

/// A sliced scan: one height profile per travel step.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Scan {
    pub(crate) inner: Arc<scan::Scan>,
}

#[wasm_bindgen]
impl Scan {
    /// Reads an `.lscan` file.
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(bytes: &[u8]) -> Result<Scan, JsError> {
        scan_file::from_bytes(bytes).map(|s| Scan { inner: Arc::new(s) }).map_err(js_err)
    }

    /// The scan as an `.lscan` file.
    #[wasm_bindgen(js_name = toBytes)]
    pub fn to_bytes(&self) -> Result<Vec<u8>, JsError> {
        scan_file::to_bytes(&self.inner).map_err(js_err)
    }

    /// Suggested file name for saving, e.g. `part.lscan`.
    #[wasm_bindgen(js_name = fileName)]
    pub fn file_name(&self) -> String {
        scan_file::file_name(&self.inner)
    }

    #[wasm_bindgen(unchecked_return_type = "ScanMeta")]
    pub fn meta(&self) -> JsValue {
        to_js(&self.inner.meta)
    }

    /// Number of profiles.
    #[wasm_bindgen(getter)]
    pub fn rows(&self) -> usize {
        self.inner.meta.rows
    }

    /// Samples per profile.
    #[wasm_bindgen(getter)]
    pub fn cols(&self) -> usize {
        self.inner.meta.cols
    }

    #[wasm_bindgen(getter, js_name = widthMm)]
    pub fn width_mm(&self) -> f32 {
        self.inner.width_mm()
    }

    #[wasm_bindgen(getter, js_name = lengthMm)]
    pub fn length_mm(&self) -> f32 {
        self.inner.length_mm()
    }

    #[wasm_bindgen(getter, js_name = zMaxMm)]
    pub fn z_max_mm(&self) -> f32 {
        self.inner.meta.z_max_mm
    }

    /// Travel position of profile `row` in mm.
    #[wasm_bindgen(js_name = rowV)]
    pub fn row_v(&self, row: usize) -> f32 {
        self.inner.meta.v0_mm + row as f32 * self.inner.meta.dv_mm
    }

    /// Heights of profile `row` in mm (NaN where nothing was hit).
    #[wasm_bindgen(js_name = rowHeights)]
    pub fn row_heights(&self, row: usize) -> Vec<f32> {
        self.inner.row_height(row.min(self.inner.meta.rows - 1)).to_vec()
    }

    /// Profile `row` split into seen and occluded polylines.
    #[wasm_bindgen(js_name = profileRuns, unchecked_return_type = "ProfileRun[]")]
    pub fn profile_runs(&self, row: usize) -> js_sys::Array {
        viz::profile_runs(&self.inner, row.min(self.inner.meta.rows - 1))
            .into_iter()
            .map(|r| {
                let points: Vec<f32> = r.points.into_iter().flatten().collect();
                let run = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&run, &"visible".into(), &r.visible.into());
                let _ = js_sys::Reflect::set(&run, &"points".into(), &js_sys::Float32Array::from(&points[..]));
                JsValue::from(run)
            })
            .collect()
    }

    /// Range image: travel to the right, along the line downwards; grey by height where
    /// seen, magenta where occluded. Large scans are subsampled.
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = rangeImage)]
    pub fn range_image(&self) -> Result<web_sys::ImageData, JsError> {
        let img = viz::range_image(&self.inner);
        web_sys::ImageData::new_with_u8_clamped_array_and_sh(wasm_bindgen::Clamped(&img.pixels), img.width as u32, img.height as u32)
            .map_err(|e| JsError::new(&format!("{e:?}")))
    }
}

/// Reports progress to a JS callback. Slicing on wasm32 runs on the calling thread, so
/// the callback is never touched from another thread.
struct JsObserver {
    on_progress: Option<js_sys::Function>,
    last: Cell<u32>,
}

// SAFETY: wasm32 without atomics has no threads; the slicer calls the observer only on
// the thread that created it.
unsafe impl Sync for JsObserver {}

impl SliceObserver for JsObserver {
    fn cancelled(&self) -> bool {
        false
    }

    fn progress(&self, permille: u32) {
        if permille / 10 != self.last.get() / 10
            && let Some(f) = &self.on_progress
        {
            self.last.set(permille);
            let _ = f.call1(&JsValue::NULL, &JsValue::from_f64(permille as f64 / 1000.0));
        }
    }
}

/// Slices `mesh` into a scan. `name` is stored as the scan's source. `onProgress`
/// receives the progress in 0..1. Runs synchronously: call it in a Web Worker to keep
/// the page responsive, and terminate the worker to cancel.
#[wasm_bindgen]
pub fn slice(
    mesh: &Mesh,
    #[wasm_bindgen(unchecked_param_type = "SliceSettings")] settings: JsValue,
    name: &str,
    #[wasm_bindgen(js_name = onProgress)] on_progress: Option<js_sys::Function>,
) -> Result<Scan, JsError> {
    let settings = from_js(settings)?;
    let observer = JsObserver { on_progress, last: Cell::new(0) };
    scan::slice_observed(&mesh.inner, &settings, name, &observer).map(|s| Scan { inner: Arc::new(s) }).map_err(js_err)
}
