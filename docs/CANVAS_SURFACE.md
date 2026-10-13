# CANVAS_SURFACE.md: a shared canvas surface and a light-touch control (issue #182)

Design for #182, part of epic #112. Nothing here is implemented yet, and
the maintainer approves it before any code is written. The companion
design is `docs/PAINT_LOADING.md` (#183). Both docs use the vocabulary
below, and section 4 checks that they don't collide.

## Vocabulary shared with PAINT_LOADING.md

| Term | Meaning |
|---|---|
| **surface** | The shared tooth of the canvas: a seeded height field over canvas position, set up once per piece by `pkcanvas`. It paints nothing. |
| **canvas space** | The user space in force when `pkcanvas` was called, captured as a matrix. The surface is defined and sampled there, whatever the CTM is when a mark is painted later. |
| **knuckle** | One raised crossing of the weave: a small ellipse centred in each cell of a `/Weave`-spaced lattice, elongated along whichever thread passes over at that crossing. |
| **touch** | How hard a tool presses into the canvas: the preset key `/Touch`, 0..1. |
| **catch** | The clip a light touch builds from the surface: every knuckle the paint reaches at that touch. A touched mark is painted through its catch. |
| **head position** `u` | PAINT_LOADING.md's term: where a bristle, lane or striation sits across the head. This doc doesn't use it. |
| **unset** | A key that is absent. Branches test `opts /Touch known`, never a default value that happens to compute the same thing (the #181 `/Direction` rule). |

## 1. The problem

A painter's lightest strokes work because paint catches only on the
raised weave of the canvas, and every stroke laid over that canvas
catches on the same weave. paintkit has plenty of break-up, but every
mechanism belongs to one stroke, so nothing is shared between strokes.
That shows in `gallery/alpine_lake.ps` (line numbers from `origin/main`
at 0533d48):

- **Snow on the ridges.** Each peak gets five overlapping `pktrowel`
  pulls of snow (lines 205-212, via `lkknife`, at `/Coverage` 0.82
  down to 0.5 and `/Scrape 0.35`). Every pull's lanes run their own
  Markov chains on the caller's random stream, so where one pull skips,
  the next pull fills in at random. A knife grazing snow onto a real
  ridge leaves its gaps where the canvas is low, and a second pull skips
  in the same places.
- **The sky block-in.** 91 criss-cross `pkbroad` strokes at `/Charge
  0.7 /Depletion 0.8` (lines 95-103) cross each other constantly. Each
  stroke's striations drop out by their own seeded affinities, so the
  crossings break up independently, and the sky reads as layered marks
  rather than as one surface.
- **The mist fade.** 17 stacked `pkbroad` strokes with falling
  `/Charge` (lines 228-238) streak horizontally where each one runs dry,
  and those streaks are uncorrelated from one stroke to the next.
- **Light on the water.** The 14 `pkliner` light lines (lines 397-404)
  each draw a fresh pool of 64 tooth knots, so each line's gaps belong
  to that line alone. `pkliner`'s own header calls that tooth "a
  property of the paper", but the paper changes with every stroke.
- **The pine highlights** (lines 321-331) are a light tap expressed as
  `pkfan /Load 0.5 /Dropout 0.4`, a contact rate along the stroke. A
  tap that catches on raised canvas has no way to say so.

The issue also notes that there is no common "how hard am I pressing"
control: `/Load` (contact rate, #141), `/Charge` (paint reservoir),
`/Coverage` (pktrowel's contact steady state) and `/Pressure` (a width
profile) all control something else.

## 2. Artist-facing API

### 2.1 Setting up the surface

```
opts pkcanvas -
```

Call it once, early, in the coordinate system the piece is painted in.
It records the current CTM as canvas space, builds the surface's
tables, and paints nothing. Calling it again replaces the surface.

| Key | Unit | Default | Meaning |
|---|---|---|---|
| `/Seed` | `[integer]` | 1 | Integer seed for this surface alone. It leaves the caller's rand stream untouched: `pkwash`'s meaning, with a fixed default rather than one draw from the caller's stream (open question Q3). |
| `/Weave` | `[points]` | 2.5 | Thread spacing of the weave, in canvas-space points; must be > 0. One knuckle per `/Weave` x `/Weave` cell. |
| `/Mottle` | `[points]` | Weave*8 | Scale of the slow variation in how proud the weave stands, in canvas-space points; must be > 0. Larger gives broader patches of catch and miss. |
| `/Relief` | `[0..1]` | 0.7 | How much the slow mottle, rather than each knuckle's own random height, decides which knuckles stand proud. 0 is an even, speckled weave; 1 is all patches. |

All four keys use units already in the closed `FIXED` list.

`/Grain`, `/Tone` and `/Blend` are deliberately not keys of `pkcanvas`.
`/Grain` already means a count in `pkbroad` and 0..1 in `pkwash`, and a
third meaning would repeat the #141 problem. `/Tone` and `/Blend` keep
their `pkpaper` meanings and are not needed here (open question Q5
covers a ground tone).

### 2.2 The light-touch key

| Key | Unit | Default | Meaning |
|---|---|---|---|
| `/Touch` | `[0..1]` | unset | How hard the tool presses into the canvas. Below 1, the mark lands only on the knuckles of the shared surface that stand proud enough for that pressure to reach; lower is lighter. 1 is full contact, identical to leaving it unset. Needs `pkcanvas` first. |

First-pass adopters: `pktrowel` (the knife grazing snow), `pkdry` (dry
brush over a sunlit field), `pkfan` (a tap of highlights), `pkbroad` (a
dry block-in), and `pkliner` (light on water). These cover the issue's
three examples and the specimen. `pkoil`, `pkdab`, `pknib` and
`pkribbon` can follow (slice S5). `pkspray` never adopts `/Touch`: spray
doesn't touch the canvas.

**What /Touch is not.** Every other "less paint" control models the tool
and stays private to its stroke. `/Touch` is the only one anchored to
canvas position:

| Key | Models | Shared between strokes? |
|---|---|---|
| `/Load`, `/Dropout` (pkdry, pkfan, pkoil) | bristles making and losing contact along the stroke, a Markov rate per `/Width` | no |
| `/Charge`, `/Depletion` (pkbroad, pkliner, pkdab) | the paint reservoir running out along the stroke | no |
| `/Coverage`, `/Viscosity`, `/Scrape` (pktrowel) | the blade's contact chain and nicks | no |
| `/Pressure` (pkribbon, pktrowel, pkoil) | a `{t -> mult}` width profile | no |
| `/Touch` | pressure against the canvas weave | **yes**: one surface, so every touched stroke catches on the same knuckles |

These all compose: a touched `pkdry` still runs its contact chain, and
the result is clipped to the catch.

**Errors.** A `/Touch` given with no surface set raises
`pkX-touch-needs-a-canvas`, rather than silently painting the full mark.
Guards are named after the preset, as everywhere in the file:
`pktrowel-touch-must-be-0-to-1`, `pktrowel-touch-must-not-be-a-procedure`
and `pktrowel-touch-area-exceeds-safety-limit` (section 3.4).

### 2.3 Byte identity when unused

- **Surface not set:** no preset reads any `pj-` name, so every existing
  specimen and gallery piece is unchanged.
- **Surface set, `/Touch` absent or exactly 1:** the preset takes exactly
  the path it takes today. The only addition is one `known` test and one
  comparison. Calling `pkcanvas` itself draws nothing and takes no
  `rand`, so adding it to an existing piece doesn't move a single mark.
- **`/Touch` below 1:** the preset's geometry, colours and random draws
  are unchanged, and the mark is clipped to the catch. Turning `/Touch`
  on or off never re-rolls this mark or anything painted after it.

### 2.4 How existing tooth maps onto it

Every existing break-up stays, unchanged, because each one models the
tool, not the canvas:

- `pkliner`'s 64-knot arc-length tooth: where the tip runs dry as
  `/Charge` falls. It stays per stroke. Its header's phrase "a property
  of the paper" is reworded in slice S3 to "of the tip on the paper"
  (open question Q4).
- `pkbroad`'s per-striation affinity: which bristles hold paint longest.
  It stays per stroke.
- The `pkdry`/`pkfan`/`pkoil` contact chains, and `pktrowel`'s
  coverage, viscosity and scrape, all stay per stroke.

The surface adds the one break-up that belongs to the canvas, and adds
it the same way to every adopting preset: through the catch. That keeps
the change the same shape everywhere: one clip, placed in the preset's
existing drawing `gsave`, and no change to any preset's run logic.

## 3. Mechanism

### 3.1 Representation (prefix `pj-`)

`pj-` has no `/name` usage anywhere under `lib/`, `examples/`,
`gallery/`, `site/`, `tests/` or `selftest/` (scanned for this doc). The
sampler runs inside `pkfan` and `pkdry`, whose nested `pkribbon` calls
clobber every `pk*` name, so it needs its own prefix.

`pkcanvas` stores:

- `pjCTM`: `matrix currentmatrix`, which defines canvas space.
- `pjP`: a 256-entry permutation of 0..255, shuffled by Fisher-Yates
  using a private generator seeded from `/Seed`. That generator is
  `pwrand`'s Schrage-decomposed minimal-standard LCG, under its own
  `pj-` state.
- `pjW`, `pjM`, `pjR`: `/Weave`, `/Mottle`, `/Relief`.
- `pjmemo`: an empty dict for knuckle heights (3.3).

The surface is a pure function of `(seed, i, j)` over knuckle cells:

```
hash(i, j, s)  = P[(P[(i + s) and 255] + j) and 255] / 255     % 0..1
mottle(x, y)   = smoothstepped bilinear value noise over hash(., ., 101),
                 lattice spacing /Mottle, at the cell centre
height(i, j)   = (1 - Relief)*hash(i, j, 0) + Relief*mottle(centre)
```

The knuckle at cell `(i, j)` is centred at `((i+0.5)W, (j+0.5)W)`,
nudged by up to `0.15W` on each axis by `hash(i,j,2)` and `hash(i,j,3)`
so that the lattice doesn't read as a grid. It is an ellipse of
semi-axes `0.8W x 0.42W`, long along x when `i + j` is odd and along y
when it is even: a plain weave's warp and weft taking turns over.

`and 255` (not `mod`) keeps negative cell indices in range, as artkit's
`noise2` learned. The per-knuckle hash repeats every 256 cells (640pt at
the default weave). The mottle doesn't repeat for 256 x `/Mottle`
(5120pt), and it decides most of the pattern at the default `/Relief`,
so the repeat isn't visible at page scale.

### 3.2 The catch

`pjcatch` is internal, and every adopting preset calls this one
implementation:

```
llx lly urx ury touch pjcatch -     % user-space box; clips the current gsave
```

1. If `touch >= 1`, do nothing. In practice the preset never calls it
   then (2.3).
2. Transform the box's four corners `transform` to device space, then
   `pjCTM itransform` to canvas space, and take their bounding box in
   cells.
3. Check the cell count against the budget (3.4) *before* building any
   path.
4. Save the CTM, `pjCTM setmatrix`, `newpath`. For each cell, compute
   `d = height(i, j) + touch - 1`. If `d > 0`, append the knuckle
   ellipse (four `curveto`s from explicit corner arithmetic, with no
   CTM changes per knuckle, as `pvfleck` does) scaled by `0.6 + 0.5 *
   min(1, d/0.3)`, so a knuckle the paint barely reaches catches a small
   speck and a well-covered one catches a full knuckle that merges with
   its neighbours.
5. If no knuckle passed, use a degenerate one-point path, so the clip is
   empty rather than unchanged. Then `clip newpath`, and restore the
   saved CTM with `setmatrix`. The clip is in device space, so it
   survives the matrix change.

Each preset calls `pjcatch` right after the `gsave` that already opens
its drawing block (`pkdry`'s `pbn 0 gt { gsave`, and so on). It passes
the bounding box of its collected stops, grown by its own reach:

| Preset | Reach added to the stops' box |
|---|---|
| pktrowel | `Width/2 * (1 + Drag) + Pitch/2 + Jitter` (blade, drag, half-pitch run extension) |
| pkdry | `Width/2 * Spread * 1.15 + BristleWidth*(1 + WidthJitter) + Jitter` |
| pkfan | `Width/2 * Spread + BristleWidth*(1 + WidthJitter) + Jitter`, or the ray length `Width/2*Spread*1.15` for a pressed fan |
| pkbroad | `Width/2 * (1 + 0.06*Edge) + Pitch/2` |
| pkliner | `Width/2 + Waver*Width` |

Overestimating only costs cells. Underestimating would clip real paint,
so each slice adds a test that a `/Touch 0.999` mark has at least as
much ink as the untouched mark minus the knuckle gaps (a test that fails
if the box is too small).

The matching `grestore` drops the clip. A caller's own clip (alpine_lake
clips its knife work to each range's silhouette) intersects with the
catch, which is what a stencilled ridge on a real canvas does.

### 3.3 Cost, measured

The prototype (outside the repo, against `origin/main`'s paintkit and
the release `pscat`) implemented 3.1 and 3.2 as a stand-alone file:

- **Look.** At `/Weave 2.5 /Relief 0.7`, a light knife stroke and a
  dry-brush stroke crossing it break into patches with weave-textured
  edges, and in the crossing the dry brush sits on the same patches as
  the knife. At `/Touch 0.35` only scattered high spots catch, and at
  0.8 the strokes are nearly solid with weave-textured holes. Rendered
  at 1x and 2x, the 2x is the same structure at finer raster.
- **Shared catch, quantified.** In the crossing region, with each
  stroke rendered alone at `/Touch 0.55`, the fraction of pixels inked
  by both was 2.22 times what independent break-up predicts (densities
  0.23 and 0.23, both 0.12). The control used the same strokes without
  a surface, broken by their own controls (`/Coverage 0.45` and
  `/Load 0.35`), and gave 1.17. That ratio is the Rust test in 6.
- **Speed.** 200 `pkbroad` strokes (80 x 18pt, so a 100 x 50pt box at
  2.5pt cells), inside an outer triangular clip, on a 760 x 560 page:
  - untouched, 0.54s;
  - touched, computing every height fresh, 4.22s (about 18ms a stroke);
  - touched, with heights memoized, 2.15s (about 8ms a stroke).

  The same run at 144 dpi took 4.32s, so the cost is PostScript
  interpretation, not rasterization. Profiling by removal showed the
  value-noise mottle is most of the fresh cost, which is why heights
  are memoized.

**Memo.** `pjmemo` maps `(i + 4096) * 8192 + (j + 4096)` to `height(i,
j)`. Every key stays below 2^26, safely inside gs's 32-bit integers.
Cells outside +-4096 (about 10000pt at the default weave) are computed
but not stored. The memo stops growing at 250000 entries, and later
cells are computed fresh. A height is a pure function, so the memo
changes speed only and never output. A self-test pins that.

**Bounds.**
- Per call, the canvas-space box may hold at most 60000 cells (a 610pt
  square at the default weave). Past that, the preset raises
  `pkX-touch-area-exceeds-safety-limit` before building anything, the
  file's usual budget-then-reject shape.
- A motion that hands a preset many subpaths in one call (`pkcrisscross`
  over a sky band) builds one catch for the whole call, so it is cheaper
  per stroke than separate calls. A 760 x 300pt sky is 36480 cells, under
  the budget.
- Under `pkwet`, every pass rebuilds its catch (3.5), so the existing
  64-pass chain budget bounds the multiplier.
- At alpine_lake's call counts, about 200 touched strokes would add an
  estimated 1.5-2s to a 35s render.

**Export size, the real cost.** PNG is unaffected. Vector export is not:

| Export of one touched pkdry stroke (prototype) | Untouched | Touched |
|---|---|---|
| SVG | 34 KB | 1.16 MB |
| PDF | 36 KB | 39.6 MB |

SVG registers each clip path once as a `clipPath` def, so a touched call
costs roughly its catch's size, about 380 bytes per knuckle. PDF
(`src/pdf.rs`, by design) wraps *every* element in `q ... Q` with its
own full clip chain. Every dash of a touched stroke therefore repeats
the whole catch, which is about 80 copies here. Slice S0 fixes that in
Rust: consecutive elements whose chain is the same `Rc` share one `q
<clip> W n ... Q` group. That is a change to the PDF recorder only.
Until it lands, the docs say "touched marks make PDF export large"
(open question Q7).

### 3.4 Determinism and independence from the caller's stream

- The surface uses no `rand` and no `srand`, only `pjrand` (the private
  Schrage LCG) and only inside `pkcanvas`. Sampling is pure table
  lookups and arithmetic. The caller's stream is therefore untouched by
  `pkcanvas`, by `pjcatch`, and by any `/Touch` value.
- `--sweep-seed` intercepts `srand` and never reaches the surface. A
  sweep varies the marks and keeps the canvas, which is the useful
  behaviour: one canvas, many paintings. `--sweep NAME=` can feed
  `/Seed` for a piece that wants to sweep canvases.
- The same `/Seed` gives the same permutation table under gs, because
  the Schrage form keeps every intermediate product below 2^31-1 and gs
  doesn't promote to real. The hash and the memo keys stay below 2^26.
  The only gs differences are real-number rounding in the mottle, which
  can move a knuckle that sits within float error of the threshold, and
  antialiasing.

### 3.5 Canvas space, and why it is captured

The surface is sampled in the user space captured by `pkcanvas`, not in
the space each mark is drawn in. Each alternative was checked against a
real case:

- **Per-mark user space** would make the weave move with every `pkwet`
  pass (each pass is a `translate`) and mirror under alpine_lake's `1 -1
  scale` reflection. Passes of one wet mark would catch on different
  weaves, and the reflection would catch on a mirrored canvas.
- **Raw device space** would make `/Weave` a pixel count. The 1x render
  and the 2x gallery still would then show different weaves.
- **Captured canvas space**, as chosen: a `pkwet` pass and the core it
  softens catch on the same knuckles, the reflection catches on the
  same canvas as the scene above it (unmirrored, which is physically
  right), and `--dpi` changes only raster fineness. A piece that works
  in a scaled coordinate system gets `/Weave` in its own units, as it
  would for every other paintkit length.

## 4. Interactions

### 4.1 With PAINT_LOADING.md (#183)

- **Keys are disjoint.** Here: `pkcanvas` with `/Seed`, `/Weave`,
  `/Mottle` and `/Relief`, and the preset key `/Touch`. There:
  `/Colors`, `/ColorMix`, `/HighlightColor`, `/ShadowColor`,
  `/TipColor` and `/TipReach`, plus `/Light` with `pkdab`'s meaning.
  No key is shared, and neither design changes an existing meaning.
- **Prefixes are disjoint.** `pj-` here, `pu-` there.
- **They compose.** A double-loaded, touched knife resolves its lane
  colours (theirs), then paints through the catch (ours). Neither
  changes the other's draws.
- **Spaces differ deliberately.** `/Light` is user space, so a
  reflection's lit side mirrors. The surface is canvas space, so a
  reflection catches unmirrored. Each matches what a painter sees.
- **pkwet.** #183 adds frame slots 4-6 to `pkwet`. This design doesn't
  touch `pkwet`.

### 4.2 With directional pkwet (#181)

A touched preset inside `pkwet` builds its catch inside each pass's own
`gsave`, in canvas space. Every pass, whether translated along
`/Direction` or scattered, therefore catches on the same knuckles, and
a pulled reflection streaks *along the weave* rather than smearing it.
The `pkwet` pass budget (at most 64 runs of the innermost procedure in
a directional chain) bounds how many catches are built. `pkwet` itself
gets no `/Touch`: a wrapped procedure that wants touch passes it to its
preset.

### 4.3 With the brush motions (#184)

The motions only build paths. A touched preset fed a motion builds one
catch over the bounding box of all the motion's subpaths:
- `pkcrisscross` over a sky band produces one catch for 40 strokes, the
  cheapest case and the most convincing, since every X catches on the
  same canvas.
- A `pkpull` set of reflection pulls produces one catch.
- `pkscumblein` over a large cloud region can approach the 60000-cell
  budget. At the default weave, a region up to about 610pt square fits.
  Past that, the guard names the fix: smaller regions or a coarser
  `/Weave`.

## 5. Ghostscript

- **Plain `gs file.ps`.** Every operator used is Level 1/2:
  `transform`, `itransform`, `setmatrix`, `currentmatrix`, `clip` with
  many subpaths, `curveto`, dict and array access, and integer `and`.
  The prototype rendered under gs 10.07.1 in 0.24s with the knuckle
  pattern in the same places as pscat's. gs doesn't antialias, so
  knuckles come out as hard pixel blocks.
- **Same surface, not only acceptance.** The surface doesn't depend on
  `rand`, so a rand-independent driver can compare gs and pscat pixels:
  `pkbroad << /Charge 1 /Depletion 0 /Edge 0 /ColorJitter 0 /Touch 0.5
  >>` deposits every striation regardless of the affinity draws. That
  is a stronger test than `ghostscript_accepts_*` (section 6, test 8).
- **No alpha.** The catch is a clip, so nothing here needs `setalpha`
  or the `pkwash` fallback.

## 6. Test plan

**Self-tests** (`%%SelfTest`, next to the guards):
- `pkcanvas-rejects-malformed-options`: `/Weave 0`, `/Weave { 3 }`,
  `/Mottle -1`, `/Relief 1.5`, `/Seed (x)`, each with `mustguard`, plus
  `mustpass` for an empty dict.
- `pkcanvas-takes-no-random-draws`: `1 srand rand /a exch def 1 srand
  << >> pkcanvas rand a eq` passes `mustbe`.
- `pkcanvas-heights-are-pure`: the same `(i, j)` gives equal heights
  twice, and equal again after `pjmemo` is cleared (`mustbe`).
- Per adopting preset, `pkX-touch-guards`: `/Touch 1.5`, `/Touch { }`,
  `/Touch 0.5` with no canvas (a block that runs before any
  `pkcanvas`), and the area budget (a 1e5pt stroke at `/Touch 0.5`),
  each with `mustguard`.

**Rust tests** (`tests/paintkit.rs`, new section "canvas surface
(issue #182)"):
1. `touch_unset_is_byte_identical`: each adopting preset renders
   pixel-equal with no canvas, with a canvas set and no `/Touch`, and
   with `/Touch 1`.
2. `touch_takes_no_random_draws`: a mark drawn after the preset from
   the caller's stream doesn't move between `/Touch` unset, 0.5 and 0.2.
3. `crossing_strokes_catch_on_the_same_weave`: a pktrowel stroke and a
   crossing pkdry stroke, each rendered alone. In the crossing region,
   the ink-coincidence ratio must exceed 1.6 with the surface, and stay
   under 1.3 for the matched-density control without one. The prototype
   measured 2.22 and 1.17.
4. `the_catch_is_anchored_to_the_canvas`: a solid `pkbroad` band at
   `/Touch 0.5`, drawn with `pkcanvas` set before a `7.5 0 translate` (3
   cells), has the same ink pixels as one drawn untranslated over their
   overlap, so the pattern doesn't move with the mark. The same holds
   for each `pkwet` pass.
5. `less_touch_less_ink`: ink is non-decreasing in `/Touch` over 0.2,
   0.4, 0.6, 0.8, 1.
6. `the_weave_is_in_canvas_points_at_any_dpi`: 72 and 144 dpi renders,
   the 144 block-downsampled 2x, agree within the golden tolerance.
7. `touch_box_contains_the_whole_mark`: for each preset, at `/Touch
   0.999` (every knuckle reached, merged), the touched ink is at least
   the untouched ink minus the knuckle gaps, measured per preset. It
   fails if a preset's reach row in 3.2 is too small.
8. `ghostscript_agrees_on_the_surface`: the rand-independent band from
   section 5, rendered by pscat and by `gs`, block-compared within the
   tolerance `tests/golden.rs` uses. It skips if gs is absent, like the
   other gs tests.
9. `pkcanvas_every_documented_parameter_changes_the_render` (the
   pkbroad pattern), and the catalog test that `pkcanvas`'s parameters
   carry units.
10. After slice S0, `touched_pdf_shares_its_clip`: the PDF of a touched
    pkdry stroke stays within 3x the untouched PDF's size, and gs
    rasterizes it back within tolerance (the `tests/pdf.rs` pattern).

**Byte identity across the repo**, in every slice that edits a preset
(#141's method):
- With an `origin/main` worktree and the same release binary, render
  every `examples/`, `gallery/` and `selftest/drivers/` file that loads
  paintkit (21 files, 30 page renders at #141), and compare md5s. Every
  file must match.
- Render `gallery/alpine_lake.ps` at 2x and compare it with the
  committed `gallery/renders/alpine_lake.png` byte for byte.
- Record the counts in the PR.

## 7. Slice plan

Each slice is one PR, gated by CI and, where a preset or `pkwet`
changes, the byte-identity check.

| Slice | What | Model | Difficulty |
|---|---|---|---|
| S0 | `src/pdf.rs`: consecutive elements with the same clip chain share one `q <clip> ... Q` group. Rust only, verified by gs round trip. Useful on its own for any heavily clipped page. | sonnet | medium |
| S1 | `pkcanvas`, the `pj-` sampler, memo and `pjcatch`. Self-tests, catalog row, test 8 (driving `pjcatch` from a test driver under a rectangle fill), test 9. No preset changes. | opus | hard |
| S2 | `/Touch` on `pktrowel` and `pkdry`: reach boxes and guards. Tests 1-5 and 7 for both. | opus | medium |
| S3 | `/Touch` on `pkfan`, `pkbroad` and `pkliner`, and the `pkliner` header wording (2.4). Tests 1, 2 and 7 for each. | sonnet | medium |
| S4 | Specimen `examples/paintkit_canvas_demo.ps`: the same light knife stroke and dry-brush stroke crossing, with and without the surface, plus a `/Touch` sweep and a touched stroke inside a directional `pkwet`. Catalog check, psart skill, README, a HANDOFF gotcha ("a touched mark is clipped in canvas space; set the canvas first"), NOTES. Test 6. | sonnet | medium |
| S5 | Optional: `/Touch` on `pkoil`, `pkdab`, `pknib` and `pkribbon`; a `pktouch` wrapper with an explicit `/Box` (Q1); `/Touch` as a `{t -> touch}` procedure (Q6); a ground tone (Q5). | sonnet | medium |

**Combined order with PAINT_LOADING.md.** L1 and S0/S1 touch no presets
and can run in parallel. The preset slices then go one preset body at a
time, so two open PRs never rewrite the same preset: L2 (pkbroad and
pktrowel colours), then S2 (pktrowel and pkdry touch), then L3 (pkwet
hook), then L4 (pkfan colours), then S3 (pkfan, pkbroad and pkliner
touch), then L5, then the two specimen slices L5b and S4.

## 8. Open questions for the maintainer

Each is phrased as a decision, with a recommendation.

- **Q1. A per-preset `/Touch` key, or a wrapper like `pkwet`?**
  Recommend the per-preset key, backed by the single `pjcatch`. The
  issue says presets opt in, and a preset knows its own reach, so the
  catch covers exactly its footprint. A wrapper (`{proc} << /Touch t >>
  pktouch`) can't see the procedure's path in advance, so it would need
  the whole clip region (the page is 68000 cells at the default weave)
  or an explicit `/Box`. The #113 argument against per-preset drift is
  answered by keeping one implementation (`pjcatch`) and one test per
  preset of its reach box (test 7). A `/Box` wrapper can come later (S5)
  for custom marks.
- **Q2. Canvas space = the CTM when `pkcanvas` is called?** Recommend
  yes (3.5). The alternative, `defaultmatrix`, would ignore a piece's
  own scaled coordinate system, and `/Weave` would stop being in the
  same units as every other length the artist passes.
- **Q3. Default `/Seed`: a constant, or one draw from the caller's
  stream as `pkwash` does?** Recommend the constant 1. A draw would make
  adding `pkcanvas` to a finished piece re-roll every mark after it,
  which is exactly the coupling 2.3 promises never to introduce. The key
  keeps `pkwash`'s meaning ("integer seed for this alone, leaving the
  caller's stream untouched"); only the default differs, and the
  `@param` says so.
- **Q4. Should `pkliner`'s arc-length tooth read the surface instead?**
  Recommend no (2.4). Its knots model where the tip runs dry, which is
  per stroke. The canvas acts through `/Touch` like everywhere else.
  The header's "property of the paper" wording is corrected rather than
  the mechanism changed. Revisit after the specimen if two touched
  liners crossing still look independent.
- **Q5. A ground tone (`pkcanvas /Tone`, the "liquid white" coat)?**
  Recommend deferring it (S5). If added, it should paint the ground in
  canvas space as an explicit opt-in, and must never silently become
  `pkwet`'s default `/Under`. That would change every `pkwet` call made
  after `pkcanvas`, the byte-identity break 2.3 rules out.
- **Q6. A constant `/Touch`, or a `{t -> touch}` profile like
  `/Pressure`?** Recommend a constant in the first pass. A profile means
  one catch per touch level along the stroke (the catch is a clip, not
  a per-stop test), which multiplies cost and needs a run-splitting
  design. Revisit with S5 once the specimen shows whether a constant
  reads well.
- **Q7. Ship `/Touch` before the PDF fix (S0)?** Recommend landing S0
  first. It is small, Rust-only and independently useful, and shipping a
  key that turns a 36KB PDF into 40MB, even if documented, would
  surprise an agent using `pscat-mcp`'s PDF path.
- **Q8. Reject a catch past 60000 cells, or coarsen the weave
  automatically?** Recommend rejecting, which is the file's convention
  everywhere. Silently coarsening would change the look of a mark based
  on its size, which an artist couldn't predict.
- **Q9. Defaults: `/Weave 2.5`, `/Mottle 8*Weave`, `/Relief 0.7`?**
  Recommend these as prototyped, with the specimen slice (S4) allowed to
  re-tune them before the defaults are pinned by tests.
