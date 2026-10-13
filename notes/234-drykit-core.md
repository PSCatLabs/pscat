## drykit core, slice D1: dksurface and the swept-band catch (issue #234, 2026-10-13)

The first half of #234 (design: `docs/DRYMEDIA.md`, slice D1). New
`lib/drykit.ps` (`dk-`, catalog-tagged), which requires artkit and paintkit
and never edits paintkit:

- **`dksurface`** (public) names one of five surfaces: `/slate`,
  `/greenboard`, `/newsprint`, `/drawing` and `/construction`. Each is a set
  of `pkcanvas` keys plus a `/Ground` colour kept in `userdict /dkGround`.
  Every guard, its own and `pkcanvas`'s, runs before any state is written.
  The surface values are the design's starting values, and D2's specimen
  re-tunes them.
- **`dkcatch`** (internal; D2's `dkstroke` is its caller) takes
  `runs touch streak`. The runs are arrays of `[x y halfwidth]` stops in
  user space. The catch:
  - uses `pjcatch`'s selection rule and scale law, with each knuckle
    stretched along its nearest centerline segment;
  - visits only cells whose centre is inside a segment's reach capsule,
    taking the exact interval column by column;
  - deduplicates cells per call;
  - takes the cell budget (150000) before touching the graphics state;
  - sets gs's fill adjust off before `clip`, behind the guard.
- **`dksrand`/`dkrand`**: paintkit's Schrage generator under drykit's own
  state, for D2's tool-side randomness. A self-test proves it draws the
  same sequence as `pjrand`.
- **The paintkit contract (Q1).** The `drykit-paintkit-contract` self-test
  pins exactly the names drykit reads:
  - `pjheight` and `pjhash`: their arity, their range, and that they are
    pure;
  - `pjCTM`, `pjP`, `pjW`, `pjM`, `pjR` and `pjmemo`: their shape.

  `pjrange`, `pjcells` and `pjbudget` are not pinned, because the swept
  band doesn't use them.

**Measured against the budget** (DRYMEDIA.md 6.1 has the table):

| Piece | pscat | gs | Budget |
|---|---|---|---|
| Core board | 14.5s | 7.2s | |
| Core board plus a one-level ghost-text pass | 21.6s | 10.7s | 30s |
| Portrait proxy (`docs/drymedia/prototypes/portrait_cost.ps`) | 35–37s | 13s | 60s |

The swept band did *not* deliver the estimated 2.9× fewer cells. It built
1.44M, against 1.59M for the bounding boxes and the 553k estimate: the
estimate under-counted the stretched knuckle's reach and the round ends
of short strokes. The time halved anyway (28s to 14.5s in pscat; 16s to
7.2s in gs), because the per-cell cost fell from about 18 µs to 10 µs.

Tests: `tests/drykit.rs` covers DRYMEDIA.md section 8's tests 1–3, plus:

- canvas anchoring;
- every `dksurface` key changing the render;
- SVG determinism;
- the gs fill-adjust check, local only (pscat 0.58 ink, gs 0.55, gs without
  the guard 0.81).

`tests/capabilities.rs` extends #141's unit check to `lib/dry*.ps`.

Found along the way:
- **Shared tooth depends on `/Relief` and `/Streak`.** On the surface the
  design's numbers came from (`/Relief 0.7`), at `/Weave 0.4` and 720 dpi,
  mean phi is 0.51 (control 0.03), which passes Q4. Through `dksurface`
  with the shipped starting values (mean over seeds 3, 7, 11, 19, 23;
  control means -0.16 to 0.01 throughout):

  | Surface | `/Streak 1.4` | `/Streak 1.8` |
  |---|---|---|
  | `/slate`, `/greenboard` (Relief 0.55) | 0.42 pass | 0.38 **fail** |
  | `/newsprint` (Relief 0.4) | 0.36 **fail** | 0.29 **fail** |
  | `/drawing` (Relief 0.65) | 0.46 pass | 0.42 pass |
  | `/construction` (Relief 0.75) | 0.45 pass | 0.42 pass |

  This blocks D2. Its surface re-tune must bring every surface to Q4 at
  the streaks its media use, or the maintainer revisits Q4 for low-relief
  paper, since #234 closes on D2.
- **The reach margin is not paid when rendering.** A pixel-exact reach
  test needs 1.5 device pixels of extra reach, because the clip mask is
  anti-aliased. That costs the board 60% more cells, so the margin is
  `dkslack`, 0 by default, and only the reach test sets it.
- **The reach test tolerates anti-aliasing noise.** Even then, a
  whole-page clip path's larger bounds change tiny-skia's fixed-point
  precision, and edge pixels move by up to 16 of 255 in either direction.
  The test compares luma with a tolerance of 40, at 4.2 device pixels per
  cell, so a knuckle covers whole pixels. A near-miss control (the band
  one cell short) shows 486 pixels at touch 0.6. Near touch 1 the same cut
  shows nothing, because the catch is all but solid, so the matrix carries
  a sparse case.
- **The reach bound is loose beside a straight segment.** A knuckle there
  lies along the stroke, so it reaches across by its short axis, not the
  long one the band is cut for. A tighter per-orientation band is a
  possible speed-up if a gallery piece runs over budget.
- **The slowest single test render** is 0.76s under `--profile ci` (a
  whole-page reach render, at load average 8). The file runs in about 7s.
- **A real bug the reach test caught.** The cell-key range grew by each
  stop's own reach. A segment reaches as far as its *wider* end, so the
  cells beside a thin end fell outside the key range and were mis-decoded.
- **Dict `forall` order is random per run for integer keys** (HANDOFF
  gotcha; filed as #277). `dkcatch` emits its cells from a first-seen
  list, so its clip, and any SVG made from it, is stable.

Deferred to D2 (same issue):
- `dkstroke` and `dkbatch`;
- tests 4–8;
- test 3's matrix for `/Tilt`, `/Wear`, `/Pressure` and `/Fallout`;
- nesting with varying half-widths. A `/Pressure` run can turn a knuckle
  between levels; see the header of `lib/drykit.ps`;
- the surface re-tune against Q4 above;
- splitting catches. `dkbudget` is 150000 unique cells per catch, and a
  level-major `dkbatch` over a portrait's hatching would be about 1M cells
  per level;
- the specimen.
