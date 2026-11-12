## wasm/npm: SVG and PDF output from the WebAssembly build (issue #1003, 2026-11-04)

The WebAssembly build and the npm package gave back only RGBA pixels, so a
web app that wanted print-ready output had to patch the engine.
[PSCatLabs/print-shop](https://github.com/PSCatLabs/print-shop) did:
`patches/pscat-vector-wasm.patch` there (46 lines, on a 0.6.3 branch) turned
both recorders on in every run and exported `ps_vector_export(kind)` (0 = the
SVG pages as a JSON array, 1 = the PDF) with `ps_vector_ptr`. This issue
takes that patch as its starting point and makes it the supported ABI.

**What was built.**
- `src/wasm.rs`: `ps_begin_vector(src, len, w, h, scale, flags)` is
  `ps_begin_scaled` plus recording (bit 0 SVG, bit 1 PDF; -1 for an unknown
  bit). It does what the CLI does around the recorders: enable them before
  the program runs and, with PDF, feed `pdf::scan_document_info` to
  `set_pdf_info`, so `%%Title:`/`%%For:` reach `/Info`. `ps_svg_count()`,
  `ps_svg(page)` and `ps_pdf()` finish into one buffer read with
  `ps_vector_ptr`/`ps_vector_len` (0 ok, -1 not recorded or no such page,
  -2 no program). The finished SVG pages are cached between `ps_svg` calls,
  cleared on begin and on every step, so reading N pages finishes the
  document once, not N times.
- `web/pscat.js`: `begin`/`run` take a fifth `record` argument
  (`{ svg, pdf }`); `svgCount()`, `svg(page)` (string or null), `pdf()`
  (`Uint8Array` or null).
- npm: page options `svg`/`pdf` (booleans), `Renderer.svg(page = 0)`,
  `svgPages()`, `pdf()`, which throw when the run was not recorded and a
  `RangeError` for a page the document does not have. Types, README, the
  API test and the tarball test (including the TypeScript consumer) cover
  them.

**Choices.**
- Opt-in, not always on as in the patch: the playground and every
  pixels-only user would otherwise pay for two recorders on every run.
  A begin variant rather than a separate "enable" call, so a caller cannot
  turn recording on after the program has started drawing.
- Pages one at a time as raw bytes, not a JSON array: no serde round-trip,
  and no copy of every page to read one.
- SVG and PDF stay sized in device pixels, as on the CLI (`--dpi 144`
  gives a 1224x1584 MediaBox for Letter). Matching the CLI byte for byte
  was the point; the docs say to keep 72 dpi for print-size output.

**Evidence.** `wasm_vector_output_matches_the_cli` (`tests/wasm.rs`): a
two-page document with Times and Helvetica text, an arc, strokes under 24
rotations, a curve and DSC comments, through `pscat --page 612x792 --dpi
{72,144} --svg --pdf` and through wasm, both via `web/pscat.js` (two runs in
each of two instances) and via the npm `Renderer`. Every SVG page and the
PDF are byte-equal to the CLI's. The trig did not round apart between the
native and wasm libm at the precision the recorders print. Packed tarball
5.48 MB, under the 6 MB budget.

**Deferred** (still on #660): the Vite/headless-browser smoke test,
playground dogfooding, size work. No transparent-background option in wasm
(the CLI's `--transparent`); nobody has asked for one.
