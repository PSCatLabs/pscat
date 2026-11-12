## arcto and arct (issue #980, 2026-11-11)

**Built.** `x1 y1 x2 y2 r arcto` -> `xt1 yt1 xt2 yt2` and `x1 y1 x2 y2 r arct`
(PLRM 3rd ed. §8.2), in `src/ops/graphics.rs`. The geometry is a pure function
(`arcto_geometry`) in user space: the tangent points are `p1 +
r/tan(corner/2)` along the two legs, and the arc is one cubic with control
points `t - r*k*u` (`k = 4/3 tan(sweep/4)`, `sweep = pi - corner`). The path
goes through `Gfx::lineto` and `curveto`, so any CTM, including a non-uniform
one, bends the circle into an ellipse with no change to `gfx.rs`. `arc` is
untouched.

**Pinned against Ghostscript 10.07.1** (`tests/arcto.rs` runs both and compares
operand stack and `pathforall`, and rasterises two rounded rectangles in each):

- One cubic per corner however far it sweeps (an acute 174 degree corner is
  still one segment); `arc` splits at 90 degrees, `arcto` does not.
- Collinear points (ahead, folded back, or p2 = p0): a line to (x1, y1) only,
  tangent points (x1 y1 x1 y1).
- `r = 0`: the line plus a zero-length cubic at (x1, y1), same tangents.
- `r < 0`: the circle mirrored through the corner (tangents on the far side).
- p0 = p1 or p1 = p2: `undefinedresult`. No current point: `nocurrentpoint`,
  checked after the operands (a bad operand wins).
- `arct` is `arcto` without the four results.

**The decision follows gs's `gs_arcto`, with no tolerance.** The legs are
exactly collinear (cross product 0) or they are a corner; a zero leg is
`undefinedresult`; a corner whose tangent points leave the device's
coordinate range is `limitcheck`, with gs's fixed-point limit (2^31/256
device units at 72 dpi; pscat measures device pixels over pixels per
point). So the hairpin `0 0 moveto 100 0 -50 0.0001 10 arcto` is
`limitcheck` in both, and a 0.0001-long leg, a far p2, a magnifying,
shrinking, sheared or non-uniform CTM all keep their corner. The combined
Codex review (batch 1) found a straightened corner in six rounds under a
tolerance; this replaced it. `tests/arcto.rs`'s sweep checks gs agreement
over 32 transforms and 11 well-conditioned geometries, CTM invariance of
the user-space answer, and sanity where rounding decides.

**Deviations from gs.** (1) A failed call consumes its operands, the same as
`lineto` and every other pscat operator (HANDOFF.md, "Deliberate deviations");
gs leaves them. (2) Where rounding decides, the two differ in noise. pscat
keeps the current point in f32 device space (read back with the inverse CTM
in f64), gs on a 1/256-unit fixed-point grid. So a short leg under a
rotation, an exact fold-back from a point other than the origin under a
rotation (a line or `limitcheck`, by rounding, in both), a 1e-3 scale, or a
near fold-back can give different tangent points or a different answer;
neither is the exact geometry there, and pscat is usually the closer.
p0 = p1 is also compared where pscat stores the current point (the same f32
transform `lineto x1 y1` would store), so a rotation cannot hide it: gs,
with its grid, returns a curve for some rotations of that case.

**Not done.** `--capabilities` lists no PLRM operators by design
(`src/capabilities.rs`), so neither operator is added there. No `%%SelfTest`
block: those belong to `lib/*.ps` files, and the cargo test covers this.

**Review render and selftest driver.** `selftest/drivers/arcto.ps` draws
rounded rectangles with `arcto` (tangent points in red), a pill, a
non-uniform-scale one, an `arct` polygon, and the collinear, zero-radius,
negative-radius and acute cases; `scripts/selftest.sh` strict-lints it.
`notes/980-arcto-arct/pscat-left-gs-right.png` is pscat (left) beside
Ghostscript (right): the shapes coincide, only stroke antialiasing differs.

**Verified.** Two-binary identity check (`--base-bin/--head-bin`) over all
tracked `examples/*.ps` and `selftest/drivers/*.ps`: 159 pages identical, the
control (the new driver) differs as it must. No tracked `.ps` in `lib/`,
`gallery/` or `fuzz/` mentions `arcto` or `arct`, so no existing program can
reach the new operators. SVG and PDF exports are pinned in `tests/arcto.rs`
(the SVG path text, and gs rasterising our PDF against our canvas).
