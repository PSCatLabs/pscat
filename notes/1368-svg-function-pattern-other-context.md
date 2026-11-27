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
or records anything, and Codex round 1 on PR #1475 (R1 to R3) tightened what is
looked at before the entry is made:
- **One cost with the mark's layer (R1).** The image is held to the steps the
  limit has left after the mark's own layer, not to the whole budget again, so a
  full-cell fill whose layer and image each fit but not both stops with the page,
  the SVG and the cache as they were.
- **The encoding (R2).** tiny-skia's PNG encoder copies the pixels and writes its
  output, which deflate bounds by about the input. Both are held against
  `--max-memory` (8 bytes a pixel, with the layer still live) and the encoding is
  charged to `--max-steps` (`PNG_ENCODE_UNITS` a pixel) before it runs; the clock
  and the cap are polled after it, before the entry is kept.
- **The serialization (R3).** The recorder writes an image as an `<image>` def of
  its own, once per recorder (`SvgRecorder::image_def`, by the image's address),
  and each `<pattern>` (one per pen, per placement) shows it with `<use>`. Before,
  every `ShadingKey` wrote its own base64 copy, and the base64 was copied twice
  more on its way into the defs. The base64 is appended to the defs directly,
  reserved once. What the def will take (its base64, the growth of the defs
  string, the id table) is `SvgRecorder::image_cost`, held against the cap before
  the mark is recorded, on a cache hit too when this recorder has not written the
  image yet, and charged as work. The cache's own growth is checked the same way,
  `None` entries included.
- The entries live on the pattern for the run. That is the process's heap, so
  `--max-memory` counts them: twelve 1000 pt cells each using a noise-table page
  pattern write twelve images, and caps of 32 and 48 MiB stop the run partway
  with the heap no more than 3 MiB past the cap, which is the cell machinery's own
  (a period and a mask a cell makes, unchecked before this change). Before, the
  same run overshot by 3.4 MiB at 48 MiB.
- The image never exceeds the canvas it is for, whatever the shading's Domain
  (a box of 1e30 is cut to the canvas).
- **Tests.** `tests/patterns.rs`: a step budget found by binary search (the
  fewest steps the page-level fill needs), against which a foreign full-cell fill
  must stop at 1.5 times it; sixty marks finish within a fifth of one image's
  steps of one (the image is rasterised once); pen variants write one image;
  the clock, at every budget of a ladder, leaves a use finished or untouched.
  `tests/pattern_use_image_memory.rs` (a binary of its own: the cap counts the
  process): the heap's peak, as a sampler thread sees it, against the cap, for a
  16 MB image (R2), twelve contexts and eight pens (R3). With the image disabled
  the step cases no longer stop. The memory caps of the earlier test sit midway
  between the flat tile's threshold (9 MiB) and the foreign use's (13 MiB).

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
