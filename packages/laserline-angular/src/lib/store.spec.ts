import { TestBed } from '@angular/core/testing';
import { init, profileCurve } from '@javerik/laserline';
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
