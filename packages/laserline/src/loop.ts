/** Longest frame step passed on, so a backgrounded tab does not jump ahead. */
const MAX_DT = 0.25;

/** Calls `frame(dt)` on every animation frame with the elapsed seconds. */
export class AnimationLoop {
  private handle = 0;
  private last = 0;

  constructor(private readonly frame: (dt: number) => void) {}

  get running(): boolean {
    return this.handle !== 0;
  }

  start(): void {
    if (this.handle) return;
    this.last = performance.now();
    const step = (now: number) => {
      const dt = Math.min(Math.max(now - this.last, 0) / 1000, MAX_DT);
      this.last = now;
      this.handle = requestAnimationFrame(step);
      this.frame(dt);
    };
    this.handle = requestAnimationFrame(step);
  }

  stop(): void {
    cancelAnimationFrame(this.handle);
    this.handle = 0;
  }
}
