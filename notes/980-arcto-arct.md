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

**Deviations from gs.** (1) A failed call consumes its operands, the same as
`lineto` and every other pscat operator (HANDOFF.md, "Deliberate deviations");
gs leaves them. (2) "Collinear" is a 1e-6 perpendicular distance relative to
the coordinates' size, because the current point is read back through f32
device space; a rotated fold-back would otherwise ask for a tangent length of
~1e8. gs raises `limitcheck` for a near-fold-back (angle under about 1e-5 rad)
where pscat either draws the large geometry or, inside the tolerance, a line.

**Not done.** `--capabilities` lists no PLRM operators by design
(`src/capabilities.rs`), so neither operator is added there. No `%%SelfTest`
block: those belong to `lib/*.ps` files, and the cargo test covers this.

**Review render.** `notes/980-arcto-arct/roundrects.ps` draws rounded
rectangles with `arcto` (tangent points in red), a pill, a non-uniform-scale
version, an `arct` polygon, and the collinear, zero-radius, negative-radius and
acute cases. `pscat-left-gs-right.png` is pscat (left) beside Ghostscript
(right); the shapes coincide, only stroke antialiasing differs.
