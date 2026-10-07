# Changelog

## Unreleased

### Added
- Split into libraries and apps: `laserline-core`, `laserline-render`, `laserline-egui`
  (crates.io), `@javerik/laserline` and `@javerik/laserline-angular` (npm), the native
  `laserline-desktop` app and the `laserline-web` Angular app.
- Browser support: wasm32 build of the core, WebGPU and WebGL2 rendering, slicing in a
  Web Worker.
- The live laser view can be detached into a window of its own and docked back (V, or
  the Detach / Dock buttons; closing its window docks it), natively and in the browser:
  `laserline_egui::show_window`, `DetachedLaserView` and `LaserView.createOffscreen`
  in `@javerik/laserline`, `LaserlineStore.detachView()` / `dockView()` and a detach
  button on `<ll-laser-view>`. Fullscreen applies to the window showing the view.
- The web slicer can open in a window of its own as well (as the native one does):
  `LaserlineStore.detachSlicer()` / `dockSlicer()`, `<ll-window-portal>` to render
  templates into another window, and `openPopup`, `CanvasBridge` and
  `MeshView.createOffscreen` in `@javerik/laserline`.
- Everything the apps can do is library API, in Rust and in JavaScript:
  - Window layout and shortcuts: `laserline_egui::{Layout, Action}` with
    `LaserUi::apply`; `actionForKey` / `SHORTCUTS` in `@javerik/laserline`;
    `LaserlineStore.dispatch()`, `controls()` and `toggleControlsDetached()` /
    `toggleControlsHidden()`. The web app gains D (detach controls), the desktop app S
    (slicer).
  - New JS bindings: `constants()`, `gridSize`, `toScanFrame`, `sampleDeformation`,
    `motionOffset`, `lineAngle`, `extentAlong`, `niceStep`, `Mesh.vertices()` /
    `triangles()`, `Scan.heights()` / `visibility()` / `rowVisible()` / `seen()` /
    `uCenterMm`, `Slicer.clearModel()`, and an `isCancelled` callback for `slice`.
- Every public Rust item and every TypeScript export is documented; `missing_docs` and
  `scripts/check-dts-docs.mjs` (part of `npm test`) enforce it, and the crate READMEs
  are compiled as doctests.

### Changed
- `LaserUi::begin_frame` clamps `dt` to `MAX_FRAME_DT` (0.25 s). The desktop app
  stores its layout under `layout` (the old `dock` / `view_placement` keys are read once).
- `profileCurve`'s parameter is `widthPx`; the wasm `start` hook is no longer typed.
- The line's deformation samples are read from a texture instead of a storage buffer
  (WebGL2 has none).
- Loading a scan file abandons a slice still running for the replaced model.

### Fixed
- `Scan.rowHeights` / `profileRuns` no longer panic on a scan without rows.
