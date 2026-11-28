## Patterns: a tile cell's canvases and makepattern's SVG image answer to the memory cap (issue #1477, 2026-11-27)

Follow-up of #1475 (#1368), which left two allocations unchecked under
`--max-memory`. Both are limits, not rendering: nothing a program under its
limits draws changes (PNG, SVG and PDF byte for byte).

**A tile's cell.** `begin_pattern` held only the capture canvas and the period
against the cap, and looked once, before the `PaintProc` ran. Now
(`pattern::cell_bytes`):

- at the start, everything the cell will take: the capture canvas, the `/BBox`
  clip's mask and the coverage mask the first fill makes (a byte a pixel each),
  the period and, for a TilingType 2 lattice, the oriented cell;
- at the end of the `PaintProc`, before `fold` and `orient_cell` make the
  period and the cell, again: the `PaintProc` has grown the heap since the first
  look. The look is made with the capture still open, and a refusal unwinds it
  as an error in the `PaintProc` does (`abort_pattern`: page, SVG and PDF as
  they were, the pattern runs again at its next use). Doing it after
  `leave_capture` would have left the PDF capture half ended.
- `finish_pattern` puts the operands back whether or not the tile was made. It
  returned on the error before, with the operand stack as the `PaintProc` left it.

**`makepattern` of a Type 1 or mesh shading** (`make_shading_pattern`), when
SVG is recorded: the same checks as the foreign-context image
(`Gfx::svg_use_image`): the layer's work and the encoding's against the step
limit before it runs, the encoder's heap against the cap before it runs, then
the recorder's write and the clock before the pattern is made. The encoding and
the write are in the work the operator charges, so a program's step count goes
up by about 32 units a pixel of its canvas (a step per KiB).

**The made image's mark.** The image `makepattern` made is written inline into
a `<pattern>` at the mark that first paints with it, which `svg_use_image` did
not look at (only the foreign image was). `pattern_source` now holds the cap
against `SvgRecorder::made_image_cost` (two copies of the base64 and the
defs' growth) before the mark records, and the recorder writes the base64
straight into the content instead of through a formatted copy (the same bytes).
A mark whose pattern the recorder wrote already (same hop, no pen) costs
nothing, so a hundred marks need no more room than one. A pen-space stroke
after it writes a copy of its own, which is not looked at (a follow-up).

**The encoder's bound was too low** (`png_encode_bytes`). #1475 held the heap
against 8 bytes a pixel for the encoder's copy and output. The png crate
(0.18) deflates the whole image into one vector and then copies it, in chunks,
into the output vector, so two vectors of the compressed size are alive and the
second is growing by doubling; with tiny-skia's clone of the pixels the peak is
4 + 5 z bytes a pixel for z bytes of compressed output a pixel. A layer is
opaque inside its shading, so z is its entropy: 3.4 measured for independent
random colours (alpha and filter bytes included), 3.1 at most in a shading.
Measured at eleven sizes from 300 to 2000 pixels a side, `encode_png` peaked
from 9.1 to 16.2 bytes a pixel, by where the doubling fell; the bound is 19.5
and both users now hold 20 a pixel.

**What it costs.** A capped SVG run now needs about 24 bytes a pixel of headroom
(the 4 of the layer and the 20) to `makepattern` a Type 1 or mesh shading on a
canvas, and to make a foreign use's image of one. `--max-memory` is a cap on a
whole process, so a cap near the heap an SVG run really takes refuses a
`makepattern` it would have finished. One test had to change for it:
`a_foreign_use_image_answers_to_the_memory_limit` (tests/patterns.rs) made a
page-sized pattern on the page at an 11 MiB cap, which `makepattern` now refuses
before the foreign use it was written to test; it makes its pattern in a 100 pt
tile (a small canvas) and uses it on the 1000 pt page instead.

**Tests.** `tests/pattern_use_image_memory.rs` (one test, its binary: the cap
counts the process): ladders of caps on a 2000 pt cell (TilingType 1, and 2 with
a fractional step, which keeps a lattice cell), on a `PaintProc` that grows the
heap by 8 MB and leaves three operands (the end-of-capture look: the PDF, SVG
and page as they were, four operands left), on `makepattern` of a noise shading
and on the mark that writes its image, with a hundred marks under the smallest
cap that holds one; each run's heap peak stays within half a MiB of its cap.
The twelve-tile allowance (3 MiB) is half a MiB now. `tests/patterns.rs`:
`makepattern` of a shading costs its encoding in steps and a budget between
refuses it with nothing changed. Each failed before the fix (the 2000 pt cell
by 4 MiB at a 38 MiB cap, the shading's encoding a 12 MiB cap by 5 MiB, the
twelve tiles a 32 MiB cap by 0.6 MiB past the new margin, the encoding's steps).

**Not covered.** The coverage mask a fill makes is checked where a capture
begins (the cell's), not where each fill makes it, so a `PaintProc` that grows
the heap before its first fill is looked at by the end-of-capture check only
for the period. An uncoloured pattern's recoloured copy of the period
(`recolor`) is made at a fill without a check.
