// npm run build && npm test
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AnimationLoop } from '../dist/loop.js';

/** A window stand-in whose animation frames run when `fire(time)` is called. */
function fakeWindow() {
  let next = 1;
  const pending = new Map();
  return {
    pending,
    requestAnimationFrame(cb) {
      pending.set(next, cb);
      return next++;
    },
    cancelAnimationFrame(handle) {
      pending.delete(handle);
    },
    fire(time) {
      const cbs = [...pending.values()];
      pending.clear();
      cbs.forEach((cb) => cb(time));
    },
  };
}

test('frames follow the window set with setWindow', () => {
  const a = fakeWindow();
  const b = fakeWindow();
  const dts = [];
  const loop = new AnimationLoop((dt) => dts.push(dt), a);
  loop.start();
  a.fire(1000);
  a.fire(1016);
  assert.deepEqual(dts, [0, 0.016]);

  loop.setWindow(b);
  assert.ok(loop.running);
  assert.equal(a.pending.size, 0, 'no frames requested from the old window');
  a.fire(1032);
  // Other time origin: the first frame from `b` only sets the reference.
  b.fire(50);
  b.fire(60);
  assert.deepEqual(dts, [0, 0.016, 0, 0.01]);

  loop.stop();
  assert.equal(b.pending.size, 0);
  loop.setWindow(a);
  assert.ok(!loop.running, 'a stopped loop stays stopped');
});

test('long gaps are clamped', () => {
  const a = fakeWindow();
  const dts = [];
  const loop = new AnimationLoop((dt) => dts.push(dt), a);
  loop.start();
  a.fire(0);
  a.fire(10_000);
  assert.deepEqual(dts, [0, 0.25]);
});
