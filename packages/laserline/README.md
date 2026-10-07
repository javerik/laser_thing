# @javerik/laserline

Laser line emulator and laser-triangulation STL slicer for the browser, compiled from
Rust to WebAssembly. Renders with WebGPU where available and WebGL2 otherwise.

```ts
import { AnimationLoop, Emulator, LaserView, init } from '@javerik/laserline';

await init(); // or init('/assets/laserline_bg.wasm') when your bundler does not resolve
              // new URL(…, import.meta.url): copy wasm/laserline_bg.wasm to your assets
const emulator = new Emulator();
const view = await LaserView.create(canvas); // canvas: HTMLCanvasElement
new AnimationLoop((dt) => {
  emulator.tick(dt);
  view.render(emulator);
}).start();
```

Slicing STL models runs in a Web Worker:

```ts
// slice.worker.ts
import { serveSlicing } from '@javerik/laserline/worker';
serveSlicing();
```

```ts
import { SliceRunner, SliceWorkerClient, Slicer } from '@javerik/laserline';

const client = new SliceWorkerClient(() => new Worker(new URL('./slice.worker', import.meta.url), { type: 'module' }));
const runner = new SliceRunner(new Slicer(undefined, true), client, (scan) => emulator.setScan(scan));
runner.loadModel(stlBytes, 'part.stl');
// every frame: runner.poll(performance.now() / 1000)
```

Also: `MeshView` (3D preview), `Scan` (`.lscan` files, range image, profiles), presets
and plot helpers. Parameter objects use the field names of the preset JSON files.

For Angular components, see `@javerik/laserline-angular`.

Licensed under either of Apache-2.0 or MIT, at your option.
