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

The live view can also run in a window of its own (a popup, e.g. fullscreen on a
projector). Open it from a user action, render it every frame, and take the animation
frames from the popup while it is visible:

```ts
import { AnimationLoop, DetachedLaserView } from '@javerik/laserline';

let detached: DetachedLaserView | undefined;
const loop = new AnimationLoop((dt) => {
  emulator.tick(dt);
  if (detached) detached.render(emulator);
  else view.render(emulator);
});
button.onclick = () => {
  detached = DetachedLaserView.open({
    onClose: () => {
      detached = undefined;
      loop.setWindow(window);
    },
  });
  if (detached) loop.setWindow(detached.window); // undefined: pop-up blocked
};
loop.start();
// detached.close() docks it again (onClose is not called then)
```

For other views in other windows, `openPopup()` opens an empty same-origin window and
`CanvasBridge` shows frames of a view created with `createOffscreen(bridge.offscreen)` on
a canvas there (call `bridge.present()` after each render).

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

Keyboard shortcuts are shared with the desktop app: `actionForKey(event)` returns an
`Action` such as `'toggleFullscreen'`, `shortcutHelp()` the help line.

## API

Every export has TSDoc; this is the map. Parameter objects use the field names of the
preset JSON files.

| Export | Purpose |
|---|---|
| `init`, `defaultWasmUrl`, `version`, `constants` | Load the module; version; limits and format constants |
| `Emulator` | Clock, scan playback and line parameters (`tick`, `stepScan`, `setScan`, …) |
| `LaserView`, `MeshView` | Render an `Emulator` / a `Slicer` scene on a canvas or `OffscreenCanvas` |
| `AnimationLoop` | `requestAnimationFrame` loop with clamped `dt`, switchable between windows |
| `DetachedLaserView`, `openPopup`, `CanvasBridge` | Views in windows of their own |
| `Mesh` | STL parsing, geometry (`vertices`, `triangles`, `bounds`) |
| `slice`, `toScanFrame`, `gridSize` | Slice a mesh (with progress and cancellation); scan-frame geometry; grid preview |
| `Scan` | `.lscan` I/O, metadata, heights and visibility per row or whole, profile runs, range image |
| `Slicer` | Model, settings, debounced re-slicing, 3D camera |
| `SliceWorkerClient`, `SliceRunner`, `serveSlicing` (`/worker`) | Slicing in a Web Worker |
| `defaultParams`, `defaultSliceSettings`, `builtinPresets`, `presetToJson`, `presetFromJson` | Parameters and presets |
| `profileCurve`, `deformCurve`, `sampleDeformation`, `motionOffset`, `lineAngle`, `extentAlong`, `laserRgb`, `wavelengthToRgb`, `niceStep` | Line model and plot helpers |
| `actionForKey`, `SHORTCUTS`, `shortcutHelp` | Keyboard shortcuts |
| `RemoteHost`, `ToLaserline`, `FromLaserline`, `REMOTE_CHANNEL` | Let another page drive the line (camera calibration), see below |
| `Emulator.envelope`, `Emulator.fitInto`, `calibrationParams`, `patternParams` | The line's envelope over its whole motion; fitting it into a camera field; calibration patterns |
| `readFileBytes`, `downloadBytes` | File helpers |

### Remote control (camera calibration)

A `RemoteHost` lets another page of the same origin drive the line over the
`BroadcastChannel` `laserline-remote`, e.g. an application that calibrates a camera
filming the screen. Messages carry `v: 1`; all coordinates are view pixels (the live
view's drawing buffer, y down).

| To laserline | Answer |
|---|---|
| `hello` | `status` from every window: `host` id, `viewPx`, the user's line `envelope`, `session`, `field` |
| `begin { to, session }` | `status`; the user's parameters are kept aside, the view goes blank and pauses. A second session gets `error`. |
| `show { to, session, id, pattern }` | `shown { id, viewPx }` once the pattern has been on screen for a frame: `{ kind: 'blank' }`; `{ kind: 'line', orientation, position_pct, width_pct? }` (a line across the view, or a band `width_pct` of the view wide); `{ kind: 'white' }`; or `{ kind: 'stripes', orientation, bit, inverted? }`, bit `bit` of a Gray code of the view's columns (`Vertical`) or rows, white on black (`stripeLit`, `drawOverlayPattern`). White and stripes are drawn over the view; from all bits a camera pixel decodes the view position it sees |
| `end { to, session }` | `status`; parameters and playback come back. A session also ends after 15 s without messages. |
| `camera-field { to, polygon, label }` | `status`; the window outlines the polygon (`null` removes it) |
| `fit-to-field { to, id, margin }` | `fitted { id, fits }` after moving and shrinking the line into the field |

The live view must stay visible: hidden windows get no animation frames, so no `shown`.

For Angular components, see `@javerik/laserline-angular`.

Licensed under either of Apache-2.0 or MIT, at your option.
