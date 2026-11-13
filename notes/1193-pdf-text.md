## PDF text output in embedded subset fonts: `--pdf-text` (issue #1193, 2026-11-12)

pscat's PDF drew every glyph as an outline path, so text in a PDF could not
be selected, searched, copied or read aloud, and `pdftotext` returned
nothing. `--svg-text` (#1119) already did the SVG half for the standard
faces. This is the PDF half, built on the same capture (`src/textrun.rs`).

**What was built.**
- `--pdf-text` (CLI, needs `--pdf`), `Interp::set_pdf_text(PdfText::Text)`
  (library; also `Gfx::set_pdf_text`). Off by default. When it is off,
  nothing in the PDF path changes: the recorder gets the same calls, and a
  document has the same objects, numbers and bytes (checked against the
  merge-base binary, below).
- With it on, a `show`-family glyph of a TrueType or OpenType face goes to
  the PDF recorder as its glyph id and character instead of its outline.
  Consecutive glyphs with the same face, size, frame, colour, alpha/blend,
  clip and baseline make one text object:
  `q r g b rg BT /Fn 1 Tf a b c d tx ty Tm [<gids> adj <gids>] TJ ET Q`.
  `Tm` is the glyph's em → device matrix (the page's flip `cm` is already
  in force), so text space is one em and `Tf` is 1. Every glyph after the
  first is placed at pscat's own x by a `TJ` adjustment against the
  advance `/W` gives it, so `ashow`, `widthshow`, `kshow`, `cshow` and
  the `xshow`/`yshow`/`xyshow` family keep their spacing. The
  adjustment is computed from where the previous printed one left the pen,
  so rounding (0.01/1000 em) does not accumulate along a line. A glyph
  painted with alpha or a blend mode is its own text object, as in SVG: the
  raster composites each glyph separately, and PDF's text knockout would
  composite one text object once.
- Each face used becomes five objects after pdfmark's named objects: a
  Type 0 font (`/Encoding /Identity-H`), a `CIDFontType0` with `/W`, a
  font descriptor, the font program (`/FontFile3 /Subtype /CIDFontType0C`,
  Flate) and a `/ToUnicode` CMap (UTF-16BE, surrogate pairs past the BMP).
  The page's `/Resources` gets `/Font` only when it shows text. The CID of
  a glyph is its glyph id in the source face, so the content stream names
  glyphs while the page is recorded, before the subset is known.
- `src/pdf_font.rs` writes the font program: a bare, CID-keyed CFF
  (Adobe-Identity-0, one FD, FDSelect format 3, charset format 0, Type 2
  charstrings, no subroutines, no hints) holding `.notdef` and the glyphs
  used. `BaseFont` is `ABCDEF+<PostScript name>`, the tag an FNV-1a hash
  of the document font index and glyph ids, so it is the same on every
  run and platform.
- The capture became format-neutral: `font::paint_resolved_glyph` builds a
  `GlyphText` when either text mode is on, and `Gfx::paint_glyph` asks
  each recorder on its own (`textrun::svg_eligible`,
  `textrun::pdf_eligible`). `unicode_glyph` now passes its character too,
  which is how Hangul, Japanese and Thai reach the PDF; SVG still turns
  them down (they are not standard faces), byte-for-byte as before.

**Why rebuild the font instead of copying it.** The CFF is written from
`ttf_parser::Face::outline_glyph`, the same outlines pscat paints, rather
than by copying tables out of the font file:
- NotoSansKR and NotoSansJP are variable fonts whose default instance is
  Thin; pscat pins them to Regular (`font::prepare_face`). Copying their
  `glyf` table, subset or not, would make every viewer draw Thin text.
  Rebuilding from the pinned outlines embeds the Regular pscat renders.
- One writer covers TrueType (`glyf`: Liberation, Noto, the catalog) and
  CFF (TeX Gyre, URW) sources. Quadratics become cubics exactly.
- No subsetting library is in the dependency tree (`read-fonts`/`skrifa`
  are there through other crates, but no subsetter), and the CFF writer is
  about 300 lines. Fonts are always subset; nothing embeds a whole file.
- Outlines are scaled to the CFF default 1000-unit em; no `FontMatrix` is
  written in the Top DICT or the FD, so no viewer can compose two of them
  differently. A coordinate that is not a whole unit is a 16.16 fixed
  operand, and each delta is taken from the previous *rounded* point.
  Hinting is not carried; pscat's own raster is unhinted.
- The cost is size: a 2048-unit TrueType outline mostly has fractional
  coordinates at 1000 units, five bytes each. It is still far smaller
  than outlines in the content stream (table below).

**Hardening (Codex review, round 1).**
- Every ttf-parser call in the font writer runs under `guard::catch`, as
  the show engine's lookups do (`font::advance_of`). The `truetype` fuzz
  regression `hvar_region_index_overflow` overflowed a `u16` inside the
  parser's advance lookup with `--pdf-text` on, a panic in debug and CI
  builds. `font::tests::the_truetype_fuzz_corpus_survives_pdf_text` shows
  every byte of every committed `truetype` seed and regression with PDF
  text on (it panics with the guard removed).
- A glyph is built into its charstring the first time it is shown, not at
  `finish`. One that cannot be written exactly is refused and keeps its
  outline in the content stream: a delta past a Type 2 operand's ±32768
  units (a valid 16-unit em at 1000/em; clamped, it drew at half size), a
  charstring past 65,535 bytes, or a parser panic. A face none of whose
  glyphs can be embedded writes no font.
- One glyph drawing two characters (NotoSansJP's U+2012 and U+2013 share
  glyph 465) gets a second CID from the face's glyph count up, with a copy
  of the charstring and the same `/W` advance, so `/ToUnicode` gives each
  its own character. Before, the first character shown won for both.
- Unit tests now read the CFF back (ROS first, CIDCount, no FontMatrix in
  the Top DICT or the FD, charset, FDSelect format 3, the Private DICT
  where the FD says), and pin the INDEX offset size at the 255 / 65,535 /
  16M boundaries, 100-entry `bfchar` blocks, surrogate pairs at U+1F600
  and U+10FFFF, and a blend-mode glyph as its own text object in its
  `ExtGState`. The gs/poppler sample now has a Multiply-blended word over
  other text.

**Which glyphs stay outlines.**
- Faces whose OS/2 `fsType` forbids it: restricted-licence embedding (bit 1
  alone), no subsetting (bit 8: this writer only subsets), bitmap only
  (bit 9). Preview & Print and Editable allow embedding in a document. Set
  bits are read permissively, as OpenType says: several TeX Gyre faces set
  both 0x4 and 0x8, which ttf-parser's `permissions()` calls malformed
  (`None`) for OS/2 version 3+, so the bits are read directly. A face with
  no OS/2 table carries no restriction (ttf-parser's
  `is_subsetting_allowed()` says `false` there, which would have refused
  it).
- Type 1 fonts defined by a program and Type 3 fonts (another glyph path
  entirely), `charpath`, `stringwidth`, painting suppressed (`nulldevice`,
  Type 3 metrics runs).
- Per glyph: a name with no Unicode value (Symbol's `alpha`, Dingbats'
  `a1` ...: `encodings::name_to_char` is `None`), `.notdef` (glyph 0, which
  is CID 0 in the subset), control characters, and a glyph the font writer
  refuses (above). Unlike SVG, any script
  qualifies, combining marks and right-to-left letters included: the PDF
  carries the glyph at pscat's position, and the character only feeds
  `/ToUnicode`.

**Licences of the bundled faces** (what permits embedding a subset):
Liberation (`fonts/LICENSE`) and the catalog's Noto, Nanum and display faces
(`fonts/catalog/*/OFL.txt`, a few Apache 2.0) are OFL 1.1/Apache: a
document may embed them, and a subset embedded in a document is not a
redistributed "Modified Version". TeX Gyre (`fonts/base35/TeXGyre-*`) is the
GUST Font License (LPPL 1.3c), which permits it; its rename request is about
distributing modified font files. URW's Standard Symbols PS and D050000L
(`fonts/base35/URW-symbols/LICENSE.md`) are AGPL with an explicit exception
for "these font programs in a Postscript or PDF file that consists of a
document", or LPPL, or OFL, at the user's choice. The `fsType` check is for
faces a program or a user font pack brings. A unit test pins that every
compiled-in face passes it (`font::tests::every_compiled_in_face_may_be_embedded_in_a_pdf`).

**Evidence.**
- `tests/pdf_text.rs` (11 tests): default equals explicit `Paths` and has no
  fonts; text objects, fonts, ToUnicode, the ashow `TJ` value, clip group and
  alpha `ExtGState`; fallbacks (charpath, Type 3, stringwidth); per-page font
  resources; mid-run reads; the CLI flag. `pdftotext` extracts Latin and
  Hangul (and the outline PDF has no text). gs and `pdftoppm` rasterize the
  text PDF of a page with every spacing operator, Hangul, rotation, a clip,
  colour and alpha: mean block difference against pscat's canvas 0.22 (gs)
  and 0.25 (poppler), against the same tool's outline PDF 0.96 and 0.49;
  `tests/pdf.rs` allows 3.0. The canvas is byte-equal across modes. The
  gs/poppler tests skip without the tool, with a `SKIPPED` line.
- `font::tests::a_restricted_licence_face_keeps_its_outlines_in_pdf_text`:
  a copy of Liberation Sans with `fsType` 0x0002 stays outlines beside
  Helvetica text.
- `fontTools.cffLib` parses every embedded CFF (ROS Adobe-Identity-0,
  default FontMatrix, the expected glyph outlines); `pdffonts` lists each
  face as `CID Type 0C`, embedded, subset, with ToUnicode. gs, poppler
  and macOS CoreGraphics (`sips`) all render the sample page the same, with
  Hangul at Regular weight.
- Default output: the merge-base binary (`c27a021`) and this branch's,
  both release builds, on every `examples/*.ps` and `selftest/drivers/*.ps`
  plus `gallery/pscat_card.ps`, `ring_of_type.ps` and `keepin_it_real.ps`
  (109 files): `--pdf` is byte-identical for all 109, and `--svg
  --svg-text` for all 95 single-page SVGs and all 57 pages of the 14
  multi-page ones. The larger gallery pieces were left out (the machine
  was short on memory).
- Corpus sweep, `--pdf` vs `--pdf --pdf-text` of `font_catalog.ps` (every
  bundled and catalog face, two pages), `specimen.ps`, `pdf_document.ps`,
  `template_letter.ps`, `postcard.ps`, `ring_of_type.ps` (rotated runs),
  `keepin_it_real.ps` and `pscat_card.ps`, each rasterized by gs
  (`-dPDFSTOPONERROR`) and by `pdftoppm`: no warning or error from either,
  the same page count, and a largest per-page mean block difference
  between the two PDFs of 2.98 (gs) and 1.81 (poppler), on
  `ring_of_type`. Most of that is gs's own outline rendering:
  against pscat's canvas the gs raster of the outline PDF differs by 3.08
  there and the text PDF by 0.92 (`font_catalog` page 2: 2.94 against
  0.56). Same glyph shapes and positions, side by side at 2x.

**Sizes** (`--pdf` vs `--pdf --pdf-text`, this branch):

| page | outlines | `--pdf-text` | |
|---|---:|---:|---:|
| a Letter page, 56 lines of Times 11 pt | 741,220 B | 8,264 B | 90x smaller |
| 41 lines of Hangul (NotoSansKR 14 pt) | 293,993 B | 11,924 B | 25x smaller |
| `examples/pdf_document.ps` | 69,587 B | 21,553 B | 3.2x |
| `examples/template_letter.ps` | 42,881 B | 13,451 B | 3.2x |
| `examples/specimen.ps` (8 faces, few glyphs each) | 48,550 B | 44,758 B | 1.1x |
| `examples/postcard.ps` | 17,809 B | 12,621 B | 1.4x |
| `gallery/ring_of_type.ps` | 168,514 B | 62,158 B | 2.7x |

A page with a few glyphs in many faces gains least: each face carries its
subset. A text page shrinks by an order of magnitude or more.

**Deferred / follow-ups.**
- wasm (`ps_begin_vector` flag bit) and npm (`pdfText`), with a CLI-vs-wasm
  byte-parity test like `wasm_svg_text_matches_the_cli`. The output is
  deterministic by construction (no hashing state, BTreeMaps, FNV tag,
  flate2 like the content streams), but nothing proves the byte equality
  yet.
- Symbol and Dingbats: map their glyph names to Unicode (the Adobe Glyph
  List's Symbol names, ITC Zapf Dingbats' `a1`...) so they become text too.
- Whether `--pdf-text` should become the default (#1026 proposes
  `--text-as-paths` as the opt-out), and the subcommands' `-o x.pdf`.
- Size: emit outlines at 6x the source em with a `FontMatrix`, making
  TrueType-derived coordinates whole numbers (2-3 bytes instead of 5), once
  the matrix is checked in more viewers.
- SVG `@font-face` with the same subsets (#1026's SVG half).
