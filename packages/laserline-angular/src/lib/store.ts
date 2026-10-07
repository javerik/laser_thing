import { Injectable, NgZone, OnDestroy, effect, inject, signal, untracked } from '@angular/core';
import {
  AnimationLoop,
  DetachedLaserView,
  Emulator,
  type FullLaserParams,
  type FullLiveInfo,
  type FullSliceSettings,
  type GridInfo,
  type LaserView,
  type MeshView,
  type ModelInfo,
  type Preset,
  Scan,
  SliceRunner,
  SliceWorkerClient,
  Slicer,
  type ViewLabel,
  builtinPresets,
  defaultParams,
  defaultSliceSettings,
  downloadBytes,
  init,
  openPopup,
  presetFromJson,
  presetToJson,
  readFileBytes,
} from '@javerik/laserline';
import { LASERLINE_CONFIG } from './config';

/** How long a status message stays visible. */
const STATUS_MS = 5000;
/** Readouts (time, frame rate, slicer state) refresh at most this often. */
const READOUT_MS = 100;
/** Settings are written to localStorage at most this often. */
const PERSIST_MS = 300;

export interface SlicerState {
  busy: boolean;
  /** Progress 0..1 of the running slice. */
  progress: number | undefined;
  /** Settings changed since the last slice. */
  stale: boolean;
  error: string | undefined;
  lastDurationS: number | undefined;
  model: ModelInfo | undefined;
  /** Model size in the scan frame (along line, travel, height) in mm. */
  sizeMm: [number, number, number] | undefined;
  grid: GridInfo | undefined;
}

const IDLE: SlicerState = {
  busy: false,
  progress: undefined,
  stale: false,
  error: undefined,
  lastDurationS: undefined,
  model: undefined,
  sizeMm: undefined,
  grid: undefined,
};

const NO_LIVE: FullLiveInfo = {
  view_px: [0, 0],
  center_px: [0, 0],
  offset_px: 0,
  length_px: 0,
  extent_along_px: 0,
  scan: undefined as unknown as FullLiveInfo['scan'],
};

interface LaserViewEntry {
  view: LaserView;
  afterRender?: () => void;
}

interface MeshViewEntry {
  view: MeshView;
  onLabels: (labels: ViewLabel[]) => void;
  afterRender?: () => void;
}

/**
 * State shared by the laserline components: the emulator, the slicer, the loaded scan
 * and the render loop driving the views. Provide it with `provideLaserline()`.
 */
@Injectable()
export class LaserlineStore implements OnDestroy {
  private readonly config = inject(LASERLINE_CONFIG);
  private readonly zone = inject(NgZone);

  /** The wasm module is loaded and the state below is valid. */
  readonly ready = signal(false);
  /** Why loading failed. */
  readonly loadError = signal<string | undefined>(undefined);

  readonly params = signal<FullLaserParams>(undefined as unknown as FullLaserParams);
  readonly playing = signal(true);
  readonly simTime = signal(0);
  readonly fps = signal(0);
  /** Measurements of the last rendered live view. */
  readonly live = signal<FullLiveInfo>(NO_LIVE);
  readonly scan = signal<Scan | undefined>(undefined);
  /** Profile currently shown. */
  readonly row = signal(0);
  readonly sliceSettings = signal<FullSliceSettings>(undefined as unknown as FullSliceSettings);
  readonly sliceAuto = signal(true);
  readonly slicer = signal<SlicerState>(IDLE);
  /** The slicer panel is shown. */
  readonly slicerOpen = signal(false);
  /**
   * The window the slicer is shown in (`detachSlicer()`), if not in the page. Render the
   * slicer there with `<ll-window-portal>`.
   */
  readonly slicerWindow = signal<Window | undefined>(undefined);
  readonly status = signal<string | undefined>(undefined);
  readonly presets = signal<Preset[]>([]);
  /** Graphics API of the first view, e.g. "WebGL2". */
  readonly backend = signal<string | undefined>(undefined);
  /**
   * The live view is shown in a window of its own (`detachView()`); `<ll-laser-view>`
   * shows a placeholder meanwhile.
   */
  readonly viewDetached = signal(false);
  /** The window showing the live view (this page or the detached one) is fullscreen. */
  readonly fullscreen = signal(false);
  /** A slicing worker is configured. */
  readonly canSlice = !!this.config.createWorker;

  private emu?: Emulator;
  private slicerCore?: Slicer;
  private runner?: SliceRunner;
  private client?: SliceWorkerClient;
  private readonly loop = new AnimationLoop((dt) => this.frame(dt));
  private readonly laserViews = new Set<LaserViewEntry>();
  private readonly meshViews = new Set<MeshViewEntry>();
  private detached?: DetachedLaserView;
  private readonly syncWindows = () => {
    const popup = this.detached;
    // A hidden window gets no animation frames: follow one being looked at, preferring
    // the live view's.
    const shown = (w: Window | undefined) => !!w && !w.closed && w.document.visibilityState === 'visible';
    const candidates = [popup?.visible ? popup.window : undefined, window, this.slicerWindow()];
    this.loop.setWindow(candidates.find(shown) ?? window);
    this.fullscreen.set(popup ? popup.fullscreen : !!document.fullscreenElement);
  };
  private lastReadout = 0;
  private statusTimer?: ReturnType<typeof setTimeout>;
  private persistTimer?: ReturnType<typeof setTimeout>;

  /** Resolves once `ready`; rejects if loading failed. */
  readonly loaded: Promise<void> = init(this.config.wasmUrl).then(
    () => this.start(),
    (e: unknown) => {
      this.loadError.set(`Loading laserline failed: ${message(e)}`);
      throw e;
    },
  );

  constructor() {
    this.loaded.catch(() => undefined);
    document.addEventListener('fullscreenchange', this.syncWindows);
    document.addEventListener('visibilitychange', this.syncWindows);
    // Closing the slicer also closes its window.
    effect(() => {
      if (!this.slicerOpen()) untracked(() => this.dockSlicer());
    });
  }

  ngOnDestroy(): void {
    document.removeEventListener('fullscreenchange', this.syncWindows);
    document.removeEventListener('visibilitychange', this.syncWindows);
    this.dockView();
    this.dockSlicer();
    this.loop.stop();
    this.client?.dispose();
    clearTimeout(this.statusTimer);
    if (this.persistTimer) {
      clearTimeout(this.persistTimer);
      this.persist();
    }
  }

  private start(): void {
    const stored = <T>(key: string): T | undefined => {
      const prefix = this.storagePrefix();
      if (prefix === null) return undefined;
      try {
        const text = localStorage.getItem(prefix + key);
        return text === null ? undefined : (JSON.parse(text) as T);
      } catch {
        return undefined;
      }
    };
    // Through the wasm side so stored values gain fields added since.
    const params = this.tryOr(() => new Emulator(stored('params')), () => new Emulator(undefined));
    this.emu = params;
    this.slicerCore = this.tryOr(
      () => new Slicer(stored('slice_settings'), stored<boolean>('slice_auto') ?? true),
      () => new Slicer(undefined, true),
    );
    this.params.set(this.emu.params as FullLaserParams);
    this.sliceSettings.set(this.slicerCore.settings as FullSliceSettings);
    this.sliceAuto.set(this.slicerCore.auto);
    this.presets.set(builtinPresets());
    if (this.config.createWorker) {
      this.client = new SliceWorkerClient(this.config.createWorker, this.config.wasmUrl);
      this.runner = new SliceRunner(this.slicerCore, this.client, (scan) => this.showScan(scan));
    }
    this.ready.set(true);
    this.zone.runOutsideAngular(() => this.loop.start());
  }

  private tryOr<T>(f: () => T, fallback: () => T): T {
    try {
      return f();
    } catch {
      return fallback();
    }
  }

  private storagePrefix(): string | null {
    return this.config.storagePrefix === undefined ? 'laserline.' : this.config.storagePrefix;
  }

  private frame(dt: number): void {
    const emu = this.emu!;
    emu.tick(dt);
    const now = performance.now();
    this.runner?.poll(now / 1000);
    let live = this.detached?.render(emu) as FullLiveInfo | undefined;
    for (const { view, afterRender } of this.laserViews) {
      live ??= view.render(emu) as FullLiveInfo;
      afterRender?.();
    }
    for (const { view, onLabels, afterRender } of this.meshViews) {
      onLabels(view.render(this.slicerCore!, emu));
      afterRender?.();
    }
    if (this.row() !== emu.currentRow) this.row.set(emu.currentRow);
    if (now - this.lastReadout >= READOUT_MS) {
      this.lastReadout = now;
      this.simTime.set(emu.simTime);
      this.fps.set(emu.fps);
      if (live && !sameLive(live, this.live())) this.live.set(live);
      this.refreshSlicer();
    }
  }

  /** Copies the slicer's state into `slicer`. */
  refreshSlicer(): void {
    const s = this.slicerCore;
    if (!s) return;
    const model = s.model();
    const next: SlicerState = {
      busy: s.isBusy,
      progress: this.runner?.progress,
      stale: s.isStale,
      error: s.error,
      lastDurationS: s.lastDurationS,
      model,
      sizeMm: model ? (Array.from(s.modelSizeMm() ?? []) as [number, number, number]) : undefined,
      grid: model ? s.gridInfo() : undefined,
    };
    if (JSON.stringify(next) !== JSON.stringify(this.slicer())) this.slicer.set(next);
  }

  // ---- views

  /**
   * Draws `view` every frame until the returned function is called, then calls
   * `afterRender` (e.g. `CanvasBridge.present`).
   */
  attachLaserView(view: LaserView, afterRender?: () => void): () => void {
    const entry = { view, afterRender };
    this.laserViews.add(entry);
    this.backend.set(view.backend);
    return () => this.laserViews.delete(entry);
  }

  /** Like `attachLaserView`; `onLabels` gets the view's labels after each frame. */
  attachMeshView(view: MeshView, onLabels: (labels: ViewLabel[]) => void, afterRender?: () => void): () => void {
    const entry = { view, onLabels, afterRender };
    this.meshViews.add(entry);
    this.backend.update((b) => b ?? view.backend);
    return () => this.meshViews.delete(entry);
  }

  // ---- live view window

  /**
   * Moves the live view into a window of its own, e.g. to show it fullscreen on another
   * screen. Call it from a user action (click, key press): browsers block pop-ups
   * otherwise. Key presses in that window are dispatched on this page's `document.body`
   * as well, so the page's shortcuts keep working there.
   */
  detachView(): void {
    if (this.detached || !this.ready()) return;
    const view = DetachedLaserView.open({
      backend: this.config.backend ?? 'auto',
      onClose: () => this.dockView(),
      onKeyDown: (e) => forwardKey(e),
      onChange: this.syncWindows,
    });
    if (!view) {
      this.setStatus('Allow pop-ups for this page to detach the live view');
      return;
    }
    this.detached = view;
    this.viewDetached.set(true);
    this.syncWindows();
    view.ready.then(
      () => {
        if (this.detached === view) this.backend.set(view.backend);
      },
      (e: unknown) => {
        if (this.detached !== view) return;
        this.dockView();
        this.setStatus(`Detaching the live view failed: ${message(e)}`);
      },
    );
  }

  /** Puts the live view back into the page and closes its window. */
  dockView(): void {
    const view = this.detached;
    if (!view) return;
    this.detached = undefined;
    view.close();
    this.viewDetached.set(false);
    this.syncWindows();
  }

  toggleViewDetached(): void {
    if (this.detached) this.dockView();
    else this.detachView();
  }

  /**
   * Toggles fullscreen of the window showing the live view (the detached one, or this
   * page). Call it from a user action.
   */
  toggleFullscreen(): void {
    if (this.detached) this.detached.toggleFullscreen();
    else if (document.fullscreenElement) void document.exitFullscreen();
    else void document.documentElement.requestFullscreen().catch(() => undefined);
  }

  // ---- slicer window

  /**
   * Opens the slicer in a window of its own (call it from a user action, like
   * `detachView()`); render it there with `<ll-window-portal [window]="store.slicerWindow()">`.
   * Closing that window closes the slicer.
   */
  detachSlicer(): void {
    if (this.slicerWindow() || !this.ready()) return;
    const win = openPopup({ name: 'laserline-slicer', title: 'Object Slicer', features: 'popup,width=1200,height=820' });
    if (!win) {
      this.setStatus('Allow pop-ups for this page to open the slicer in its own window');
      return;
    }
    win.addEventListener('pagehide', () => {
      if (this.slicerWindow() !== win) return;
      this.slicerWindow.set(undefined);
      this.slicerOpen.set(false);
      this.syncWindows();
    });
    win.addEventListener('keydown', forwardKey);
    win.document.addEventListener('visibilitychange', this.syncWindows);
    this.slicerWindow.set(win);
    this.slicerOpen.set(true);
    this.syncWindows();
  }

  /** Closes the slicer's window; the slicer shows in the page again if still open. */
  dockSlicer(): void {
    const win = this.slicerWindow();
    if (!win) return;
    this.slicerWindow.set(undefined);
    win.close();
    this.syncWindows();
  }

  toggleSlicerDetached(): void {
    if (this.slicerWindow()) this.dockSlicer();
    else this.detachSlicer();
  }

  // ---- 3D view camera

  orbit(dx: number, dy: number): void {
    this.slicerCore?.orbit(dx, dy);
  }

  pan(dx: number, dy: number): void {
    this.slicerCore?.pan(dx, dy);
  }

  zoom(scroll: number, pinch = 1): void {
    this.slicerCore?.zoom(scroll, pinch);
  }

  resetCamera(): void {
    this.slicerCore?.resetCamera();
  }

  // ---- parameters and playback

  setParams(params: FullLaserParams): void {
    this.emu!.params = params;
    this.params.set(params);
    this.schedulePersist();
  }

  patchParams(patch: Partial<FullLaserParams>): void {
    this.setParams({ ...this.params(), ...patch });
  }

  togglePlaying(): void {
    this.setPlaying(!this.playing());
  }

  setPlaying(playing: boolean): void {
    this.emu!.playing = playing;
    this.playing.set(playing);
  }

  /** Rewinds the clock and the scan playback. */
  resetTime(): void {
    this.emu!.resetTime();
    this.syncPlayback();
  }

  stepScan(delta: number): void {
    this.emu!.stepScan(delta);
    this.syncPlayback();
  }

  seekRow(row: number): void {
    this.emu!.seekRow(row);
    this.syncPlayback();
  }

  private syncPlayback(): void {
    this.row.set(this.emu!.currentRow);
    this.simTime.set(this.emu!.simTime);
  }

  applyPreset(index: number): void {
    const preset = this.presets()[index];
    if (!preset) return;
    this.setParams(preset.params as FullLaserParams);
    this.emu!.simTime = 0;
    this.setStatus(`Applied “${preset.name}”`);
  }

  /** Default parameters (a fresh object each call). */
  defaults(): FullLaserParams {
    return defaultParams() as FullLaserParams;
  }

  resetParams(): void {
    this.setParams(defaultParams() as FullLaserParams);
    this.setStatus('All parameters reset');
  }

  savePreset(): void {
    downloadBytes(presetToJson(this.params()), 'laser_preset.json', 'application/json');
    this.setStatus('Saved laser_preset.json');
  }

  async loadPreset(file: File): Promise<void> {
    try {
      this.setParams(presetFromJson(await file.text()) as FullLaserParams);
      this.setStatus(`Loaded ${file.name}`);
    } catch (e) {
      this.setStatus(`Load failed: ${message(e)}`);
    }
  }

  // ---- scans and models

  /** Opens an `.stl` model (into the slicer) or a `.lscan` scan. */
  async openFile(file: File): Promise<void> {
    const ext = file.name.split('.').pop()?.toLowerCase();
    if (ext === 'stl') await this.importStl(file);
    else if (ext === 'lscan') await this.loadScan(file);
    else this.setStatus(`${file.name}: expected an .stl or .lscan file`);
  }

  async importStl(file: File): Promise<void> {
    if (!this.runner) {
      this.setStatus('Import failed: no slicing worker configured');
      return;
    }
    try {
      const triangles = this.runner.loadModel(await readFileBytes(file), file.name);
      this.slicerOpen.set(true);
      this.setStatus(`Imported ${file.name} (${triangles} triangles)`);
    } catch (e) {
      this.setStatus(`Import failed: ${file.name}: ${message(e)}`);
    }
    this.refreshSlicer();
  }

  /** Plays a `.lscan` file without its model; its settings are shown in the slicer. */
  async loadScan(file: File): Promise<void> {
    try {
      const scan = Scan.fromBytes(await readFileBytes(file));
      this.runner?.cancel();
      this.slicerCore!.adoptScan(scan);
      this.sliceSettings.set(this.slicerCore!.settings as FullSliceSettings);
      this.emu!.seekRow(0);
      this.showScan(scan);
      this.setStatus(`Loaded ${file.name}`);
    } catch (e) {
      this.setStatus(`Load failed: ${file.name}: ${message(e)}`);
    }
    this.refreshSlicer();
  }

  saveScan(): void {
    const scan = this.scan();
    if (!scan) return;
    try {
      downloadBytes(scan.toBytes(), scan.fileName());
      this.setStatus(`Saved ${scan.fileName()}`);
    } catch (e) {
      this.setStatus(`Save failed: ${message(e)}`);
    }
  }

  private showScan(scan: Scan): void {
    this.emu!.setScan(scan);
    this.scan.set(scan);
    this.row.set(this.emu!.currentRow);
  }

  setSliceSettings(settings: FullSliceSettings): void {
    this.slicerCore!.settings = settings;
    this.sliceSettings.set(settings);
    this.schedulePersist();
    this.refreshSlicer();
  }

  resetSliceSettings(): void {
    this.setSliceSettings(defaultSliceSettings() as FullSliceSettings);
  }

  setSliceAuto(auto: boolean): void {
    this.slicerCore!.auto = auto;
    this.sliceAuto.set(auto);
    this.schedulePersist();
  }

  sliceNow(): void {
    this.runner?.sliceNow();
    this.refreshSlicer();
  }

  // ---- status and persistence

  setStatus(msg: string): void {
    clearTimeout(this.statusTimer);
    this.status.set(msg);
    this.statusTimer = setTimeout(() => this.status.set(undefined), STATUS_MS);
  }

  private schedulePersist(): void {
    if (this.storagePrefix() === null || this.persistTimer) return;
    this.persistTimer = setTimeout(() => {
      this.persistTimer = undefined;
      this.persist();
    }, PERSIST_MS);
  }

  private persist(): void {
    const prefix = this.storagePrefix();
    if (prefix === null || !this.ready()) return;
    try {
      localStorage.setItem(prefix + 'params', JSON.stringify(this.params()));
      localStorage.setItem(prefix + 'slice_settings', JSON.stringify(this.sliceSettings()));
      localStorage.setItem(prefix + 'slice_auto', JSON.stringify(this.sliceAuto()));
    } catch {
      // Storage unavailable (private mode, quota): settings just do not persist.
    }
  }
}

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** Runs this page's key handlers for a key press in one of its other windows. */
function forwardKey(e: KeyboardEvent): void {
  // Typing into the window's own fields is not a shortcut.
  if ((e.target as Element | null)?.closest?.('input, select, textarea, [contenteditable]')) return;
  const { key, code, location, repeat, ctrlKey, shiftKey, altKey, metaKey } = e;
  const copy = new KeyboardEvent('keydown', {
    key, code, location, repeat, ctrlKey, shiftKey, altKey, metaKey, bubbles: true, cancelable: true,
  });
  if (!document.body.dispatchEvent(copy)) e.preventDefault();
}

function sameLive(a: FullLiveInfo, b: FullLiveInfo): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
