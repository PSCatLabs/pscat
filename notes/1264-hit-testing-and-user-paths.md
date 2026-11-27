## Hit testing and user paths (issue #1264, 2026-11-27)

`infill`, `ineofill`, `instroke`, `inufill`, `inueofill`, `inustroke`,
`uappend`, `upath`, `ufill`, `ueofill`, `ustroke`, `ustrokepath`, `ucache`
and `setbbox` were undefined (the 2026-11-18 completeness audit). They are
operators now, in `src/ops/upath.rs`, each case pinned against gs 10.07.1 in
`tests/upath.rs` (182 programs, re-run through gs where it is installed).

**Left for follow-ups (they raise `typecheck`):** the forms of the hit tests
whose first operand is a user path in place of an `x y` point (`userpath
infill`, `userpath1 userpath2 inufill`, ...: gs answers whether the two
regions share a pixel), and encoded user paths (a data array or string with
an operator string). `ucache` is a no-op, as in gs.

### What a user path is

An array (literal or executable, packed or not) of numbers and the operators
`moveto lineto rmoveto rlineto curveto rcurveto arc arcn arct closepath`,
led by `[ucache] llx lly urx ury setbbox`. The operators are executable names
or operator objects (a `bind` makes them those); a literal name is a
`typecheck`, as in gs. Anything else, a wrong operand count, a `ucache` that
is not first, a second `setbbox`, an empty array: `typecheck`. `arcto` is not
one of them.

`setbbox` is a real operator, as it is in gs: the box is a property of the
current path (`PsPath::bbox`), kept in device space, rounded outward onto
gs's 1/256 grid (`0.001 0.001 10.001 10.001 setbbox pathbbox` is `-3 -3 2564
2564` in 256ths) and then widened by 3/256 on every side; a point added past it is a `rangecheck`, compared in 1/256 steps
like gs's fixed-point path; `pathbbox` answers it; `newpath` forgets it and
`grestore` gives back the one `gsave` kept. The operator joins the path's own
points and any box already set; a user path's own box does not, so
`50 50 moveto { 0 0 10 10 setbbox 50 5 lineto } uappend` is a `rangecheck`.
Enforced by `moveto`, `lineto`, `rmoveto`, `rlineto`, `curveto`, `rcurveto`,
`arc`, `arcn`, `arcto` and `arct` (the last three whole or not at all), not by
`rectfill`, `charpath` and the other builders.

`uappend` runs the array as those operators, against a path that has lost
its current point (a user path starting with a `lineto` is a `rangecheck`, not
`nocurrentpoint`), and keeps the box only while it runs. It also takes away a
pending lone `moveto`, as gs does, even for a user path that is a box alone
(`1 1 moveto { 0 0 10 10 setbbox } uappend` leaves no path element, the
current point 1, 1, and a `lineto` after it is `nocurrentpoint`;
`PsPath::can_extend`). Any failure, a
limit included, takes everything it added back (`PsPath::mark`/`rollback`,
which also undoes a `moveto` that overwrote a trailing `moveto`).

`upath` writes the current path as such an array of executable names: the
`pathbbox` as `setbbox` (the empty path is `0 0 0 0 setbbox`), a
`moveto` put back before a `lineto` or `curveto` that follows a `closepath`,
as `pathforall` shows it, `ucache` first for a `true` flag.

`ufill`, `ueofill`, `ustroke` and the in-operators run the user path on a path
of their own, swapped for the current path and swapped back, and the CTM too
(`ufill` is `gsave newpath userpath uappend fill grestore`). The matrix of
`ustroke`, `ustrokepath` and `inustroke` is the top operand when it is an
array of six numbers (any other array is a user path, as gs reads them), and
shapes the pen as `matrix concat` after the path is made; `inustroke`'s
point is in the space the operator was called in. `ustrokepath` keeps the
outline. A singular CTM strokes nothing.

### Hit tests

`x y infill` is true when pscat's own `fill` would paint the device pixel the
point falls in (`floor` of its device coordinates), whatever the clip, the
page and `nulldevice` say, and without painting. The implementation is the
area-coverage rasteriser `fill` uses (`coverage::rasterize_limited`), on a
1 x 1 mask with the path shifted so the pixel is at the origin, after the
subpaths that cannot reach the pixel are left out (`PsPath::near`: a closed
subpath's winding is zero outside its box, so this is exact for fills). A
pixel the path covers by less than 1/255 is not painted by `fill` and is not a
hit. `instroke` strokes the subpaths that can reach the pixel (the pixel grown
by half the widest pen times the miter limit or a square cap's corner) with
tiny-skia's stroker, the way `strokepath` does, a group of subpaths at a
time, and fills the outline nonzero. `Gfx::stroke_path` became
`stroke_outline_of(&PsPath)` for it.

gs agrees on every probe pinned in the tests, with stroke adjustment off
(`false setstrokeadjust` in the test prelude). With it on, which is gs's
default, gs moves a stroke's edges to pixel boundaries and also answers for
the pixel an edge only touches (`30 8 instroke` on a 4-wide line at y 10 is
true in gs, false in pscat, which does not adjust). A zero-area fill (`10 10
moveto 20 10 lineto closepath`) is a hit in gs and nothing here: `fill`
paints nothing for it either.

### Limits

Every loop over user data is polled, charged or priced before it runs
(`FLATTEN_POLL` = 4096 segments, a step per 1024):

- `near` scans every segment (polled, the whole count against the step limit
  at the end), and the hit test returns that count as work. Every copy asks
  the heap first (Codex, PR #1460): `near` for each subpath it keeps, `groups`
  for each piece it hands to the stroker, `to_skia_shifted` for the builder it
  makes (at its final size) and the shifted copy of a stroke outline. A refusal
  leaves the operands, the path, the CTM, the graphics state and the `setbbox`
  box as they were (`tests/limits_memory.rs`, which watches the heap while the
  operator runs).
- The rasteriser looks at the limits as it flattens and lays edges, as in
  `fill`.
- tiny-skia's stroker and dasher cannot be stopped. Each group handed to
  them is priced first (`Gfx::stroke_cost`: a line 8 units, a curve 64, a
  dash 2, and a dash count made from the path's length over the pattern's
  period, so a tiny pattern on a long path is millions of dashes; the heap
  the outline can take, 256 bytes a line, 1 KiB a curve, 128 a dash; measured
  on 2,000 segments, the finished outline is 64 bytes a line and 210 to 511 a
  curve, with the stroker's copy before it about as much again) and held
  against the step limit and the heap cap. A group is about 32,768 units.
  What a group made, not what it was priced at, is charged as well: the
  outline's elements in steps and against the heap, before the outline is made
  into a path and kept (Codex, PR #1460: a curve of 1e6 under a dash of 1e9 is
  priced at 68 units and makes 5,119 elements, a pen 1e6 wide on a curve of 1e8
  is priced at 64 and makes millions), so the sum over the groups is bounded
  though one call may overshoot its estimate. The per-curve price stays fixed:
  how many elements tiny-skia's stroker makes for one curve depends on the pen
  width, the curve's extent and the stroker's own limits in ways that do not
  reduce to a formula this code could trust (the two programs above differ by a
  factor of 500 in output for the same price), so a better guess would only
  move the failure. The single call that overshoots has already made its outline
  by the time it is charged, which is the open part of the limit.
  `strokepath` and `ustrokepath` do the same, a group at a time
  (`stroke_path_limited`, the outlines joined in order; `strokepath` had no
  limit before), polling and pricing between groups. What stays open: the
  time of one stroker call on one subpath of very many segments or curves,
  which is priced but not interruptible.
- `uappend` asks the heap cap for 64 bytes an element before it starts, looks
  at the limits every 4096 elements, and charges an arc by its segments
  (`arc_apply`, as `arc` does). `upath` asks for ten objects a segment first
  and polls as it writes.

Tests: `tests/upath.rs` (gs pins, a render agreement check of `infill` and
`instroke` against `fill` and `stroke`, far subpaths), the `issue_1264`
module of `tests/limits.rs` (a million segments, a million move/close-only
subpaths, huge curves, a long user path, a ten-turn arc, the stack and path
and CTM left alone), and unit tests in `src/gfx.rs` (`near`'s polling over
move/close-only subpaths, `groups`, rollback, stroke pricing).

### Deliberate differences from gs

Each program is pinned on both sides in `tests/upath.rs` (`DEVIATIONS`).

- **Curved hit tests (S3).** `newpath 10 10 moveto 20 35 30 5 40 30 curveto
  closepath 15 19.5 infill` is `true` here and `false` in gs (default
  flatness); `1 setflat newpath 50 50 20 0 360 arc closepath 2 setlinewidth
  43.5 29.5 instroke` likewise, and `100 setflat` turns gs's answer to `true`
  and changes nothing here. gs tests the curve as it flattens it for `setflat`;
  pscat tests what its own `fill` and `stroke` paint, and does not follow
  `setflat` (it never has). That agreement is the design.
- **Singular matrix, `ustrokepath` (S4).** `newpath 2 setlinewidth { 0 0 100
  100 setbbox 10 10 moveto 20 10 lineto } [0 0 0 0 0 0] ustrokepath pathbbox`:
  gs gives a one-pixel outline (y 9.5 to 10.5), pscat the hair of 0.1 device
  pixels every thin stroke gets (y 9.949 to 10.051). The hit tests treat a
  singular CTM as drawing nothing, as gs does.
- **Arc control points (S5).** `{ 0 0 100 100 setbbox 50 50 20 0 90 arc }
  uappend` has two control-point coordinates one 1/256 from gs's (15628, not
  15627, in 256ths); the plain `arc` operator is the same. gs rounds the path
  to a 1/256 grid after computing the curve, pscat does not.
- gs drops a current path that lies outside a user path's box when it is
  appended; pscat keeps it.
- `flattenpath` and `reversepath` do not carry a `setbbox` box over (gs does,
  oddly, for `reversepath`; `flattenpath` is a `rangecheck` there).
  `rectfill`, `charpath` and the other builders do not enforce the box.
- `executeonly` and `noaccess` do not exist for arrays here, so gs's
  `invalidaccess` on a user path with them has no counterpart.
