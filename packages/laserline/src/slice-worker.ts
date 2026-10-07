/**
 * Worker side of the slicing protocol. A worker entry script only needs:
 *
 * ```ts
 * import { serveSlicing } from '@javerik/laserline/worker';
 * serveSlicing();
 * ```
 */
import { Mesh, init, slice } from './index.js';
import type { FromWorker, ToWorker } from './protocol.js';

/** The parts of a worker's global scope used here. */
export interface WorkerScope {
  /** Sends a message to the page. */
  postMessage(message: FromWorker, transfer?: Transferable[]): void;
  /** Receives messages from the page. */
  addEventListener(type: 'message', listener: (event: MessageEvent<ToWorker>) => void): void;
}

/** Answers `SliceWorkerClient` requests in this worker. */
export function serveSlicing(scope: WorkerScope = self as unknown as WorkerScope): void {
  let ready: Promise<void> | undefined;
  let model: { id: number; mesh: Mesh } | undefined;

  scope.addEventListener('message', async ({ data: msg }) => {
    if (msg.type === 'init') {
      ready = init(msg.wasm);
      return;
    }
    await (ready ??= init());
    const id = msg.type === 'slice' ? msg.id : -1;
    try {
      if (msg.type === 'model') {
        model?.mesh.free();
        model = { id: msg.modelId, mesh: Mesh.parseStl(msg.bytes) };
      } else if (msg.type === 'slice') {
        if (model?.id !== msg.modelId) throw new Error('model not loaded in the worker');
        const t0 = performance.now();
        const progress = (value: number) => scope.postMessage({ type: 'progress', id, value });
        const scan = slice(model.mesh, msg.settings, msg.name, progress);
        const bytes = scan.toBytes();
        scan.free();
        const durationS = (performance.now() - t0) / 1000;
        scope.postMessage({ type: 'done', id, bytes, durationS }, [bytes.buffer]);
      }
    } catch (e) {
      scope.postMessage({ type: 'error', id, message: e instanceof Error ? e.message : String(e) });
    }
  });
}
