// The npm package's API, run against the built package in npm/
// (./scripts/build_npm.sh first, then `npm test` here). Not shipped:
// package.json's `files` leaves test/ out. The packed tarball has its own
// install-and-render test, scripts/test_npm_tarball.sh; the comparison
// with the native renderer is tests/wasm.rs.

import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

import { createRenderer, PostScriptError, Renderer } from '../node.js';

const repo = new URL('../../', import.meta.url);
const font = (path) => readFile(new URL(`fonts/catalog/${path}`, repo));

function count(img, pred) {
  let n = 0;
  for (let i = 0; i < img.data.length; i += 4) if (pred(img.data[i], img.data[i + 1], img.data[i + 2])) n++;
  return n;
}
const dark = (r, g, b) => r + g + b < 200;
const inked = (r, g, b) => r + g + b < 765;

test('render returns the page as RGBA with its size', async () => {
  const r = await createRenderer();
  assert.ok(r instanceof Renderer);
  const img = r.render('1 0 0 setrgbcolor 0 0 50 50 rectfill', { width: 100, height: 80 });
  assert.equal(img.width, 100);
  assert.equal(img.height, 80);
  assert.ok(img.data instanceof Uint8ClampedArray);
  assert.equal(img.data.length, 100 * 80 * 4);
  // Row 0 is the top: the red square sits at the bottom left.
  assert.deepEqual([...img.data.slice(0, 4)], [255, 255, 255, 255]);
  const bottomLeft = (79 * 100) * 4;
  assert.deepEqual([...img.data.slice(bottomLeft, bottomLeft + 4)], [255, 0, 0, 255]);
  assert.equal(count(img, (r, g, b) => r === 255 && g === 0 && b === 0), 50 * 50);
});

test('the default page is US Letter at 72 dpi, and dpi scales it', async () => {
  const r = await createRenderer();
  const letter = r.render('');
  assert.deepEqual([letter.width, letter.height], [612, 792]);
  const hi = r.render('0 0 50 50 rectfill', { width: 100, height: 100, dpi: 144 });
  assert.deepEqual([hi.width, hi.height], [200, 200]);
  assert.equal(count(hi, dark), 100 * 100);
});

test('output captures what the program prints, fresh per run', async () => {
  const r = await createRenderer();
  assert.equal(r.render('3 4 add == (hi) print', { width: 10, height: 10 }).output, '7\nhi');
  assert.equal(r.render('', { width: 10, height: 10 }).output, '');
});

test('a PostScript error is a PostScriptError naming the error, and the renderer stays usable', async () => {
  const r = await createRenderer();
  assert.throws(() => r.render('(before) print 1 2 bogus', { width: 10, height: 10 }), (e) => {
    assert.ok(e instanceof PostScriptError);
    assert.equal(e.name, 'PostScriptError');
    assert.equal(e.errorName, 'undefined');
    assert.equal(e.command, 'bogus');
    assert.equal(e.line, 1);
    assert.equal(e.output, 'before');
    assert.match(e.report, /^%%\[ Error: undefined;/);
    assert.deepEqual(e.warnings, []);
    return true;
  });
  assert.throws(() => r.render('1 0 div'), { errorName: 'undefinedresult' });
  assert.throws(() => r.render('{ 1 2'), { errorName: 'syntaxerror' });
  assert.throws(() => r.render('pop'), { errorName: 'stackunderflow' });
  assert.equal(r.render('0 0 5 5 rectfill', { width: 10, height: 10 }).width, 10);
});

test('the stepped API ends on the same page render() draws', async () => {
  const r = await createRenderer();
  const src = '0 1 99 { dup 3 mul 0 moveto 0 100 rlineto stroke pop } for';
  const page = { width: 300, height: 100 };
  const full = r.render(src, page);
  let frames = 0;
  let drawnMidway = 0;
  for (const n of r.frames(src, { ...page, budget: 40 })) {
    assert.equal(n, frames++);
    if (n === 3) drawnMidway = count(r.image(), inked);
  }
  assert.ok(frames > 5, `only ${frames} frames`);
  assert.ok(drawnMidway > 0 && drawnMidway < count(full, inked), 'frames should show a partial page');
  assert.deepEqual(r.pixels(), full.data);

  r.begin(src, page);
  while (r.step(1000));
  assert.deepEqual(r.pixels(), full.data);
});

test('svgText: standard-font text as selectable <text>, opt-in, needs svg', async () => {
  const r = await createRenderer();
  const doc = '/Helvetica findfont 12 scalefont setfont 10 40 moveto (Hi & bye) show';
  r.render(doc, { width: 100, height: 80, svg: true, svgText: true });
  assert.match(r.svg(), /<text [^>]*font-size="12"[^>]*>Hi &amp; bye<\/text>/);
  r.render(doc, { width: 100, height: 80, svg: true });
  assert.doesNotMatch(r.svg(), /<text/);
  assert.throws(() => r.render(doc, { svgText: true }), /svgText needs svg: true/);
  assert.throws(() => r.render(doc, { svg: true, svgText: 1 }), TypeError);
});

test('svg and pdf: the run recorded as vector pages, repeatable, opt-in', async () => {
  // Byte equality with `pscat --svg`/`--pdf` is checked in tests/wasm.rs.
  const r = await createRenderer();
  const doc = '%%Title: Two pages\n0 0 1 setrgbcolor 10 10 50 30 rectfill showpage\n'
    + '/Helvetica findfont 12 scalefont setfont 10 40 moveto (two) show showpage';
  const page = { width: 100, height: 80, svg: true, pdf: true };
  const img = r.render(doc, page);
  assert.equal(img.width, 100);
  const svgs = r.svgPages();
  assert.equal(svgs.length, 2);
  for (const svg of svgs) assert.match(svg, /^<svg [^>]*width="100pt" height="80pt"/);
  assert.match(svgs[0], /fill="#0000ff"/);
  assert.equal(r.svg(1), svgs[1]);
  assert.equal(r.svg(), svgs[0]);
  assert.throws(() => r.svg(2), RangeError);
  assert.throws(() => r.svg(-1), RangeError);
  const pdf = r.pdf();
  assert.ok(pdf instanceof Uint8Array);
  const text = new TextDecoder('latin1').decode(pdf);
  assert.ok(text.startsWith('%PDF-'));
  assert.equal(text.match(/\/Type \/Page\b/g).length, 2);
  assert.match(text, /\/Title \(Two pages\)/);
  // The same program again gives the same bytes.
  r.render(doc, page);
  assert.deepEqual(r.svgPages(), svgs);
  assert.deepEqual(r.pdf(), pdf);
  // Only what was asked for is recorded.
  r.render(doc, { width: 100, height: 80, svg: true });
  assert.equal(r.svgPages().length, 2);
  assert.throws(() => r.pdf(), /pdf: true/);
  r.render(doc, { width: 100, height: 80 });
  assert.throws(() => r.svg(), /svg: true/);
  assert.throws(() => r.svgPages(), /svg: true/);
  assert.throws(() => r.render('', { svg: 'yes' }), TypeError);
});

test('svg and pdf mid-run show the document so far, then the finished one', async () => {
  const r = await createRenderer();
  const doc = '0 1 199 { dup 2 mul 0 moveto 0 100 rlineto stroke pop } for';
  const page = { width: 400, height: 100, svg: true, pdf: true };
  r.render(doc, page);
  const [full, fullPdf] = [r.svgPages(), r.pdf()];
  r.begin(doc, page);
  for (let i = 0; i < 5; i++) r.step(40);
  const partial = r.svgPages();
  const partialPdf = r.pdf();
  assert.equal(partial.length, 1);
  assert.ok(partial[0].length < full[0].length, 'mid-run SVG should be partial');
  assert.notDeepEqual(partialPdf, fullPdf);
  while (r.step(1000));
  assert.deepEqual(r.svgPages(), full);
  assert.deepEqual(r.pdf(), fullPdf);
});

test('the libraries in lib/ are embedded', async () => {
  const r = await createRenderer();
  const img = r.render('(lib/artkit.ps) run /frnd where { pop (ok) print } if', { width: 10, height: 10 });
  assert.equal(img.output, 'ok');
});

test('the standard 35 are built in; other faces come from addFont', async () => {
  const r = await createRenderer();
  const std = '/Palatino-Italic findfont /FontName get == /Symbol findfont /FontName get == /ZapfDingbats findfont /FontName get ==';
  assert.equal(
    r.render(std, { width: 10, height: 10 }).output,
    '/texgyrepagella-italic\n/StandardSymbolsPS\n/D050000L\n',
  );
  const name = '/Helvetica findfont /FontName get == /Garamond findfont /FontName get ==';
  assert.equal(r.render(name, { width: 10, height: 10 }).output, '/Helvetica\n/Helvetica\n');
  r.addFont('EBGaramond-Regular', await font('EBGaramond/EBGaramond-Regular.ttf'));
  assert.equal(r.render(name, { width: 10, height: 10 }).output, '/Helvetica\n/EBGaramond-Regular\n');
  const text = '/Garamond findfont 40 scalefont setfont 10 20 moveto (Garamond) show';
  assert.ok(count(r.render(text, { width: 300, height: 80 }), dark) > 500);
});

test('a pack face that was not added is reported in warnings, once, naming the stand-in', async () => {
  const r = await createRenderer();
  const page = { width: 10, height: 10 };
  const name = '/Bangers findfont /FontName get ==';
  const img = r.render(name, page);
  assert.equal(img.output, '/Helvetica\n');
  assert.equal(img.warnings.length, 1);
  const [note] = img.warnings;
  assert.match(note, /^pscat: warning: font \/Bangers is in the extras font pack/);
  assert.match(note, /using Helvetica/);
  assert.match(note, /addFont\('Bangers-Regular', bytes\)/);
  // A browser cannot run `pscat fonts install`: the note must not say to.
  assert.doesNotMatch(note, /fonts install/);
  assert.ok(!note.includes('\n'));
  assert.deepEqual(r.warnings, img.warnings);
  // One note per face, however often the program asks, and a face with a
  // different stand-in (the typewriter faces become Courier) gets its own.
  const again = r.render(`${name} ${name} /CourierPrime findfont pop /Bangers findfont pop`, page);
  assert.equal(again.warnings.length, 2);
  assert.match(again.warnings[1], /\/CourierPrime is in the extras font pack.*using Courier\. Load.*addFont\('CourierPrime-Regular', bytes\)/);
  // Each run starts clean, and programs that need nothing warn about nothing.
  assert.deepEqual(r.render('/Palatino-Italic findfont pop /Nonesuch findfont pop', page).warnings, []);
  assert.deepEqual(r.warnings, []);
  // The notes survive a PostScript error, like the page.
  let caught;
  assert.throws(() => r.render('/Bangers findfont pop bogus', page), (e) => {
    caught = e;
    return e instanceof PostScriptError;
  });
  assert.equal(r.warnings.length, 1);
  // The error carries them too, as a snapshot a later run does not change.
  assert.deepEqual(caught.warnings, r.warnings);
  assert.match(caught.warnings[0], /\/Bangers .*addFont\('Bangers-Regular'/);
  r.render('', page);
  assert.equal(caught.warnings.length, 1);
  // With the face added there is nothing to say, and it is the real face.
  r.addFont('Bangers-Regular', await font('Bangers/Bangers-Regular.ttf'));
  const real = r.render(name, page);
  assert.deepEqual(real.warnings, []);
  assert.equal(real.output, '/Bangers-Regular\n');
});

test('addFont: a Unicode-mode face (Thai) shows UTF-8 text by its pack name', async () => {
  // The pack stem picks the encoding, as on disk: under NotoSansThai-Regular
  // the UTF-8 string is six Thai glyphs; under another name the same face
  // reads it as eighteen bytes of StandardEncoding (mostly .notdef), and
  // without the face findfont substitutes Helvetica.
  const thai = '(\u0e2a\u0e27\u0e31\u0e2a\u0e14\u0e35) show';
  const page = { width: 300, height: 80 };
  const show = (name) => new TextEncoder().encode(`/${name} findfont 40 scalefont setfont 10 20 moveto ${thai}`);
  const r = await createRenderer();
  const without = count(r.render(show('NotoSansThai-Regular'), page), dark);
  const bytes = await font('NotoSansThai/NotoSansThai-Regular.ttf');
  r.addFont('NotoSansThai-Regular', bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
  r.addFont('ThaiUnderAnotherName', bytes);
  const unicode = count(r.render(show('NotoSansThai-Regular'), page), dark);
  const standard = count(r.render(show('ThaiUnderAnotherName'), page), dark);
  assert.ok(unicode > 1.3 * without && unicode > 1.3 * standard, `unicode ${unicode}, standard ${standard}, without ${without}`);
});

test('bad arguments throw before reaching the interpreter', async () => {
  const r = await createRenderer();
  assert.throws(() => r.render('', { width: 0 }), RangeError);
  assert.throws(() => r.render('', { width: 1.5 }), RangeError);
  assert.throws(() => r.render('', { dpi: -1 }), RangeError);
  assert.throws(() => r.render('', { width: 100000 }), RangeError);
  assert.throws(() => r.render('', { width: 2 ** 32 + 10, height: 10 }), RangeError); // would wrap to 10
  assert.throws(() => r.render(42), TypeError);
  assert.throws(() => r.addFont('', new Uint8Array(4)), TypeError);
  assert.throws(() => r.addFont('Junk', 'not bytes'), TypeError);
  assert.throws(() => r.addFont('Junk', new Uint8Array([1, 2, 3, 4])), /not a TrueType\/OpenType font/);
  assert.throws(() => r.step(), /before begin/);
  r.begin('1 1 add', { width: 10, height: 10 });
  assert.throws(() => r.step(2 ** 32), RangeError);
  assert.throws(() => r.step(0), RangeError);
});

test('createRenderer: each renderer is its own instance; wasm can be given', async () => {
  const wasm = new URL('../pscat.wasm', import.meta.url);
  const a = await createRenderer({ wasm });
  const b = await createRenderer({ wasm: await readFile(wasm) });
  const c = await createRenderer({ wasm: await WebAssembly.compile(await readFile(wasm)) });
  for (const form of [wasm.href, fileURLToPath(wasm)]) {
    assert.equal((await createRenderer({ wasm: form })).render('', { width: 3, height: 2 }).width, 3);
  }
  a.addFont('EBGaramond-Regular', await font('EBGaramond/EBGaramond-Regular.ttf'));
  const q = '/Garamond findfont /FontName get ==';
  assert.equal(a.render(q, { width: 10, height: 10 }).output, '/EBGaramond-Regular\n');
  assert.equal(b.render(q, { width: 10, height: 10 }).output, '/Helvetica\n');
  assert.equal(c.render(q, { width: 10, height: 10 }).output, '/Helvetica\n');
  await assert.rejects(createRenderer({ wasm: 12 }), TypeError);
});

test('pageCount: pages so far, mid-step and at the end, whatever is recorded (#1011)', async () => {
  const r = await createRenderer();
  const page = { width: 40, height: 40 };
  const doc = '1 1 40 40 rectfill showpage 0 0 20 20 rectfill showpage 5 5 9 9 rectfill copypage showpage';
  assert.equal(r.pageCount(), 0, 'before any begin()');
  for (const record of [{}, { svg: true }, { pdf: true }, { svg: true, pdf: true }]) {
    // Stepped one object at a time: the count never falls and ends at 4.
    r.begin(doc, { ...page, ...record });
    const seen = [];
    do seen.push(r.pageCount()); while (r.step(1));
    seen.push(r.pageCount());
    assert.deepEqual(seen, [...seen].sort((a, b) => a - b), `non-decreasing ${JSON.stringify(record)}`);
    assert.equal(seen[0], 1, 'a blank page before anything is drawn');
    assert.ok(seen.includes(2) && seen.includes(3), `saw every count: ${seen}`);
    assert.equal(seen.at(-1), 4, JSON.stringify(record));
    if (record.svg) assert.equal(r.pageCount(), r.svgPages().length);
    if (record.pdf) assert.equal((Buffer.from(r.pdf()).toString('latin1').match(/\/Type \/Page\b/g) ?? []).length, 4);
  }
});

test('pageCount: the page being drawn counts, and agrees with svgPages()', async () => {
  const r = await createRenderer();
  const page = { width: 20, height: 20, svg: true };
  for (const [src, n] of [
    ['', 1], // an empty program is a one-page document, as svgPages() says
    ['0 0 5 5 rectfill', 1], // drawn, never shown
    ['showpage', 1],
    ['0 0 5 5 rectfill showpage', 1],
    ['0 0 5 5 rectfill showpage 0 0 5 5 rectfill', 2], // art after the last showpage
    ['showpage showpage showpage', 3],
  ]) {
    r.render(src, page);
    assert.equal(r.pageCount(), n, src);
    assert.equal(r.pageCount(), r.svgPages().length, src);
  }
  // After an error the pages so far are still counted.
  assert.throws(() => r.render('showpage showpage bogus', page), PostScriptError);
  assert.equal(r.pageCount(), 2);
  assert.equal(r.pageCount(), r.svgPages().length);
});

// Node's fetch refuses file: URLs today; a future Node that reads them would
// make index.js work by path, and the error below could not happen.
const nodeFetchesFiles = await fetch(new URL('../package.json', import.meta.url)).then(() => true, () => false);

test('index.js imported by path in Node fails with a message that names the fix (#1085)', { skip: nodeFetchesFiles && "this Node's fetch reads file: URLs" }, async () => {
  const browserEntry = fileURLToPath(new URL('../index.js', import.meta.url));
  const { createRenderer: browserCreate } = await import(browserEntry);
  await assert.rejects(browserCreate(), (e) => {
    assert.match(e.message, /browser entry point/);
    assert.match(e.message, /import 'pscat'/);
    assert.match(e.message, /pscat\/node\.js/);
    assert.match(e.message, /createRenderer\(\{ wasm \}\)/);
    assert.ok(e.cause, 'the fetch error is kept as the cause');
    return true;
  });
  // The fix the message names works: bytes go through either entry.
  const bytes = await readFile(new URL('../pscat.wasm', import.meta.url));
  assert.equal((await browserCreate({ wasm: bytes })).render('', { width: 3, height: 2 }).width, 3);
  // A failed load is not cached: the next call fails the same helpful way.
  await assert.rejects(browserCreate(), /browser entry point/);
});

test("'pscat/node.js' selects the Node entry explicitly, and 'pscat' resolves to it (#1085)", async () => {
  const explicit = await import('pscat/node.js');
  const bare = await import('pscat');
  assert.equal(explicit.createRenderer, bare.createRenderer);
  assert.equal(explicit.createRenderer, (await import('../node.js')).createRenderer);
  assert.equal((await explicit.createRenderer()).render('', { width: 3, height: 2 }).width, 3);
});
