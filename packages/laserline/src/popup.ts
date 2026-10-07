/** Options of {@link openPopup}. */
export interface PopupOptions {
  /** Window name; opening another popup with the same name reuses that window. */
  name: string;
  /** Document title of the window. */
  title: string;
  /** `window.open` features (default: a 1280×720 popup). */
  features?: string;
}

/**
 * Opens an empty window of this page's origin to show part of the page in, e.g. on
 * another screen. Relative URLs in it resolve against this page, and it closes along
 * with this page. Browsers only allow this in response to a user action (a click, a
 * key press); returns `undefined` if the popup was blocked.
 */
export function openPopup(options: PopupOptions): Window | undefined {
  const win = window.open('', options.name, options.features ?? 'popup,width=1280,height=720');
  if (!win) return undefined;
  const doc = win.document;
  const base = doc.createElement('base');
  base.href = document.baseURI;
  doc.head.replaceChildren(base);
  doc.body.replaceChildren();
  doc.title = options.title;
  const closeWithPage = () => win.close();
  window.addEventListener('pagehide', closeWithPage);
  win.addEventListener('pagehide', () => window.removeEventListener('pagehide', closeWithPage));
  win.focus();
  return win;
}

/**
 * Shows frames rendered into an `OffscreenCanvas` of this window on `canvas`, which may
 * belong to another window (a popup): WebGL / WebGPU canvases of another window cannot
 * be drawn from here. Render into {@link offscreen}, then call {@link present}.
 */
export class CanvasBridge {
  /** Canvas to render into; same size as the target canvas at construction. */
  readonly offscreen: OffscreenCanvas;
  private readonly output: ImageBitmapRenderingContext;

  /** Throws if `canvas` offers no `bitmaprenderer` context. */
  constructor(canvas: HTMLCanvasElement) {
    const output = canvas.getContext('bitmaprenderer');
    if (!output) throw new Error('canvas offers no bitmaprenderer context');
    this.output = output;
    this.offscreen = new OffscreenCanvas(Math.max(1, canvas.width), Math.max(1, canvas.height));
  }

  /** Hands the frame just rendered into `offscreen` to the canvas. */
  present(): void {
    this.output.transferFromImageBitmap(this.offscreen.transferToImageBitmap());
  }
}
