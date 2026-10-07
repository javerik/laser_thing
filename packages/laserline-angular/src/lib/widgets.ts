import { ChangeDetectionStrategy, Component, computed, input, model, output } from '@angular/core';

/**
 * Theme: every colour, font and radius comes from a `--ll-*` custom property with a
 * dark default, so hosts can map their own design tokens onto them.
 */
export const THEME = `
:host {
  --_bg: var(--ll-bg, #15171a);
  --_surface: var(--ll-surface, #1d2024);
  --_inset: var(--ll-inset, #0f1113);
  --_border: var(--ll-border, #31353b);
  --_text: var(--ll-text, #d9dce1);
  --_weak: var(--ll-text-weak, #8a9099);
  --_accent: var(--ll-accent, #ff5a3c);
  --_on-accent: var(--ll-on-accent, #ffffff);
  --_error: var(--ll-error, #ff6b6b);
  --_warn: var(--ll-warn, #e8b44a);
  --_visible: var(--ll-profile-visible, #ff4632);
  --_occluded: var(--ll-profile-occluded, #be46c8);
  --_radius: var(--ll-radius, 3px);
  --_font: var(--ll-font, system-ui, sans-serif);
  --_mono: var(--ll-font-mono, ui-monospace, monospace);
  font-family: var(--_font);
  font-size: var(--ll-font-size, 13px);
  color: var(--_text);
}
button {
  font: inherit;
  color: var(--_text);
  background: var(--_surface);
  border: 1px solid var(--_border);
  border-radius: var(--_radius);
  padding: 2px 8px;
  cursor: pointer;
}
button:hover:not(:disabled) { border-color: var(--_weak); }
button:disabled { opacity: 0.45; cursor: default; }
button.on { background: var(--_accent); border-color: var(--_accent); color: var(--_on-accent); }
select, input[type='number'] {
  font: inherit;
  color: var(--_text);
  background: var(--_inset);
  border: 1px solid var(--_border);
  border-radius: var(--_radius);
}
input[type='range'] { accent-color: var(--_accent); }
input[type='checkbox'] { accent-color: var(--_accent); }
.weak { color: var(--_weak); }
.error { color: var(--_error); }
.warn { color: var(--_warn); }
.row { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
.grid { display: grid; grid-template-columns: max-content 1fr; gap: 6px 10px; align-items: center; }
`;

/** Number input with a slider, optionally on a logarithmic scale. Typed values may exceed the slider range. */
@Component({
  selector: 'll-slider',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <input
      type="range"
      min="0"
      max="1000"
      [value]="position()"
      [disabled]="disabled()"
      [attr.aria-label]="label()"
      (input)="fromRange($any($event.target).valueAsNumber)"
    />
    <input
      type="number"
      [value]="shown()"
      [disabled]="disabled()"
      [attr.aria-label]="label()"
      (change)="fromNumber($any($event.target).valueAsNumber)"
    />
    <span class="suffix">{{ suffix() }}</span>
  `,
  styles: [
    THEME,
    `
      :host { display: flex; align-items: center; gap: 6px; min-width: 0; }
      input[type='range'] { flex: 1; min-width: 60px; }
      input[type='number'] { width: 70px; font-family: var(--_mono); }
      .suffix { color: var(--_weak); min-width: 2.5em; }
    `,
  ],
})
export class LlSlider {
  readonly value = model.required<number>();
  readonly min = input.required<number>();
  readonly max = input.required<number>();
  readonly log = input(false);
  readonly suffix = input('');
  readonly decimals = input(2);
  /** Clamp typed values to the slider range. */
  readonly clamp = input(false);
  readonly disabled = input(false);
  readonly label = input<string>();

  readonly position = computed(() => toPosition(this.value(), this.min(), this.max(), this.log()));
  readonly shown = computed(() => Number(this.value().toFixed(this.decimals())));

  fromRange(pos: number): void {
    const v = fromPosition(pos, this.min(), this.max(), this.log());
    this.value.set(Number(v.toPrecision(6)));
  }

  fromNumber(v: number): void {
    if (!Number.isFinite(v)) return;
    this.value.set(this.clamp() ? Math.min(Math.max(v, this.min()), this.max()) : v);
  }
}

/** Slider position 0..1000 of `v`. */
export function toPosition(v: number, min: number, max: number, log: boolean): number {
  if (log && min > 0) {
    const t = Math.log(Math.max(v, min) / min) / Math.log(max / min);
    return Math.round(Math.min(Math.max(t, 0), 1) * 1000);
  }
  return Math.round(Math.min(Math.max((v - min) / (max - min), 0), 1) * 1000);
}

/** Value at slider position `pos` (0..1000). */
export function fromPosition(pos: number, min: number, max: number, log: boolean): number {
  const t = pos / 1000;
  return log && min > 0 ? min * Math.pow(max / min, t) : min + (max - min) * t;
}

/** One option of `<ll-choice>`. */
export interface Choice<T> {
  /** Value selected by the option. */
  value: T;
  /** Button text. */
  label: string;
  /** Tooltip. */
  title?: string;
  /** Shown but not selectable. */
  disabled?: boolean;
}

/** One of several values, as a row of toggle buttons. */
@Component({
  selector: 'll-choice',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @for (o of options(); track o.label) {
      <button
        type="button"
        [class.on]="o.value === value()"
        [disabled]="o.disabled || disabled()"
        [attr.title]="o.title"
        [attr.aria-pressed]="o.value === value()"
        (click)="value.set(o.value)"
      >
        {{ o.label }}
      </button>
    }
  `,
  styles: [THEME, `:host { display: inline-flex; flex-wrap: wrap; gap: 4px; }`],
})
export class LlChoice<T> {
  readonly value = model.required<T>();
  readonly options = input.required<Choice<T>[]>();
  readonly disabled = input(false);
}

/** Collapsible section with a button restoring its defaults. */
@Component({
  selector: 'll-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <details [open]="open()">
      <summary>{{ heading() }}</summary>
      <div class="body">
        <ng-content />
        @if (resettable()) {
          <div class="reset">
            <button type="button" [title]="'Reset ' + heading() + ' to defaults'" (click)="reset.emit()">↺ Reset</button>
          </div>
        }
      </div>
    </details>
  `,
  styles: [
    THEME,
    `
      :host { display: block; border-bottom: 1px solid var(--_border); }
      summary { cursor: pointer; font-weight: 600; padding: 6px 0; user-select: none; }
      .body { display: flex; flex-direction: column; gap: 6px; padding: 0 0 8px; }
      .reset { display: flex; justify-content: flex-end; }
      .reset button { font-size: 0.9em; padding: 0 6px; }
    `,
  ],
})
export class LlSection {
  readonly heading = input.required<string>();
  readonly open = input(true);
  readonly resettable = input(true);
  readonly reset = output<void>();
}

/** Small line plot of `values` sampled evenly over x0..x1 with y range y0..y1. */
@Component({
  selector: 'll-plot',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <svg viewBox="0 0 340 56" preserveAspectRatio="none" role="img" [attr.aria-label]="caption()">
      @if (zero(); as z) {
        <line x1="0" x2="340" [attr.y1]="z" [attr.y2]="z" class="zero" />
      }
      <polyline [attr.points]="points()" class="curve" />
    </svg>
    <span class="caption">{{ caption() }}</span>
  `,
  styles: [
    THEME,
    `
      :host { display: block; position: relative; max-width: 340px; height: 56px; background: var(--_inset); border-radius: var(--_radius); }
      svg { width: 100%; height: 100%; display: block; }
      .curve { fill: none; stroke: var(--_visible); stroke-width: 1.5; vector-effect: non-scaling-stroke; }
      .zero { stroke: var(--_weak); stroke-width: 1; vector-effect: non-scaling-stroke; }
      .caption { position: absolute; left: 4px; top: 2px; font-size: 10px; color: var(--_weak); }
    `,
  ],
})
export class LlPlot {
  readonly values = input.required<ArrayLike<number>>();
  readonly y0 = input.required<number>();
  readonly y1 = input.required<number>();
  readonly caption = input('');

  private y(v: number): number {
    return 53 - ((v - this.y0()) / (this.y1() - this.y0())) * 50;
  }

  readonly points = computed(() => {
    const v = this.values();
    const n = Math.max(v.length - 1, 1);
    let out = '';
    for (let i = 0; i < v.length; i++) out += `${((i / n) * 340).toFixed(1)},${this.y(v[i]).toFixed(1)} `;
    return out;
  });

  readonly zero = computed(() => (this.y0() < 0 && this.y1() > 0 ? this.y(0) : undefined));
}

/** Hidden file input opened through `open()`. */
@Component({
  selector: 'll-file-button',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <button type="button" [disabled]="disabled()" (click)="input.click()"><ng-content /></button>
    <input #input type="file" hidden [accept]="accept()" (change)="pick(input)" />
  `,
  styles: [THEME, `:host { display: inline-block; }`],
})
export class LlFileButton {
  readonly accept = input('');
  readonly disabled = input(false);
  readonly file = output<File>();

  pick(el: HTMLInputElement): void {
    const f = el.files?.[0];
    el.value = '';
    if (f) this.file.emit(f);
  }
}
