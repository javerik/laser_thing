import { ChangeDetectionStrategy, Component, computed, input, model } from '@angular/core';
import { type FullLaserParams, deformCurve, laserRgb, profileCurve, wavelengthToRgb } from '@javerik/laserline';
import { type Choice, LlChoice, LlPlot, LlSlider, THEME } from './widgets';

type P = FullLaserParams;
/** Value of `<ll-line-controls>`. */
export type LineParamsValue = P['line'];
/** Value of `<ll-profile-controls>`. */
export type ProfileParamsValue = P['profile'];
/** Value of `<ll-color-controls>`. */
export type ColorParamsValue = P['color'];
/** Value of `<ll-motion-controls>`. */
export type MotionParamsValue = P['motion'];
/** Value of `<ll-deform-controls>`. */
export type DeformParamsValue = P['deform'];
/** Value of `<ll-noise-controls>`. */
export type NoiseParamsValue = P['noise'];
/** Value of `<ll-scan-params-controls>`. */
export type ScanParamsValue = P['scan'];

/** Immutable single-field update of a `model()` value. */
function patcher<T>(value: { update(f: (v: T) => T): void }) {
  return <K extends keyof T>(key: K, v: T[K]) => value.update((o) => ({ ...o, [key]: v }));
}

/** CSS colour of linear-ish RGB 0..1 as the shader writes it to the pixels. */
export function cssRgb(rgb: ArrayLike<number>): string {
  const c = (x: number) => Math.round(Math.min(Math.max(x, 0), 1) * 255);
  return `rgb(${c(rgb[0])}, ${c(rgb[1])}, ${c(rgb[2])})`;
}

const GRID = `.grid { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 6px 10px; align-items: center; }`;

@Component({
  selector: 'll-line-controls',
  imports: [LlSlider, LlChoice],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid">
      <span>Intensity</span>
      <ll-slider label="Intensity" [value]="v().intensity_pct" (valueChange)="set('intensity_pct', $event)" [min]="0" [max]="100" suffix="%" [decimals]="1" [clamp]="true" />
      <span>Width</span>
      <ll-slider label="Width" [value]="v().width_px" (valueChange)="set('width_px', $event)" [min]="0.2" [max]="200" [log]="true" suffix="px" title="Full width at half maximum for Gaussian" />
      <span>Length</span>
      <div class="col">
        <ll-slider label="Length" [value]="v().length" (valueChange)="set('length', $event)" [min]="0" [max]="v().length_unit === 'Pixels' ? 8000 : 150" [suffix]="v().length_unit === 'Pixels' ? 'px' : '%'" [decimals]="1" />
        <ll-choice [value]="v().length_unit" (valueChange)="setUnit($event)" [options]="units" />
      </div>
      <span>Position X</span>
      <ll-slider label="Position X" [value]="v().position_pct[0]" (valueChange)="set('position_pct', [$event, v().position_pct[1]])" [min]="0" [max]="100" suffix="%" [decimals]="1" />
      <span>Position Y</span>
      <ll-slider label="Position Y" [value]="v().position_pct[1]" (valueChange)="set('position_pct', [v().position_pct[0], $event])" [min]="0" [max]="100" suffix="%" [decimals]="1" />
      <span></span>
      <div><button type="button" (click)="set('position_pct', [50, 50])">Center</button></div>
      <span>Orientation</span>
      <ll-choice [value]="v().orientation" (valueChange)="set('orientation', $event)" [options]="orientations" />
      <span>Tilt</span>
      <ll-slider label="Tilt" [value]="v().tilt_deg" (valueChange)="set('tilt_deg', $event)" [min]="-45" [max]="45" suffix="°" [decimals]="1" />
    </div>
  `,
  styles: [THEME, GRID, `.col { display: flex; flex-direction: column; gap: 4px; }`],
})
export class LlLineControls {
  readonly value = model.required<LineParamsValue>();
  /** View extent along the line in px, to keep the visible length when switching units. */
  readonly extentAlong = input(0);
  protected readonly v = this.value;
  protected readonly set = patcher(this.value);
  protected readonly units: Choice<LineParamsValue['length_unit']>[] = [
    { value: 'Percent', label: '% of view' },
    { value: 'Pixels', label: 'pixels' },
  ];
  protected readonly orientations: Choice<LineParamsValue['orientation']>[] = [
    { value: 'Horizontal', label: '— Horizontal' },
    { value: 'Vertical', label: '| Vertical' },
  ];

  protected setUnit(unit: LineParamsValue['length_unit']): void {
    const l = this.value();
    const extent = this.extentAlong();
    if (unit === l.length_unit || extent <= 0) {
      this.set('length_unit', unit);
      return;
    }
    const length = unit === 'Pixels' ? Math.round((l.length / 100) * extent) : (l.length / extent) * 100;
    this.value.set({ ...l, length_unit: unit, length });
  }
}

@Component({
  selector: 'll-profile-controls',
  imports: [LlSlider, LlChoice, LlPlot],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <ll-choice [value]="v().kind" (valueChange)="set('kind', $event)" [options]="kinds" />
    <div class="grid">
      @if (v().kind === 'TopHat') {
        <span>Edge softness</span>
        <ll-slider label="Edge softness" [value]="v().edge_softness_px" (valueChange)="set('edge_softness_px', $event)" [min]="0" [max]="50" suffix="px" />
      }
      <span>End fade</span>
      <ll-slider label="End fade" [value]="v().end_softness_px" (valueChange)="set('end_softness_px', $event)" [min]="0" [max]="300" suffix="px" title="Fade-out length at both line ends (0 = hard ends)" />
    </div>
    <ll-plot [values]="curve()" [y0]="0" [y1]="1" caption="cross-section" />
  `,
  styles: [THEME, GRID, `:host { display: flex; flex-direction: column; gap: 6px; }`],
})
export class LlProfileControls {
  readonly value = model.required<ProfileParamsValue>();
  /** Line width in px, for the cross-section plot. */
  readonly width = input(6);
  protected readonly v = this.value;
  protected readonly set = patcher(this.value);
  protected readonly kinds: Choice<ProfileParamsValue['kind']>[] = [
    { value: 'Gaussian', label: 'Gaussian' },
    { value: 'TopHat', label: 'Top-hat' },
    { value: 'Hard', label: 'Hard edge' },
  ];
  /** Cross-section over ±1.5 × the visible width. */
  protected readonly curve = computed(() => {
    const p = this.value();
    const half = 1.5 * Math.max(this.width() + (p.kind === 'TopHat' ? p.edge_softness_px : 0), 1);
    return profileCurve(p, this.width(), -half, half, 170);
  });
}

const WAVELENGTH_PRESETS = [405, 450, 520, 635, 660];

@Component({
  selector: 'll-color-controls',
  imports: [LlSlider, LlChoice],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="row">
      <ll-choice [value]="v().mode" (valueChange)="set('mode', $event)" [options]="modes" />
      <span class="swatch" [style.background]="swatch()"></span>
    </div>
    @if (v().mode === 'Wavelength') {
      <ll-slider label="Wavelength" [value]="v().wavelength_nm" (valueChange)="set('wavelength_nm', $event)" [min]="380" [max]="780" suffix="nm" [decimals]="0" [clamp]="true" />
      <div class="row">
        @for (w of presets; track w.nm) {
          <button type="button" [class.on]="v().wavelength_nm === w.nm" [style.color]="v().wavelength_nm === w.nm ? null : w.color" (click)="set('wavelength_nm', w.nm)">{{ w.nm }}</button>
        }
      </div>
    } @else {
      <div class="row">
        <input type="color" aria-label="Laser colour" [value]="hex()" (input)="setHex($any($event.target).value)" />
        @for (c of named; track c.name) {
          <button type="button" (click)="set('custom_rgb', c.rgb)">{{ c.name }}</button>
        }
      </div>
    }
  `,
  styles: [
    THEME,
    `
      :host { display: flex; flex-direction: column; gap: 6px; }
      .swatch { display: inline-block; width: 36px; height: 18px; border-radius: var(--_radius); border: 1px solid var(--_border); }
      input[type='color'] { width: 40px; height: 22px; padding: 0; border: 1px solid var(--_border); background: none; }
    `,
  ],
})
export class LlColorControls {
  readonly value = model.required<ColorParamsValue>();
  protected readonly v = this.value;
  protected readonly set = patcher(this.value);
  protected readonly modes: Choice<ColorParamsValue['mode']>[] = [
    { value: 'Wavelength', label: 'Wavelength' },
    { value: 'Custom', label: 'Custom RGB' },
  ];
  protected readonly presets = WAVELENGTH_PRESETS.map((nm) => ({ nm, color: cssRgb(wavelengthToRgb(nm)) }));
  protected readonly named: { name: string; rgb: [number, number, number] }[] = [
    { name: 'White', rgb: [1, 1, 1] },
    { name: 'Red', rgb: [1, 0, 0] },
    { name: 'Green', rgb: [0, 1, 0] },
    { name: 'Blue', rgb: [0, 0, 1] },
  ];
  protected readonly swatch = computed(() => cssRgb(laserRgb(this.value())));
  protected readonly hex = computed(
    () => '#' + this.value().custom_rgb.map((c) => Math.round(Math.min(Math.max(c, 0), 1) * 255).toString(16).padStart(2, '0')).join(''),
  );

  protected setHex(hex: string): void {
    const n = parseInt(hex.slice(1), 16);
    this.set('custom_rgb', [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255]);
  }
}

@Component({
  selector: 'll-motion-controls',
  imports: [LlSlider, LlChoice],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <ll-choice [value]="v().kind" (valueChange)="set('kind', $event)" [options]="kinds" />
    <div class="grid">
      @switch (v().kind) {
        @case ('Sweep') {
          <span>Speed</span>
          <ll-slider label="Speed" [value]="v().speed_px_s" (valueChange)="set('speed_px_s', $event)" [min]="-3000" [max]="3000" suffix="px/s" [decimals]="1" title="Negative values reverse the direction" />
          <span>Span</span>
          <ll-slider label="Span" [value]="v().span_pct" (valueChange)="set('span_pct', $event)" [min]="0" [max]="100" suffix="%" [decimals]="1" title="Travel range, percent of the view across the line, centered on the position" />
          <span>At end</span>
          <ll-choice [value]="v().sweep_mode" (valueChange)="set('sweep_mode', $event)" [options]="sweepModes" />
        }
        @case ('Sine') {
          <span>Amplitude</span>
          <ll-slider label="Amplitude" [value]="v().amplitude_px" (valueChange)="set('amplitude_px', $event)" [min]="0" [max]="1000" suffix="px" [decimals]="1" />
          <span>Frequency</span>
          <ll-slider label="Frequency" [value]="v().frequency_hz" (valueChange)="set('frequency_hz', $event)" [min]="0.01" [max]="20" [log]="true" suffix="Hz" [decimals]="3" />
          <span>Phase</span>
          <ll-slider label="Phase" [value]="v().phase_deg" (valueChange)="set('phase_deg', $event)" [min]="-180" [max]="180" suffix="°" [decimals]="1" />
        }
      }
    </div>
    @if (v().kind !== 'Static') {
      <span class="weak">Current offset: {{ offset() >= 0 ? '+' : '' }}{{ offset().toFixed(1) }} px</span>
    }
  `,
  styles: [THEME, GRID, `:host { display: flex; flex-direction: column; gap: 6px; }`],
})
export class LlMotionControls {
  readonly value = model.required<MotionParamsValue>();
  /** Current displacement of the line in px. */
  readonly offset = input(0);
  protected readonly v = this.value;
  protected readonly set = patcher(this.value);
  protected readonly kinds: Choice<MotionParamsValue['kind']>[] = [
    { value: 'Static', label: 'Static' },
    { value: 'Sweep', label: 'Sweep' },
    { value: 'Sine', label: 'Oscillate' },
  ];
  protected readonly sweepModes: Choice<MotionParamsValue['sweep_mode']>[] = [
    { value: 'Bounce', label: 'Bounce' },
    { value: 'Wrap', label: 'Wrap' },
  ];
}

@Component({
  selector: 'll-deform-controls',
  imports: [LlSlider, LlChoice, LlPlot],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (inactive()) {
      <span class="weak">The object scan shapes the line; this deformation is inactive.</span>
    }
    <ll-choice [value]="v().kind" (valueChange)="set('kind', $event)" [options]="kinds" />
    @if (v().kind !== 'None') {
      <div class="grid">
        @if (v().kind === 'Sine') {
          <span>Amplitude</span>
          <ll-slider label="Amplitude" [value]="v().amplitude_px" (valueChange)="set('amplitude_px', $event)" [min]="0" [max]="300" suffix="px" />
          <span>Wavelength</span>
          <ll-slider label="Wavelength" [value]="v().wavelength_px" (valueChange)="set('wavelength_px', $event)" [min]="4" [max]="4000" [log]="true" suffix="px" />
        } @else {
          <span>Position</span>
          <ll-slider label="Position" [value]="v().position_px" (valueChange)="set('position_px', $event)" [min]="-2000" [max]="2000" suffix="px" title="Along the line, relative to its center" />
          <span>Height</span>
          <ll-slider label="Height" [value]="v().height_px" (valueChange)="set('height_px', $event)" [min]="-500" [max]="500" suffix="px" title="Displacement across the line; sign sets the direction" />
          @if (v().kind === 'Bump' || v().kind === 'Box') {
            <span>Width</span>
            <ll-slider label="Width" [value]="v().width_px" (valueChange)="set('width_px', $event)" [min]="1" [max]="3000" [log]="true" suffix="px" />
          }
          @if (v().kind === 'Box') {
            <span>Flank</span>
            <ll-slider label="Flank" [value]="v().slope_px" (valueChange)="set('slope_px', $event)" [min]="0" [max]="1000" suffix="px" />
          }
        }
        <span>Travel</span>
        <ll-slider label="Travel" [value]="v().scroll_px_s" (valueChange)="set('scroll_px_s', $event)" [min]="-2000" [max]="2000" suffix="px/s" title="Moves the feature along the line, like an object passing through" />
      </div>
      <ll-plot [values]="plot().values" [y0]="plot().y0" [y1]="plot().y1" caption="shape along the line" />
    }
  `,
  styles: [THEME, GRID, `:host { display: flex; flex-direction: column; gap: 6px; }`],
})
export class LlDeformControls {
  readonly value = model.required<DeformParamsValue>();
  /** Visible line length in px, the plot's range. */
  readonly lengthPx = input(500);
  /** Simulation time, for travelling features. */
  readonly time = input(0);
  /** A scan drives the line instead. */
  readonly inactive = input(false);
  protected readonly v = this.value;
  protected readonly set = patcher(this.value);
  protected readonly kinds: Choice<DeformParamsValue['kind']>[] = [
    { value: 'None', label: 'None' },
    { value: 'Step', label: 'Step' },
    { value: 'Bump', label: 'Bump' },
    { value: 'Box', label: 'Box' },
    { value: 'Sine', label: 'Wave' },
  ];
  protected readonly plot = computed(() => {
    const half = 0.5 * Math.max(this.lengthPx(), 10);
    const values = deformCurve(this.value(), this.time(), -half, half, 201);
    let lo = 0;
    let hi = 0;
    for (const v of values) {
      lo = Math.min(lo, v);
      hi = Math.max(hi, v);
    }
    const pad = Math.max((hi - lo) * 0.1, 1);
    return { values, y0: lo - pad, y1: hi + pad };
  });
}

@Component({
  selector: 'll-noise-controls',
  imports: [LlSlider],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid">
      <span>Ambient light</span>
      <ll-slider label="Ambient light" [value]="v().ambient_pct" (valueChange)="set('ambient_pct', $event)" [min]="0" [max]="50" suffix="%" />
      <span>Speckle</span>
      <ll-slider label="Speckle" [value]="v().speckle_pct" (valueChange)="set('speckle_pct', $event)" [min]="0" [max]="100" suffix="%" title="Speckle contrast on the laser line" />
      @if (v().speckle_pct > 0) {
        <span>Grain size</span>
        <ll-slider label="Grain size" [value]="v().grain_px" (valueChange)="set('grain_px', $event)" [min]="0.5" [max]="20" [log]="true" suffix="px" />
      }
      <span>Sensor noise σ</span>
      <ll-slider label="Sensor noise" [value]="v().noise_pct" (valueChange)="set('noise_pct', $event)" [min]="0" [max]="20" suffix="%" />
      <span></span>
      <label title="Off = frozen pattern"><input type="checkbox" [checked]="v().animated" (change)="set('animated', $any($event.target).checked)" /> Animated</label>
    </div>
  `,
  styles: [THEME, GRID],
})
export class LlNoiseControls {
  readonly value = model.required<NoiseParamsValue>();
  protected readonly v = this.value;
  protected readonly set = patcher(this.value);
}

/** How the scan maps onto the live line (`LiveInfo.scan`), shown by `<ll-scan-params-controls>`. */
export interface DisplayMappingValue {
  /** Screen px per mm along the line. */
  px_per_mm: number;
  /** Screen px of deflection per mm of height. */
  height_px_per_mm: number;
  /** Heights deflect the line the other way. */
  invert: boolean;
}

@Component({
  selector: 'll-scan-params-controls',
  imports: [LlSlider, LlChoice],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid">
      <span>Scale</span>
      <ll-choice [value]="v().scale" (valueChange)="set('scale', $event)" [options]="scales" />
      @if (v().scale === 'Fixed') {
        <span></span>
        <ll-slider label="Pixels per mm" [value]="v().px_per_mm" (valueChange)="set('px_per_mm', $event)" [min]="0.05" [max]="200" [log]="true" suffix="px/mm" [decimals]="3" title="Screen pixels per mm along the line; sets the line length" />
      }
      <span>Height gain</span>
      <div class="col">
        <ll-choice [value]="v().height_gain" (valueChange)="set('height_gain', $event)" [options]="gains()" />
        @if (v().height_gain === 'Manual') {
          <ll-slider label="Height gain" [value]="v().height_px_per_mm" (valueChange)="set('height_px_per_mm', $event)" [min]="0.01" [max]="200" [log]="true" suffix="px/mm" [decimals]="3" title="Line displacement per mm of object height" />
        } @else if (v().height_gain === 'FromAngle' && angleDeg() < 1) {
          <span class="warn">The scan was sliced at a triangulation angle of 0°, so the height gain is zero and the line stays flat.</span>
        }
        <label><input type="checkbox" [checked]="v().invert_height" (change)="set('invert_height', $any($event.target).checked)" /> Invert (higher = down)</label>
      </div>
      @if (live(); as m) {
        <span></span>
        <span class="weak">{{ m.px_per_mm.toFixed(3) }} px/mm ({{ (1 / Math.max(m.px_per_mm, 1e-9)).toFixed(4) }} mm/px) · height {{ m.height_px_per_mm.toFixed(3) }} px/mm</span>
      }
      <span>Rate</span>
      <ll-choice [value]="v().rate" (valueChange)="set('rate', $event)" [options]="rates" />
      @if (v().rate === 'PerSecond') {
        <span></span>
        <ll-slider label="Profiles per second" [value]="v().profiles_per_s" (valueChange)="set('profiles_per_s', $event)" [min]="0.1" [max]="1000" [log]="true" suffix="profiles/s" title="Scaled by the simulation speed" />
      }
      <span>At end</span>
      <ll-choice [value]="v().end" (valueChange)="set('end', $event)" [options]="ends" />
    </div>
  `,
  styles: [THEME, GRID, `.col { display: flex; flex-direction: column; gap: 4px; }`],
})
export class LlScanParamsControls {
  readonly value = model.required<ScanParamsValue>();
  /** Triangulation angle the scan was sliced at. */
  readonly angleDeg = input(30);
  /** Mapping used in the last frame. */
  readonly live = input<DisplayMappingValue | undefined>();
  protected readonly Math = Math;
  protected readonly v = this.value;
  protected readonly set = patcher(this.value);
  protected readonly scales: Choice<ScanParamsValue['scale']>[] = [
    { value: 'FitLine', label: 'Fit line length', title: 'Stretch the scan width over the line length' },
    { value: 'Fixed', label: 'Fixed' },
  ];
  protected readonly gains = computed<Choice<ScanParamsValue['height_gain']>[]>(() => [
    { value: 'MatchScale', label: '1:1', title: 'Same px/mm as along the line: the line shows the true-scale profile' },
    { value: 'FromAngle', label: `sin ${this.angleDeg().toFixed(0)}°`, title: 'px/mm · sin(triangulation angle of the scan): what a camera at that angle sees' },
    { value: 'Manual', label: 'Manual' },
  ]);
  protected readonly rates: Choice<ScanParamsValue['rate']>[] = [
    { value: 'PerFrame', label: '1 / frame', title: 'One profile per displayed frame, for a camera synchronised to the display' },
    { value: 'PerSecond', label: 'Fixed rate' },
  ];
  protected readonly ends: Choice<ScanParamsValue['end']>[] = [
    { value: 'Loop', label: 'Loop' },
    { value: 'Bounce', label: 'Bounce' },
    { value: 'Once', label: 'Once' },
  ];
}
