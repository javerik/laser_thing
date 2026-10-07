import { TestBed } from '@angular/core/testing';
import { LlSlider, fromPosition, toPosition } from './widgets';

describe('slider scale', () => {
  it('maps linear and logarithmic ranges', () => {
    expect(toPosition(50, 0, 100, false)).toBe(500);
    expect(fromPosition(250, 0, 100, false)).toBe(25);
    // Log: 1..100, the geometric middle 10 sits halfway.
    expect(toPosition(10, 1, 100, true)).toBe(500);
    expect(fromPosition(500, 1, 100, true)).toBeCloseTo(10, 6);
    expect(toPosition(1000, 1, 100, true)).toBe(1000);
    expect(toPosition(-5, 1, 100, true)).toBe(0);
  });

  it('round-trips positions', () => {
    for (const pos of [0, 137, 500, 999, 1000]) {
      expect(toPosition(fromPosition(pos, 0.2, 200, true), 0.2, 200, true)).toBe(pos);
    }
  });
});

describe('LlSlider', () => {
  it('updates its value from the number field, clamping when asked', async () => {
    const fixture = TestBed.createComponent(LlSlider);
    fixture.componentRef.setInput('value', 5);
    fixture.componentRef.setInput('min', 0);
    fixture.componentRef.setInput('max', 10);
    fixture.componentRef.setInput('clamp', true);
    await fixture.whenStable();
    const values: number[] = [];
    fixture.componentInstance.value.subscribe((v) => values.push(v));
    const number = fixture.nativeElement.querySelector('input[type=number]') as HTMLInputElement;
    number.value = '42';
    number.dispatchEvent(new Event('change'));
    expect(values).toEqual([10]);
    const range = fixture.nativeElement.querySelector('input[type=range]') as HTMLInputElement;
    range.value = '250';
    range.dispatchEvent(new Event('input'));
    expect(values.at(-1)).toBe(2.5);
  });
});
