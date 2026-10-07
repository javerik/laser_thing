import type { Orientation, Pattern } from '../wasm/laserline.js';

/**
 * Calibration patterns drawn over the laser view rather than by it: a white view, and the
 * stripes of a Gray code. Shown in order from the highest bit down, Gray-code stripes give every
 * camera pixel the view position it sees; the white and blank views give each pixel's threshold.
 */
export type OverlayPattern =
  | { kind: 'white' }
  | {
      kind: 'stripes';
      /** `Vertical` stripes encode x (the view column), `Horizontal` ones y. */
      orientation: Orientation;
      /** Bit of the Gray code shown: stripes `2^bit` px wide. */
      bit: number;
      /** Swap lit and dark. */
      inverted?: boolean;
    };

/** Every pattern a controller can show: the line patterns and the overlay ones. */
export type RemotePattern = Pattern | OverlayPattern;

/** Whether `pattern` is drawn over the view (see {@link drawOverlayPattern}) rather than by the laser. */
export function isOverlayPattern(pattern: RemotePattern): pattern is OverlayPattern {
  return pattern.kind === 'white' || pattern.kind === 'stripes';
}

/** The Gray code of `n`. */
export function grayCode(n: number): number {
  return n ^ (n >>> 1);
}

/** Whether view pixel `p` (column or row) is lit in the stripes of `bit`. */
export function stripeLit(p: number, bit: number, inverted = false): boolean {
  return (((grayCode(p) >>> bit) & 1) === 1) !== inverted;
}

/** Draws `pattern` into a `width`×`height` px 2D context: white on black. */
export function drawOverlayPattern(ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D, pattern: OverlayPattern, width: number, height: number): void {
  ctx.fillStyle = '#000';
  ctx.fillRect(0, 0, width, height);
  ctx.fillStyle = '#fff';
  if (pattern.kind === 'white') {
    ctx.fillRect(0, 0, width, height);
    return;
  }
  const vertical = pattern.orientation === 'Vertical';
  const extent = vertical ? width : height;
  // Runs of lit pixels, one rectangle each.
  let start = -1;
  for (let p = 0; p <= extent; p++) {
    const lit = p < extent && stripeLit(p, pattern.bit, pattern.inverted);
    if (lit && start < 0) start = p;
    if (!lit && start >= 0) {
      if (vertical) ctx.fillRect(start, 0, p - start, height);
      else ctx.fillRect(0, start, width, p - start);
      start = -1;
    }
  }
}
