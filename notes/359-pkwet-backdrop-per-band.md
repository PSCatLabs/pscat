## pkwet /Box and /Bands: a backdrop per band (issue #359, 2026-10-14)

#179 let `pkwet /Under` be a procedure asked once per softening pass, at
`/At` plus the pass's displacement. A mark that is tall against the backdrop
kept the part of the rim from its own height. `/Box [x y w h]` with `/Bands
[cols rows]` (instead of `/At`) now asks the procedure once per cell of a
grid over the mark and paints each softening pass once per cell.

Design calls, and why:

- **Bands, not a per-fill hook.** pkwet cannot see where the wrapped
  procedure paints, and an inherited-colour mark has no per-fill site at all,
  so the only mechanism that works for every preset is to clip. The
  per-fill hook for loaded colours (`puwet` already sits at each of those
  sites) is filed as #403. The issue's first step, an extent so the backdrop
  is asked at a pass's leading edge, is subsumed: `/Box` is the extent.
- **Asked per cell, once, at the cell's centre, not displaced.** A cell is a
  fixed piece of page; the question is what is under it. The grid is `/Box`
  grown by `/Offset` on every side (a pass lands up to `/Offset` away), with
  the outer cells' clips running on outward so no rim is cut off. The first
  prototype sampled the unpadded box and the outer rows were asked too far
  from where the outer passes land.
- **The random stream is replayed per cell** (`rrand` at pass start, `srand`
  before each cell after the first). Each cell then lays the same marks and
  the clips cut them into pieces, and the stream ends where one unbanded run
  leaves it, so nothing drawn after the call moves and the cell count never
  re-rolls a mark. Proof: a banded call over a constant backdrop leaves the
  same `rand` as the literal and renders within 3 levels of it
  (`wet_bands_over_a_constant_backdrop_match_the_literal`).
  The `srand` makes a program that never seeds look seeded to `--sweep-seed`'s
  never-called-`srand` diagnostic; accepted, only a banded call does it.
- **Cell edges are snapped to the device pixel grid** (`transform`, `round`,
  `itransform`). Unsnapped, an edge inside a pixel gives each neighbour
  partial coverage and two opaque partial fills let the ground through
  (measured: steps of up to 30 levels along the edge at a 101.3pt boundary;
  snapped, 2 levels everywhere, the clip mask's rounding). Exact for a CTM
  that does not rotate or skew; a turned page can show a faint hairline.
- **Pass budget.** A banded call runs the mark (Layers - 1) x cells + 1 times
  and counts as chain-restricted like a directional one: the 64 cap applies,
  so 12 cells at 6 layers (61 runs), 21 at 4. 16 x 16 is accepted by
  `/Bands` validation and refused by the budget with the same
  `pkwet-too-many-passes`. Boundary tested in the `%%SelfTest`: 61 passes, 81
  refused.
- **Loaded colours.** Each cell points frame slot 5 (the copy of `/Under`
  `puwet` reads) at its own backdrop, so `puwet` needed no change. The frame
  grew a slot 8 holding the banded record (clips, backdrops, pass colours,
  saved stream); it is null for every other call and the pass loop tests it,
  so an unbanded call takes the code that was there. Everything per-cell is
  read from the frame, never a global, so a banded call nests.
- **Validation** mirrors #179's: `/Box` and `/Bands` go together, exclude
  `/At`, and are rejected with a literal `/Under`; named errors in the
  `pkwet-rejects-malformed-bands` `%%SelfTest`. `pqaskunder` was factored out
  of `pqwetunder` (same checks, same unwinding, same error names).

Measured (`graded_column`, a 110pt column over the 150pt gradient, /Offset 34,
rows 15..135 of `rim_residual`): per pass 186.7, 1x2 149.9, 1x4 112.5, 1x8
82.9 (2.25x less). On the short cloud of #179's test, per pass is already near
the answer: 1x3 bands are worse (146.9 against 119.8), 1x6 better (98.4). So
the advice is a cell no taller than about half `/Offset`, and `/At` for a mark
about as tall as `/Offset`.

Byte identity: `scripts/identity_check.sh --loader lib/paintkit.ps` renders
43 of 43 pages identical (including `paintkit_wet_under_demo.ps`, the
gallery, and the drivers); the control (`/pqwetbands where`) differs as it
must. The eleven cookbook snippets render byte-identically in both trees. The
cookbook's snippets are unchanged on purpose (a cloud as tall as `/Offset` is
as well served by `/At`); only its prose and "Tools not yet landed" table
changed.

Specimen: `examples/paintkit_wet_bands_demo.ps` and
`docs/paintkit_wet_bands_demo.png` (per pass against 12 bands on a tall
plume); `ghostscript_accepts_paintkit_wet_bands_demo` runs it under gs.
