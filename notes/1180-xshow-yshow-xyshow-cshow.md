## The Level 2 text operators xshow, yshow, xyshow and cshow (issue #1180, 2026-11-12)

pscat had none of the four, so PostScript from typesetters and PDF
converters (dvips, pdftops, Ghostscript's ps2write emit `xshow` and
`xyshow` for per-glyph placement) stopped at `undefined`.

**What was built.**

- `xshow`, `yshow`, `xyshow` (`src/ops/font.rs`) queue an ordinary
  `Frame::Show` carrying a `font::Displacements` list. After each glyph
  the pen moves by the next dx, dy or dx dy in user space, in place of
  the glyph's width, so Type 3 BuildChar, Type 1, Unicode-mode faces and
  SVG text output (`--svg-text`, which records each glyph's own x) all
  work as they do for `show`.
- Encoded number strings (PLRM 3.14.5) in `src/numstring.rs`: every
  representation (16- and 32-bit fixed point with any number of fraction
  bits, IEEE and native reals, both byte orders), with gs's checks: a
  short string or a first byte other than 149 is `typecheck`; an
  undefined representation, or a count that is not the number of whole
  values after the header, is `rangecheck`; a non-finite real is
  `undefinedresult`.
- `cshow` runs the show frame in `ShowMode::Width` and hands each
  character's `code wx wy` to its proc. It paints nothing, needs no
  current point and never moves it.

**Decisions, all settled against Ghostscript 10.07.1** (the pinned
report in `tests/xshow.rs`):

- Validation happens before anything paints, in gs's order: both
  operand types, then an encoded string's header and count, then too few
  numbers (`rangecheck`, also before `nocurrentpoint`), then the current
  point, then the values: every array element a number (`typecheck`,
  after `nocurrentpoint`, and for unused extra elements too) and every
  encoded real finite (`undefinedresult`, also after `nocurrentpoint`
  and the count check). Extra numbers are fine. Codex's review of PR
  #1190 caught the first version decoding the values with the header.
- The glyph count is `font::glyph_count`, cut by `decode_one` like the
  show itself, so a Unicode-mode font counts UTF-8 scalars.
- One stray byte after the last whole value of an encoded string is
  allowed; a whole extra value is a `rangecheck` (gs compares the count
  with `(length - 4) / size`).
- Representations 49 and 177 ("native" reals) read the host's byte
  order, which is little-endian on every pscat target, like gs here.
- cshow sets the base font back after every proc call, at the end, and
  when a `stop` or error leaves the proc (`ShowCtx::cleanup`), because gs
  does. pscat has no composite fonts, so the base font is the font cshow
  started with.
- cshow takes its operands in either order (`string proc cshow` too),
  as gs does, so a program written against gs runs here.

**Not done.**

- `lib/warp.ps` does not decorate the four operators yet; PR #1179 was
  editing it at the same time. A follow-up adds the decorators.
- gs also accepts an encoded number string for `rectfill`, `rectstroke`
  and `rectclip`; pscat's take arrays only. `NumOperand` is ready for it.
- PDF output has no text objects yet, for any show, so these glyphs are
  outlines there, as `show`'s are. SVG text output keeps them as text.
- Differences inherited from the rest of pscat, unchanged here: an
  erroring operator consumes its operands (gs leaves them), access
  attributes are not enforced (gs gives `invalidaccess` for a
  `noaccess` array), and there is no `limitcheck` for a huge
  displacement.
