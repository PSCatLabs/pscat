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
The check is made at every mark that uses the made image, a cached one too, as
the recorder does not say which pen the mark will have: on purpose
conservative, only near the cap.

**The encoder's bound was too low** (`png_encode_bytes`). #1475 held the heap
against 8 bytes a pixel for the encoder's copy and output. Measured on a
1000 x 1000 image of independent random colours, which deflate barely shrinks,
`encode_png` peaks 11.0 bytes a pixel past its layer (the copy of the pixels, 4,
and the output vector growing by doubling). Both users now hold 12 a pixel.

**Tests.** `tests/pattern_use_image_memory.rs` (one test, its binary: the cap
counts the process): a ladder of caps on a 2000 pt cell, TilingType 1 and 2,
and on `makepattern` of a noise shading; each run's heap peak stays within half a
MiB of its cap, and a refused run leaves the SVG, the page and the operand
stack as they were. The twelve-tile allowance (3 MiB) is half a MiB now.
`tests/patterns.rs`: `makepattern` of a shading costs its encoding in steps and
a budget between refuses it with nothing changed. Before the fix: the 2000 pt
cell passed a 32 MiB cap by 2.3 MiB, the shading's encoding a 12 MiB cap by
5 MiB, and the twelve tiles a 32 MiB cap by 0.6 MiB past the new margin.

**Not covered.** The coverage mask a fill makes is checked where a capture
begins (the cell's), not where each fill makes it, so a `PaintProc` that grows
the heap before its first fill is looked at by the end-of-capture check only
for the period. An uncoloured pattern's recoloured copy of the period
(`recolor`) is made at a fill without a check.
