import { TestBed } from '@angular/core/testing';
import {
  DetachedLaserView,
  type DetachedViewOptions,
  type FromLaserline,
  type LaserView,
  type ToLaserline,
  init,
  profileCurve,
} from '@javerik/laserline';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { LASERLINE_CONFIG, type LaserlineConfig } from './config';
import { LaserlineStore } from './store';

// The store would fetch `wasmUrl`; load the module from disk first so it reuses it.
// Tests run from the workspace root.
const wasm = readFile(resolve('packages/laserline/wasm/laserline_bg.wasm'));

async function store(storagePrefix: string | null = 'test.', extra: Partial<LaserlineConfig> = {}): Promise<LaserlineStore> {
  await init(await wasm);
  TestBed.configureTestingModule({
    providers: [{ provide: LASERLINE_CONFIG, useValue: { wasmUrl: 'unused', storagePrefix, ...extra } }, LaserlineStore],
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

  it('runs shortcut actions', async () => {
    const s = await store();
    expect(s.dispatch('togglePlay')).toBe(true);
    expect(s.playing()).toBe(false);
    s.dispatch('toggleSlicer');
    expect(s.slicerOpen()).toBe(true);
    s.dispatch('toggleControlsHidden');
    expect(s.controls()).toBe('hidden');
    s.dispatch('toggleControlsHidden');
    expect(s.controls()).toBe('docked');
    expect(s.dispatch('exitFullscreen')).toBe(false);
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
        setOverlay: vi.fn(),
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

  describe('slicer and controls windows', () => {
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

    it('detaches, hides and docks the controls', async () => {
      const s = await store();
      const win = fakePopup();
      expect(s.dispatch('toggleControlsDetached')).toBe(true);
      expect(s.controls()).toBe('detached');
      expect(s.controlsWindow()).toBe(win);
      s.dispatch('toggleControlsHidden');
      expect(s.controls()).toBe('hidden');
      expect(win.close).toHaveBeenCalled();
      expect(s.controlsWindow()).toBeUndefined();
      const again = fakePopup();
      s.dispatch('toggleControlsHidden');
      expect(s.controls()).toBe('detached');
      expect(s.controlsWindow()).toBe(again);
      again.close();
      expect(s.controls()).toBe('docked');
    });

        it('stays in the page when the pop-up is blocked', async () => {
      const s = await store();
      vi.spyOn(window, 'open').mockReturnValue(null);
      s.detachSlicer();
      expect(s.slicerWindow()).toBeUndefined();
      expect(s.status()).toContain('Allow pop-ups');
    });
  });

  describe('remote calibration', () => {
    const VIEW = [1000, 500];
    const channels: BroadcastChannel[] = [];
    afterEach(() => channels.splice(0).forEach((c) => c.close()));

    /** A store with a fake view (rendering `VIEW`), a frame stepper and a controller channel. */
    async function remoteStore(name: string) {
      const s = await store('test.', { remote: { channel: name } });
      const rendered: unknown[] = [];
      const view = {
        backend: 'fake',
        render: (emu: { params: unknown }) => {
          rendered.push(emu.params);
          return { view_px: VIEW, center_px: [500, 250], offset_px: 0, length_px: 800, extent_along_px: 1000 };
        },
      } as unknown as LaserView;
      s.attachLaserView(view);
      const step = () => (s as unknown as { frame(dt: number): void }).frame(0.016);
      const ch = new BroadcastChannel(name);
      channels.push(ch);
      const got: FromLaserline[] = [];
      ch.onmessage = (e) => got.push(e.data);
      type Msg = { [K in ToLaserline['type']]: Omit<Extract<ToLaserline, { type: K }>, 'v'> }[ToLaserline['type']];
      const send = (msg: Msg) => ch.postMessage({ v: 1, ...msg });
      const next = async <T extends FromLaserline['type']>(type: T) => {
        for (let i = 0; i < 200; i++) {
          const k = got.findIndex((m) => m.type === type);
          if (k >= 0) return got.splice(k, 1)[0] as Extract<FromLaserline, { type: T }>;
          await new Promise((r) => setTimeout(r, 5));
        }
        throw new Error(`no ${type}`);
      };
      const until = async (cond: () => boolean) => {
        for (let i = 0; i < 200 && !cond(); i++) await new Promise((r) => setTimeout(r, 5));
        expect(cond()).toBe(true);
      };
      return { s, step, send, next, until, rendered };
    }

    it('reports the view and the envelope of the user\'s line', async () => {
      const { s, step, send, next } = await remoteStore('spec-remote-1');
      step();
      send({ type: 'hello' });
      const status = (await next('status')).status;
      expect(status.viewPx).toEqual(VIEW);
      expect(status.envelope!.center_px).toEqual([500, 250]);
      expect(status.session).toBeNull();
      s.ngOnDestroy();
    });

    it('shows patterns without touching the parameters, then restores them', async () => {
      const { s, step, send, next, until, rendered } = await remoteStore('spec-remote-2');
      step();
      send({ type: 'hello' });
      const host = (await next('status')).host;
      const before = s.params();
      send({ type: 'begin', to: host, session: 'cal' });
      await until(() => s.calibrating());
      send({ type: 'show', to: host, session: 'cal', id: 1, pattern: { kind: 'line', orientation: 'Vertical', position_pct: 30 } });
      await new Promise((r) => setTimeout(r, 20));
      step();
      const drawn = rendered.at(-1) as typeof before;
      expect(drawn.line.orientation).toBe('Vertical');
      expect(drawn.line.position_pct).toEqual([30, 50]);
      expect(drawn.profile.kind).toBe('Hard');
      step();
      expect((await next('shown')).id).toBe(1);
      send({ type: 'show', to: host, session: 'cal', id: 2, pattern: { kind: 'line', orientation: 'Vertical', position_pct: 50, width_pct: 10 } });
      await new Promise((r) => setTimeout(r, 20));
      step();
      expect((rendered.at(-1) as typeof before).line.width_px).toBe(100);
      step();
      expect((await next('shown')).id).toBe(2);
      // Overlay patterns: drawn over a blank laser view, gone when the session ends.
      send({ type: 'show', to: host, session: 'cal', id: 3, pattern: { kind: 'stripes', orientation: 'Vertical', bit: 4 } });
      await until(() => s.overlayPattern()?.kind === 'stripes');
      step();
      expect((rendered.at(-1) as typeof before).line.intensity_pct).toBe(0);
      step();
      expect((await next('shown')).id).toBe(3);
      // Edits during the session wait for its end.
      s.patchParams({ line: { ...before.line, width_px: 9 } });
      step();
      expect((rendered.at(-1) as typeof before).line.width_px).not.toBe(9);
      send({ type: 'end', to: host, session: 'cal' });
      await until(() => !s.calibrating());
      expect(s.overlayPattern()).toBeUndefined();
      step();
      expect((rendered.at(-1) as typeof before).line.width_px).toBe(9);
      expect(s.playing()).toBe(true);
      s.ngOnDestroy();
    });

    it('keeps the camera field and fits the line into it', async () => {
      const { s, step, send, next, until } = await remoteStore('spec-remote-3');
      step();
      send({ type: 'hello' });
      const host = (await next('status')).host;
      const field: [number, number][] = [[400, 100], [700, 100], [700, 300], [400, 300]];
      send({ type: 'camera-field', to: host, polygon: field, label: 'cam 0' });
      await until(() => !!s.cameraField());
      expect(s.cameraField()).toEqual(field);
      expect(s.cameraFieldLabel()).toBe('cam 0');
      expect(JSON.parse(localStorage.getItem('test.camera_field')!).label).toBe('cam 0');
      send({ type: 'fit-to-field', to: host, id: 5, margin: 10 });
      expect((await next('fitted')).fits).toBe(true);
      expect(s.params().line.position_pct[0]).toBeCloseTo(55, 1);
      s.setCameraField(undefined);
      expect(localStorage.getItem('test.camera_field')).toBeNull();
      s.ngOnDestroy();
    });
  });
});
