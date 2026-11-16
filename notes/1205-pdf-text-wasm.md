## `--pdf-text` for the wasm build and npm (issue #1205, 2026-11-16)

#1200 added `--pdf-text` and `Interp::set_pdf_text`; browser and npm users
could not turn it on. It now mirrors `svgText` (#1119) exactly.

**What changed.**
- `src/wasm.rs`: `ps_begin_vector` flag bit 8 (`PDF_TEXT`). It needs bit 2
  (`RECORD_PDF`), as bit 4 needs bit 1; anything else returns -1. The
  recorder is set up in the CLI's order: `set_pdf_text` before `enable_pdf`.
- `web/pscat.js`: `begin(..., { pdf: true, pdfText: true })` sets the bit
  (`record.pdf && record.pdfText`).
- `npm/renderer.js`: `pdfText` is a boolean page option; `pdfText: true`
  without `pdf: true` throws a `TypeError`, like `svgText` without `svg`.
- `npm/index.d.ts`, `npm/README.md`, `docs/guides/browser.md`,
  `docs/guides/getting-started.md`: the option and the flag are documented.

**Fonts.** The wasm build has no font catalog or packs. A face the page
needs beyond the embedded standard 35 (NotoSansKR for Hangul, say) is given
to `addFont(name, bytes)`; PDF text embeds the registered face like any
other, so the output equals the CLI's with the same face installed.

**Test.** `wasm_pdf_text_matches_the_cli` (`tests/wasm.rs`) compares the
CLI's `--pdf --pdf-text` bytes with the raw ABI's and the npm Renderer's for
a Latin page and a Hangul page, asserts the CLI really wrote text objects
and the embedded NotoSansKR (so a shared fallback cannot pass), and checks a
run without `pdfText` has none. `npm/test/api.test.mjs` covers the option's
validation.
