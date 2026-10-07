import { InjectionToken } from '@angular/core';

export interface LaserlineConfig {
  /**
   * URL of `laserline_bg.wasm`. Copy it from `node_modules/@javerik/laserline/wasm/`
   * into the app's assets, e.g. in `angular.json`:
   * `{ "glob": "laserline_bg.wasm", "input": "node_modules/@javerik/laserline/wasm", "output": "laserline" }`
   * and pass `'laserline/laserline_bg.wasm'`.
   */
  wasmUrl: string;
  /**
   * Creates the slicing worker, whose script calls `serveSlicing()` from
   * `@javerik/laserline/worker`. Without it, the slicer is unavailable.
   */
  createWorker?: () => Worker;
  /** Graphics API: automatic (WebGPU, falling back to WebGL2) by default. */
  backend?: 'auto' | 'webgpu' | 'webgl';
  /** Prefix of the localStorage keys that persist the settings; `null` disables persistence. */
  storagePrefix?: string | null;
}

export const LASERLINE_CONFIG = new InjectionToken<LaserlineConfig>('LASERLINE_CONFIG');
