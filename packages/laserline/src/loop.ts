/** Longest frame step passed on, so a backgrounded tab does not jump ahead. */
const MAX_DT = 0.25;

/** The part of `Window` the loop needs. */
export type FrameScheduler = Pick<Window, 'requestAnimationFrame' | 'cancelAnimationFrame'>;

/**
 * Calls `frame(dt)` on every animation frame with the elapsed seconds. Frames follow
 * the display of `win` (default: this window); see {@link AnimationLoop.setWindow}.
 */
export class AnimationLoop {
  private handle = 0;

  constructor(
    private readonly frame: (dt: number) => void,
    private win: FrameScheduler = globalThis as unknown as FrameScheduler,
  ) {}

  get running(): boolean {
    return this.handle !== 0;
  }

  /**
   * Takes animation frames from `win`, e.g. a popup showing a view while this window is
   * hidden (a hidden window gets no animation frames). Keeps running if it was.
   */
  setWindow(win: FrameScheduler): void {
    if (win === this.win) return;
    const running = this.running;
    this.stop();
    this.win = win;
    if (running) this.start();
  }

  start(): void {
    if (this.handle) return;
    const win = this.win;
    // Frame timestamps count from the time origin of `win`, so the first frame only
    // sets the reference.
    let last: number | undefined;
    const step = (now: number) => {
      const dt = last === undefined ? 0 : Math.min(Math.max(now - last, 0) / 1000, MAX_DT);
      last = now;
      this.handle = win.requestAnimationFrame(step);
      this.frame(dt);
    };
    this.handle = win.requestAnimationFrame(step);
  }

  stop(): void {
    try {
      this.win.cancelAnimationFrame(this.handle);
    } catch {
      // `win` was closed: it schedules nothing any more.
    }
    this.handle = 0;
  }
}
