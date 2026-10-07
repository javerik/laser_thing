import type { SliceSettings } from '../wasm/laserline.js';

/** Messages to the slicing worker. */
export type ToWorker =
  | { type: 'init'; wasm?: string }
  | { type: 'model'; modelId: number; bytes: Uint8Array }
  | { type: 'slice'; id: number; modelId: number; settings: SliceSettings; name: string };

/** Messages from the slicing worker. */
export type FromWorker =
  | { type: 'progress'; id: number; value: number }
  | { type: 'done'; id: number; bytes: Uint8Array; durationS: number }
  | { type: 'error'; id: number; message: string };
