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

## The npm package

From 0.6.2 the same module is published to npm as **`pscat`**
(not on npm yet; until then, build it from a checkout as below). It
works in browsers (with or without a bundler) and in Node 22 or later,
and ships TypeScript types.

```sh
npm install pscat                  # from 0.6.2
```

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
- Errors are `PostScriptError`s with the PostScript `errorName`, the
  offending `command`, the `line`, the full `report`, and what the
  program printed first. The renderer stays usable afterwards.
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
  licence.
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

The package's README ([`npm/README.md`](../../npm/README.md)) has the
full API. To build and test it from a checkout:

```sh
./scripts/build_npm.sh --pack dist-npm    # npm/ built in place, plus dist-npm/pscat-X.Y.Z.tgz
(cd npm && npm test)                       # API tests against npm/
./scripts/test_npm_tarball.sh dist-npm/pscat-*.tgz   # install it into a fresh project, render
```

For the full low-level JavaScript API (`web/pscat.js`), see the
[JavaScript page](https://pscatlabs.github.io/pscat/javascript.html)
on the project site. To try the interpreter without building anything, open the
[playground](https://pscatlabs.github.io/pscat/playground.html).
