# laserline-core

Platform-independent core of [laserline](https://git.javerik.dev/opensource/laserthing):
emulating the laser line of a laser-triangulation sensor and slicing STL meshes into the
profiles such a sensor measures. No GUI, GPU or file-system dependencies; builds for
native targets and `wasm32-unknown-unknown`.

```rust,no_run
use laserline_core::{mesh, scan};
use std::sync::atomic::{AtomicBool, AtomicU32};

let model = mesh::parse_stl(&std::fs::read("part.stl")?)?;
let settings = scan::SliceSettings { triangulation_deg: 35.0, ..Default::default() };
let scan = scan::slice(&model, &settings, "part.stl", &AtomicBool::new(false), &AtomicU32::new(0))?;
println!("{} profiles of {} samples", scan.meta.rows, scan.meta.cols);
let heights_mm = scan.row_height(0); // NaN where the laser hits nothing
# Ok::<(), Box<dyn std::error::Error>>(())
```

- `params`, `model`, `presets`: line parameters (serde) and the laser model.
- `emulator::Emulator`: simulation clock, scan playback and per-frame shader uniforms.
- `mesh`, `scan`, `scan_file`: STL parsing, slicing (threaded natively), `.lscan` files.
- `slicer::Slicer`: interactive slicer state (debounced re-slicing, previews).
- `camera`, `viz`, `gpu_types`: 3D preview geometry for renderers such as `laserline-render`.

Time is always passed in by the host; nothing here reads a clock.

Features: `wasm` adds TypeScript definitions for the serde types (tsify); `test-utils`
exposes test helpers.

Licensed under either of Apache-2.0 or MIT, at your option.
