import { type ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideLaserline } from '@javerik/laserline-angular';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideLaserline({
      // Copied from packages/laserline/wasm by angular.json.
      wasmUrl: 'laserline/laserline_bg.wasm',
      createWorker: () => new Worker(new URL('./slice.worker', import.meta.url), { type: 'module' }),
      // ?backend=webgl forces WebGL2, ?backend=webgpu WebGPU.
      backend: (new URLSearchParams(location.search).get('backend') as 'webgl' | 'webgpu' | null) ?? 'auto',
      // Lets a camera application of this origin (e.g. RACS) calibrate against the line.
      remote: true,
    }),
  ],
};
