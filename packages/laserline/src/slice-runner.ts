import { Mesh, type Scan, type SliceRequest, type Slicer } from '../wasm/laserline.js';
import type { SliceWorkerClient } from './slice-client.js';

/**
 * Connects a `Slicer` to a `SliceWorkerClient`: runs what the slicer requests,
 * cancels superseded slices and reports results back. Call `poll` every frame.
 */
export class SliceRunner {
  /** Progress (0..1) of the running slice, `undefined` when idle. */
  progress: number | undefined;
  private job: number | undefined;

  /** `onScan` receives each current scan; it owns it and must free it. */
  constructor(
    /** The slicer whose requests are run. */
    readonly slicer: Slicer,
    private readonly client: SliceWorkerClient,
    private readonly onScan: (scan: Scan) => void,
  ) {}

  /** Parses an STL file (throws on errors) and loads it; returns its triangle count. */
  loadModel(bytes: Uint8Array, name: string, location = name): number {
    const mesh = Mesh.parseStl(bytes);
    const triangles = mesh.triangleCount;
    const request = this.slicer.setModel(mesh, name, location);
    mesh.free();
    this.client.setModel(this.slicer.model()!.id, bytes);
    if (request) this.run(request);
    return triangles;
  }

  /** Slices with the current settings. */
  sliceNow(): void {
    const request = this.slicer.start();
    if (request) this.run(request);
  }

  /** Call every frame with a clock in seconds (e.g. `performance.now() / 1000`). */
  poll(nowS: number): void {
    if (this.job !== undefined && this.slicer.running !== this.job) this.cancel();
    const request = this.slicer.poll(nowS);
    if (request) this.run(request);
  }

  /** Stops the running slice; the slicer records it as cancelled, not as an error. */
  cancel(): void {
    this.client.cancel();
    this.job = undefined;
    this.progress = undefined;
  }

  private run(request: SliceRequest): void {
    this.client.cancel();
    this.job = request.id;
    this.progress = 0;
    const current = () => this.job === request.id;
    this.client
      .slice(request, (p) => {
        if (current()) this.progress = p;
      })
      .then(
        ({ scan, durationS }) => {
          if (this.slicer.finish(request.id, durationS)) this.onScan(scan);
          else scan.free();
        },
        (e: unknown) => this.slicer.fail(request.id, e instanceof Error ? e.message : String(e)),
      )
      .finally(() => {
        if (current()) {
          this.job = undefined;
          this.progress = undefined;
        }
      });
  }
}
