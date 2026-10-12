// npm run build && npm test
import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';
import { RemoteHost } from '../dist/remote.js';

const STATUS = { viewPx: [800, 600], envelope: null, field: null, constraint: null };

/** A target recording calls; `show` renders on `flush()`. */
function fakeTarget() {
  const calls = [];
  let pending = [];
  return {
    calls,
    field: null,
    constraint: null,
    status() {
      return { ...STATUS, field: this.field, constraint: this.constraint };
    },
    begin: () => calls.push('begin'),
    end: () => calls.push('end'),
    show(pattern, rendered) {
      calls.push(['show', pattern]);
      pending.push(rendered);
    },
    setField(polygon, label) {
      this.field = polygon;
      calls.push(['field', label]);
    },
    constrain(polygon, label) {
      this.constraint = polygon;
      calls.push(['constrain', label]);
      return polygon ? ['length'] : [];
    },
    fitToField(margin) {
      calls.push(['fit', margin]);
      return this.field ? true : undefined;
    },
    flush() {
      pending.forEach((f) => f());
      pending = [];
    },
  };
}

const open = [];
afterEach(() => open.splice(0).forEach((c) => c.close()));

/** A controller end of the channel collecting what the hosts send. */
function controller(name) {
  const ch = new BroadcastChannel(name);
  const got = [];
  ch.onmessage = (e) => got.push(e.data);
  open.push(ch);
  return {
    got,
    send: (msg) => ch.postMessage({ v: 1, ...msg }),
    next: async (type) => {
      for (let i = 0; i < 200; i++) {
        const k = got.findIndex((m) => m.type === type);
        if (k >= 0) return got.splice(k, 1)[0];
        await new Promise((r) => setTimeout(r, 5));
      }
      throw new Error(`no ${type} message`);
    },
  };
}

function host(target, options) {
  const h = new RemoteHost(target, options);
  open.push(h);
  return h;
}

test('hello, a session with patterns, and end', async () => {
  const name = 'remote-test-1';
  const target = fakeTarget();
  const h = host(target, { channel: name });
  const c = controller(name);

  c.send({ type: 'hello' });
  const status = await c.next('status');
  assert.equal(status.host, h.id);
  assert.deepEqual(status.status, { ...STATUS, session: null });

  c.send({ type: 'begin', to: h.id, session: 's1' });
  assert.equal((await c.next('status')).status.session, 's1');
  assert.deepEqual(target.calls, ['begin']);

  c.send({ type: 'show', to: h.id, session: 's1', id: 7, pattern: { kind: 'blank' } });
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(c.got.filter((m) => m.type === 'shown').length, 0, 'not before it rendered');
  target.flush();
  const shown = await c.next('shown');
  assert.deepEqual([shown.session, shown.id, shown.viewPx], ['s1', 7, [800, 600]]);

  c.send({ type: 'end', to: h.id, session: 's1' });
  assert.equal((await c.next('status')).status.session, null);
  assert.equal(target.calls.at(-1), 'end');
});

test('a second session is refused, and messages for other hosts are ignored', async () => {
  const name = 'remote-test-2';
  const target = fakeTarget();
  const h = host(target, { channel: name });
  const c = controller(name);
  c.send({ type: 'begin', to: h.id, session: 'a' });
  await c.next('status');
  c.send({ type: 'begin', to: h.id, session: 'b' });
  assert.match((await c.next('error')).message, /another calibration/);
  c.send({ type: 'show', to: h.id, session: 'b', id: 1, pattern: { kind: 'blank' } });
  assert.match((await c.next('error')).message, /no such session/);
  c.send({ type: 'end', to: 'someone-else', session: 'a' });
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(h.activeSession, 'a');
});

test('an idle session ends by itself', async () => {
  const name = 'remote-test-3';
  const target = fakeTarget();
  const h = host(target, { channel: name, idleMs: 30 });
  const c = controller(name);
  c.send({ type: 'begin', to: h.id, session: 's' });
  await c.next('status');
  await new Promise((r) => setTimeout(r, 120));
  assert.equal(h.activeSession, null);
  assert.equal(target.calls.at(-1), 'end');
});

test('camera field and fitting', async () => {
  const name = 'remote-test-4';
  const target = fakeTarget();
  const h = host(target, { channel: name });
  const c = controller(name);
  c.send({ type: 'fit-to-field', to: h.id, id: 1, margin: 4 });
  assert.match((await c.next('error')).message, /no camera field/);
  const field = [[0, 0], [10, 0], [10, 10]];
  c.send({ type: 'camera-field', to: h.id, polygon: field, label: 'cam 0' });
  assert.deepEqual((await c.next('status')).status.field, field);
  c.send({ type: 'fit-to-field', to: h.id, id: 2, margin: 4 });
  assert.deepEqual(await c.next('fitted'), { v: 1, type: 'fitted', host: h.id, id: 2, fits: true });
});

test('constraining the line and releasing it', async () => {
  const name = 'remote-test-5';
  const target = fakeTarget();
  const h = host(target, { channel: name });
  const c = controller(name);
  const area = [[0, 0], [10, 0], [10, 10]];
  c.send({ type: 'constrain', to: h.id, id: 7, polygon: area, label: 'RACS' });
  assert.deepEqual(await c.next('constrained'), { v: 1, type: 'constrained', host: h.id, id: 7, clamped: ['length'] });
  assert.deepEqual((await c.next('status')).status.constraint, area);
  // An empty polygon releases it like null.
  c.send({ type: 'constrain', to: h.id, id: 8, polygon: [] });
  assert.deepEqual((await c.next('constrained')).clamped, []);
  assert.equal((await c.next('status')).status.constraint, null);
  assert.deepEqual(target.calls, [['constrain', 'RACS'], ['constrain', null]]);
});
