## `pathbbox` under a rotated CTM (issue #288, 2026-10-13)

Found by #264 (`strokepath`). `pathbbox` mapped every path point back to user
space and took the min/max (the tight user-space box). Ghostscript, and the
PLRM's wording ("the bounding box of the path in the *current* user space",
computed from the device-space box), take the device-space bounding box first
and map its four corners back through the inverse CTM. Under rotation or skew
that box is larger:

```
200 200 translate 30 rotate newpath 0 0 moveto 100 0 lineto 100 50 lineto pathbbox
gs, now pscat: 0 -43.30 121.65 80.80       before: 0 0 100 50
```

- **Fix.** `Gfx::path_bbox` (src/gfx.rs) takes the device bbox of every path
  point (curve control points included, as before: gs does too, `flattenpath`
  first tightens it), maps all four corners with the inverse CTM and min/maxes
  them. All four corners, not two: a flip, a 90 degree turn or a negative scale
  swaps which corner is the minimum. An axis-aligned CTM gives the same numbers
  as before. Empty path stays `nocurrentpoint`; a lone moveto reports that point
  in both (gs-pinned).
- **Verification.** `tests/pathbbox.rs`: gs-pinned values (run in CI, which has
  no Ghostscript) plus a live matrix, 12 CTMs (identity, translate, rotations
  30/45/90/-20, anisotropic scale, scale+rotate, rotate+scale, two skews, flip)
  x 10 paths (lines, curve, flattened curve, lone moveto, `moveto closepath`,
  circle, two subpaths, empty, `strokepath` of lines and of a curve). Plain
  paths agree with gs to 0.05 (gs is fixed point, ~2e-4 noise); flattened
  curves to 0.5 (different flattening); the `strokepath` of a curve to 4.0 (the
  stroker's offset-curve control points differ from gs already at the identity
  CTM, 2.4 units at width 8; that is the stroker, not this change). Stroke
  outlines under anisotropic CTMs are skipped: the round-pen deviation
  (HANDOFF.md) moves the outline itself. #264's rotated raster-only case in
  `tests/strokepath.rs` is now a bbox comparison like the rest (0.05, with
  dashes).
- **Not changed, filed as #316: the trailing moveto.** gs ignores a `moveto`
  that is the last segment (unless the path is only that moveto);
  `charpath` leaves one at the pen's advance point, so gs reports ink width
  where pscat reports advance width. Matching it was tried: it changes 8
  renders through `lettering`'s `ltaddpath pathbbox` (the glyph box shrinks, so
  its patch count and rand stream move): examples/apparel_second, apparel_shirt,
  groovy_faces, keepin_it_real_shirt, keepin_it_real_shirt_chewy, lettering,
  fillkit and gallery/keepin_it_real, and so the committed art/ and
  gallery/renders stills. That is a separate decision, so #288 only does the
  rotation. `tests/pathbbox.rs` pins pscat's current answer as a known
  deviation.
- **Render impact: none for committed renders; one prototype's printed numbers
  move.** Release binary before vs after over all 115 tracked non-lib .ps
  (examples, gallery, selftest drivers, docs; `art/` holds no .ps, its stills are
  built from examples/): every PNG byte identical. `scripts/identity_check.sh
  --loader` (with a wrapper binary that picks old/new by directory) for artkit
  (82 pages), drykit (4), paintkit (36), fillkit (9), hatchkit (3), halftonekit
  (2), printkit (2) and lettering (9): all identical. `path_bbox` has one
  caller, the `pathbbox` operator, so no recorder (SVG viewBox, PDF box) reads it.
  An instrumented build that logged every `pathbbox` whose answer changed found
  only two corpus programs with a rotated or skewed CTM at the call:
  examples/apparel_second.ps (3 calls; PNG, SVG and PDF all byte-identical to
  before) and docs/drymedia/prototypes/surface_cost.ps (`dqclip`, 1422 calls).
  The prototype's PNG is identical, but its SVG/PDF differ (extra clipped
  cells) and its printed bounding-box catch cost rises about 4% in modes 1-3
  (MODE 1/2: 387825 -> 403750 cells, MODE 3: 815003 -> 846631; modes 4-6 and the
  band estimate unchanged). That is the prototype's rotated "footprint box grown
  by half the line width", the stand-in `strokepath` (#264) was built to replace;
  DRYMEDIA.md's quoted figures (553k band estimate) are not among the moved
  numbers, so no doc was edited.
- **Caller audit** (acceptance criterion 2). Nobody needs the tight box, so no
  tight-box helper was added. A looser user-space box is a superset of the
  tight one, and every caller uses it as a clipped region, a lattice sweep
  range or a scatter range: hatchkit and halftonekit's default `/BBox` (more
  lattice cells outside the clip), paintkit's wash region (line 5610, "a
  slightly generous region costs nothing but a few rejected candidate points"),
  toon's `dotfill`, printkit's presets, lettering's glyph box (its patch and
  speck counts scale with the box area, so a rotated lettering call would
  draw a few more patches than before), apparel's `clippath pathbbox` (a
  page-level, axis-aligned device rectangle) and drykit:347, which sets the
  canvas matrix first. The comment-only mentions (headline, jersey, drycrayon,
  strokeglyphs) walk their own paths and do not read `pathbbox` under rotation.
  A library that wants the tight box under rotation should `flattenpath` and walk
  `pathforall` (as `headline`'s `hlink?` and `jersey`'s `jybbox` do) or work
  in the canvas matrix first.
