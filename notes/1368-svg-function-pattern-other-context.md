## SVG: a Type 1 or mesh shading pattern paints in a context it was not made in (issue #1368, 2026-11-27)

**The bug.** `makepattern` of a Type 1 (function-based) or mesh shading
pattern wrote its SVG image over the canvas it ran on, and a use in another
context saw nothing past it. Two shapes reach it: a page pattern used in a
`PaintProc` whose cell is larger than the page (or lies partly off it), and a
pattern made inside a `PaintProc` and used on the page. The PNG, our PDF
(through hayro and gs) and gs on the program all paint the shading there. Same
for a pattern made on one page and used on a later page of another size.
Measured, mean block difference out of 255 against our PNG through resvg:

| picture | SVG before | SVG after |
|---|---|---|
| Type 1 page pattern in a 600 pt cell at `[1 0 0 1 -300 -100]` | 69.1 | 0.03 |
| the same with a Type 4 mesh | 99.0 | 0.05 |
| Type 1 made in an 80 pt tile, used on the page | 72.2 | 0.00 |
| Type 1 reused on a 600 pt page after a 400 pt one | 27.3 | 0.00 |

The PNG against gs 10.07.1 (the program, `-dMaxPatternBitmap=1` for the made-
inside picture) is 0.2 to 0.6 for all three, our PDF against the PNG 0.1 to 0.6.

**Why the tile's own picture was right.** A page pattern's hop (#1300) puts the
tile's canvas at the top-left of the page, in gs's accumulator coordinates, so
a tile smaller than the page reads shading the page's canvas holds. Only a
tile bigger than the page, or the other direction, leaves the image short.

**The fix.**
- `SvgShading` keeps the canvas its image was cut to and the shading's device
  box, cut to the `/BBox` (`shape`; `None` for a gradient, which SVG draws
  itself).
- `ShadingPattern::svg_use_key(hop, canvas)` says whether a mark needs an
  image of its own: the use is in another context (a hop that is not the
  identity) or on a canvas of another size. Used in its own context on its own
  canvas the key is `None` and the output is byte for byte what it was.
- The mark that needs the image makes it, in `Gfx::pattern_source` (the one
  gate every fill, stroke, show and imagemask passes before anything is
  recorded), as `shading_layer` over the using canvas, in that context's
  device space (`hop * matrix`), for the box's reach on it. One per key, held
  in `ShadingPattern::svg_uses`. The recorder only reads it; with no entry (a
  route that never made one) it falls back to the made image, so the worst
  case is the old output.
- `SvgRecorder::shading_ref` writes the image outside the hop group (it is
  already in the recorder's coordinates, so no inverse round trip), cut to the
  `/BBox` corners as this recorder sees them. The background and a gradient
  keep going through the hop group. The `ShadingKey` carries the canvas size.

**Limits.** The image is made under the mark's own probe, before the mark paints
or records anything.
- Work is `shading_layer`'s (a step per 1024 units, held against `--max-steps`
  and polled against `--max-time` while it paints); its memory is held against
  `--max-memory` before the layer is allocated, and the encoded PNG against it
  twice (what the entry keeps, and the base64 the recorder writes).
- A stop leaves no entry, and the SVG and the page as they were.
- The entries live on the pattern for the run. That is the process's heap, so
  `--max-memory` counts them: sixty tiles each using a noise-table page pattern
  write sixty images, and a 40 MiB cap stops the run partway
  (`the_images_of_many_foreign_uses_add_up_against_the_memory_limit`). A tile
  of a pattern is one context, so many marks in it share one image.
- The image never exceeds the canvas it is for, whatever the shading's Domain
  (a box of 1e30 is cut to the canvas).
- Each limit case in `tests/patterns.rs` has a control, the same tile painting
  a flat colour under the same limit, which finishes; with the image disabled
  the steps and time cases no longer stop, so they are about the image. The
  memory caps sit midway between the flat tile's threshold (9 MiB) and the
  foreign use's (13 MiB).

**Known limit.** The image is in the using context's pixels, which is what the
PNG paints, but a viewer that scales a tile's `<pattern>` up resamples it. That
holds for every image in a pattern and is unchanged.

**Identity.** PNG: all 173 tracked examples, gallery pieces and drivers (212 page
renders, 4 files skipped that fail identically in both trees) render byte for
byte alike with the base binary and this branch's, and the three pictures above
too (`scripts/identity_check.sh` with `--base-bin/--head-bin`). The change has
no PNG-visible behaviour, so there is no PNG control: the two binaries are told
apart by the SVG of the pictures above instead (same PNG, different SVG). SVG:
of the tracked programs only `examples/shading_patterns.ps` uses shading
patterns, and its SVG is byte for byte the same (it has no foreign use).
