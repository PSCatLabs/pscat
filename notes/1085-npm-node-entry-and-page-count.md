## npm: a clear error for index.js in Node, `pscat/node.js`, and `Renderer.pageCount()` (issues #1085 and #1011, 2026-11-11)

Two small npm-package issues, one PR.

### #1085: importing `index.js` by path in Node

`import 'pscat'` works because the `exports` map sends Node to `node.js`.
A script that imports `/path/to/node_modules/pscat/index.js` skips the map
and gets the browser entry, which `fetch()`es a `file:` URL; Node's fetch
refuses it with "not implemented... yet...", which names nothing useful.

Of the issue's three suggestions this takes the first and third:

- `fetchWasm` in `npm/renderer.js` wraps `fetch`. When it fails on a `file:`
  URL under Node (`process.versions.node`) the error names the fix: import
  `pscat` or `pscat/node.js`, or pass `createRenderer({ wasm })` the module's
  bytes. The fetch error is kept as `cause`. Only a *failed* fetch is
  rewritten, so Deno and Bun, whose fetch reads `file:`, keep working.
- `exports` gains `./node.js` (with types), so the Node entry can be chosen
  explicitly and stays a documented, stable name.
- Not taken: reading `file:` through `node:fs` inside `index.js`. It would
  put a Node import in the browser entry, which bundlers then have to
  stub or reject. `node.js` already does exactly that job.

The test imports `index.js` by absolute path in-process, expects the new
message, and is skipped on a Node whose fetch reads `file:` URLs. Checked
by hand under Node 22.23 with a script outside the package.

### #1011: `Renderer.pageCount()`

- C ABI: `ps_page_count()` beside `ps_svg_count()` in `src/wasm.rs`;
  `Pscat.pageCount()` in `web/pscat.js`; `Renderer.pageCount()`; typed in
  `npm/index.d.ts`; in `npm/README.md` and `docs/guides/browser.md`.
- It is recorder-independent: `gfx().pages().len()` plus the page being
  drawn when it holds art (`has_trailing_art`) or when it is the first, the
  rule `svg_pages` and `pdf_document` use to finish a document. So it equals
  `svgPages().length` with `svg: true` and the PDF's page count with
  `pdf: true`, between `step()` calls and after the run, and after an error.
  The same rule as held PR #1129's `Interp::page_count` (Rust side); once both
  are in, `ps_page_count` can call it.
- Consequences to know: an empty program is a one-page document (count 1),
  and art drawn after the last `showpage` counts as another page. "Pages
  emitted by showpage" would disagree with `svgPages().length` for those, and
  the issue's acceptance line asks for the latter. Before `begin()` it is 0.
- Tests: `npm/test/api.test.mjs` (stepped one object at a time with each
  recorder combination, with `copypage`; the trailing-page cases against
  `svgPages().length`; after an error) and the raw ABI in `tests/wasm.rs`.
- `scripts/build_npm_global.mjs` (held PR #1072) builds from `renderer.js`
  and `web/pscat.js`, so the classic-script build gets `pageCount` with no
  change there.
