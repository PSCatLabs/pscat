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
current path (`PsPath::bbox`), kept in device space and widened by 3/256 on
every side; a point added past it is a `rangecheck`, compared in 1/256 steps
like gs's fixed-point path; `pathbbox` answers it; `newpath` forgets it and
`grestore` gives back the one `gsave` kept. The operator joins the path's own
points and any box already set; a user path's own box does not, so
`50 50 moveto { 0 0 10 10 setbbox 50 5 lineto } uappend` is a `rangecheck`.
Enforced by `moveto`, `lineto`, `rmoveto`, `rlineto`, `curveto`, `rcurveto`,
`arc`, `arcn`, `arcto` and `arct` (the last three whole or not at all), not by
`rectfill`, `charpath` and the other builders.

`uappend` runs the array as those operators, against a path that has lost
its current point (a user path starting with a `lineto` is a `rangecheck`, not
`nocurrentpoint`), and keeps the box only while it runs. Any failure, a
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

- `near` scans every segment (polled, memory in view through the probe, the
  whole count against the step limit at the end), and the hit test returns
  that count as work.
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

### Not changed

`flattenpath` and `reversepath` do not carry a `setbbox` box over (gs does,
oddly, for `reversepath`; `flattenpath` is a `rangecheck` there). gs also
drops a current path that lies outside a user path's box when it is
appended; pscat appends and ignores the old points. `executeonly` and
`noaccess` do not exist for arrays here, so gs's `invalidaccess` on a user
path with them has no counterpart.
