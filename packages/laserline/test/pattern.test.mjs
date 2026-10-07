// npm run build && npm test
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { drawOverlayPattern, grayCode, isOverlayPattern, stripeLit } from '../dist/pattern.js';

test('Gray-code stripes decode back to the column', () => {
  const bits = 11;
  for (const x of [0, 1, 2, 3, 100, 1023, 1919]) {
    // What a camera pixel reads from the stripes, highest bit first, then Gray → binary.
    let gray = 0;
    for (let b = bits - 1; b >= 0; b--) gray |= (stripeLit(x, b) ? 1 : 0) << b;
    let value = 0;
    for (let g = gray; g; g >>>= 1) value ^= g;
    assert.equal(value, x);
  }
  assert.equal(grayCode(5), 7);
  assert.equal(stripeLit(5, 0), true);
  assert.equal(stripeLit(5, 0, true), false);
});

test('overlay patterns are told apart and drawn as runs', () => {
  assert.equal(isOverlayPattern({ kind: 'white' }), true);
  assert.equal(isOverlayPattern({ kind: 'stripes', orientation: 'Vertical', bit: 3 }), true);
  assert.equal(isOverlayPattern({ kind: 'blank' }), false);
  const rects = [];
  const ctx = { fillStyle: '', fillRect: (x, y, w, h) => rects.push([ctx.fillStyle, x, y, w, h]) };
  drawOverlayPattern(ctx, { kind: 'stripes', orientation: 'Vertical', bit: 2 }, 16, 4);
  // Gray bit 2 of 0..15 is set for 4..11.
  assert.deepEqual(rects, [['#000', 0, 0, 16, 4], ['#fff', 4, 0, 8, 4]]);
});
