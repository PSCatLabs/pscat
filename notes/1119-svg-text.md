## SVG text output for the standard fonts: `--svg-text` (issue #1119, 2026-11-11)

pscat's SVG drew every glyph as an outline path. That is exact in any
viewer, but large: a 600x400 pt page with three short strings was 52 KB
(the issue measured 88 KB on another page), and MCP clients that return SVG
as text cap it (pscat-mcp: 256 KiB inline). The text also could not be
selected, searched or copied.

**What was built.**
- `--svg-text` (CLI), `Interp::set_svg_text(SvgText::Text)` (library; also
  `Gfx::set_svg_text`), `ps_begin_vector` flag bit 2 (value 4) in wasm,
  `record.svgText` in `web/pscat.js` and `svgText: true` in the npm
  Renderer. Off by default. When it is off, nothing in the SVG path changes:
  the recorder takes the same calls with the same arguments.
- With it on, a glyph of one of the twelve built-in faces (Helvetica, Times,
  Courier; regular, bold, italic/oblique, bold italic) goes to the SVG
  recorder as its character instead of its outline. Consecutive glyphs with
  the same face, size, frame, colour, alpha/blend, clip and baseline form one
  `<text>`. It carries `xml:space="preserve"`, an `x` per glyph (pscat's own
  advances, so ashow/widthshow/kshow spacing and kerning-free layout hold), one `y`,
  a CSS font-family list and font-size, weight, style and fill, and a
  `transform` only when the text is rotated, skewed or stretched (a unit
  `matrix(a b c d 0 0)` with the scale divided into font-size; x/y are in that frame).
  Spaces are kept as characters so copied text keeps its words.
- `src/textrun.rs` is the capture, and it does not depend on SVG:
  `GlyphText` (character, glyph id, FID, em→device matrix), `Placement`
  (size, unit frame, local origin), `TextRun` (the grouping rule) and
  `standard_face`. `font::paint_resolved_glyph` builds a `GlyphText` and calls
  `Gfx::paint_glyph`, which paints the outline into the raster and the PDF as
  before and gives SVG the character.

**Which glyphs stay outlines, and why.**
- Faces other than the twelve: the rest of the standard 35 (Palatino,
  Bookman, New Century Schoolbook, Avant Garde, Zapf Chancery,
  Helvetica-Narrow) have no metric-compatible face that browsers commonly
  have. With each glyph pinned at pscat's x, a substitute with other widths
  would overlap or gap inside words. The same applies to Symbol and Dingbats
  (non-Unicode encodings), font-pack faces, Unicode-mode CJK/Thai faces,
  and Type 1 and Type 3 fonts defined by a program.
- `charpath`, `stringwidth`, glyphs under painting suppression (Type 3
  metrics runs, `nulldevice`).
- Per glyph: a name with no Unicode value (`name_to_char` is `None`),
  `.notdef`, a glyph the face lacks, and any character outside an
  allowlist of left-to-right spacing blocks (Latin, Greek, Cyrillic without
  its combining marks, punctuation, currency, symbols, Latin ligatures).
  Controls and format characters draw nothing in a browser, combining marks
  (in any block) attach to the previous character, and right-to-left
  letters are reordered by the viewer's bidi pass. Any of these would move
  a glyph. The first version had a denylist, which missed marks outside
  U+0300-036F (Codex round 2).
- A glyph painted with alpha or a blend mode is its own `<text>`, never part
  of a run. The raster composites each glyph separately, so overlapping
  glyphs blend twice; a viewer composites one element once (Codex round 1).
- A blank glyph (space) that would open a new page after `showpage` is
  dropped. It draws nothing, and recording it would put it on the old page's
  body just before the erase.

**Choices.**
- *The name.* Converters that default to outlines spell the opt-in for text
  as an option on the SVG writer: MuPDF's `-O text=text|path` (default
  `path`). Inkscape's `--export-text-to-path` and Ghostscript's
  `-dNoOutputFonts` are the opt-out in tools whose default is text. #1026
  proposes `--text-as-paths` for the day text becomes the default in PDF and
  SVG. Here only SVG changes and the default stays outlines, so the flag is
  SVG-scoped: `--svg-text`. The enum `SvgText { Paths, Text }` leaves room
  for a third mode (text with an embedded subset font, #1026's SVG half)
  without a second flag. pscat-mcp will call its option `svg_text`.
- *Per-glyph x rather than one x and the viewer's advances.* Same layout as
  the outlines whatever face the viewer picks, at a cost of about 6 bytes per
  glyph. Each `<text>` has one baseline. Shows on the same baseline join one
  run, so a line set word by word copies as one line.
- *CSS families.* The PostScript family first, then the metric-compatible
  faces: `Helvetica,Arial,'Liberation Sans',sans-serif`,
  `Times,'Times New Roman','Liberation Serif',serif`, and `'Courier New',Courier,
  'Liberation Mono',monospace`. Courier New comes first because macOS's
  Courier is a different, lighter design. All have the Liberation advances.
- The open run is written after the body by `document()` and not flushed by
  it, so `svg_pages()` mid-run (wasm `ps_svg`, npm `svg()`) shows text drawn
  so far, and more glyphs on the same line still join that run. Any other mark
  (fill, stroke, image, shfill), `end_page`, `set_size` and `erase` close it
  first, so paint order holds.

**Gradients (the issue's second ask).** `shfill` axial and radial shadings
already export as native `<linearGradient>`/`<radialGradient>` (since #20).
Large SVGs come from other things. The 400x200 `pscat chalk 'E = mc^2'` board
is 5.1 MB, of which 36 `<clipPath>` defs are 3.3 MB, plus 2782 small paths
(chalk dabs inside glyph-shaped clips). It has no standard-font text and no
gradients, so `--svg-text` does not shrink it. The libraries' own gradient
helpers (artkit's `gradfill`/`axialsh`/`radialsh`) already go through
`shfill`, so they are native gradients in SVG too. A gradient drawn as many
separate shapes is a program looping `rectfill`s itself, which an exporter
cannot recognise reliably. The SVG-size lever left is the clip-heavy
dry-media output (chalk, crayon, charcoal: tens of MB). It is a proposed
follow-up, not part of this change.

**Evidence.**
- `tests/svg_text.rs` (19 tests): default equals explicit `Paths` and
  has no `<text>`; positions, styles, escaping, ISOLatin1 re-encoding, run
  splitting, rotation (origin maps back), fallbacks (Symbol, Palatino,
  charpath, Type 3, a combining mark mid-show, marks outside U+0300-036F and
  a right-to-left letter), alpha/blend glyphs one element each, paint order, mid-run reads, clip/alpha, page count,
  raster and PDF byte-equal across modes, the CLI flag.
- `tests/wasm.rs` `wasm_svg_text_matches_the_cli`: with text on, the CLI,
  the raw wasm ABI and the npm Renderer give byte-equal SVG at 72 and 144
  dpi, including rotated, skewed and stretched text.
- Default output: every `gallery/*.ps` and `examples/*.ps`, rendered to SVG
  by the merge-base binary and by this branch's,
  is byte-identical (127 of 127 files, every page). PNG, the same two
  binaries, gallery at 144 dpi plus `selftest/drivers/*.ps`: 138 of 138
  identical, and the drivers' SVGs 11 of 11.
- Browser (Chromium via Playwright, macOS fonts): a page of Times, Helvetica
  and Courier in all styles, with a rotated and a letter-spaced line, looks
  the same as the outline SVG side by side, and the two coincide when
  overlaid at 2.5x (same layout, baselines and sizes; only the letter shapes
  of the substituted faces differ). "fi/ffi/ffl" do not ligate. Selection
  and copy return the text with its spaces and line breaks; find-in-page
  finds words.

**Sizes** (`--svg` vs `--svg --svg-text`, this branch, every page of the
file summed; 63 of the 127 corpus files contain standard-font text):

| page | outlines | `--svg-text` | smaller by |
|---|---:|---:|---:|
| 600x400 scene, three strings (the issue's case) | 52,414 B | 1,536 B | 34x |
| a letter page in Times/Helvetica/Courier (perceptual check) | 364,504 B | 7,911 B | 46x |
| `examples/pdf_document.ps` | 244,818 B | 5,151 B | 48x |
| `examples/template_letter.ps` | 153,824 B | 3,414 B | 45x |
| `examples/specimen.ps` | 163,334 B | 6,379 B | 26x |
| `examples/postcard.ps` | 57,100 B | 3,318 B | 17x |
| `gallery/pscat_card.ps` | 186,558 B | 66,249 B | 2.8x |
| `gallery/ring_of_type.ps` (1226 rotated runs) | 520,830 B | 277,925 B | 1.9x |
| `gallery/keepin_it_real.ps` | 592,401 B | 538,649 B | 1.1x |
| `pscat chalk 'E = mc^2'`, 400x200 | 5,146,969 B | unchanged | 1x |

Text-led pages shrink 17-48x. Art with a few labels shrinks little, because
the art is the bytes. The largest outputs (`gallery/released_memo.ps` 131 MB,
the dry-media specimens 64-95 MB) are set in typewriter/handwriting faces
or are paint texture, and stay as they were.

**How #1026 builds on this.** #1026 asks for real text in PDF (and SVG) with
embedded subset fonts. The capture it needs exists now:
`GlyphText`/`TextRun` carry the FID, glyph id, character and placement of
every standard-face glyph in show order, grouped into runs.
1. Widen `standard_face`'s gate to "faces whose licence allows embedding"
   (Liberation, TeX Gyre, URW, Noto: all OFL/LPPL/GUST) for an embedding
   writer; keep the metric-compatibility gate for the no-embedding SVG mode.
2. PDF: a `PdfRecorder::glyph` like `SvgRecorder::glyph`, emitting per run
   `BT /Fn size Tf a b c d x y Tm [..] TJ ET`, with the run's frame and
   size as `Tm`. A per-glyph x becomes TJ adjustments against the font's
   own widths. Collect `(fid, gid)` per document, subset each face at
   `finish` (glyf/loca or CFF), and write a Type 0/CIDFontType2 (or Type 1C)
   font with a `/ToUnicode` CMap from the run characters. The raster path
   stays as it is. Determinism: subset in glyph-id order, no timestamps.
3. SVG: a third `SvgText` mode with an `@font-face` data-URI subset per
   face, written once per document, and the same `<text>` runs.
4. Then `--text-as-paths` (#1026's name) can become the opt-out if the
   default flips, with `--svg-text` kept as an alias.

**Deferred / follow-ups.** `pscat-mcp` (in-repo `src/mcp_server.rs`)
`render_postscript` does not expose the mode (pscat-mcp exposes its own
`svg_text`). The subcommands' `-o x.svg` (`typewrite`, `handwrite`,
`chalk`, ...) do not take `--svg-text`. Helvetica-Narrow could use a
horizontal scale of Helvetica. The chalk/clip size and library-drawn ramps
are covered in the gradients paragraph above.
