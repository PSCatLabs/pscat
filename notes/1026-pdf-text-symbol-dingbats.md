## PDF text for Symbol and ZapfDingbats, and where #1026 stands (issue #1026, 2026-11-18)

#1026 asked for real text in PDF and SVG: embedded subset fonts, a
ToUnicode map, `--text-as-paths` as the way back to outlines, and CLI/WASM
byte identity. Most of it shipped as opt-ins before this work: `--svg-text`
(#1119, standard faces as `<text>`), `--pdf-text` (#1193, text objects in
embedded CID-keyed CFF subsets with `/ToUnicode`, every embeddable face
including the catalog and font packs) and its wasm/npm form (`pdfText`,
#1205, with a CLI-vs-wasm byte test). This change closes the one gap left
in what `--pdf-text` covers that needed no owner decision.

**The gap.** With `--pdf-text`, a glyph becomes text only when its glyph
name has a Unicode value. `encodings::name_to_char` knows the Standard and
ISOLatin1 names, not Symbol's (`alpha`, `summation`, `lessequal`) or
ZapfDingbats' (`a1` ... `a191`). So Symbol and Dingbats glyphs stayed
outlines: `pdftotext` on a page with `αβγ ∑ ≤ ∞` returned only the Times
text around them. Found PostScript sets Greek and maths in Symbol (groff's
`S` font, gnuplot's enhanced text); pscat's own mathkit draws its symbols
with strokeglyphs, not the Symbol font, so its PDFs are not affected.

**What was built.**
- `encodings::symbol_name_to_char` and `dingbat_name_to_char`: sorted
  name → code point tables (159 Symbol names, 187 Dingbats names), read by
  binary search. Source: the Unicode Consortium's Adobe mapping files
  `symbol.txt` and `zdingbat.txt` (the Dingbats encoding was already
  checked against `zdingbat.txt` in #563), cross-checked against Adobe's
  AGL `zapfdingbats.txt`: they agree on every name both list.
- `font::symbolic_text_char`: the character for a glyph name when the face
  is the Symbol or Dingbats catalog face (`CatalogEncoding::Symbol` /
  `Dingbats`) and PDF text is on. It is asked before `name_to_char`,
  because `name_to_char("mu")` is the Latin-1 micro sign and Symbol's `mu`
  is the Greek letter.
- Only the text character changes. The glyph is still found by its name in
  the face (`resolve_glyph`), so the raster, the outline PDF and SVG are the
  same. SVG text never takes these faces (`textrun::standard_face`).

**Choices.**
- *Corporate Use entries are left out.* `symbol.txt` maps 29 names
  (bracket and integral pieces, `radicalex`, the serif and sans-serif
  ©®™) and `zdingbat.txt` 14 (`a85`-`a96`, `a205`, `a206`, the ornamental
  brackets) into the private-use area. A private-use value means nothing to
  a search or a screen reader, so those glyphs keep their outlines. The AGL
  maps the 14 Dingbats brackets to U+2768-2775 (Unicode 3.2); pscat's
  Dingbats encoding leaves those codes `.notdef` anyway (#605).
- *Duplicates.* Where `symbol.txt` gives a name two values the Greek letter
  wins (`Delta` U+0394 over U+2206, `Omega` U+03A9 over U+2126, `mu`
  U+03BC over U+00B5), which is what a search for the letter finds, and
  `fraction` is U+2044 as in the AGL.
- *Gated by face.* The tables are not merged into `name_to_char`. That
  function also drives the `cmap` fallback of glyph lookup, so widening it
  would change which glyph some fonts draw. And `a1` means a dingbat only in
  a Dingbats-encoded face. A Times copy re-encoded with `/alpha` keeps that
  glyph as an outline, as before (tested).

**Evidence.**
- `tests/pdf_text.rs`: `symbol_and_dingbats_glyphs_are_text` (the
  `/ToUnicode` entries, both faces embedded, `radicalex` the only outline),
  `symbol_names_outside_the_symbol_face_are_not_text`;
  `pdftotext_extracts_latin_hangul_and_symbols` reads back
  `Sum: αβγ ∑ ≤ ∞ πθΩ μ`, `∫ √ ∞`, `✓✔✕✖ ■●◆`, `★♣`; the gs and pdftoppm
  raster tests now render a Symbol/Dingbats page too. Mean block difference
  of the text PDF against pscat's canvas: 0.09 (gs) and 0.15 (pdftoppm);
  against the same tool's outline PDF 0.35 and 0.23 (`tests/pdf.rs` allows
  3.0). `encodings` unit tests: tables sorted, no private-use or control
  values, agreement with `name_to_char` except `mu`, and every name in the
  Symbol and Dingbats encodings covered except the Corporate Use ones.
- `tests/wasm.rs` `wasm_pdf_text_matches_the_cli`: a third page of Symbol
  and Dingbats text, byte-equal between `pscat --pdf --pdf-text`, the raw
  wasm ABI and the npm Renderer.
- Merge-base (`296b335`) and this branch's `ci` binaries on the 45
  `examples/` and `selftest/drivers/` files that set type (the paint and
  dry-media pieces left out for time): `--pdf`, `--pdf --pdf-text` and
  `--svg --svg-text` (every page) are byte-identical for all of them
  except `examples/font_catalog.ps` under `--pdf-text`, the one file with
  Symbol and Dingbats text. Its Symbol row now extracts as
  `αβγδεπφϕκλμνωθ√∞↔` (before, only `µ`, Symbol's `mu` read as the micro
  sign) and its Dingbats row as `❁❂❃❄❅❆❇★✩✪✫✬✭✮` (before, nothing), and the
  file is 3.6 KB smaller (344,118 to 340,550 B; the outline PDF is
  566,645 B). qpdf 12.4 (`check_pdf_syntax`, through pikepdf) finds no
  problem, and gs (`-dPDFSTOPONERROR`) and pdftoppm render both pages
  without a message.

**What #1026 still lists, not done here.**
- *Text as the default, `--text-as-paths` as the opt-out.* An owner
  decision: it changes every user's PDF. It also needs a wasm ABI call
  (bit 8 means "text on"; either a new "paths" bit or npm/web defaulting
  `pdfText` to true) and changes pscat-mcp's output, which uses the library
  default.
- *SVG with embedded fonts.* `--svg-text` writes `<text>` for Helvetica,
  Times and Courier only and relies on the viewer's metric-compatible
  faces. Embedding needs an `@font-face` subset a browser accepts: an
  OpenType wrapper (`cmap`, `head`, `hhea`, `hmtx`, `maxp`, `name`, `OS/2`,
  `post`) around the CFF `src/pdf_font.rs` already writes, then the same
  runs. A third `SvgText` variant was planned for it in #1119.
- *Type 1 fonts defined by a program* stay outlines in PDF text; their
  outlines are available (`src/type1.rs`), and the CFF writer could take
  them.
- *Subcommands.* `typewrite`, `handwrite`, `chalk` and the others write
  `-o x.pdf` without a `--pdf-text` option.
