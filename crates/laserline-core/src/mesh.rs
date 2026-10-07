//! Triangle meshes: STL loading and the transform into the scan frame.
//!
//! STL is parsed here rather than with a crate because many CAD exporters write
//! binary files whose 80-byte header starts with "solid", which naive format
//! probes mistake for ASCII. The binary size check is unambiguous.

use crate::scan::{SliceSettings, UpAxis};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    /// Axis-aligned bounds `(min, max)`; zeros for an empty mesh.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        bounds(&self.vertices)
    }
}

pub fn bounds(points: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    if points.is_empty() {
        return ([0.0; 3], [0.0; 3]);
    }
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for p in points {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

/// Parses a binary or ASCII STL file.
pub fn parse_stl(bytes: &[u8]) -> Result<Mesh, String> {
    let mut builder = Builder::default();
    if is_binary(bytes) {
        parse_binary(bytes, &mut builder)?;
    } else {
        parse_ascii(bytes, &mut builder)?;
    }
    if builder.mesh.triangles.is_empty() {
        return Err("no triangles found".into());
    }
    Ok(builder.mesh)
}

fn is_binary(bytes: &[u8]) -> bool {
    if bytes.len() < 84 {
        return false;
    }
    let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    let expected = 84usize.saturating_add(count.saturating_mul(50));
    if expected == bytes.len() {
        return true;
    }
    // Not an exact binary size: ASCII if it looks like text, else binary with trailing junk.
    let text_start = bytes.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(0);
    !bytes[text_start..].starts_with(b"solid") && expected < bytes.len()
}

fn parse_binary(bytes: &[u8], b: &mut Builder) -> Result<(), String> {
    let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    let f = |o: usize| f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    for t in 0..count {
        let base = 84 + t * 50 + 12; // skip the stored normal
        if base + 36 > bytes.len() {
            return Err(format!("binary STL truncated at triangle {t} of {count}"));
        }
        let v = |i: usize| [f(base + i * 12), f(base + i * 12 + 4), f(base + i * 12 + 8)];
        b.push_triangle([v(0), v(1), v(2)]);
    }
    Ok(())
}

fn parse_ascii(bytes: &[u8], b: &mut Builder) -> Result<(), String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "not a valid ASCII or binary STL file")?;
    let mut tokens = text.split_ascii_whitespace();
    if tokens.next() != Some("solid") {
        return Err("not a valid ASCII or binary STL file".into());
    }
    let mut corners = Vec::with_capacity(3);
    while let Some(tok) = tokens.next() {
        match tok {
            "vertex" => {
                let mut v = [0.0f32; 3];
                for c in &mut v {
                    let s = tokens.next().ok_or("unexpected end of file in vertex")?;
                    *c = s.parse().map_err(|_| format!("bad number “{s}”"))?;
                }
                corners.push(v);
            }
            "endloop" => {
                // Polygons with more than three corners are fanned into triangles.
                for i in 1..corners.len().saturating_sub(1) {
                    b.push_triangle([corners[0], corners[i], corners[i + 1]]);
                }
                corners.clear();
            }
            _ => {}
        }
    }
    Ok(())
}

/// Collects triangles while merging identical vertices.
#[derive(Default)]
struct Builder {
    mesh: Mesh,
    index: HashMap<[u32; 3], u32>,
}

impl Builder {
    fn push_triangle(&mut self, corners: [[f32; 3]; 3]) {
        if corners.iter().flatten().any(|c| !c.is_finite()) {
            return;
        }
        let tri = corners.map(|v| {
            *self.index.entry(v.map(f32::to_bits)).or_insert_with(|| {
                self.mesh.vertices.push(v);
                (self.mesh.vertices.len() - 1) as u32
            })
        });
        if tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2] {
            self.mesh.triangles.push(tri);
        }
    }
}

/// Model vertices expressed in the scan frame, in millimetres:
/// `u` along the laser line, `v` along the travel direction, `z` up.
/// The object rests on z = 0, is centered in `u` and starts at `v = 0`.
pub fn to_scan_frame(mesh: &Mesh, s: &SliceSettings) -> Vec<[f32; 3]> {
    let (up, up_sign) = s.up_axis.index_sign();
    let travel = s.travel_axis_index();
    let along = 3 - up - travel;
    let scale = s.unit.mm() * s.scale;
    let (sin, cos) = s.yaw_deg.to_radians().sin_cos();
    let along_sign = if s.flip_along { -1.0 } else { 1.0 };

    let mut pts: Vec<[f32; 3]> = mesh
        .vertices
        .iter()
        .map(|p| {
            let u = p[along] * along_sign;
            let v = p[travel];
            let z = p[up] * up_sign;
            // Yaw rotates the object on the conveyor, about the up axis.
            [(u * cos - v * sin) * scale, (u * sin + v * cos) * scale, z * scale]
        })
        .collect();

    let (lo, hi) = bounds(&pts);
    let shift = [-0.5 * (lo[0] + hi[0]), -lo[1], -lo[2]];
    for p in &mut pts {
        for k in 0..3 {
            p[k] += shift[k];
        }
    }
    pts
}

impl UpAxis {
    /// Model axis index and sign that become scan-frame +z.
    pub fn index_sign(self) -> (usize, f32) {
        match self {
            UpAxis::PosX => (0, 1.0),
            UpAxis::NegX => (0, -1.0),
            UpAxis::PosY => (1, 1.0),
            UpAxis::NegY => (1, -1.0),
            UpAxis::PosZ => (2, 1.0),
            UpAxis::NegZ => (2, -1.0),
        }
    }
}

/// Axis-aligned box from `lo` to `hi` as 12 triangles (test helper).
#[cfg(any(test, feature = "test-utils"))]
pub fn box_mesh(lo: [f32; 3], hi: [f32; 3]) -> Mesh {
    let c = |i: usize| [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    let mut b = Builder::default();
    for q in quads {
        b.push_triangle([c(q[0]), c(q[1]), c(q[2])]);
        b.push_triangle([c(q[0]), c(q[2]), c(q[3])]);
    }
    b.mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{Axis, ModelUnit};

    fn to_binary(m: &Mesh) -> Vec<u8> {
        let mut out = b"solid but actually binary".to_vec();
        out.resize(80, 0);
        out.extend((m.triangles.len() as u32).to_le_bytes());
        for t in &m.triangles {
            out.extend([0u8; 12]);
            for &i in t {
                for c in m.vertices[i as usize] {
                    out.extend(c.to_le_bytes());
                }
            }
            out.extend([0u8; 2]);
        }
        out
    }

    #[test]
    fn ascii_stl() {
        let src = "solid test\n facet normal 0 0 1\n  outer loop\n   vertex 0 0 0\n   vertex 1 0 0\n   vertex 0 1 1e0\n  endloop\n endfacet\n\
                   facet normal 0 0 1\n  outer loop\n   vertex 1 0 0\n   vertex 1 1 0\n   vertex 0 1 1\n  endloop\n endfacet\nendsolid test\n";
        let m = parse_stl(src.as_bytes()).unwrap();
        assert_eq!(m.triangles.len(), 2);
        assert_eq!(m.vertices.len(), 4, "shared vertices are merged");
        assert_eq!(m.bounds(), ([0.0; 3], [1.0, 1.0, 1.0]));
    }

    #[test]
    fn binary_stl_with_solid_header() {
        let m = box_mesh([0.0; 3], [1.0, 2.0, 3.0]);
        let parsed = parse_stl(&to_binary(&m)).unwrap();
        assert_eq!(parsed.triangles.len(), 12);
        assert_eq!(parsed.vertices.len(), 8);
        assert_eq!(parsed.bounds(), ([0.0; 3], [1.0, 2.0, 3.0]));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_stl(b"hello").is_err());
        assert!(parse_stl(b"solid x\nendsolid x\n").is_err());
    }

    #[test]
    fn scan_frame_mapping() {
        // 10 (x) × 20 (y) × 30 (z) box, Y up, travel along Z, in cm.
        let m = box_mesh([5.0, 0.0, 0.0], [15.0, 20.0, 30.0]);
        let s = SliceSettings { up_axis: UpAxis::PosY, travel_axis: Axis::Z, unit: ModelUnit::Cm, ..Default::default() };
        let (lo, hi) = bounds(&to_scan_frame(&m, &s));
        assert_eq!(lo, [-50.0, 0.0, 0.0]);
        assert_eq!(hi, [50.0, 300.0, 200.0]);

        // Upside down: still rests on z = 0.
        let s = SliceSettings { up_axis: UpAxis::NegY, travel_axis: Axis::Z, ..Default::default() };
        let (lo, hi) = bounds(&to_scan_frame(&m, &s));
        assert_eq!((lo[2], hi[2]), (0.0, 20.0));

        // 90° yaw swaps the along / travel extents.
        let s = SliceSettings { yaw_deg: 90.0, ..Default::default() };
        let (lo, hi) = bounds(&to_scan_frame(&m, &s));
        assert!((hi[0] - lo[0] - 20.0).abs() < 1e-3);
        assert!((hi[1] - lo[1] - 10.0).abs() < 1e-3);
    }
}
