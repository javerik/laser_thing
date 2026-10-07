# @javerik/laserline-angular

Angular 21 components for the laserline emulator: the live laser view, the full control
panel and the STL slicer with 3D preview, built on
[`@javerik/laserline`](https://www.npmjs.com/package/@javerik/laserline).

## Setup

1. Copy the WebAssembly module into your assets (`angular.json`, build options):

   ```json
   { "glob": "laserline_bg.wasm", "input": "node_modules/@javerik/laserline/wasm", "output": "laserline" }
   ```

2. Add a slicing worker (`src/app/slice.worker.ts`, and `"webWorkerTsConfig"` in
   `angular.json`):

   ```ts
   import { serveSlicing } from '@javerik/laserline/worker';
   serveSlicing();
   ```

3. Provide the store:

   ```ts
   provideLaserline({
     wasmUrl: 'laserline/laserline_bg.wasm',
     createWorker: () => new Worker(new URL('./slice.worker', import.meta.url), { type: 'module' }),
   });
   ```

4. Use the components:

   ```html
   <ll-laser-view />
   <ll-controls />
   @if (store.slicerOpen()) { <ll-slicer /> }
   ```

Smaller pieces (`ll-line-controls`, `ll-mesh-view`, `ll-range-image`, `ll-slider`, …)
and the `LaserlineStore` are exported for custom layouts.

## Theming

Components take colours, fonts and radius from CSS custom properties, with dark defaults:
`--ll-bg`, `--ll-surface`, `--ll-inset`, `--ll-border`, `--ll-text`, `--ll-text-weak`,
`--ll-accent`, `--ll-on-accent`, `--ll-error`, `--ll-warn`, `--ll-profile-visible`,
`--ll-profile-occluded`, `--ll-radius`, `--ll-font`, `--ll-font-mono`, `--ll-font-size`,
`--ll-view-bg`.

Licensed under either of Apache-2.0 or MIT, at your option.
