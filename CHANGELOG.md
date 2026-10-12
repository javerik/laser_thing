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
- Camera calibration support: another page of the same origin can drive the line over a
  `BroadcastChannel` (`RemoteHost` in `@javerik/laserline`, `provideLaserline({ remote: true })`,
  on in `laserline-web`). It reads the view size and the line's envelope, shows
  calibration patterns (a blank view, lines across it) while the user's parameters stay
  aside, and reports the part of the view its camera sees. That field is outlined over the
  view, and "Fit line to camera" (`LaserlineStore.fitToCameraField()`) moves and shrinks
  the line into it. Core: `laserline_core::calibration` (`line_envelope`,
  `calibration_params`, `pattern_params`, `fit_into`); JS: `Emulator.envelope` /
  `fitInto`, `calibrationParams`, `patternParams`, `DetachedLaserView.setOverlay`.
  Patterns include bands (`width_pct`), a white view and Gray-code stripes drawn over the view
  (`OverlayPattern`, `drawOverlayPattern`, `stripeLit`, `DetachedLaserView.setPattern`,
  `LaserlineStore.overlayPattern`), so a camera seeing only part of the display can be mapped.
- A controller can limit where the line goes: `constrain { polygon, label }` (answered with
  `constrained { clamped }`, released with `null`) keeps the line, its motion, deformation and
  scan heights inside a convex area. Every parameter change is adjusted as little as needed
  (moved towards the area's middle, then length and motion shrunk, last the heights), so
  controls and presets stop at the limit. The limit is kept across reloads, outlined solid over
  the view and reported in `status.constraint`; *Release limit* lets the line go. Core:
  `calibration::constrain_into`; JS: `Emulator.constrainInto`, `RemoteTarget.constrain`,
  `DetachedLaserView.setLimit`, `LaserlineStore.constraint` / `setConstraint()` / `clamped`.
- The default laser colour is white (`ColorParams` defaults to `Custom` 1,1,1); the coloured
  presets set their wavelength. Calibration lines are white.
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
