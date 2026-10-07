import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import type { Scan } from '@javerik/laserline';
import {
  LlColorControls,
  LlDeformControls,
  LlLineControls,
  LlMotionControls,
  LlNoiseControls,
  LlProfileControls,
  LlScanParamsControls,
} from './params';
import { LaserlineStore } from './store';
import { LlProfilePlot } from './views';
import { LlFileButton, LlSection, LlSlider, THEME } from './widgets';

/** Play/pause, reset and simulation speed. */
@Component({
  selector: 'll-transport',
  imports: [LlSlider],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="row">
      <button type="button" class="play" title="Space" (click)="store.togglePlaying()">{{ store.playing() ? '⏸ Pause' : '▶ Play' }}</button>
      <button type="button" title="R (also rewinds the object scan)" (click)="store.resetTime()">⟲ Reset time</button>
      <span class="mono">t = {{ store.simTime().toFixed(2) }} s</span>
    </div>
    <div class="row">
      <span>Speed</span>
      <ll-slider label="Speed" [value]="store.params().time_scale" (valueChange)="store.patchParams({ time_scale: $event })" [min]="0.05" [max]="10" [log]="true" suffix="×" />
      <button type="button" (click)="store.patchParams({ time_scale: 1 })">1×</button>
    </div>
  `,
  styles: [
    THEME,
    `
      :host { display: flex; flex-direction: column; gap: 6px; }
      .row { flex-wrap: nowrap; }
      ll-slider { flex: 1; }
      .play { min-width: 80px; }
      .mono { font-family: var(--_mono); }
    `,
  ],
})
export class LlTransport {
  protected readonly store = inject(LaserlineStore);
}

/** Built-in presets, preset files and resetting everything. */
@Component({
  selector: 'll-presets',
  imports: [LlFileButton],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="row">
      <select aria-label="Built-in preset" [value]="index()" (change)="index.set(+$any($event.target).value)">
        @for (p of store.presets(); track p.name) {
          <option [value]="$index">{{ p.name }}</option>
        }
      </select>
      <button type="button" (click)="store.applyPreset(index())">Apply</button>
    </div>
    <div class="row">
      <button type="button" (click)="store.savePreset()">💾 Save…</button>
      <ll-file-button accept=".json,application/json" (file)="store.loadPreset($event)">📂 Load…</ll-file-button>
      <button type="button" (click)="store.resetParams()">Reset all</button>
    </div>
    @if (store.status(); as s) {
      <span class="weak">{{ s }}</span>
    }
  `,
  styles: [THEME, `:host { display: flex; flex-direction: column; gap: 6px; } select { max-width: 220px; }`],
})
export class LlPresets {
  protected readonly store = inject(LaserlineStore);
  protected readonly index = signal(0);
}

/** View size, line center and length, frame rate. */
@Component({
  selector: 'll-readout',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid">
      <span>View</span><span>{{ l().view_px[0].toFixed(0) }} × {{ l().view_px[1].toFixed(0) }} px</span>
      <span>Center</span><span>{{ l().center_px[0].toFixed(1) }}, {{ l().center_px[1].toFixed(1) }} px</span>
      <span>Length</span><span>{{ l().length_px.toFixed(0) }} px</span>
      <span>Frame rate</span><span>{{ store.fps().toFixed(0) }} fps</span>
      @if (store.backend(); as b) {
        <span>Graphics</span><span>{{ b }}</span>
      }
    </div>
  `,
  styles: [THEME, `.grid > span:nth-child(even) { font-family: var(--_mono); }`],
})
export class LlReadout {
  protected readonly store = inject(LaserlineStore);
  protected readonly l = this.store.live;
}

/** Name, size and resolution of a scan. */
@Component({
  selector: 'll-scan-summary',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <strong>{{ meta().source || 'Scan' }}</strong>
    <span class="weak">
      {{ scan().rows }} profiles × {{ scan().cols }} samples · {{ scan().widthMm.toFixed(1) }} × {{ scan().lengthMm.toFixed(1) }} mm · max height
      {{ scan().zMaxMm.toFixed(2) }} mm
    </span>
  `,
  styles: [THEME, `:host { display: flex; flex-direction: column; gap: 2px; }`],
})
export class LlScanSummary {
  readonly scan = input.required<Scan>();
  protected readonly meta = computed(() => this.scan().meta());
}

/** Profile stepping: previous / next buttons and a slider over all profiles. */
@Component({
  selector: 'll-profile-stepper',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <button type="button" title="Previous profile (←)" (click)="store.stepScan(-1)">◀</button>
    <button type="button" title="Next profile (→)" (click)="store.stepScan(1)">▶</button>
    <input type="range" aria-label="Profile" min="0" [max]="rows() - 1" [value]="store.row()" (input)="store.seekRow($any($event.target).valueAsNumber)" />
    <span class="mono">profile {{ store.row() }}</span>
    <ng-content />
  `,
  styles: [
    THEME,
    `
      :host { display: flex; align-items: center; gap: 6px; }
      input { flex: 1; min-width: 80px; }
      .mono { font-family: var(--_mono); white-space: nowrap; }
    `,
  ],
})
export class LlProfileStepper {
  protected readonly store = inject(LaserlineStore);
  readonly rows = input.required<number>();
}

/** STL import, scan files and how the scan drives the line. */
@Component({
  selector: 'll-object-scan',
  imports: [LlFileButton, LlScanParamsControls, LlScanSummary, LlProfileStepper, LlProfilePlot],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="row">
      <ll-file-button accept=".stl" [disabled]="!store.canSlice" (file)="store.importStl($event)">📂 Import STL…</ll-file-button>
      <button type="button" [class.on]="store.slicerOpen()" title="Slice settings and 3D preview" (click)="store.slicerOpen.set(!store.slicerOpen())">🔧 Slicer</button>
      <ll-file-button accept=".lscan" (file)="store.loadScan($event)">Load scan…</ll-file-button>
      <button type="button" [disabled]="!store.scan()" (click)="store.saveScan()">Save scan…</button>
    </div>
    @if (store.slicer().busy) {
      <progress [value]="store.slicer().progress ?? 0" max="1"></progress>
    }
    @if (store.scan(); as scan) {
      <ll-scan-summary [scan]="scan" />
      <label>
        <input type="checkbox" [checked]="store.params().scan.enabled" (change)="setEnabled($any($event.target).checked)" />
        Drive the line from the scan
      </label>
      <ll-scan-params-controls
        [value]="store.params().scan"
        (valueChange)="store.patchParams({ scan: $event })"
        [angleDeg]="scan.meta().settings.triangulation_deg ?? 30"
        [live]="store.live().scan ?? undefined"
      />
      <ll-profile-stepper [rows]="scan.rows" />
      <ll-profile-plot [scan]="scan" [row]="store.row()" style="height: 56px" />
    } @else {
      <span class="weak">No scan loaded. Import an STL model or load a .lscan file.</span>
    }
  `,
  styles: [THEME, `:host { display: flex; flex-direction: column; gap: 6px; } progress { width: 100%; accent-color: var(--_accent); }`],
})
export class LlObjectScan {
  protected readonly store = inject(LaserlineStore);

  protected setEnabled(enabled: boolean): void {
    this.store.patchParams({ scan: { ...this.store.params().scan, enabled } });
  }
}

/**
 * The complete control panel: transport, all line parameters, object scan, presets
 * and readouts. Renders once the store is loaded.
 */
@Component({
  selector: 'll-controls',
  imports: [
    LlTransport,
    LlSection,
    LlLineControls,
    LlProfileControls,
    LlColorControls,
    LlMotionControls,
    LlDeformControls,
    LlNoiseControls,
    LlObjectScan,
    LlPresets,
    LlReadout,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (store.loadError(); as e) {
      <p class="error">{{ e }}</p>
    } @else if (store.ready()) {
      <ll-transport />
      @let p = store.params();
      @let live = store.live();
      <ll-section heading="Line" (reset)="reset('line')">
        <ll-line-controls [value]="p.line" (valueChange)="store.patchParams({ line: $event })" [extentAlong]="live.extent_along_px" />
      </ll-section>
      <ll-section heading="Profile" (reset)="reset('profile')">
        <ll-profile-controls [value]="p.profile" (valueChange)="store.patchParams({ profile: $event })" [width]="p.line.width_px" />
      </ll-section>
      <ll-section heading="Color" [open]="false" (reset)="reset('color')">
        <ll-color-controls [value]="p.color" (valueChange)="store.patchParams({ color: $event })" />
      </ll-section>
      <ll-section heading="Movement" (reset)="reset('motion')">
        <ll-motion-controls [value]="p.motion" (valueChange)="store.patchParams({ motion: $event })" [offset]="live.offset_px" />
      </ll-section>
      <ll-section heading="Deformation" [open]="false" (reset)="reset('deform')">
        <ll-deform-controls
          [value]="p.deform"
          (valueChange)="store.patchParams({ deform: $event })"
          [lengthPx]="live.length_px"
          [time]="store.simTime()"
          [inactive]="p.scan.enabled && !!store.scan()"
        />
      </ll-section>
      <ll-section heading="Background & noise" [open]="false" (reset)="reset('noise')">
        <ll-noise-controls [value]="p.noise" (valueChange)="store.patchParams({ noise: $event })" />
      </ll-section>
      <ll-section heading="Object scan" (reset)="reset('scan')">
        <ll-object-scan />
      </ll-section>
      <ll-section heading="Presets" [resettable]="false">
        <ll-presets />
      </ll-section>
      <ll-readout />
    } @else {
      <p class="weak">Loading…</p>
    }
  `,
  styles: [THEME, `:host { display: flex; flex-direction: column; gap: 6px; } ll-readout { padding-top: 6px; }`],
})
export class LlControls {
  protected readonly store = inject(LaserlineStore);

  /** Restores the defaults of one parameter section. */
  protected reset(key: 'line' | 'profile' | 'color' | 'motion' | 'deform' | 'noise' | 'scan'): void {
    this.store.patchParams({ [key]: this.store.defaults()[key] });
  }
}
