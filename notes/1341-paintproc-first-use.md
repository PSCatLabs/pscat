## A pattern's PaintProc runs at its first use, as in Ghostscript (issue #1341, 2026-11-19)

Before: `makepattern` ran the `PaintProc` at once. A `PaintProc` that names
a pattern made after the outer `makepattern` was `undefined` in pscat and
worked in gs (the gap `notes/1300-patterns-across-contexts.md` lists).

## What gs does (10.07.1, `nullpage` and `png16m`)

- **When.** Not at `makepattern`, `setcolorspace` or `setcolor`: at the
  first mark (`fill`, `stroke`, `rectfill`, `show`, `imagemask`, ...) that
  paints with the pattern. A pattern that is set and never painted never
  runs. A fill under `nulldevice` runs it too.
- **How often.** Once per pattern, across any number of fills, colours of
  an uncoloured pattern and `gsave` levels, and once even when the pattern
  is also used inside another pattern's `PaintProc`. A `PaintProc` that
  errors leaves the pattern unmade: the next fill runs it again.
- **What it sees.** The graphics state `makepattern` had (line width 3 and
  red there, not the 7 and green at the fill), the dictionary stack as it
  is at the fill (a name defined after `makepattern` resolves), and the
  operand stack as the marking operator left it with the dictionary on top
  (`7 8` under `rectfill`'s four operands is 7 deep); the stack after the
  fill is as before.
- `/BBox [0 0 0 0]` is still `rangecheck` at `makepattern`.

## How

- `makepattern` validates (including the tile geometry) and keeps a
  `pattern::LazyPattern`: spec, `PaintProc`, a copy of the graphics state,
  and its `Origin` (context id, its coordinates to gs's, its PDF default
  space), all read at `makepattern`, since the `PaintProc` may run in
  another context. The capture id (SVG's `t<id>_` names) is reserved there
  too. `PatternColor` holds the shared `LazyPattern`, so every `gsave` copy
  sees one realised tile.
- A marking operator opens with `Interp::pattern_gate(op)`. When the
  current colour is an unmade pattern it pushes the operator itself as a
  one-element procedure frame, then `Frame::Pattern`, then the
  `PaintProc`, and returns having changed nothing; the operator runs
  again when the `PaintProc` returns, with its operands back as they were.
  The show family and the glyph loop do the same per glyph step
  (`Action::RunPattern`: the `Frame::Show` stays below and steps again), so
  a `kshow` proc that sets a pattern mid-string is covered. `image` with a
  dictionary and `/ImageMask` is not gated: it already raises `rangecheck`
  in a pattern space, with or without this change.
- The `PaintProc` starts from the saved state with the page's device and
  an empty `clipsave` stack (a state of another device would resize the
  canvas under the capture at the first `grestore`).
- `LazyPattern` frees its saved states in a loop: patterns made one after
  another, each with the last as its colour, would otherwise drop through
  each other recursively.

## Verification

`tests/patterns.rs`: the runs-once-at-first-paint ordering, the late name
(page identical whichever order the two patterns are made in, and against
gs's run of the program), the makepattern-time state, the operand stack in
and after, a failing `PaintProc` running again, every marking operator in
a table, `kshow`, `nulldevice`, and a chain of 30000 unused patterns. The
fifteen earlier pictures are unchanged (PNG bytes and all three outputs).

## Differences from gs and gaps

- Vector output follows the order patterns are first used in, and a
  pattern that is never painted writes nothing, so SVG and PDF bytes can
  differ from before for a program that makes a pattern it does not use or
  uses patterns in another order than it made them. PNG is unchanged.
- An error in a `PaintProc` now surfaces at the first mark, with that
  operator as the offending command (gs does the same); `--max-memory`
  and a tile too large for memory too (the geometry limits still refuse at
  `makepattern`).
- A `PaintProc` that redefines names the program relies on changes what
  later code sees at first use rather than at `makepattern`, as in gs.
