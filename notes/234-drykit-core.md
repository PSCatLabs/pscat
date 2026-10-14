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

## drykit core, slice D2: dkstroke and dkbatch (issue #234, 2026-10-13)

The second half of #234. It adds two public procedures to `lib/drykit.ps`:

- **`dkstroke`** treats every subpath of the current path as a stick's
  centerline and paints in the current colour. It walks the path with
  `walkpath`, then resolves each stop once: touch (a number, or a
  `{t -> touch}` profile quantized to `/Bands`), `/Pressure`, `/Wear` and
  `/Tilt` (blended toward `/Side`).
  - **Lanes.** A tilted stick gets at least three, and its edge lanes press
    0.06·sin(Tilt) harder than the face. Each lane has its own `/Load` and
    `/Dropout` contact chain, drawn from `dkrand`, one draw per stop.
  - **Seeds.** Each subpath is seeded from its own first point on the
    canvas, or from `/Seed + i` when a seed is given.
  - **Records.** The stops become a record of runs keyed by touch band,
    plus the CTM the stroke was given in.
- **`dkbatch`** collects the records of the `dkstroke` calls inside it and
  renders them together, level-major. It allows one pigment per batch and
  no nesting. An error inside the batch closes it and re-raises.

**Rendering (`dkrender`).**
- **Levels.** Faintest first, aligned so each record's brightest level
  lands last. A level's colour is lerp(`/Under`, pigment,
  `/Strength`·j/L), its touch is the band's + (L − j)·0.12, and the faintest
  level is widened by `/Halo` when L ≥ 2.
- **Shared catches.** Runs with equal touch, streak and colour share one
  `dkcatch`, chunked by estimated cells under `dkbudget`. A run too long
  for one catch is split into pieces that share an end stop.
- **Thinned centerline.** The catch takes a thinned centerline. Walking at
  the ink's pitch made every short capsule revisit each cell up to 6
  times (19 µs per cell, against 3.5).
- **Ink.** Constant width is a round-capped stroke. Varying width is a
  union of a disc at every stop and a counter-clockwise quad along every
  segment, so the ink stays inside the reach the catch was cut for.
- **Fallout.** Fallout is drawn last, through artkit `scatter`: one
  rectangle per ~20pt piece, thinned away from the line by `/Weight`,
  drifted down canvas y by up to `/Drift`, with `/Seed` from the
  subpath's seed.

**Decisions:**
- **Ink geometry without `pkribbon`.** A union of discs and quads is exact
  against the catch's reach. pkribbon's outline isn't, and it clobbers
  `pk*` names.
- **Records keep their CTM.** The advisor's plan review found that a
  record holding user-space stops would land wrongly when rendered under
  the batch's CTM. Hatching is written as `gsave rotate … dkstroke
  grestore`. Tested: a batch of rotated, scaled strokes is pixel-identical
  to the same strokes drawn unbatched.
- **One pigment per batch.** This is the design's Q6. A second colour
  raises `dkbatch-mixes-pigments` rather than being grouped, because
  grouping would put B's dim level under A's core.
- **`/Touch` keeps its `[0..1]` unit.** The procedure form is described
  in the text, since the unit vocabulary is closed.
- **Newsprint `/Relief` went from 0.4 to 0.55.** At 0.4 it failed Q4
  (mean phi 0.36). Test 1 now checks all five surfaces at their media's
  streak: the boards, newsprint and drawing paper at 1.4, construction
  paper at 1.8. Means are 0.42, 0.42, 0.46 and 0.42.
- **Walk pitch is 0.75·Width, clamped to 1..4pt.** That costs no visible
  change and saves walk and catch time.

**Measured** (DRYMEDIA.md 6.1, D2 table), CPU time at 144 dpi:

| Piece | pscat | gs | Budget | Status |
|---|---|---|---|---|
| Specimen, at 2x after trimming its layout (it was 11s under load) | 7.5s | 3.5s | 10s | inside |
| Portrait proxy, through one `dkbatch` | 40–42s | 27–28s | 60s | inside |
| Board core + one-level ghost pass | 31.2–32.8s | 16.1–16.9s | 30s | gs inside, pscat ~7% over |

The board overrun is filed as #283, with a cost breakdown. The main lever
is a reach band cut per orientation: beside a straight segment the knuckle
needs only its short axis.

**Tests** (`tests/drykit.rs`, 18 total; 12–19s for the file, depending on load):
- **Test 1**, per surface.
- **Test 3, extended to `dkstroke`.** It covers `/Streak` 4, `/Tilt` 90,
  `/Wear` 1, `/Pressure` above 1, lanes with dropout, a banded touch
  profile, fallout with `/Drift`, and a run split under a tiny budget,
  each against a whole-page catch. Fallout is drawn unclipped, so its row
  only shows it leaves the catch alone.
- **Test 4.** The same input gives the same pixels. A caller's later mark
  doesn't move under any key. A stroke elsewhere re-rolls nothing.
- **Test 5.** One call with two subpaths is pixel-identical to a batch of
  two calls. No dim pixel lands over the other stroke's core. The
  control: separate calls do dim it.
- **A batch keeps each stroke's own space.**
- **Nesting under a `/Pressure` profile.** 0 violating pixels were
  measured.
- **Test 6**, gs block agreement: 0.43 against 18.5 for another seed.
- **Test 7.** All 18 `dkstroke` keys are live, and the advertised and
  tested sets are equal.
- **Test 8.** 100 strokes at one level render in 0.56s with the file in
  parallel. At two levels it was 1.2s, too close to the 2s bar for a slow
  CI runner.
- **A curvy run at `/Streak 4` under a small budget.** Chunking estimates
  cells before thinning grows the reach by the chord deviation, so a curvy
  run's catch can exceed the estimate. Measured: it still fits at
  `dkbudget` 800. The gap is noted in #283, together with a second one: a
  budget error raised mid-render leaves that level's `gsave` open and the
  earlier levels drawn.

New self-tests cover `dkstroke`'s guards, the missing-surface error, no
caller draws with every random feature on, and `dkbatch`'s guards.

**Demo:**
- `examples/drykit_specimen.ps`: five surfaces × three pressures × three
  tilts, plus a wearing, landing-and-lift stroke and a crossing pair in
  one batch;
- a render line in `scripts/build_site.sh`;
- a card in `site/gallery.html`.

**Open, for the maintainer's eye on the specimen:**
- The tilt lanes' round caps show as three bumps at a tilted stroke's ends.
- The `/Pressure` lift on the wear stroke ends in a sharp point.

Both are presets' business (#235, #239, #241), not the core's.
