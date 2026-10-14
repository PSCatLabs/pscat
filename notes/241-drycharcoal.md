## drycharcoal: charcoal presets, hatching, smudge, lift, construction lines (issue #241, 2026-10-13)

New `lib/drycharcoal.ps` (prefix `dh-`, catalog-tagged), on drykit (#234)
and hatchkit's `/Mark` hook (#271). It changes neither library. Five public
procedures, all inking through `dkstroke` in the current colour, and none
takes a draw from the caller's `rand`:

| Procedure | What it draws |
|---|---|
| `dhstroke` | the current path in `/Medium` `/vine`, `/compressed` or `/pencil`, at `/Grade` 0 (hard) to 1 (soft); any other `dkstroke` key overrides the preset |
| `dhhatch` | the current path's region hatched to `/Value`, a number or `{x y -> value}`; every layer goes into one `dkbatch` |
| `dhsmudge` | the current path as a stump's track: a wide, soft-edged, high-touch stroke leaving `/Value` between paper and pigment |
| `dhlift` | the current path as an eraser's track, in paper colour through the catch: `/Eraser /kneaded` or `/hard` |
| `dhsketch` | faint construction lines, laid `/Passes` times, each shifted and turned by up to `/Shift` |

**Hatching design.**
- **`hatch` lays the lines and inks nothing.** Each layer is one `hatch` call with
  `/Mark`, a `/Density` that is the value's position inside the layer's own interval
  (`(v - k/L)·L`), and `/Width [0 1]` as a carrier. `hatchwidths` then returns the
  six-bucket quantized value, which the hook maps to stroke width.
- **No `rand` draws from `hatch`.** It gets no `/Seed`, `/Wobble`, `/Trim`,
  `/Dropout` or width range, so it draws nothing (the advisor's plan review). The hook
  splits each line into hand strokes, `/Length` × 0.6..1.4, with a small jog across
  the line. It draws from drykit's `dkrand`, re-seeded per line from (`/Seed`, layer,
  line index), so pscat and gs lay identical strokes. The gs block comparison is 1.27
  against 5.17 for another seed.
- **One touch per call.** Catches are shared only between runs with equal touch,
  streak and colour, so varying touch per stroke split them. Two touch bands cost 2×
  the catch time, so the value varies width and layer count instead. A *number*
  `/Value` also shifts the touch by `0.3·(v − 0.5)`: light hatching breaks into dots
  (H2), dark hatching fills more tooth.
- **Uneven layer angles.** Layers lie at `/Angle` + 0, 65, 130 and 25. The first
  try, 0/90/45/135, wove a lattice no hand draws (see the comparison image).
- **One level, one lane, no fallout by default.** One deposit level costs about 40%
  less than two (8.0 s against 4.8 s on a 120 pt square at four layers, before the
  other cuts). Crossing layers build the tone that a second level would.
- **Batches and clips.** The region is clipped and the batch closes inside that clip,
  because a batch renders under the clip in force at its end. So `dhhatch` refuses to
  run inside a caller's `dkbatch`, and at `/Levels 2` or more two `dhhatch` calls over
  the same area grey each other's crossings. At the default one level there is no
  faint level, and two calls draw as one. `tests/drycharcoal.rs::one_call_keeps_crossings_dark`
  measures it at two levels: one call gives mean luma 125.9, two calls 137.1. Filed
  as #312: per-record clips in `dkbatch`.
- **A malformed key passed through to `dkstroke`** (for example `/Levels 9`) fails
  inside hatch's `gsave` and ours. The hook catches it, stops drawing, and re-raises
  after both are closed. `dhsmudge`, `dhlift` and `dhsketch` give back their `gsave`
  the same way. Self-tests pin each case (the advisor's implementation review).
- **A `/Value` procedure that raises, or leaves anything but one number,** runs under
  `stopped`. Its error, or `dhhatch-value-procedure-must-return-a-number`, is raised
  after the batch and both `gsave`s close. Raising from inside `hatch` left `gsave`s
  open: first for a non-number result, then, as Codex round 1 found, for an error
  inside the procedure.

**Smudge semantics** (the advisor asked for the justification). "Softens and spreads
existing marks in a region along a direction" needs readback, which this interpreter
does not have (HANDOFF; #134). So the smudge is a *motion*: the path is the stump's
track, which gives the direction. It is told the tone to leave (`/Value`, from paper
to pigment: smudging spreads charcoal, so over a compressed mass it is lighter than
the mass) and the tone beneath (`/Under`, for its soft edge). At `/Touch` 0.74 the
tooth is mostly filled, but the darkest specks of a hatch beneath show between the
knuckles. The smudging value scale is built that way: a light vine hatch, then the
stump. pkwet's `/Direction` re-runs a mark along a line. It was not used, because
it re-draws the same mark rather than filling the tooth.

**Lift.** Paper colour through the catch, at low touch, so peaks lift first and the
valleys stay dark. A kneaded lift is three levels with a wide halo (1.4·Width), so its
edge grades from the tone beneath. Levels 2 with a narrow halo gave a hard
mid-grey ring. A hard eraser is one level at touch 0.95.

**Construction lines.** `dhsketch` captures the path with `pathforall` and re-runs it
under a small seeded translate and rotate per pass. The turn moves the path's far end
by up to `/Shift`. The passes go in one `dkbatch` when none is open, so they may run
inside a caller's batch of the same colour.

**Media tuning** (against H2, H4, H6 and H7). Vine: grey (strength 0.58–0.83), three
lanes that occasionally lift off, a little fallout, and a flicked tail (`/Pressure`
thinning over the last 15%). Compressed: touch 0.74–0.88, strength 0.88–0.98, no
halo; at 0.1 the halo drew a grey outline round every stroke. Pencil: width
0.3–0.65 pt, touch 0.46–0.71 and a wide halo that gives a granular fringe.

**Surface.** At drykit's `/drawing` defaults (`/Relief 0.65`), hatching breaks up in
patches several points across, which reads as blotches. The specimen and tests use
`/Relief 0.35`, where it breaks up knuckle by knuckle. Filed as #313: the default
relief is a Q4 trade-off for the maintainer.

**Measured** (CPU `user`, release pscat `--dpi 144`, gs 10.07.1 `-r144`, libraries
concatenated for gs's `-dSAFER`). The machine's load average was 4–12 during this work; the
drykit specimen (7.5 s in #234) measured 7.7–11.75 s here, so the earlier
numbers below are inflated by up to about 1.5×. The final specimen run was at
load 4, with the drykit specimen at 7.7 s.

| Piece | pscat | gs | Budget |
|---|---|---|---|
| `examples/drycharcoal_specimen.ps` (final, after the rebase) | 8.7 s, twice; the drykit specimen measured 7.7 s alongside (7.5 s in #234) | 6.4 s, twice | 10 s |
| `dhhatch`, 120 × 120 pt, value 1, one layer / four layers | 1.3 s / 4.0 s | | |
| slowest CI test render, 40 × 40 pt at four layers (`--profile ci`, file in parallel) | 0.74 s | | 2 s |
| `tests/drycharcoal.rs`, whole file (`--profile ci`) | 6.4 s | | 30 s for all drykit tests |

- **Hatching cost** is about 0.07 ms per square point per layer in pscat. That is
  catch cells: a `/Touch 1` render, which builds no catch, took a fifth of the time.
  #242's portrait should budget its hatched area with this.
- **An earlier specimen layout took 42 s.** It hatched about 66k pt²·layers and drew
  vine first. Cells and the sphere were shrunk to fit.
- **Ghostscript slows after `scatter`.** drykit work after a first artkit `scatter`
  runs about 2× slower in gs (a plain integer loop or a `pjheight` loop barely slows) (12.3 s against 6.3 s for this specimen, by drawing
  order alone; `-dNOGC` 7.7 s). Vine's fallout calls `scatter`, so the specimen
  draws its vine strokes last. Filed as #311, not root-caused.
- **A fresh options dict per hatch stroke** was suspected of feeding that GC cost.
  Reusing one dict made no measurable difference. It is kept, because `dkstroke`
  copies the values it needs into its record.

**Tests** (`tests/drycharcoal.rs`, 11):
- the two value scales are monotone and reach the darks. Hatching runs 239 → 61 luma
  at two deposit levels (one level stops at 80, a dark grey). Smudging runs 239 → 55;
- a `/Value` ramp shades across its region;
- one call keeps crossings darker than two;
- a kneaded lift is grey and never paper, and a hard eraser is lighter along its line;
- a softer grade inks more, in each medium;
- more sketch passes lay more line;
- every documented key of all five procedures changes the render, and the advertised
  and tested sets are equal;
- the CI budget smoke test;
- two real-gs tests, run locally only: a packed `/Value` procedure under
  `true setpacking` is accepted (with `/Seed ±999999`, the guard's bound), and pscat
  and gs agree on a hatched region.

Self-tests cover the guards, the no-draws rule for all five procedures (`dhhatch`
with default options included), and the refusal inside a `dkbatch`.

**Comparison with the references:** `docs/drycharcoal_refs_compare.png`. Gaps the
maintainer should weigh:
- Hatching is crisper and more regular than H2/H4. Real hatching is denser, finer
  and lower in contrast.
- Smudged tone is flatter than H1's felt grain, with streak flecks instead.
- Vine and compressed strokes have round caps and stepped lane starts, where H6 has
  ragged, soft edges.
- The kneaded lift reads as a grey patch with a soft rim, not H7's white streaks.
