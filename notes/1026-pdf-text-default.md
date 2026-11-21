## PDF text is the default; `--text-as-paths` opts out (issue #1026, 2026-11-20)

Owner decision (2026-11-20, on #1026 and in #1287's owner decisions): make
`--pdf-text` the default for PDF output and add `--text-as-paths` to opt out.
There are no existing users whose PDF bytes need protecting, so the change
does not keep the old default anywhere. 0.7.1 is a patch release, and
`docs/STABILITY.md` says a patch keeps the library API. This change to
`PdfText::default()` and to what `pscat --pdf` writes is an exception the
owner approved.

**What changed.**
- `PdfText::default()` is `Text`, and `Gfx::new` takes the default. Every
  PDF writer inherits it: `pscat --pdf`, the subcommands, the MCP server
  (it runs the CLI), the wasm build, the npm package, and a library caller
  such as pscat-mcp. `PdfText`, `Interp::set_pdf_text` and
  `Gfx::set_pdf_text` keep their signatures. A caller that wants outlines
  calls `set_pdf_text(PdfText::Paths)`.
- CLI: `--text-as-paths` (needs `--pdf`) writes the outline PDF, the bytes
  `--pdf` wrote before. `--pdf-text` is still accepted, still needs
  `--pdf`, and changes nothing, so scripts and docs that name it keep
  working. Giving both is a usage error ("contradict each other"); a pair
  of flags that cancel each other has no obvious winner. `--help` says all
  of this.
- wasm `ps_begin_vector` flags (by value): 8 (bit 3, text, #1205) is
  accepted and is a no-op. The new 16 (bit 4) is the outline opt-out.
  8 and 16 together are refused, and so is either one without 2 (PDF).
  `web/pscat.js` sends 16 for `record.pdfText === false` and never sends
  8. In npm,
  `pdfText` defaults to true, and `pdfText: false` (with `pdf`) gives
  outlines. `pdfText: true` without `pdf` is still a TypeError, and
  `pdfText: false` without `pdf` asks for nothing and is accepted.
- Subcommands: `typewrite`, `chalk` and `photo-etch` write PDFs and inherit
  the default. Checked with `pdffonts`/`pdftotext` on default runs: their
  PDFs hold no fonts and no text. The typewriter face is a Type 3 font
  (`lib/typewriter.ps`), which stays outlines. Chalk draws its own strokes,
  so none of its marks reach the PDF as `show` text. Photo-etch draws no
  text. So the subcommands get no `--text-as-paths` (the same finding
  that closed #1131 for `--svg-text`). `handwrite` and `sheet` write PNG
  only.

**Limits (the code is now on the default path).** The opt-in code from
#1193 was written for a flag. As the default, it now charges the work it
adds:
- At `show`: before a new glyph's charstring is built (`Charstring::new`
  reserves the whole 64 KiB Type 2 budget), the recorder asks the memory
  limit (`probe.check_alloc(CHARSTRING_BUDGET)`). It asks before anything
  is mutated, so a refusal leaves no half-registered font. Each new
  charstring is charged to `work` in whole KiB, at least one step per new
  glyph. The show frame charges `work / 1024` per glyph, so rounding up to
  whole KiB means the charge is never rounded away. A glyph the font
  already holds costs nothing extra (`tests/pdf_text.rs`
  `new_embedded_glyphs_are_charged_as_steps`).
- A second character on the same glyph (an extra CID) used to clone the
  charstring. The charstring is now an `Rc<[u8]>` shared between CIDs.
- At write time, `pdf_font::font_objects` takes the writer's probe. Before
  building the CFF, which copies every charstring twice, it asks the cap
  for that much. The ToUnicode map is asked for too. Both streams use the
  budgeted deflate (`pdf::deflate_limited`). A refusal is the writer's
  memory error (`PdfWriteError::Memory`), like every other object.
  Unit test: `font_objects_ask_the_memory_cap`. With room, the bytes are
  the same as with no cap.

**Determinism.** `tests/wasm.rs` `wasm_pdf_text_matches_the_cli` compares
the CLI, the raw ABI and npm, byte for byte, on Latin, Hangul (an `addFont`
face) and Symbol/Dingbats pages. It checks the default (`--pdf`,
`{ pdf: true }`), the no-op aliases (`--pdf-text`, `pdfText: true`) and
the opt-out (`--text-as-paths`, flag value 16, `pdfText: false`). It also checks
the flag combinations the ABI refuses.

**Size and time** (release build, macOS, one run each; `--text-as-paths`
vs default):

| file | paths | text | smaller by | time paths / text |
|---|---:|---:|---:|---|
| a page of 11 pt Times, 60 lines | 726,734 | 14,402 | 50x | |
| examples/dataviz_paper.ps | 68,174 | 15,858 | 4.3x | 0.06 / 0.04 s |
| examples/pdf_document.ps | 69,587 | 21,553 | 3.2x | 0.05 / 0.04 s |
| examples/type3_demo.ps | 44,984 | 20,526 | 2.2x | 0.03 / 0.03 s |
| examples/graphing.ps | 36,424 | 17,146 | 2.1x | 0.05 / 0.04 s |
| gallery/pscat_card.ps | 55,293 | 31,833 | 1.7x | 0.08 / 0.08 s |
| gallery/hortus.ps | 102,780 | 61,194 | 1.7x | 0.11 / 0.10 s |
| examples/font_catalog.ps (many faces, few glyphs each) | 566,645 | 340,550 | 1.7x | 0.26 / 0.12 s |
| examples/postcard.ps | 17,809 | 12,621 | 1.4x | 0.04 / 0.04 s |
| gallery/fugitive_pigments.ps | 1,553,169 | 1,386,967 | 1.1x | 0.85 / 0.72 s |
| examples/specimen.ps | 48,550 | 44,758 | 1.08x | 0.04 / 0.03 s |
| examples/font_library.ps (Type 1 program fonts) | 117,258 | 117,258 | same | 0.08 / 0.08 s |

No file grew. Peak resident memory: fugitive_pigments 37 MiB as paths, 34
MiB as text; font_catalog 54 MiB and 52 MiB.

**Pixels.** pdftoppm at 72 dpi, text PDF against outline PDF, mean
difference over 10 px blocks (the measure `tests/pdf_text.rs` uses, with a
limit of 3.0): dataviz_paper 0.18, pdf_document 0.20, postcard 0.08,
pscat_card 0.13, hortus 0.21, specimen 0.50, fugitive_pigments 0.64,
font_catalog 0.82. The dense 11 pt Times page is 3.39 at 72 dpi, 1.99 at
150 dpi and 0.85 at 300 dpi. It is the outline PDF that strays. Against
pscat's own canvas, the text PDF scores 0.55 in pdftoppm and the outline PDF
3.13; in gs (72 dpi, AlphaBits 4) they score 1.41 and 10.27. Each reader
antialiases many small filled paths heavier than its own text. Side by side
at 300 dpi the two are the same. `tests/pdf_text.rs` holds the gs and pdftoppm comparisons on its
sample pages.

**Not done here.** SVG with an embedded `@font-face` subset (`--svg-text`
still covers only Helvetica, Times and Courier, unembedded). Type 1 program
fonts as PDF text. An MCP `text_as_paths` argument: the MCP server's PDFs
are text now, and its schema has no opt-out yet. The four committed
`art/keepin-it-real*/*.pdf` prints were not regenerated.
