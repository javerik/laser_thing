import { TestBed } from '@angular/core/testing';
import { DetachedLaserView, type DetachedViewOptions, init, profileCurve } from '@javerik/laserline';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { LASERLINE_CONFIG } from './config';
import { LaserlineStore } from './store';

// The store would fetch `wasmUrl`; load the module from disk first so it reuses it.
// Tests run from the workspace root.
const wasm = readFile(resolve('packages/laserline/wasm/laserline_bg.wasm'));

async function store(storagePrefix: string | null = 'test.'): Promise<LaserlineStore> {
  await init(await wasm);
  TestBed.configureTestingModule({
    providers: [{ provide: LASERLINE_CONFIG, useValue: { wasmUrl: 'unused', storagePrefix } }, LaserlineStore],
  });
  const s = TestBed.inject(LaserlineStore);
  await s.loaded;
  return s;
}

/** In-memory `localStorage` (Node's own Web Storage hides jsdom's without a backing file). */
function memoryStorage(): Storage {
  const items = new Map<string, string>();
  return {
    get length() {
      return items.size;
    },
    clear: () => items.clear(),
    getItem: (k) => items.get(k) ?? null,
    key: (i) => [...items.keys()][i] ?? null,
    removeItem: (k) => void items.delete(k),
    setItem: (k, v) => void items.set(k, String(v)),
  };
}

describe('LaserlineStore', () => {
  beforeEach(() => vi.stubGlobal('localStorage', memoryStorage()));
  afterEach(() => vi.unstubAllGlobals());

  it('starts with the defaults and the built-in presets', async () => {
    const s = await store();
    expect(s.ready()).toBe(true);
    expect(s.params().line.width_px).toBe(6);
    expect(s.presets().length).toBe(6);
    expect(s.canSlice).toBe(false);
  });

  it('applies presets and persists parameters', async () => {
    vi.useFakeTimers();
    const s = await store();
    s.applyPreset(1);
    expect(s.params().motion.kind).toBe('Sweep');
    expect(s.status()).toContain('Sweep scan');
    vi.advanceTimersByTime(500);
    expect(JSON.parse(localStorage.getItem('test.params')!).motion.kind).toBe('Sweep');
    vi.useRealTimers();
  });

  it('restores stored parameters, filling in missing fields', async () => {
    localStorage.setItem('test.params', JSON.stringify({ line: { width_px: 3.5 } }));
    const s = await store();
    expect(s.params().line.width_px).toBe(3.5);
    expect(s.params().line.intensity_pct).toBe(100);
  });

  it('ignores storage when persistence is off', async () => {
    localStorage.setItem('params', JSON.stringify({ line: { width_px: 3.5 } }));
    const s = await store(null);
    expect(s.params().line.width_px).toBe(6);
  });

  it('reports unreadable scan files', async () => {
    const s = await store();
    await s.loadScan(new File([new Uint8Array([1, 2, 3])], 'broken.lscan'));
    expect(s.status()).toMatch(/^Load failed: broken.lscan: not a valid scan file/);
    expect(s.scan()).toBeUndefined();
  });
});

describe('plot helpers', () => {
  it('match the Gaussian cross-section of the shader', async () => {
    await init(await wasm);
    const width = 8;
    const curve = profileCurve({ kind: 'Gaussian', edge_softness_px: 0, end_softness_px: 0 }, width, -width, width, 33);
    const sigma = width / 2.35482;
    curve.forEach((v, i) => {
      const x = -width + i * 0.5;
      expect(v).toBeCloseTo(Math.exp(-0.5 * (x / sigma) ** 2), 5);
    });
  });
});

describe('LaserlineStore windows', () => {
  beforeEach(() => vi.stubGlobal('localStorage', memoryStorage()));
  afterEach(() => vi.unstubAllGlobals());

  describe('detached live view', () => {
    /** Stands in for the popup: records the options and calls. */
    function fakeWindow(ready: Promise<void> = Promise.resolve()) {
      let options!: DetachedViewOptions;
      const view = {
        ready,
        visible: true,
        fullscreen: false,
        backend: 'WebGL2',
        window: {
          closed: false,
          document: { visibilityState: 'visible' },
          requestAnimationFrame: vi.fn(() => 1),
          cancelAnimationFrame: vi.fn(),
        },
        render: vi.fn(),
        close: vi.fn(),
        toggleFullscreen: vi.fn(),
      };
      vi.spyOn(DetachedLaserView, 'open').mockImplementation((o) => {
        options = o;
        return view as unknown as DetachedLaserView;
      });
      return { view, options: () => options };
    }

    afterEach(() => vi.restoreAllMocks());

    it('detaches and docks the view', async () => {
      const s = await store();
      const { view } = fakeWindow();
      s.toggleViewDetached();
      expect(s.viewDetached()).toBe(true);
      s.toggleFullscreen();
      expect(view.toggleFullscreen).toHaveBeenCalled();
      s.toggleViewDetached();
      expect(s.viewDetached()).toBe(false);
      expect(view.close).toHaveBeenCalled();
    });

    it('docks the view when its window is closed', async () => {
      const s = await store();
      const { options } = fakeWindow();
      s.detachView();
      options().onClose();
      expect(s.viewDetached()).toBe(false);
    });

    it('stays docked when the pop-up is blocked', async () => {
      const s = await store();
      vi.spyOn(DetachedLaserView, 'open').mockReturnValue(undefined);
      s.detachView();
      expect(s.viewDetached()).toBe(false);
      expect(s.status()).toContain('Allow pop-ups');
    });

    it('docks again when the window cannot render', async () => {
      const s = await store();
      const { view } = fakeWindow(Promise.reject(new Error('no GPU')));
      s.detachView();
      await view.ready.catch(() => undefined);
      await Promise.resolve();
      expect(s.viewDetached()).toBe(false);
      expect(s.status()).toContain('no GPU');
    });

    it("runs the page's key handlers for keys pressed in the window", async () => {
      const s = await store();
      const { options } = fakeWindow();
      s.detachView();
      const seen: string[] = [];
      const handler = (e: KeyboardEvent) => {
        seen.push(e.key);
        e.preventDefault();
      };
      document.addEventListener('keydown', handler);
      const key = new KeyboardEvent('keydown', { key: 'v', cancelable: true });
      options().onKeyDown!(key);
      document.removeEventListener('keydown', handler);
      expect(seen).toEqual(['v']);
      expect(key.defaultPrevented).toBe(true);
    });
  });

  describe('slicer window', () => {
    /** A same-origin window stand-in with its own document. */
    function fakePopup() {
      const doc = document.implementation.createHTMLDocument('');
      const target = new EventTarget();
      const win = Object.assign(target, {
        document: doc,
        closed: false,
        focus: vi.fn(),
        close: vi.fn(() => {
          win.closed = true;
          target.dispatchEvent(new Event('pagehide'));
        }),
      });
      vi.spyOn(window, 'open').mockReturnValue(win as unknown as Window);
      return win;
    }

    afterEach(() => vi.restoreAllMocks());

    it('opens the slicer in its own window and back', async () => {
      const s = await store();
      const win = fakePopup();
      s.toggleSlicerDetached();
      expect(s.slicerWindow()).toBe(win);
      expect(s.slicerOpen()).toBe(true);
      expect(win.document.title).toBe('Object Slicer');
      s.toggleSlicerDetached();
      expect(win.close).toHaveBeenCalled();
      expect(s.slicerWindow()).toBeUndefined();
      expect(s.slicerOpen()).toBe(true);
    });

    it('closing the window closes the slicer', async () => {
      const s = await store();
      const win = fakePopup();
      s.detachSlicer();
      win.close();
      expect(s.slicerWindow()).toBeUndefined();
      expect(s.slicerOpen()).toBe(false);
    });

    it('closing the slicer closes its window', async () => {
      const s = await store();
      const win = fakePopup();
      s.detachSlicer();
      s.slicerOpen.set(false);
      TestBed.tick();
      expect(win.close).toHaveBeenCalled();
      expect(s.slicerWindow()).toBeUndefined();
    });

    it('stays in the page when the pop-up is blocked', async () => {
      const s = await store();
      vi.spyOn(window, 'open').mockReturnValue(null);
      s.detachSlicer();
      expect(s.slicerWindow()).toBeUndefined();
      expect(s.status()).toContain('Allow pop-ups');
    });
  });
});
