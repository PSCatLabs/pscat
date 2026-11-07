# pscat in the browser

The interpreter core compiles to WebAssembly, and `web/pscat.js` is
a dependency-free ES module that renders — and *executes* —
PostScript in a browser, including step-driven drawing into a
`<canvas>`: the live window, in a tab.

```sh
./scripts/build_wasm.sh              # → web/pscat.wasm (~7.5 MB; the base fonts and lib/)
python3 -m http.server -d web        # open http://localhost:8000
```

```js
import { Pscat } from './pscat.js';
const ps = await Pscat.load();
ps.run('0 0 300 300 rectfill showpage');   // to completion...
ps.paintTo(canvas);

ps.begin(source);                          // ...or watch it draw:
const frame = () => {
  if (ps.step(300) === 1) requestAnimationFrame(frame);
  ps.paintTo(canvas);
};
requestAnimationFrame(frame);
```

`web/index.html` is a ready-made playground (editor, speed slider,
live canvas). Errors come back REPL-style via `ps.error` with the
standard PostScript error names. The wasm build has no window, no
filesystem, and no clock (`usertime`/`realtime` read 0 — the one
documented deviation). Works under node too — `tests/wasm.rs` drives
the real module through the same JS library.

### SVG and PDF

The wasm build writes the same vector output as `pscat --svg` and
`pscat --pdf` (#1003, since 0.7.0-rc.4). Ask for it when
the program begins: recording is off by default, so a pixels-only page
pays nothing.

```js
ps.run(source, 612, 792, 1, { svg: true, pdf: true });
const pages = ps.svgCount();        // one SVG document per page
const first = ps.svg(0);            // a string; null if not recorded
const doc = ps.pdf();               // Uint8Array, every page in one PDF
```

For the same program, page size and dpi, the bytes are the ones the
command-line tool writes: `tests/wasm.rs` compares them at 72 and 144 dpi
and checks that a second run, in the same instance or a fresh one, gives
the same bytes. A `%%Title:` or `%%For:` comment becomes the PDF's title
or author, as for a file on the command line. As there, SVG and PDF are
sized in device pixels, so keep the default 72 dpi (scale 1) for a
print-size vector page. The C ABI is `ps_begin_vector(src, len, w, h,
scale, flags)` (flag 1 SVG, 2 PDF), `ps_svg_count()`, `ps_svg(page)`,
`ps_pdf()` and `ps_vector_ptr()`/`ps_vector_len()`; `src/wasm.rs` has the
contract. It began as the vector adapter that
[print-shop](https://github.com/PSCatLabs/print-shop) carried as a patch.

## The npm package

From 0.6.2 the same module is published to npm as **`pscat`**. SVG and
PDF output arrive on npm with 0.7.0; until then, install the 0.7.0-rc.4
package from its
[GitHub pre-release](https://github.com/PSCatLabs/pscat/releases/tag/v0.7.0-rc.4)
(`npm install https://github.com/PSCatLabs/pscat/releases/download/v0.7.0-rc.4/pscat-0.7.0-rc.4.tgz`)
or build it from a checkout as below. It
works in browsers (with or without a bundler) and in Node 22 or later,
and ships TypeScript types.

```sh
npm install pscat                  # from 0.6.2
```

From 0.7.0-rc.4 on, every GitHub release also carries the same package as a file,
`pscat-X.Y.Z.tgz`, listed in its `SHA256SUMS`. Release candidates are
never published to npm, so this is how to try one: install it from its
URL, for example

```sh
npm install https://github.com/PSCatLabs/pscat/releases/download/vX.Y.Z-rc.N/pscat-X.Y.Z-rc.N.tgz
```

and change the dependency to `pscat@X.Y.Z` once that is released
([RELEASING.md](../dev/RELEASING.md#testing-a-candidates-npm-package)
has the checksum and attestation checks).

```js
import { createRenderer, PostScriptError } from 'pscat';

const r = await createRenderer();          // loads pscat.wasm from the package
const page = r.render('0 0 1 setrgbcolor 72 72 288 288 rectfill',
                      { width: 432, height: 432, dpi: 144 });
// page.width, page.height, page.data (RGBA, ImageData layout), page.output
r.paintTo(canvas);                         // browsers: draw it

for (const _ of r.frames(source, { budget: 300 })) {   // watch it draw
  r.paintTo(canvas);
  await new Promise(requestAnimationFrame);
}

try { r.render('1 2 bogus'); }
catch (e) { if (e instanceof PostScriptError) console.log(e.errorName); } // 'undefined'
```

- `render(source, { width, height, dpi })` runs to completion and returns
  the page; `begin` + `step(budget)` or the `frames()` generator drive it
  step by step. Page sizes are in points (default US Letter, 612x792) at
  `dpi` (default 72, one pixel per point). `source` is a string or bytes.
- **SVG and PDF** (since 0.7.0-rc.4): add `svg: true` and/or
  `pdf: true` to the page options, then `r.svg(page)` (from 0) or
  `r.svgPages()` give the SVG pages as strings and `r.pdf()` the whole
  document as a `Uint8Array`, the bytes `pscat --svg`/`--pdf` writes (see
  [SVG and PDF](#svg-and-pdf) above).
- Errors are `PostScriptError`s with the PostScript `errorName`, the
  offending `command`, the `line`, the full `report`, what the
  program printed first, and the font `warnings` up to the error. The renderer stays usable afterwards.
- `output` is what the program printed (`=`, `==`, `print`, `stack`).
- The `lib/` libraries are embedded: `(lib/artkit.ps) run` works with no
  fetches.
- **Fonts:** all of the PostScript standard 35 are built in (Liberation,
  TeX Gyre and URW faces, as on native builds). Load any other face with
  `r.addFont(name, bytes)`, named by its font pack file stem, for example
  `r.addFont('EBGaramond-Regular', bytes)`; `/Garamond findfont` then finds
  it, exactly as with the installed pack. The font files are in the
  `fonts-*` pack archives on the
  [releases page](https://github.com/PSCatLabs/pscat/releases), each with its
  licence. A pack face you have not added stands in (Helvetica, or Courier
  for the typewriter faces) and `render()`'s result, and `r.warnings`, carry
  the same one-line note the command-line tool prints on stderr, naming the
  face, the stand-in and the `addFont` call that supplies it (#750).
- Each `createRenderer()` is its own interpreter (its own wasm instance and
  fonts). `createRenderer({ wasm })` loads the module from a URL, a
  `Response`, bytes or a compiled `WebAssembly.Module` instead. The default
  is `new URL('./pscat.wasm', import.meta.url)`; CI tests the package in
  Node, and a bundler and headless-browser test is #660 (with Vite, add
  `pscat` to `optimizeDeps.exclude`).
- Licence: `(MIT OR Apache-2.0) AND OFL-1.1 AND LPPL-1.3c`, the crate's:
  the OFL covers the embedded font data (Liberation, the URW Symbol and
  ZapfDingbats faces, and the Courier Prime outlines in
  `lib/typewriter_outlines.ps`), and LPPL-1.3c (the GUST Font License)
  the embedded TeX Gyre faces. The package carries the licence texts,
  `THIRD_PARTY_NOTICES.md`, and the Rust dependencies' notices
  (`THIRD_PARTY_LICENSES.html`).

### A plain `<script>` tag (no modules, no bundler)

The package also ships **`pscat.global.js`**, the same API as one classic
script: no `import`, no `export`, no `import.meta`. It defines one global,
`pscat = { createRenderer, PostScriptError, Renderer }`, so a page needs no
bundler and no module support, and the script can be inlined into a single
HTML file or used from a classic worker.

```html
<script src="pscat.global.js"></script>
<script>
  pscat.createRenderer().then((r) => {
    r.render('0 0 1 setrgbcolor 20 20 100 60 rectfill', { width: 160, height: 100 });
    r.paintTo(document.querySelector('canvas'));
  });
</script>
```

By default `pscat.wasm` is fetched from next to the script, so serve the two
files together (`node_modules/pscat/pscat.global.js` and `pscat.wasm`; from a CDN once a
release is on npm, name the file: `https://unpkg.com/pscat@X.Y.Z/pscat.global.js`,
not the bare package URL). Where the
script cannot tell where it was loaded from (an inline `<script>`, a
`blob:` URL, a worker), `createRenderer()` rejects and says so; pass the
module yourself: `createRenderer({ wasm: url })`, or `{ wasm: bytes }`
for a page that carries the module with it. A page opened from `file://`
cannot `fetch` its neighbours in Chrome, so it needs the bytes route too
(read the file with an `<input type="file">`, or embed it as base64).
`scripts/build_npm_global.mjs` generates the script from `web/pscat.js` and
`npm/renderer.js`, and `npm test` loads it as a classic script and compares
its pages with the ES module's. There is no inlined-wasm build: the module
is 11 MB (5.4 MB gzipped, 7.2 MB as base64 of that), more than the whole
package, so a self-contained page embeds it itself
(`createRenderer({ wasm })` takes the bytes).

The package's README ([`npm/README.md`](../../npm/README.md)) has the
full API. To build and test it from a checkout:

```sh
./scripts/build_npm.sh --pack dist-npm    # npm/ built in place, plus dist-npm/pscat-X.Y.Z.tgz
(cd npm && npm test)                       # API tests (and the classic-script build) against npm/
./scripts/test_npm_tarball.sh dist-npm/pscat-*.tgz   # install it into a fresh project, render
```

For the full low-level JavaScript API (`web/pscat.js`), see the
[JavaScript page](https://pscatlabs.github.io/pscat/javascript.html)
on the project site. To try the interpreter without building anything, open the
[playground](https://pscatlabs.github.io/pscat/playground.html).
