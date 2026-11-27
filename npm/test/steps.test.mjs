// The step cap (#1223): `maxSteps` on a program, `frameMaxSteps` on frames(),
// and the `stepsUsed` readout. The wasm module has no clock, so a step cap is
// the only limit an npm consumer has, and it is the only thing that stops one
// operator draining a decompression bomb (decoded filter bytes count a step
// per KiB, #1208). The same checks as tests/wasm.rs, through the Renderer.

import assert from 'node:assert/strict';
import { Readable } from 'node:stream';
import { test } from 'node:test';
import zlib from 'node:zlib';

import { createRenderer, PostScriptError } from '../node.js';

const page = { width: 20, height: 20 };

// Zeros, `mib` mebibytes of them, deflated `layers` times, streamed so the
// expanded size never sits in memory. Returns the compressed bytes.
async function bombBytes(mib, layers) {
  let r = Readable.from((async function* () {
    const z = Buffer.alloc(1 << 20);
    for (let i = 0; i < mib; i++) yield z;
  })());
  for (let i = 0; i < layers; i++) r = r.pipe(zlib.createDeflate({ level: i === 0 ? 1 : 9 }));
  const parts = [];
  for await (const c of r) parts.push(c);
  return Buffer.concat(parts);
}

// currentfile's rest is the bomb: two filters, then one operator (flushfile)
// drains them both, so no step boundary falls inside the drain.
async function bombProgram(mib = 32) {
  const data = await bombBytes(mib, 2);
  assert.ok(data.length < 2000, `the bomb is small: ${data.length} bytes`);
  return new Uint8Array(Buffer.concat([
    Buffer.from('currentfile /FlateDecode filter /FlateDecode filter flushfile\n'),
    data,
    Buffer.from('\n'),
  ]));
}

// Three million passes: an endless loop for a 50,000-step cap, yet finite, so a
// run with the cap missing ends in a fraction of a second instead of hanging.
const ENDLESS = '0 1 3000000 { pop } for';
// 100,000 passes of a loop: well over a 1000-step cap, finishes uncapped.
const FINITE = '0 1 100000 { pop } for';

function limitStop(e) {
  assert.ok(e instanceof PostScriptError, String(e));
  assert.equal(e.errorName, 'limitexceeded');
  assert.match(e.report, /limitexceeded \(steps\)/);
  return true;
}

test('maxSteps stops a decompression bomb inside one operator, with limitexceeded (steps)', async () => {
  const r = await createRenderer();
  const bomb = await bombProgram();
  assert.throws(() => r.render(bomb, { ...page, maxSteps: 1000 }), (e) => limitStop(e) && e.command === 'flushfile');
  // Stopped within a few KiB of the cap, not after the 32 MiB (32,768 steps).
  assert.ok(r.stepsUsed >= 1000 && r.stepsUsed <= 1400, `stepsUsed ${r.stepsUsed}`);
  // The renderer survives the limit and the next program runs clean.
  assert.equal(r.render('0 0 10 10 rectfill', page).width, 20);
});

test('maxSteps stops a runaway loop through render(), begin()+step() and frames()', async () => {
  const r = await createRenderer();
  assert.throws(() => r.render(ENDLESS, { ...page, maxSteps: 50_000 }), limitStop);
  assert.ok(r.stepsUsed >= 50_000, `stepsUsed ${r.stepsUsed}`);

  r.begin(ENDLESS, { ...page, maxSteps: 50_000 });
  assert.throws(() => { while (r.step(1000)); }, limitStop);

  assert.throws(() => { for (const frame of r.frames(ENDLESS, { ...page, budget: 1000, maxSteps: 50_000 })) void frame; }, limitStop);
  assert.ok(r.stepsUsed >= 50_000, `stepsUsed ${r.stepsUsed}`);
});

test('a roomy cap lets the program finish and counts what it ran', async () => {
  const r = await createRenderer();
  const bomb = await bombProgram();
  // 32 MiB is 32,768 KiB steps.
  r.render(bomb, { ...page, maxSteps: 1_000_000 });
  assert.ok(r.stepsUsed >= 32_768 && r.stepsUsed <= 33_500, `stepsUsed ${r.stepsUsed}`);
  assert.equal(r.render('0 0 1 1 rectfill', { ...page, maxSteps: 1_000_000 }).width, 20);
});

test('the cap is off by default and one render never leaks into the next', async () => {
  const r = await createRenderer();
  assert.equal(r.stepsUsed, 0, 'nothing counted before any program');
  r.render(FINITE, page);
  assert.equal(r.stepsUsed, 0, 'nothing counted with no cap');
  assert.throws(() => r.render(FINITE, { ...page, maxSteps: 1000 }), limitStop);
  assert.ok(r.stepsUsed >= 1000);
  // No option now: the 1000-step cap of the previous render is gone.
  r.render(FINITE, page);
  assert.equal(r.stepsUsed, 0);
  // 0 and null mean no limit, as `pscat --max-steps 0` does.
  for (const off of [0, null, undefined]) {
    r.render(FINITE, { ...page, maxSteps: off });
    assert.equal(r.stepsUsed, 0, `maxSteps: ${off}`);
  }
  // begin() with no option lifts a cap an earlier begin() set.
  r.begin(FINITE, { ...page, maxSteps: 1000 });
  r.begin(FINITE, page);
  while (r.step(1 << 16));
  assert.equal(r.stepsUsed, 0);
});

test('stepsUsed stays readable after an ordinary PostScript error, counted under a cap', async () => {
  const r = await createRenderer();
  assert.throws(() => r.render('1 2 add pop bogus', { ...page, maxSteps: 1000 }), { errorName: 'undefined' });
  assert.ok(r.stepsUsed > 0 && r.stepsUsed < 1000, `stepsUsed ${r.stepsUsed}`);
  assert.throws(() => r.render('1 2 add pop bogus', page), { errorName: 'undefined' });
  assert.equal(r.stepsUsed, 0, 'uncounted without a cap');
  // 0 before begin() on a fresh renderer, and a step() before begin() fails without touching it.
  const fresh = await createRenderer();
  assert.throws(() => fresh.step(), /before begin/);
  assert.equal(fresh.stepsUsed, 0);
});

test('frameMaxSteps caps each frame, not the program', async () => {
  const r = await createRenderer();
  const bomb = await bombProgram();
  // The bomb is one operator inside one frame: the frame cap stops it.
  assert.throws(() => { for (const frame of r.frames(bomb, { ...page, frameMaxSteps: 1000 })) void frame; }, limitStop);
  assert.ok(r.stepsUsed >= 1000 && r.stepsUsed <= 1400, `stepsUsed ${r.stepsUsed}`);

  // A program of 100,000 loop passes in frames of 1000: far over the cap in all,
  // far under it in any one frame, so it finishes (a whole-program cap would not).
  let frames = 0;
  for (const n of r.frames(FINITE, { ...page, budget: 1000, frameMaxSteps: 5000 })) frames = n + 1;
  assert.ok(frames > 20, `${frames} frames`);
  // stepsUsed is the last frame's, since each frame re-arms the count.
  assert.ok(r.stepsUsed > 0 && r.stepsUsed <= 5000, `stepsUsed ${r.stepsUsed}`);
  assert.throws(() => r.render(FINITE, { ...page, maxSteps: 5000 }), limitStop);

  // The frame cap does not outlive frames(): the next render is uncapped.
  r.render(FINITE, page);
  assert.equal(r.stepsUsed, 0);
});

test('a bad maxSteps is refused before the interpreter or the previous program is touched', async () => {
  const r = await createRenderer();
  const bad = [-1, 1.5, NaN, Infinity, -Infinity, 2 ** 53, Number.MAX_VALUE];
  for (const maxSteps of bad) {
    assert.throws(() => r.render('', { ...page, maxSteps }), RangeError, `maxSteps ${maxSteps}`);
    assert.throws(() => r.begin('', { ...page, maxSteps }), RangeError, `begin maxSteps ${maxSteps}`);
    assert.throws(() => r.frames('', { ...page, frameMaxSteps: maxSteps }).next(), RangeError, `frameMaxSteps ${maxSteps}`);
  }
  for (const maxSteps of ['100', true, 10n, {}, []]) {
    assert.throws(() => r.render('', { ...page, maxSteps }), TypeError, `maxSteps ${String(maxSteps)}`);
    assert.throws(() => r.frames('', { ...page, frameMaxSteps: maxSteps }).next(), TypeError);
  }
  // A program in progress is untouched by a refused begin().
  r.begin('1 1 add 2 3 add == (kept) print', page);
  assert.throws(() => r.begin('', { ...page, maxSteps: -1 }), RangeError);
  while (r.step(1000));
  assert.equal(r.output, '5\nkept');
  // The largest exact cap is accepted.
  assert.equal(r.render('', { ...page, maxSteps: Number.MAX_SAFE_INTEGER }).width, 20);
  // A frame cap replaces the program cap, so asking for both is refused.
  assert.throws(() => r.frames('', { ...page, maxSteps: 10, frameMaxSteps: 10 }).next(), TypeError);
});
