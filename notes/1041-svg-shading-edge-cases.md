## shfill: the SVG's edge cases match the PNG; equal concentric circles decided (issue #1041, 2026-11-19)

#1028 (PR #1031) fixed five degenerate-geometry cases in the PDF writer. This
issue checked the SVG writer against the PNG for the same cases, under each
of the four `/Extend`s, by rasterising the SVG with resvg
(`tests/svg_shading.rs`).

**Before** (pixels more than 24 levels off, out of 250,000):

| case | `/Extend` | SVG |
|---|---|---|
| equal, nearly equal, huge equal circles | start extends (end, for circles shrinking by a hair) | the whole page in C0 or C1, where the PNG paints #1040's disk (up to 179,312 px) |
| `/Coords [0 0 1e-7 0]` under `1e8 scale`, axial and radial | all | flattened to one colour: resvg has tiny-skia's 2^-15 tolerance on the raw numbers (4,500–250,000 px) |
| radial with radii near 1e38 | an end not extended | `extended_for_svg` lengthened it past f32::MAX: `r="inf"` (62,828–250,000 px) |
| hard edge at either end of the axis | all | matched under resvg, but wrote two stops at offset 1 |
| axis that collapses in f32 | all | matched |

**What changed** (`vector_shading`, `src/gfx.rs`; `SvgRecorder::shfill`,
`src/svg.rs`):

- The PDF branch's preparation is now a free function used by both
  recorders: the SAFE_LEN rescale with its inverse in the transform, the
  degenerate radial recorded as the raster's disk (or nothing), and the
  collapsed axis as a solid fill (or nothing). The SVG writes the disk as a
  radial from the centre, one colour, transparent past the radius, and the
  solid case as a plain `<path>` fill. PDF output is unchanged byte for byte.
- If lengthening the gradient for an unextended end is not finite, it is
  done at an eighth of the size, with `gradientTransform` multiplying it back
  (a power of two, so exact). If that still is not finite, nothing is
  written, as the PDF does.
- A hard edge at an end that pads is never two stops at offset 0 (or 1).
  The inner stop moves in by 1e-6 (the PDF's `EDGE`), or onto its neighbour
  if that is nearer. Chrome 158 and resvg pad with the outermost stop either
  way. #1040 found renderers differ on duplicate terminal stops, so this is
  a guard, and it writes nothing new for a gradient without such an edge.

After: every case under every `/Extend` matches (0 pixels off), and no SVG
carries `inf` or `NaN`.

### Equal concentric circles: the decision

`/Coords [x y r x y r]`: one circle, twice.

**The specs do not define it.**

- PLRM 3rd ed., §4.9.3 "Type 3 (Radial) Shadings", Table 4.14 and pp.
  269–270, and ISO 32000-1:2008 §8.7.4.5.4 (Table 81, p. 187), say the
  same thing: the blend circles are `r(s) = r0 + s × (r1 − r0)`, "painted
  in order of increasing values of s", and "if a point lies within more than
  one blend circle, its final colour shall be that of the last of the
  enclosing circles to be painted".
- On extension, both say: "If the shading is extended at the smaller end,
  the family of blend circles continues as far as that value of s for which
  the radius of the blend circle r(s) = 0. If the shading is extended at the
  larger end, the blend circles continue as far as that s value for which
  r(s) is large enough to encompass the shading's entire bounding box."
- With `r0 = r1` and one centre, every blend circle, extended or not, is the
  same circle. No end is smaller or larger, r(s) never reaches 0 or grows,
  and the family sweeps no area.

**What gs does** (gs 10.07.1, 500x500 at 72 dpi, gray page,
`/Coords [250 250 150 250 250 150]`, C0 red, C1 blue; pixels at the
centre, r=149, r=150, r=151 and a corner):

```
[false false] [(127,127,127), (255,0,0), (127,127,127), (127,127,127), (127,127,127)]
[true false]  [(255,0,0),     (255,0,0), (127,127,127), (127,127,127), (127,127,127)]
[false true]  [(127,127,127), (255,0,0), (127,127,127), (127,127,127), (127,127,127)]
[true true]   [(255,0,0),     (255,0,0), (127,127,127), (127,127,127), (127,127,127)]
```

That is a disk of C0 when the start extends. When it does not, gs paints a
one-pixel ring of C0 on the circle itself, through its any-part-of-pixel
fill rule. That ring is the "thin ring" in the issue; #1040's note called
that case "nothing", which was not quite right. Outside the circle, gs paints
nothing in every case.

CoreGraphics (macOS Preview) draws such a PDF shading as nothing (#1028's
note). Acrobat was not available to check.

**pscat** (since #1040, kept here): a disk of C0 when the start extends
(of C1 when the end extends and the circles shrink by a hair, as gs does),
else nothing; nothing outside. PNG, SVG and PDF all agree.

**The one deviation from gs is the ring.** pscat does not paint a hairline
for the unextended case: the circle has no area, the spec gives the shading
none, and an anti-aliased zero-width curve covers no pixel. That case is a
one-pixel outline in gs, and nothing in a content stream depends on it.

`equal_concentric_circles_paint_the_start_disk_only_when_the_start_extends`
pins all four `/Extend`s in the PNG and the SVG; `tests/pdf.rs` pins the
PDF against gs.
