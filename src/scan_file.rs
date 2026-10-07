//! `.lscan` files: a sliced scan that can be played back without the model.
//!
//! Layout (little endian): 6-byte magic, u16 version, u32 meta length, meta JSON
//! (`ScanMeta`), `cols·rows` f32 heights (NaN = no surface), `cols·rows` u8 visibility.

use crate::mesh::{self, Mesh};
use crate::scan::{MAX_COLS, MAX_ROWS, Scan, ScanMeta};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 6] = b"LSCAN\0";
const VERSION: u16 = 1;

pub fn to_bytes(scan: &Scan) -> Result<Vec<u8>, String> {
    let meta = serde_json::to_vec(&scan.meta).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(12 + meta.len() + scan.height.len() * 5);
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

/// Returns `Ok(None)` if the user cancelled.
pub fn save_dialog(scan: &Scan) -> Result<Option<PathBuf>, String> {
    let stem = Path::new(&scan.meta.source).file_stem().and_then(|s| s.to_str()).unwrap_or("scan");
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Laser scan", &["lscan"])
        .set_file_name(format!("{stem}.lscan"))
        .save_file()
    else {
        return Ok(None);
    };
    std::fs::write(&path, to_bytes(scan)?).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path))
}

/// Returns `Ok(None)` if the user cancelled.
pub fn load_dialog() -> Result<Option<(Scan, PathBuf)>, String> {
    let Some(path) = rfd::FileDialog::new().add_filter("Laser scan", &["lscan"]).pick_file() else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let scan = from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some((scan, path)))
}

/// Returns `Ok(None)` if the user cancelled.
pub fn import_stl_dialog() -> Result<Option<(Mesh, PathBuf)>, String> {
    let Some(path) = rfd::FileDialog::new().add_filter("STL model", &["stl", "STL"]).pick_file() else {
        return Ok(None);
    };
    Ok(Some((mesh::load_stl(&path)?, path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::tests::box_mesh;
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
    fn rejects_damaged_files() {
        let bytes = to_bytes(&sample()).unwrap();
        assert!(from_bytes(&bytes[..bytes.len() - 1]).is_err());
        assert!(from_bytes(&bytes[..20]).is_err());
        assert!(from_bytes(b"LSCAN\0\x02\0\0\0\0\0").unwrap_err().contains("version"));
        assert!(from_bytes(b"hello world, not a scan").is_err());
    }
}
