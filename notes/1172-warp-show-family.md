## warp: the rest of the show family draws through the warp (issue #1172, 2026-11-12)

`lib/warp.ps` decorated only `show` when it landed (#962), so text set with
`ashow`, `widthshow`, `awidthshow`, `kshow` or `glyphshow` stayed flat while
everything around it bent. `wp-decorators` now decorates all five. pscat has
no `xshow`, `yshow`, `xyshow` or `cshow`, so those are not decorated
(`aop-wrap-def` refuses a name that is not defined).

- **One glyph at a time.** No operator builds a path with per-glyph spacing,
  so the decorators cut the string into glyphs and `charpath` each one. The
  cut has to match the interpreter's (`decode_one` in `src/font.rs`): one
  byte per glyph, or one UTF-8 sequence per glyph in a Unicode-mode font,
  with an ill-formed sequence shown as U+FFFD glyphs of its longest valid
  prefix (Unicode's maximal subpart, table 3-7). `wp--glyph-len` and
  `wp--code` do that in PostScript. Whether the font is Unicode-mode is read
  from `/UnicodeBuildChar` for a Type 3 font and otherwise probed by counting
  the glyphs the real `kshow` finds in a two-byte UTF-8 string on the null
  device. The probe never runs a glyph procedure: a Type 3 font whose
  BuildChar fails on unknown codes would otherwise fail under the warp only.
  The probe runs only when the text has a byte of 128 or more.
- **ashow, widthshow, awidthshow** share one helper: every glyph's outline
  and its extra advance go into one path inside a gsave, which is warped and
  filled once, then the current point moves to the end, as `show` does.
- **kshow** keeps its state in a dictionary held by the loop body itself, so
  nothing of the decorator's is on the operand or dictionary stack while the
  procedure runs. The next glyph is cut against the font live at that point,
  so a procedure that switches between a byte font and a Unicode-mode font
  mid-string cuts the rest as the interpreter does (#31). An `exit` from the
  procedure leaves the text unfinished; it is re-raised as `invalidexit`, as
  pscat and Ghostscript raise it for the real `kshow`.
- **glyphshow** sets a copy of the font whose Encoding maps code 0 to the
  name, inside the gsave, and `charpath`s `(\000)`. A Unicode-mode font
  ignores its Encoding, so there the real `glyphshow` draws, unwarped.

Tests (`tests/warp.rs`): for each operator an identity warp matches the
plain render (block means, since flattening rules out byte identity), a
`wp-shift` matches the plain render under `translate`, and the current point
and operand stack match the plain operator; kshow's codes, current points,
dictionary depth, a font switch and `exit`; the glyph cut for Hangul and
ill-formed UTF-8 in a Unicode-mode and a byte font.

Review round 1 (agent review on PR #1179):

- **Type 3 text.** `charpath` traces no Type 3 glyph, so the first version
  made Type 3 text vanish under the new decorators, where on `main` the
  real operators ran BuildChar, whose `fill` and `stroke` reach the
  decorated ones and bend. Every show decorator, `show` included (whose
  Type 3 text already vanished on `main`), now hands a Type 3 font's text
  to the real operator; `kshow` decides glyph by glyph, showing a Type 3
  glyph with the real `show`. Tested with Marquee under each operator.
- **Errors.** Bad operands, too few of them, or no current point now go to
  the real operator after the decorator drops its own operands, so the
  error, its order (typecheck before nocurrentpoint) and the operand stack
  left behind are the real operator's. A glyph the `kshow` procedure left
  with no current point raises `nocurrentpoint` with nothing of ours on
  the stack (the loop body holds the state dictionary).
- **gsave guard.** The charpath-and-fill section runs inside
  `gsave ... stopped ... grestore` (`wp--guarded`), so a warp that errors
  leaves neither the gsave nor the glyph path behind; the error is then
  re-raised.
