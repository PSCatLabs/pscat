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
- **Shared tooth depends on `/Relief`.** On the surface the design's
  numbers came from (`/Relief 0.7`), at `/Weave 0.4` and 720 dpi, mean phi
  is 0.51 (control 0.03), which passes Q4. At `/slate`'s starting
  `/Relief 0.55` it is 0.38, under 0.4. D2's surface re-tune must re-check
  this per surface.
- **The reach margin is not paid when rendering.** A pixel-exact reach
  test needs 1.5 device pixels of extra reach, because the clip mask is
  anti-aliased. That costs the board 60% more cells, so the margin is
  `dkslack`, 0 by default, and only the reach test sets it.
- **The reach test tolerates anti-aliasing noise.** Even then, a
  whole-page clip path's larger bounds change tiny-skia's fixed-point
  precision, and edge pixels move by up to 16 of 255 in either direction.
  The test compares luma with a tolerance of 40. A real missed knuckle
  leaves whole pixels uninked, and the negative control proves the test
  sees that.
- **A real bug the reach test caught.** The cell-key range grew by each
  stop's own reach. A segment reaches as far as its *wider* end, so the
  cells beside a thin end fell outside the key range and were mis-decoded.
- **Dict `forall` order is random per run for integer keys** (HANDOFF
  gotcha). `dkcatch` emits its cells from a first-seen list, so its clip,
  and any SVG made from it, is stable.

Deferred to D2 (same issue):
- `dkstroke` and `dkbatch`;
- tests 4–8;
- test 3's matrix for `/Tilt`, `/Wear`, `/Pressure` and `/Fallout`;
- the specimen.
