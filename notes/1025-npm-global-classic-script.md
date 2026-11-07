## npm: a classic-script build, `pscat.global.js` (issue #1025, 2026-11-07)

A page with no bundler (a single HTML file someone opens from disk, a
classic worker) could not use the package: `index.js` -> `renderer.js` ->
`core.js` are ES modules that find `pscat.wasm` through `import.meta.url`.
Two downstream projects had each hand-written a loader that rewrote the
imports to Blob URLs.

- `scripts/build_npm_global.mjs` makes `npm/pscat.global.js` from the same
  sources (`web/pscat.js`, `npm/renderer.js`): strip the `export`s and the
  one `import`, replace the one `import.meta.url`, wrap in an IIFE, and
  expose `globalThis.pscat = { createRenderer, PostScriptError, Renderer }`.
  Every assumption fails the build loudly (an export it does not know about,
  a missing anchor), and the output is parsed with `vm.Script`, which is a
  classic-script parse, so module syntax anywhere is a SyntaxError.
  `build_npm.sh` runs it; the file is gitignored like `core.js`, listed in
  the package `files`, in the exports map (`pscat/pscat.global.js`), as
  `unpkg` / `jsdelivr`, and in the tarball's expected file set.
- Default wasm location: `pscat.wasm` next to the script, from
  `document.currentScript.src` read at the script's top level (it is null by
  the time an async function runs). When there is no usable script URL
  (inline script, `blob:`/`data:`, a worker, where `self.location` is the
  worker's own file), `createRenderer()` rejects with a message naming
  `createRenderer({ wasm })` rather than fetching a guess.
- From `file://` Chrome loads the script but refuses `fetch('file:///.../pscat.wasm')`
  with a bare "Failed to fetch", so the default load rethrows that with the
  `createRenderer({ wasm })` hint (the cause kept). A single HTML file with the
  script and the wasm as base64 inlined renders from `file://` (checked in
  headless Chrome). The first inline attempt failed on a literal end-script tag in
  the banner comment, so the generator now refuses `</script` and `<!--` anywhere
  in the output.
- Tests: `npm/test/global.test.mjs` (classic parse, one global and nothing
  leaked, pages/SVG/PDF byte-equal to the ES module's, the default URL
  resolved against the script URL, the no-URL error) and a check in
  `scripts/test_npm_tarball.sh` that the packed file loads and renders. By
  hand, in Chromium via Playwright: a `<script src>` page rendering to a
  canvas, a classic worker with `importScripts`, and the no-URL error in the
  worker.

**Not done: an inlined-wasm variant.** The wasm is 11 MB, 5.36 MB gzipped,
7.15 MB as base64 of the gzip. That alone exceeds the package's whole
6 MB tarball budget, so shipping it in the package would mean raising the
budget for a file most users do not want. A page that wants one file embeds
the bytes itself (`createRenderer({ wasm: bytes })`). Deferred: a
`pscat.global.js` release asset (needs `release.yml`, outside this change),
a `decodeInlineWasm(base64gz)` helper, an `index.d.ts` declaration for the
global.
