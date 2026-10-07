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

### Changed
- The line's deformation samples are read from a texture instead of a storage buffer
  (WebGL2 has none).
- Loading a scan file abandons a slice still running for the replaced model.
