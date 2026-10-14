## paintkit palette-knife plane: pktrowelplane (issue #180, 2026-10-14)

`pktrowel` walks a centerline, so covering an area (a mountain face, a rock)
had to be faked. In #118 (`gallery/alpine_lake.ps`) the shadow planes took
three attempts: a pull along a shallow ridge drew a bar across the range,
pulls stepped down the ridge made stripes, and what worked was a hand-built
triangle, a clip and vertical pulls at a fixed spacing. `pktrowelplane` is
that pattern as a procedure (epic #112).

What was built (`lib/paintkit.ps`, right after `pktrowel`):

- `opts pktrowelplane -`: the current path is the area. A sibling entry
  point, not a `pktrowel` mode (a mode would have to answer "what does the
  centerline mean" for a closed area). `opts` is passed straight to
  `pktrowel` per row, so every key it knows keeps its meaning. New keys:
  `/Direction` (pull direction, default -90), `/Overlap`, `/RowJitter`,
  `/AngleJitter`.
- Edge: the path is the clip, so the edge is crisp because of clipping, not
  blade geometry. Rows overrun the area at both ends by enough to hide the
  blade's 6% end trim, its drag and its skew (`(0.07*span + h*(|sin A| +
  Drag) + Pitch) / 0.85`).
- Spacing: `F = Width * cos(Angle) * (0.35 + 0.65*Cover)` is the band one pull
  deposits across its travel, rows go `F * (1 - Overlap)` apart, and `/Overlap`
  unset is `0.15 + 0.6*(1 - Coverage)`.
- The extent comes from `flattenpath` + `pathforall` projected on the pull
  axis and its normal, not `pathbbox` under a rotated CTM (loose in the
  PLRM, and the box is a place the two interpreters could differ). The rows are
  then drawn under `Direction rotate`.
- Determinism: two draws per row (row shift, row tilt), then the row's
  `pktrowel` draws. The stream is fixed by the area, `/Direction`, `/Width`,
  `/Angle`, `/Cover` and `/Overlap`, so turning any of those re-rolls the
  plane; a pure texture key does not. That is weaker than `pktrowel`'s
  per-control promise and the docs say so.
- Safety: 4000 rows, counted before anything is drawn; each row is also
  bounded by `pktrowel`'s own budget.
- Scratch prefix `tpl` (checked unused): `pktrowel` overwrites every `pt*` name
  on each row, so the loop state must not use them.

Found by looking:

- A first render had a bare full-height white line between two rows. Cause:
  a default `/RowJitter` of 20% of the spacing lets two neighbouring rows
  move 40% apart, more than the 26% overlap. The default is now `0.4 * F *
  Overlap`, and the default `/AngleJitter` is the tilt that moves a row's far
  end by 0.15 of the overlap (at most 1.5 degrees). A user-set larger value
  can still open a seam; the docs say so.
- A bare track in the middle of a row is a dead lane from `/Scrape`, which no
  neighbour covers below `/Overlap` about 0.5. That is the nicked blade of
  `pktrowel`, kept on purpose; the specimen uses `/Overlap 0.5` and low
  `/Scrape` for solid planes.
- `/EdgeBuildup` ridges did not make visible banding at the default spacing;
  the planes carry fine streaks the way a knife's pulls do.
- Pigment amounts: titanium white's tinting strength is 12, so "3 parts
  white" is white. The specimen's planes use 0.2-0.9 parts of white against
  about 1 of the chromatic pigments. Real palette-knife mountains (looked up:
  pulls follow the peak's own angles, light pressure so the paint breaks, a
  highlight paired with a shadow) are muted warm greys against blue-violet
  shadows, which is what the specimen is tuned toward.

Not done: `alpine_lake.ps` still builds its shadow planes by hand (swapping it
would change a committed render; a follow-up if wanted); no unclipped mode (a
blade cannot be steered along an arbitrary boundary, an unclipped plane is a
rectangle); `/Pressure` tapering opens gaps between rows because spacing uses
the nominal width.

Tests: `tests/pktrowelplane.rs` (the edge, coverage, direction, overlap, path
and clip survive, rotated and scaled CTMs, `/Touch`, determinism, CPU cost,
Ghostscript, catalog) and a `%%SelfTest` block of guards. Specimen:
`examples/paintkit_plane_demo.ps`, `docs/paintkit_plane_demo.png`.
