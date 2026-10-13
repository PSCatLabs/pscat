## Canvas surface S3: `/Touch` on pkfan, pkbroad and pkliner (issue #225, 2026-10-13)

Slice S3 of #182's design (`docs/CANVAS_SURFACE.md` 2.2, 2.4, 3.2 and 6). With a
surface set by `pkcanvas`, a `/Touch` below 1 clips a fan, broad-brush or liner
mark to the weave's knuckles. Same shape as S2: the parse and four guards
(`pkX-touch-must-not-be-a-procedure`, `-must-be-0-to-1`, `-needs-a-canvas`,
`-area-exceeds-safety-limit`), a reach box from `pjstopbox` checked with
`pjcells`/`pjbudget` before anything is drawn, and `pjcatch` called right after
the preset's drawing `gsave`. Unset or 1 takes the old path and no draws.

- **Reach boxes** (each emitter is listed in a comment next to the code).
  - pkfan: `max(half + |Jitter|, 1.15*half) + BristleWidth*(1 + WidthJitter)/2`,
    `half = Width/2 * Spread`. `pkribbon` is called without `/Jitter`, its caps
    are pointed and it resamples a polyline, so a mark stays within its
    half-width of a vertex.
  - pkbroad: `Width/2 + 0.06*Width*Edge + Pitch`. Every boundary point is `|u|`
    across the band from its stop whatever `/Angle` is, so the strip needs only
    the edge roughness. A single-stop run or bare moveto is a stamp that reaches
    along the travel by at most one `/Pitch`.
  - pkliner: `Width/2 + Waver*Width`, except a degenerate stop (a pressed dot),
    which is not displaced and takes `Width/2`. The displaced centerline does not
    exist yet when the box is built, so the bound is a constant. The dot case
    matters for the budget: a dot at `/Waver 10` would otherwise claim a box 20
    times its size.
- **Mutation-checked.** Each reach term was dropped or halved in turn (11
  mutations: pkfan's Jitter, 1.15 ray factor, half, WidthJitter and bristle
  half-width; pkbroad's Edge, Pitch and half; pkliner's Waver, half and dot
  reach). Each fails a case of `touch_box_contains_the_whole_mark`. The catch's
  own slack (the box grows by 1.25 weave plus up to a cell of rounding, about 3
  to 5.6pt) hid the small terms at first, so the cases that isolate Edge, bristle
  width and the dot use large values: `/Width 120 /Edge 1`, `/BristleWidth 30
  /WidthJitter 1` on a thin fan, and a `/Width 40` dot.
- **Tests.** `TOUCH_PRESETS` now lists all five adopters, so tests 1, 2, 4 and 5
  run on each, including S2's carried-over test 4 on a solid `pkbroad` band.
  `touch_solid` has a dense fan (bristles 0.75pt apart, 3pt wide) and a `/Width
  40` liner, wide enough that test 4 has more than 200 caught and missed pixels
  to compare. A touched fan and broad brush join the uniform-load-under-`pkwet`
  test, `Touch` joins pkbroad's and pkliner's advertised and documented-parameter
  tests, and one new test checks every adopter advertises `/Touch [0..1]` and
  names `pkcanvas`. Test 3 (a crossing knife and dry brush) stays S2's.
- **Self-tests.** `pkfan-touch-guards`, `pkbroad-touch-guards` and
  `pkliner-touch-guards`, each with the no-canvas cases first; pkfan's and
  pkbroad's also run a touched stroke with a colour load.
- **Byte identity.** `scripts/identity_check.sh` from a worktree at the branch
  base, with the same release binary: 30 of 30 page renders identical across 21
  files, the 2x `alpine_lake` equal to the committed still in both trees, and the
  control (a touched pkbroad, pkfan and pkliner) differing between the trees.
- **pkliner's header** said the tooth "is a property of the paper"; it is now "of
  the tip on the paper", in the header and in the code comment, and the header
  says the shared canvas is `/Touch` (2.4, open question Q4). The mechanism is
  unchanged.
- Not here: S4 (the specimen, README, HANDOFF gotcha, psart docs, test 6) and S5
  (`pkoil`, `pkdab`, `pknib`, `pkribbon`). `pkcanvas`'s `@summary` now names all
  five adopters.
