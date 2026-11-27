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

**A curve after a dropped segment (Codex round 1, PR #1454).** gs rounds
every point to its grid, so a curve's first control point on the start's grid
point has no direction and the cap or join follows the next control point.
tiny-skia does that only for a control point that is the start to the bit,
and otherwise takes a hair's width as the tangent: after dropping
`25.001 25.001 lineto`, `25.001 25 60 60 90 40 curveto` (20 wide, square caps)
gave a cap along +x, `strokepath pathbbox` starting `[15 15 ...]` against gs's
`[10.859375 10.859375 ...]` and the base's `[10.858812 10.858826 ...]`. The
trimmed path now sets a curve's first control point to the start it has in
the copy when both are on one grid point, and its second to its end
likewise (`without_degenerate_segments`), so the pre-existing near-miss the
base got by luck is also right. The far end of the curve's outline is a
separate 2.9 px gap to gs on the clean path (the upper y), left alone.

**Cost and limits (Codex round 1, PR #1454).** The walks are held to the
limits as `reversepath` (#1266) is. `without_degenerate_segments` takes the
operator's probe and counts every segment each walk looks at on a `Meter`:
the subpath discovery (`subpaths`, now fallible, polls inside the loop that
finds a subpath's end and the one that tests whether it moves), the scan
for what changes, and the copy. The probe is looked at every 4096 segments
counted (`FLATTEN_POLL`, a step per 1024). The count comes back as the work
`stroke` and `strokepath` hand to the operator, which charges a step per 1024
(`charge_probed_steps`). A path with nothing to change is walked about
twice (find the subpaths, test them) and charged for it; one with something
to change about three times. `stroke_space` runs the same walk over the
pen-space path, so a stroke under an anisotropic CTM is charged for both:
twice the walks, counted twice. The scan counts the path's subpaths first, and
the copy (at most the path's segments and a moveto per subpath) is held
against the heap cap before it is made. A stop returns before the path or
the graphics state is touched, since the trimmed path is a new one.
`strokepath` has the same limited form (`stroke_path_limited`). Cost with no
limits: the walks are a counter increment per segment, no clone when nothing
changes.

**Tests** (`tests/stroke_anisotropic.rs`, last section). Butt and square caps
with miter and bevel joins are pinned against gs (`strokepath` bbox and the
page's ink) and against the same path without the segment. Round caps and
round joins are not pinned against gs: tiny-skia's quad circles differ from
gs's by about 0.2 px even on a clean path (pre-existing, not touched here), so
they are pinned to the clean path in pscat. The same cases run as a hairline,
under two anisotropic CTMs (the pen-space path), and in SVG and PDF.
