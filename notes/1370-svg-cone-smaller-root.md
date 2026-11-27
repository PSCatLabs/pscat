## SVG: a radial whose circles are not nested paints its smaller root (issue #1370, 2026-11-27)

**The bug.** A radial shading whose circles are not nested (a cone,
`|c1 - c0| > |r1 - r0|`), with its end not extended, left a hole in the SVG
where the PNG, the PDF and gs paint the smaller root: a point can lie on two
circles, gs paints the larger `t` in range, and where that one is past an
end that does not extend, the smaller `t` shows. It was open since #1040 for
`shfill` and #1298 for shading patterns. Measured with resvg against our PNG
(`tests/svg_shading.rs`): 11,304 of 250,000 pixels for the grow cone below,
and a mean block difference of 11.0 for the pattern.

**Why SVG could not say it.** An SVG radial gradient paints the largest of
*its own* `t` that a point's circles reach (focal circle `t` 0, outer circle
`t` 1). PostScript's larger root needs SVG's `t` to rise with PostScript's;
the smaller root needs it to fall. For any cone, one of those two spellings
has the focal circle larger than the outer (`fr > r`).

**What was wrong beyond the hole.** `gradient_def` swapped the circles of any
shrinking radial (`r0 > r1`) so that `fr <= r`, reversing the stops. For nested
circles only one root is in range, so that was harmless. For a shrinking cone
it painted the smaller root where gs paints the larger. It was invisible
because nothing compared a shrinking cone; it now is (`cone-shrink`, all four
`/Extend`s).

**Does `fr > r` draw?** Probed before relying on it. Chrome 158 (Playwright)
draws it as canvas `createRadialGradient` does (0 of 21,124 painted pixels differ
from the canvas gradient; and 0 differ from our PNG over sixteen cone
programs under every `/Extend`, two apex pixels aside), and so does resvg.
WebKit (`qlmanage` thumbnail) differs from the PNG only on the cone's edge
pixels. Firefox was not checked. SVG 1.1 has no `fr` at all: a viewer
without SVG 2 radial gradients moves a focal point outside the circle onto
it, so the backward gradient's padding can flood the page in the start
colour, which is worse than the old hole, but those viewers were already
wrong for every cone.

**The fix** (`src/svg.rs`).
- A cone is never swapped (`is_cone`): the start is the focal circle, so the
  gradient is the larger root.
- `needs_smaller_root` (a cone whose end does not extend, the raster's own
  test in `gradient_parts`) writes a second gradient, `run_backwards`: the
  circles swapped, the stops mirrored, the ends exchanged. It goes through
  the same `extended_for_svg` and rescale. `gradient_defs` returns both ids in paint
  order, the smaller root's first.
- `shfill` paints them as two paths in one group that carries the mark's
  opacity and blend, so a translucent mark composites once as the raster's
  layer does.
- A shading pattern with no `/BBox` and no `/Background` is normally the
  gradient itself; a cone is two, so it takes the canvas `<pattern>` (as one
  with a `/Background` does). Its `<pattern>` is one canvas in size, as the
  Type 1 and `/BBox` cases already are (#1368 is that gap).
- An outer circle of radius 0 (a cone to a point, or from one, run backwards)
  would be SVG's "paint the last stop everywhere", so it gets a hair of
  radius. The hair moves the apex along the axis by hair x distance / radius
  change, so it is sized for a shift of `CONE_APEX_SHIFT` (1e-3 units),
  `|r0 - r1| x CONE_APEX_SHIFT / distance`, at least 1e-5. Codex round 1
  found that sizing it by the cone's reach put the apex 100 units off for
  circles 1e7 apart (`/Coords [10000000 250 1 160 250 0]`: gs leaves (160,200)
  gray; the SVG painted a region about 200 wide there). Fixed, resvg against
  the PNG: `a_cone_to_a_point_with_a_far_circle_keeps_its_apex_in_place`.
  **Known limit:** Chrome 158 paints the whole page in the last stop for that
  one program, with any hair from 1e-5 to 1 (its focal circle is 2e7 away and
  larger than the outer one), where the swapped spelling before this change
  was right in Chrome. There is no hair that satisfies both: it must be
  above the viewers' own tolerance and its apex shift is hair x distance /
  radius change. It needs a cone more than about 1e7 times longer than it
  is wide, so no follow-up is proposed beyond noting it here.

**gs** (10.07.1, 500x500 at 72 dpi, gray page, C0 red to C1 blue). Pixels
(250,250), (300,250), (150,250), (330,250), from the PostScript, against our
PNG, for `/Coords [160 250 20 260 250 60]` and `[260 250 60 160 250 20]`,
`/Extend [true false]` and `[false false]`:
- grow: gs (126,0,128), (35,0,219), (209,0,45), (127,127,127); ours
  (127,0,128), (36,0,219), (210,0,45), (128,128,128);
- shrink, `[true false]`: gs (128,0,126), (219,0,35), (44,0,210), (255,0,0);
  ours (128,0,127), (219,0,36), (45,0,210), (255,0,0).
`extend_reaches_the_pdf_and_gs_paints_it_as_the_raster_does` now also holds
the shrinking cone's PNG and PDF against gs.

**Limits.** The raster already charged a cone's two layers (three passes). The
second SVG gradient is one more tag a stop, up to 8190 for a 4095-leg ramp
however small the page: `shfill_limited` and `shading_layer` check
`SVG_STOP_BYTES` a stop against `--max-memory` before anything is painted and
charge `svg_second_gradient_work` to `--max-steps`, so a refused one leaves
the page and the SVG as they were (`limits.rs`, `limits_memory.rs`).

**Deferred.** A Type 1 shading pattern's SVG image past its canvas (#1368)
is unchanged. A cone's `<pattern>` canvas has the same reach: nothing past the
page can be seen in this recorder, but a pattern made in another context and
used past its canvas shows nothing there.
