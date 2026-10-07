//! Preview geometry and images derived from a scan: the 3D overlay (conveyor, laser
//! plane, camera, current profile), the heightmap surface, the range image and the
//! profile polylines. Colours are plain RGBA so any frontend can draw them.

use crate::gpu_types::Overlay;
use crate::scan::{Scan, SliceSettings};
use glam::Vec3;

/// Larger range images are subsampled for display.
pub const RANGE_IMAGE_MAX: usize = 2048;
/// Larger heightmaps are subsampled when shown as a 3D surface.
pub const SURFACE_MAX: usize = 400;

/// Profile parts the camera sees.
pub const VISIBLE: [u8; 3] = [255, 70, 50];
/// Profile parts hidden from the camera.
pub const OCCLUDED: [u8; 3] = [190, 70, 200];

/// The part of the settings that moves the model in the scan frame; equal values
/// give the same preview geometry.
pub fn transform_part(s: &SliceSettings) -> SliceSettings {
    SliceSettings {
        up_axis: s.up_axis,
        travel_axis: s.travel_axis,
        flip_along: s.flip_along,
        yaw_deg: s.yaw_deg,
        unit: s.unit,
        scale: s.scale,
        ..Default::default()
    }
}

/// Subsampled triangle surface of a scan's heightmap, for scans loaded without a model.
pub fn heightmap_surface(s: &Scan) -> (Vec<[f32; 3]>, Vec<u32>) {
    let m = &s.meta;
    let step = m.cols.max(m.rows).div_ceil(SURFACE_MAX).max(1);
    let (nc, nr) = ((m.cols - 1) / step + 1, (m.rows - 1) / step + 1);
    let mut positions = Vec::with_capacity(nc * nr);
    for j in 0..nr {
        for i in 0..nc {
            let (ci, rj) = (i * step, j * step);
            let h = s.height[rj * m.cols + ci];
            positions.push([m.u0_mm + ci as f32 * m.du_mm, m.v0_mm + rj as f32 * m.dv_mm, h]);
        }
    }
    let mut indices = Vec::new();
    let at = |i: usize, j: usize| (j * nc + i) as u32;
    for j in 0..nr.saturating_sub(1) {
        for i in 0..nc.saturating_sub(1) {
            let q = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
            if q.iter().all(|&k| positions[k as usize][2].is_finite()) {
                indices.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
            }
        }
    }
    // The GPU must not see NaN positions, even unreferenced ones.
    for p in &mut positions {
        if !p[2].is_finite() {
            p[2] = 0.0;
        }
    }
    (positions, indices)
}

/// Grid spacing of 1, 2 or 5 × 10ⁿ giving about ten lines over `span`.
pub fn nice_step(span: f32) -> f32 {
    let raw = (span / 10.0).max(1e-3);
    let mag = 10f32.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0].into_iter().map(|k| k * mag).find(|s| *s >= raw).unwrap_or(10.0 * mag)
}

fn rgba(c: [u8; 3], a: f32) -> [f32; 4] {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, a]
}

/// Text placed at a 3D position of the preview.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    /// Anchor in scan-frame mm.
    pub pos: Vec3,
    /// Text to show.
    pub text: &'static str,
    /// sRGB, 0..255.
    pub color: [u8; 3],
}

/// Overlay geometry plus the labels to draw over it.
#[derive(Clone, Debug, Default)]
pub struct OverlayWithLabels {
    /// Lines and translucent triangles.
    pub overlay: Overlay,
    /// Labels at 3D positions.
    pub labels: Vec<Label>,
}

/// Bounds `(lo, hi)` in scan-frame mm covering a scan's grid up to its highest point.
pub fn scan_bounds(s: &Scan) -> (Vec3, Vec3) {
    let m = &s.meta;
    let lo = Vec3::new(m.u0_mm, m.v0_mm, 0.0);
    (lo, lo + Vec3::new(s.width_mm(), s.length_mm(), m.z_max_mm))
}

/// Conveyor grid, travel arrow, laser plane and fan, camera direction and the current profile.
pub fn build_overlay(scan: Option<&Scan>, row: usize, s: &SliceSettings, lo: Vec3, hi: Vec3) -> OverlayWithLabels {
    let mut o = Overlay::default();
    let mut labels = Vec::new();
    let (u0, u1, v0, v1) = match scan {
        Some(sc) => (sc.meta.u0_mm, sc.meta.u0_mm + sc.width_mm(), sc.meta.v0_mm, sc.meta.v0_mm + sc.length_mm()),
        None => (lo.x, hi.x, lo.y, hi.y),
    };
    let step = nice_step((u1 - u0).max(v1 - v0));
    let grid_c = [0.45, 0.47, 0.5, if s.base_plane { 0.55 } else { 0.25 }];
    let mut u = (u0 / step).ceil() * step;
    while u <= u1 {
        o.line(Vec3::new(u, v0, 0.0), Vec3::new(u, v1, 0.0), grid_c);
        u += step;
    }
    let mut v = (v0 / step).ceil() * step;
    while v <= v1 {
        o.line(Vec3::new(u0, v, 0.0), Vec3::new(u1, v, 0.0), grid_c);
        v += step;
    }
    let edge = [0.7, 0.72, 0.76, 0.9];
    let c = [Vec3::new(u0, v0, 0.0), Vec3::new(u1, v0, 0.0), Vec3::new(u1, v1, 0.0), Vec3::new(u0, v1, 0.0)];
    for k in 0..4 {
        o.line(c[k], c[(k + 1) % 4], edge);
    }

    // Travel arrow beside the scan area.
    let yellow = [0.95, 0.78, 0.2, 1.0];
    let ua = u1 + step;
    let (va, vb) = (v0, v0 + (v1 - v0).max(step));
    o.line(Vec3::new(ua, va, 0.0), Vec3::new(ua, vb, 0.0), yellow);
    o.line(Vec3::new(ua, vb, 0.0), Vec3::new(ua - 0.4 * step, vb - 0.8 * step, 0.0), yellow);
    o.line(Vec3::new(ua, vb, 0.0), Vec3::new(ua + 0.4 * step, vb - 0.8 * step, 0.0), yellow);
    labels.push(Label { pos: Vec3::new(ua + 0.3 * step, vb, 0.0), text: "travel", color: [242, 199, 51] });

    let Some(sc) = scan else {
        return OverlayWithLabels { overlay: o, labels };
    };
    let m = &sc.meta;
    let v = m.v0_mm + row as f32 * m.dv_mm;
    let top = hi.z.max(m.z_max_mm).max(step) * 1.2;
    let uc = 0.5 * (u0 + u1);

    // Laser plane and fan from the source above.
    o.quad([Vec3::new(u0, v, 0.0), Vec3::new(u1, v, 0.0), Vec3::new(u1, v, top), Vec3::new(u0, v, top)], [1.0, 0.2, 0.15, 0.16]);
    let src = Vec3::new(uc, v, top * 1.3);
    let fan = [1.0, 0.3, 0.2, 0.55];
    o.line(src, Vec3::new(u0, v, 0.0), fan);
    o.line(src, Vec3::new(u1, v, 0.0), fan);
    labels.push(Label { pos: src, text: "laser", color: [255, 110, 90] });

    // Camera viewing direction in the travel/height plane.
    let alpha = m.settings.triangulation_deg.to_radians();
    let side = m.settings.camera_side.sign() as f32;
    let cam = Vec3::new(uc, v + side * alpha.sin() * top * 1.3, alpha.cos() * top * 1.3);
    let cyan = [0.35, 0.85, 1.0, 1.0];
    o.line_on_top(Vec3::new(uc, v, 0.0), cam, cyan);
    labels.push(Label { pos: cam, text: "camera", color: [90, 215, 255] });

    // Current profile, always on top: red where seen, magenta where occluded.
    let heights = sc.row_height(row);
    let vis = sc.row_visible(row);
    let lift = 0.002 * top;
    for i in 0..m.cols.saturating_sub(1) {
        let (a, b) = (heights[i], heights[i + 1]);
        if !(a.is_finite() && b.is_finite()) {
            continue;
        }
        let color = if vis[i] != 0 && vis[i + 1] != 0 { rgba(VISIBLE, 1.0) } else { rgba(OCCLUDED, 0.8) };
        let ua = m.u0_mm + i as f32 * m.du_mm;
        o.line_on_top(Vec3::new(ua, v, a + lift), Vec3::new(ua + m.du_mm, v, b + lift), color);
    }
    OverlayWithLabels { overlay: o, labels }
}

/// RGBA8 image, row-major from the top left.
#[derive(Clone, Debug, PartialEq)]
pub struct RgbaImage {
    /// Width in px.
    pub width: usize,
    /// Height in px.
    pub height: usize,
    /// `width × height` RGBA pixels, 4 bytes each.
    pub pixels: Vec<u8>,
}

/// Range image with travel to the right and the line direction downwards: grey by
/// height where seen, magenta where occluded, black where nothing was hit.
pub fn range_image(scan: &Scan) -> RgbaImage {
    let m = &scan.meta;
    let (sx, sy) = (m.rows.div_ceil(RANGE_IMAGE_MAX).max(1), m.cols.div_ceil(RANGE_IMAGE_MAX).max(1));
    let (w, h) = (m.rows.div_ceil(sx), m.cols.div_ceil(sy));
    let z_max = m.z_max_mm.max(1e-6);
    let mut pixels = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let i = y * sy;
        for x in 0..w {
            let k = x * sx * m.cols + i;
            let z = scan.height[k];
            let rgb = if !z.is_finite() {
                [0, 0, 0]
            } else {
                let t = (z / z_max).clamp(0.0, 1.0);
                if scan.visible[k] != 0 {
                    let g = (45.0 + 210.0 * t) as u8;
                    [g, g, g]
                } else {
                    [(100.0 + 120.0 * t) as u8, 25, (95.0 + 110.0 * t) as u8]
                }
            };
            pixels.extend([rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    RgbaImage { width: w, height: h, pixels }
}

/// A run of neighbouring profile samples with the same visibility.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileRun {
    /// The camera sees these samples.
    pub visible: bool,
    /// `(column, height mm)` pairs.
    pub points: Vec<[f32; 2]>,
}

/// Height along the line for profile `row`, split into polylines of seen and occluded
/// samples. Neighbouring runs share their joint so the outline stays continuous.
pub fn profile_runs(scan: &Scan, row: usize) -> Vec<ProfileRun> {
    let heights = scan.row_height(row);
    let vis = scan.row_visible(row);
    let mut runs = Vec::new();
    let mut run: Vec<[f32; 2]> = Vec::new();
    let mut run_class = 0u8;
    let flush = |run: &mut Vec<[f32; 2]>, class: u8, runs: &mut Vec<ProfileRun>| {
        if run.len() >= 2 {
            runs.push(ProfileRun { visible: class == 1, points: std::mem::take(run) });
        }
        run.clear();
    };
    for i in 0..scan.meta.cols {
        let class = if !heights[i].is_finite() { 0 } else if vis[i] != 0 { 1 } else { 2 };
        if class != run_class {
            let last = run.last().copied();
            flush(&mut run, run_class, &mut runs);
            if let (Some(p), true) = (last, class != 0) {
                run.push(p);
            }
            run_class = class;
        }
        if class != 0 {
            run.push([i as f32, heights[i]]);
        }
    }
    flush(&mut run, run_class, &mut runs);
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::box_mesh;
    use crate::scan::{Progress, slice};
    use std::sync::atomic::AtomicBool;

    #[test]
    fn surface_of_loaded_scan() {
        let m = box_mesh([0.0; 3], [10.0, 10.0, 2.0]);
        let s = SliceSettings { lateral_res_mm: 1.0, travel_step_mm: 1.0, base_plane: false, margin_lateral_mm: 2.0, margin_travel_mm: 2.0, ..Default::default() };
        let scan = slice(&m, &s, "", &AtomicBool::new(false), &Progress::new(0)).unwrap();
        let (pos, idx) = heightmap_surface(&scan);
        assert_eq!(pos.len(), 15 * 15);
        assert!(pos.iter().all(|p| p[2].is_finite()));
        // Only the 10 × 10 quads on top of the box have four finite corners.
        assert_eq!(idx.len(), 10 * 10 * 6);
    }

    #[test]
    fn nice_steps() {
        assert_eq!(nice_step(100.0), 10.0);
        assert_eq!(nice_step(130.0), 20.0);
        assert_eq!(nice_step(4.0), 0.5);
    }

    #[test]
    fn profile_runs_split_by_visibility() {
        let m = box_mesh([0.0; 3], [10.0, 10.0, 10.0]);
        let s = SliceSettings { lateral_res_mm: 1.0, travel_step_mm: 1.0, margin_travel_mm: 20.0, triangulation_deg: 45.0, ..Default::default() };
        let scan = slice(&m, &s, "", &AtomicBool::new(false), &Progress::new(0)).unwrap();
        // Lead-in row 15 lies in the block's shadow except at the far ends.
        let all: usize = profile_runs(&scan, 15).iter().map(|r| r.points.len()).sum();
        assert!(all >= scan.meta.cols);
        let runs = profile_runs(&scan, 25);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].visible);
        assert_eq!(runs[0].points.len(), scan.meta.cols);
    }

    #[test]
    fn range_image_size() {
        let m = box_mesh([0.0; 3], [10.0, 5.0, 3.0]);
        let s = SliceSettings { lateral_res_mm: 1.0, travel_step_mm: 1.0, ..Default::default() };
        let scan = slice(&m, &s, "", &AtomicBool::new(false), &Progress::new(0)).unwrap();
        let img = range_image(&scan);
        assert_eq!((img.width, img.height), (scan.meta.rows, scan.meta.cols));
        assert_eq!(img.pixels.len(), img.width * img.height * 4);
    }
}
