import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { LaserlineStore, LlControls, LlLaserView, LlSlicer } from '@javerik/laserline-angular';

/**
 * Web version of the desktop emulator: the live view fills the window, the controls
 * are docked on the right (H hides them) and the slicer opens over the view.
 */
@Component({
  selector: 'app-root',
  imports: [LlLaserView, LlControls, LlSlicer],
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    '(document:keydown)': 'key($event)',
    '(dragover)': '$event.preventDefault()',
    '(drop)': 'drop($event)',
  },
  template: `
    <main [class.hidden]="hidden()">
      <ll-laser-view class="live" (activate)="toggleFullscreen()" />
      @if (hidden()) {
        @if (hintVisible()) {
          <div class="hint">Controls hidden — press H to show</div>
        }
      } @else {
        <aside>
          <header>
            <h1>Laser Line</h1>
            <button type="button" title="Hide controls (H)" (click)="toggleHidden()">Hide</button>
            <button type="button" title="Toggle fullscreen (F11, double-click the view)" (click)="toggleFullscreen()">
              {{ fullscreen() ? 'Windowed' : 'Fullscreen' }}
            </button>
          </header>
          <ll-controls />
          <p class="keys">
            Keys: F11 fullscreen · H hide · Space play/pause · R reset time · ←/→ step scan profile · S slicer · drop .stl / .lscan files anywhere
          </p>
        </aside>
      }
      @if (store.slicerOpen()) {
        <div class="slicer" role="dialog" aria-label="Object slicer">
          <header>
            <h1>Object Slicer</h1>
            <button type="button" aria-label="Close slicer" (click)="store.slicerOpen.set(false)">✕</button>
          </header>
          <ll-slicer />
        </div>
      }
    </main>
  `,
  styles: `
    main { position: fixed; inset: 0; display: grid; grid-template-columns: minmax(0, 1fr) 380px; font-family: system-ui, sans-serif; color: #d9dce1; }
    main.hidden { grid-template-columns: 1fr; }
    .live { min-width: 0; min-height: 0; }
    aside { overflow-y: auto; background: #15171a; border-left: 1px solid #31353b; padding: 8px 12px; }
    header { display: flex; align-items: center; gap: 6px; }
    h1 { font-size: 1.3em; margin: 0 auto 0 0; }
    header button { font: inherit; font-size: 13px; color: inherit; background: #1d2024; border: 1px solid #31353b; border-radius: 3px; padding: 2px 8px; cursor: pointer; }
    .keys { font-size: 11px; color: #8a9099; }
    .hint { position: absolute; left: 0; right: 0; bottom: 24px; text-align: center; color: #a0a0a0; font-size: 16px; pointer-events: none; }
    .slicer { position: absolute; inset: 24px; display: flex; flex-direction: column; background: #15171a; border: 1px solid #31353b; border-radius: 4px; box-shadow: 0 8px 40px #000a; }
    .slicer header { padding: 6px 10px; border-bottom: 1px solid #31353b; }
    .slicer ll-slicer { flex: 1; min-height: 0; overflow: auto; }
    @media (max-width: 760px) {
      main { grid-template-columns: 1fr; grid-template-rows: 45vh minmax(0, 1fr); }
      aside { border-left: 0; border-top: 1px solid #31353b; }
      .slicer { inset: 0; border-radius: 0; }
    }
  `,
})
export class App {
  protected readonly store = inject(LaserlineStore);
  protected readonly hidden = signal(false);
  protected readonly hintVisible = signal(false);
  protected readonly fullscreen = signal(false);
  private hintTimer?: ReturnType<typeof setTimeout>;

  constructor() {
    document.addEventListener('fullscreenchange', () => this.fullscreen.set(!!document.fullscreenElement));
  }

  protected toggleHidden(): void {
    this.hidden.update((h) => !h);
    clearTimeout(this.hintTimer);
    this.hintVisible.set(this.hidden());
    if (this.hidden()) this.hintTimer = setTimeout(() => this.hintVisible.set(false), 3000);
  }

  protected toggleFullscreen(): void {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void document.documentElement.requestFullscreen();
  }

  protected key(e: KeyboardEvent): void {
    const target = e.target as HTMLElement | null;
    if (e.ctrlKey || e.metaKey || e.altKey || target?.closest('input, select, textarea')) return;
    const actions: Record<string, () => void> = {
      F11: () => this.toggleFullscreen(),
      h: () => this.toggleHidden(),
      H: () => this.toggleHidden(),
      ' ': () => this.store.togglePlaying(),
      r: () => this.store.resetTime(),
      R: () => this.store.resetTime(),
      ArrowLeft: () => this.store.stepScan(-1),
      ArrowRight: () => this.store.stepScan(1),
      s: () => this.store.slicerOpen.update((o) => !o),
      S: () => this.store.slicerOpen.update((o) => !o),
    };
    const action = this.store.ready() ? actions[e.key] : undefined;
    if (action) {
      e.preventDefault();
      action();
    }
  }

  protected drop(e: DragEvent): void {
    e.preventDefault();
    const file = e.dataTransfer?.files[0];
    if (file && this.store.ready()) void this.store.openFile(file);
  }
}
