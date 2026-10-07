# Changelog

## Unreleased

### Added
- Split into libraries and apps: `laserline-core`, `laserline-render`, `laserline-egui`
  (crates.io), `@javerik/laserline` and `@javerik/laserline-angular` (npm), the native
  `laserline-desktop` app and the `laserline-web` Angular app.
- Browser support: wasm32 build of the core, WebGPU and WebGL2 rendering, slicing in a
  Web Worker.

### Changed
- The line's deformation samples are read from a texture instead of a storage buffer
  (WebGL2 has none).
- Loading a scan file abandons a slice still running for the replaced model.
