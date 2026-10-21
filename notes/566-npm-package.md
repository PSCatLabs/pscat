## The `pscat` npm package for the wasm renderer (issue #566, 2026-10-20)

**What was built.** `npm/` is the `pscat` npm package: the wasm build the
site already uses (`scripts/build_wasm.sh`), with `web/pscat.js` copied in
as the low-level `core.js` and a typed API on top (`npm/renderer.js`,
`index.d.ts`). `createRenderer()` gives a `Renderer`, with `render(source,
{width, height, dpi})` returning `{width, height, data, output}` (RGBA,
ImageData layout), `begin`/`step(budget)`, a `frames()` generator for
watch-it-draw, `paintTo(canvas)`, `imageData()`, `output`, and
`addFont(name, bytes)`. Errors are `PostScriptError`s with the PostScript
`errorName`, `command`, `line`, `report` and `output`. `index.js` is the
browser/bundler entry (it fetches `new URL('./pscat.wasm',
import.meta.url)`, streaming only when the server sends `application/wasm`);
`node.js` reads the file from disk, and the exports map's `node` condition
picks it. Node 22 or later, ESM only, no dependencies, no install scripts.
Two new wasm exports: `ps_begin_scaled` (dpi) and `ps_add_font`, backed by
`font::register_font_bytes`, which registers a font under a pack file stem
the way the disk catalog would, so aliases (`/Garamond`) and the
Unicode-mode CJK/Thai encodings work unchanged. On wasm, `findfont`
consults those registered faces. The native lookup is unchanged
(`load_catalog_face` now reads the file and calls the shared
`register_face`).

**Release.** The version moves with the crate (`bump-manifests` already
handled `npm/package.json`; tests now cover it, plus a lockstep check
against `Cargo.toml` and the bump keeping the file's layout). `verify` packs
the tarball once into `release-inputs/` (`build_npm.sh --pack`, after the
crate, with the same cargo-about notices). The `npm` stage publishes
that exact file with `npm publish <tgz> --provenance` and compares npm's
`dist.integrity` with the tarball's SHA-512 (checked against `npm pack`'s
own value in a test). A missing tarball, or one holding another
name or version, halts. The first publish is a manual bootstrap
(RELEASING.md, one-time setup step 7).

**Size (0.6.0).** `pscat.wasm` 7.5 MB, tarball 3.52 MB, 8.26 MB unpacked,
15 files. Budget: 4 MiB packed, 9 MiB unpacked (`build_npm.sh`). Almost all of
it is data: the Liberation fonts (4.2 MB) and `lib/` (2.6 MB). wasm-opt
`-Oz` (binaryen 132) saved 3.4% raw and about 1% gzipped, and stripping
names alone saved 2.3% raw. Neither was worth a binaryen dependency in the
build. The old "~5.5 MB" in the docs was stale.

**Tests.** `npm/test/api.test.mjs` (node:test, against `npm/`): page
geometry and dpi, output, typed errors and reuse after one, stepped equals
one-shot, embedded `lib/`, `addFont` (alias and Unicode-mode Thai), bad
arguments, wasm sources. `tests/wasm.rs` now also renders five programs
(fills, curves, rotated strokes, base fonts, artkit, a 144 dpi page) in
wasm under Node and natively. They are byte-identical on macOS arm64. The
tolerance (≤0.5% of pixels, ≤8 levels) allows for libm rounding on other
hosts. `scripts/test_npm_tarball.sh` installs the packed tarball into a fresh
project, renders through `import 'pscat'`, and type-checks a TS consumer
with typescript 5.9.3. The `npm` workflow runs all of these. `test` skips
`tests/wasm.rs` because it has no wasm target.

**Deferred** (#660): a headless-browser and Vite smoke test, the playground
running on the package, optional SVG/PDF output, and a "core" build without
lib/.
