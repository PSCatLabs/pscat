## Zero-length segments inside a stroked subpath (issue #1224, 2026-11-26)

A subpath that *starts* with a zero-length segment and then continues, such as
`4 setlinewidth 2 setlinecap 25 25 moveto 25 25 lineto 60 40 lineto
strokepath pathbbox`, gave `[23 21.97 62.6 42.6]` in pscat and `[22.38 22.38
62.62 42.62]` in gs 10.07.1. This is the "Not changed" item of
`notes/1215-square-cap-zero-length-subpath.md`. It reproduced on
`feature/0.8.0`, and it is wider than square caps.

**What gs does** (10.07.1, `false setstrokeadjust`, `strokepath pathbbox`,
4 wide, every join 0/1/2 and cap 0/1/2): a zero-length segment is ignored
wherever it is, at the start, at the end or in the middle of the subpath, and
whatever the join. The caps follow the first and last real segments and a
join sees the real segments around it, so the bbox is the bbox of the path
without it, to the digit. That holds for a `curveto` whose control points and
end all sit on the start, for a closed subpath whose first segment has no
length, and for a step inside gs's 1/256 px grid (`25.001`, `90.0001`): gs
rounds every point to the grid, so those steps have no length either.

**What pscat did.** tiny-skia skips a zero-length `lineto` only when it is
bit-identical to the point before, and then only in the middle of a stroke:

| segment | pscat before |
|---|---|
| leading, exactly zero, square cap | a cap along the default (+x) direction at the start (the issue) |
| leading, exactly zero, round cap | a start cap about 0.06 px short of the real one |
| leading, exactly zero, closed subpath | the start join is lost (miter tip 16.7 became 23.0) |
| leading, trailing or middle, within the 1/256 px grid but not zero (`25.001`) | a cap or a join along that step's direction: a butt cap turned 90 degrees |

The middle, exactly zero case already matched gs.

**Fix.** `PsPath::without_degenerate_segments` (it replaces
`without_point_subpaths`, which it contains) returns the path less its
zero-length segments, on gs's 1/256 px grid (the same grid #1220 uses for
point subpaths), or `None` when there is nothing to remove so a caller clones
nothing. A segment is zero-length when it ends on the grid point it began on;
a curve when its control points do too. A subpath with no length at all is a
point subpath: it is dropped under square caps as before and left alone
otherwise, since a round cap draws a dot for it. `Gfx::stroke_dropped` now
runs for every cap rather than only square (the point subpaths still only drop
under square), and the pen-space path of `stroke_space` is trimmed the same
way, exactly (no grid there). `stroke`, `strokepath`, the SVG `d` data and the
PDF path all read the trimmed path, since they all start from
`stroke_dropped`.

Dashing runs after the trim. The dashes fall where they do on the real line,
zero-length dashes included (they are cut from a real subpath, not a
zero-length segment).

**Cost and limits.** One pass over the path's segments per stroke to find a
zero-length segment, and a clone of the path only when one is found (before,
only a square-capped path with a point subpath was cloned). A stroke already
copies the whole path into a tiny-skia path (`to_skia_for_stroke`), uncharged
and the same size, so the clone at most doubles a transient the stroke
already makes. `--max-memory` is a process-heap count (`src/memtrack.rs`,
`TrackingAllocator`), so the clone is counted like any allocation and the run
stops at the next step check if it crosses the cap. `--max-steps` and
`--max-time` are charged per executed operator, and the pass is linear in a
path whose segments those operators already paid for; nothing loops on a
user-chosen count and nothing is quadratic.

**Tests** (`tests/stroke_anisotropic.rs`, last section). Butt and square caps
with miter and bevel joins are pinned against gs (`strokepath` bbox and the
page's ink) and against the same path without the segment. Round caps and
round joins are not pinned against gs: tiny-skia's quad circles differ from
gs's by about 0.2 px even on a clean path (pre-existing, not touched here), so
they are pinned to the clean path in pscat. The same cases run as a hairline,
under two anisotropic CTMs (the pen-space path), and in SVG and PDF.
