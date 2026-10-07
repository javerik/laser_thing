//! `.lscan` files: a sliced scan that can be played back without the model.
//!
//! Layout (little endian): 6-byte magic, u16 version, u32 meta length, meta JSON
//! (`ScanMeta`), `cols·rows` f32 heights (NaN = no surface), `cols·rows` u8 visibility.

use crate::scan::{MAX_COLS, MAX_ROWS, Scan, ScanMeta};

/// First bytes of every scan file.
pub const MAGIC: &[u8; 6] = b"LSCAN\0";
/// Format version written by [`to_bytes`]; [`from_bytes`] rejects others.
pub const VERSION: u16 = 1;
/// Payload bytes per grid cell: an f32 height plus a u8 visibility flag.
pub const BYTES_PER_CELL: usize = 5;
/// File extension of scan files, without the dot.
pub const EXTENSION: &str = "lscan";

/// Suggested file name for saving `scan`: the model's file stem plus `.lscan`.
pub fn file_name(scan: &Scan) -> String {
    let base = scan.meta.source.rsplit(['/', '\\']).next().unwrap_or("");
    let stem = base.rsplit_once('.').map_or(base, |(stem, _)| stem);
    format!("{}.{EXTENSION}", if stem.is_empty() { "scan" } else { stem })
}

/// Encodes `scan` as an `.lscan` file.
pub fn to_bytes(scan: &Scan) -> Result<Vec<u8>, String> {
    let meta = serde_json::to_vec(&scan.meta).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(12 + meta.len() + scan.height.len() * BYTES_PER_CELL);
    out.extend(MAGIC);
    out.extend(VERSION.to_le_bytes());
    out.extend((meta.len() as u32).to_le_bytes());
    out.extend(&meta);
    for h in &scan.height {
        out.extend(h.to_le_bytes());
    }
    out.extend(&scan.visible);
    Ok(out)
}

/// Decodes an `.lscan` file; fails on a wrong magic, version or size.
pub fn from_bytes(bytes: &[u8]) -> Result<Scan, String> {
    let bad = |what: &str| format!("not a valid scan file ({what})");
    if bytes.len() < 12 || &bytes[..6] != MAGIC {
        return Err(bad("missing header"));
    }
    let version = u16::from_le_bytes([bytes[6], bytes[7]]);
    if version != VERSION {
        return Err(format!("unsupported scan file version {version}"));
    }
    let meta_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let meta_end = 12usize.checked_add(meta_len).filter(|&e| e <= bytes.len()).ok_or_else(|| bad("truncated"))?;
    let meta: ScanMeta = serde_json::from_slice(&bytes[12..meta_end]).map_err(|e| bad(&e.to_string()))?;
    if meta.cols == 0 || meta.rows == 0 || meta.cols > MAX_COLS || meta.rows > MAX_ROWS || !(meta.du_mm > 0.0 && meta.dv_mm > 0.0) {
        return Err(bad("invalid grid"));
    }
    let n = meta.cols * meta.rows;
    let data = &bytes[meta_end..];
    if data.len() != n * 5 {
        return Err(bad("truncated"));
    }
    let height = data[..n * 4].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
    let visible = data[n * 4..].to_vec();
    Ok(Scan { meta, height, visible })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::box_mesh;
    use crate::scan::{Progress, SliceSettings, slice};
    use std::sync::atomic::AtomicBool;

    fn sample() -> Scan {
        let m = box_mesh([0.0; 3], [10.0, 5.0, 3.0]);
        let s = SliceSettings { lateral_res_mm: 1.0, travel_step_mm: 1.0, base_plane: false, ..Default::default() };
        slice(&m, &s, "box.stl", &AtomicBool::new(false), &Progress::new(0)).unwrap()
    }

    #[test]
    fn round_trip() {
        let scan = sample();
        assert!(scan.height.iter().any(|h| h.is_nan()));
        let back = from_bytes(&to_bytes(&scan).unwrap()).unwrap();
        assert_eq!(back.meta, scan.meta);
        assert_eq!(back.visible, scan.visible);
        // NaN != NaN, so compare bit patterns.
        let bits = |s: &Scan| s.height.iter().map(|h| h.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&back), bits(&scan));
    }

    #[test]
    fn suggested_names() {
        let mut scan = sample();
        assert_eq!(file_name(&scan), "box.lscan");
        scan.meta.source = "/models/part.v2.stl".into();
        assert_eq!(file_name(&scan), "part.v2.lscan");
        scan.meta.source = String::new();
        assert_eq!(file_name(&scan), "scan.lscan");
    }

    #[test]
    fn rejects_damaged_files() {
        let bytes = to_bytes(&sample()).unwrap();
        assert!(from_bytes(&bytes[..bytes.len() - 1]).is_err());
        assert!(from_bytes(&bytes[..20]).is_err());
        assert!(from_bytes(b"LSCAN\0\x02\0\0\0\0\0").unwrap_err().contains("version"));
        assert!(from_bytes(b"hello world, not a scan").is_err());
    }
}
