// The classic-script build, pscat.global.js (#1025), run the way a page
// runs it: parsed as a classic script (module syntax would be a
// SyntaxError), executed in the global scope, and used through the one
// global it defines. Built by ./scripts/build_npm.sh with the rest of the
// package. A real-browser check (a <script> tag, a file:// page) is manual:
// the npm guide says how.

import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import vm from 'node:vm';

import { createRenderer as createEsRenderer } from '../node.js';

const scriptUrl = new URL('../pscat.global.js', import.meta.url);
const source = await readFile(scriptUrl, 'utf8');
const wasm = await readFile(new URL('../pscat.wasm', import.meta.url));

// Run the script as a page would (a classic script, global scope), with
// `document.currentScript.src` set to `src` when given, and return the global it made.
function load(src) {
  const saved = { document: globalThis.document, pscat: globalThis.pscat };
  delete globalThis.pscat;
  if (src === undefined) delete globalThis.document;
  else globalThis.document = { currentScript: { src } };
  try {
    vm.runInThisContext(source, { filename: 'pscat.global.js' });
    return globalThis.pscat;
  } finally {
    if (saved.document === undefined) delete globalThis.document;
    else globalThis.document = saved.document;
    delete globalThis.pscat;
  }
}

test('it is a classic script: it parses with no module syntax and defines one global', () => {
  assert.doesNotThrow(() => new vm.Script(source), 'a module-syntax leftover is a SyntaxError here');
  assert.equal(source.includes('import.meta'), false);
  assert.equal(/<\/script|<!--/i.test(source), false, 'it must be safe to paste inside an HTML script element');
  assert.equal(/^\s*(import|export)\s/m.test(source), false);
  const before = new Set(Object.getOwnPropertyNames(globalThis));
  const p = load('https://example.com/lib/pscat.global.js');
  const added = Object.getOwnPropertyNames(globalThis).filter((k) => !before.has(k));
  assert.deepEqual(added.filter((k) => k !== 'pscat'), [], 'only `pscat` may leak into the global scope');
  assert.deepEqual(Object.keys(p).sort(), ['PostScriptError', 'Renderer', 'createRenderer']);
});

test('it renders what the ES module renders, byte for byte, with the wasm given as bytes', async () => {
  const p = load();
  const r = await p.createRenderer({ wasm });
  const e = await createEsRenderer();
  const src = '0.2 0.4 0.9 setrgbcolor 10 10 80 80 rectfill /Times-Roman findfont 20 scalefont setfont 0 setgray 10 92 moveto (pscat) show (ok) print';
  const page = { width: 100, height: 120, svg: true, pdf: true };
  const a = r.render(src, page);
  const b = e.render(src, page);
  assert.equal(a.output, 'ok');
  assert.deepEqual([...a.data], [...b.data]);
  assert.equal(r.svg(0), e.svg(0));
  assert.deepEqual([...r.pdf()], [...e.pdf()]);
  assert.ok(r instanceof p.Renderer);
});

test('errors are the global build\'s own PostScriptError', async () => {
  const p = load();
  const r = await p.createRenderer({ wasm });
  assert.throws(() => r.render('1 2 bogus'), (err) => err instanceof p.PostScriptError && err.errorName === 'undefined' && err.command === 'bogus');
});

test('by default pscat.wasm is fetched from next to the script', async () => {
  const p = load('https://cdn.example.com/pscat/0.7.0/pscat.global.js');
  const saved = globalThis.fetch;
  const asked = [];
  globalThis.fetch = async (url) => {
    asked.push(String(url));
    return new Response(wasm, { headers: { 'content-type': 'application/wasm' } });
  };
  try {
    const r = await p.createRenderer();
    await p.createRenderer(); // the module is fetched once and shared
    assert.deepEqual(asked, ['https://cdn.example.com/pscat/0.7.0/pscat.wasm']);
    assert.equal(r.render('0 0 5 5 rectfill', { width: 10, height: 10 }).width, 10);
  } finally {
    globalThis.fetch = saved;
  }
});

test('with no script URL (inline, blob:, a worker) the default asks for options.wasm instead of guessing', async () => {
  for (const src of [undefined, '', 'blob:https://example.com/1-2-3', 'data:text/javascript,0']) {
    const p = load(src);
    await assert.rejects(p.createRenderer(), /pass createRenderer\(\{ wasm \}\)/, `src ${JSON.stringify(src)}`);
    // ...and naming the module still works.
    assert.equal((await p.createRenderer({ wasm })).render('', { width: 4, height: 4 }).width, 4);
  }
});

test('a page opened from file:// says why the default fetch failed and what to pass instead', async () => {
  const p = load('file:///home/me/page/pscat.global.js');
  const saved = globalThis.fetch;
  globalThis.fetch = async () => { throw new TypeError('Failed to fetch'); };
  try {
    await assert.rejects(p.createRenderer(), (e) => /file: page.*Failed to fetch.*pass createRenderer\(\{ wasm \}\)/s.test(e.message) && e.cause instanceof TypeError);
    // The failure is not cached, and naming the module works.
    assert.equal((await p.createRenderer({ wasm })).render('', { width: 3, height: 3 }).width, 3);
  } finally {
    globalThis.fetch = saved;
  }
});
