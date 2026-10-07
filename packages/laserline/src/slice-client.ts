import { Scan, type SliceRequest } from '../wasm/laserline.js';
import type { FromWorker, ToWorker } from './protocol.js';

export interface SliceResult {
  scan: Scan;
  durationS: number;
}

interface Pending {
  id: number;
  resolve: (r: SliceResult) => void;
  reject: (e: Error) => void;
  onProgress?: (value: number) => void;
}

/**
 * Runs slices in a Web Worker that calls `serveSlicing()`. One slice at a time;
 * cancelling terminates the worker, a fresh one starts with the next slice.
 */
export class SliceWorkerClient {
  private worker?: Worker;
  private workerModel?: number;
  private model?: { id: number; bytes: Uint8Array };
  private pending?: Pending;

  /**
   * @param createWorker creates the worker, e.g.
   *   `() => new Worker(new URL('./slice.worker', import.meta.url), { type: 'module' })`
   * @param wasm URL of `laserline_bg.wasm` for the worker (as passed to `init`).
   */
  constructor(
    private readonly createWorker: () => Worker,
    private readonly wasm?: string | URL,
  ) {}

  /** STL file of model `modelId` (`Slicer.model().id`), sent to the worker when needed. */
  setModel(modelId: number, bytes: Uint8Array): void {
    this.model = { id: modelId, bytes };
  }

  get busy(): boolean {
    return this.pending !== undefined;
  }

  /** Slices `request`; a running slice is cancelled first. */
  slice(request: SliceRequest, onProgress?: (value: number) => void): Promise<SliceResult> {
    this.cancel();
    this.ensureWorker();
    if (this.workerModel !== request.modelId) {
      if (this.model?.id !== request.modelId) return Promise.reject(new Error(`model ${request.modelId} not set`));
      this.post({ type: 'model', modelId: request.modelId, bytes: this.model.bytes });
      this.workerModel = request.modelId;
    }
    return new Promise<SliceResult>((resolve, reject) => {
      this.pending = { id: request.id, resolve, reject, onProgress };
      this.post({ type: 'slice', id: request.id, modelId: request.modelId, settings: request.settings, name: request.name });
    });
  }

  /** Stops the running slice; its promise rejects with `Error('cancelled')`. */
  cancel(): void {
    const p = this.pending;
    if (!p) return;
    this.pending = undefined;
    this.worker?.terminate();
    this.worker = undefined;
    this.workerModel = undefined;
    p.reject(new Error('cancelled'));
  }

  dispose(): void {
    this.cancel();
    this.worker?.terminate();
    this.worker = undefined;
  }

  private ensureWorker(): Worker {
    if (this.worker) return this.worker;
    const worker = this.createWorker();
    worker.onmessage = (e: MessageEvent<FromWorker>) => this.receive(e.data);
    worker.onerror = (e) => {
      const p = this.pending;
      this.pending = undefined;
      this.worker?.terminate();
      this.worker = undefined;
      this.workerModel = undefined;
      p?.reject(new Error(e.message || 'slicing worker failed'));
    };
    this.worker = worker;
    const wasm = this.wasm === undefined ? undefined : new URL(this.wasm, location.href).href;
    this.post({ type: 'init', wasm });
    return worker;
  }

  private post(msg: ToWorker): void {
    this.worker!.postMessage(msg);
  }

  private receive(msg: FromWorker): void {
    const p = this.pending;
    if (!p || msg.id !== p.id) return;
    if (msg.type === 'progress') {
      p.onProgress?.(msg.value);
      return;
    }
    this.pending = undefined;
    if (msg.type === 'done') {
      try {
        p.resolve({ scan: Scan.fromBytes(msg.bytes), durationS: msg.durationS });
      } catch (e) {
        p.reject(e instanceof Error ? e : new Error(String(e)));
      }
    } else {
      p.reject(new Error(msg.message));
    }
  }
}
