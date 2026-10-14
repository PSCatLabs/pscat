# DRYMEDIA.md: drykit design, chalk, crayon and charcoal (issue #233)

This is the design for epic #232. The maintainer approved it on 2026-10-13
(PR #265), accepting every recommended answer to Q1–Q9 in section 10.
Slices D1 and D2 (#234) are implemented in `lib/drykit.ps`
(`dksurface`, `dkcatch`, `dkstroke`, `dkbatch`); measurements are in 6.1,
and the deviations from this text are marked "As built". G1, the medium
files and the gallery pieces are not built yet.

Read these first:

- `docs/drymedia/REFERENCES.md`, the reference board. Observations C1–C10
  (chalk), Y1–Y6 (crayon) and H1–H8 (charcoal) are cited by number below.
- `docs/CANVAS_SURFACE.md`, the approved #182 design. This document builds on
  its surface and cites its sections (CS 3.1, CS 3.2, and so on) rather than
  restating them.
- `docs/drymedia/prototypes/`, the prototypes behind the cost and look
  claims below. They are not library code.

## 1. What the references say

Across the three media, eight mechanisms cover the observations:

| | Mechanism | Evidence | Shared or per medium |
|---|---|---|---|
| M1 | Pigment catches only on tooth peaks the stick reaches. Light pressure reaches only the highest peaks, so a thin line breaks into dots. | C3, Y2, H2, H4 | shared: the surface |
| M2 | A skipped valley becomes a **streak along the direction of travel**, not a round pit, because the rigid stick bridges the valley. | C4, Y1, Y3 | shared: the shape of the catch follows the tool |
| M3 | Pressure varies along a stroke: a heavy landing, a tired middle, a fading lift. Pressure changes brightness and break-up more than width. | C1, C2, Y4, H6 | shared: the engine |
| M4 | Deposit is a tone ramp, not on/off. Chalk is grey-white and brighter where pressed. A lifted highlight is grey, not paper white. | C1, C3, H8 | shared: deposit levels |
| M5 | The contact patch is not uniform. A worn flat face presses at its edges, and a chisel end gives square caps and split tails. | C3, Y6, H6 | shared core, tuned by preset |
| M6 | Loose particles: dust haze, fallout, ledge dust, hand smear. | C5, C9, H6 | shared core (fallout); chalk dresses the board |
| M7 | Removal and spreading: eraser swipes, ghosting, finger wipes, stump smudge, kneaded-eraser lift. A lift is paper colour applied through the peaks. | C5, C6, H5, H8 | per medium, on core primitives |
| M8 | Lines are built from several passes that never coincide: double underlines, re-stroked lines, scribble fill with seams. | C4, C8, Y2 | motion generators and layout, not the engine |

The first prototype (section 7) confirmed that M1 alone is not enough. A shared
binary catch on a plain stroke reads as a perforated screen (MODE 1 in
`prototypes/chalk_compare.jpg`). M2 (oriented knuckles) and M4 (two deposit
levels) are what move it toward chalk. No prototype render passes for real
chalk yet. Real chalk is greyer, softer and more varied along each stroke
(C1, C2). Tuning that look is #235's job, against these crops.

## 2. Surface model

### 2.1 Decision: build on #182 and extend it, without forking it

#233 asks whether drykit builds on #182, extends it, or defines the model for
#182 to adopt. **drykit builds on #182's surface and extends how it is
sampled.** In detail:

- **One height field.** drykit uses the #182 surface as-is: `pkcanvas`'s
  seeded height function over canvas-space cells (CS 3.1). That covers
  canvas-space capture (CS 3.5), the private Schrage generator (CS 3.4), the
  memo, and `pjheight`. A piece that mixes paintkit and drykit marks has one
  canvas, and two crossing marks catch on the same knuckles whichever library
  drew them. That is the epic's principle "converge on one surface model".
- **No edits to `lib/paintkit.ps`.** drykit `@requires` paintkit and *reads*
  `pjCTM`, `pjP`, `pjW` and `pjheight`. Measured load cost of
  artkit+paintkit is under 10 ms in pscat, so requiring paintkit is free.
  The epic's reason for a sibling file is edit conflicts, not load time.
- **drykit adds its own catch emitter, `dkcatch`.** It uses `pjcatch`'s
  selection rule and scale law (CS 3.2 step 4) unchanged, so a knuckle is
  selected at exactly the same `/Touch` in both. It differs in three ways:
  1. **Oriented knuckles (M2).** Each knuckle is an ellipse whose long axis
     follows the local direction of travel, stretched by `/Streak`
     (`prototypes/dqcatch.ps`). `pjcatch` alternates warp and weft, which is
     right for canvas and wrong for paper or slate.
  2. **Swept-band cells.** It visits only the cells within reach of the
     walked centerline, not every cell of the mark's bounding box. For
     handwriting the prototype counted about 2.9× fewer cells (section 7).
     Each knuckle takes its orientation from the nearest centerline stop.
     A per-call dict deduplicates cells, keyed as `pjmemo` is (CS 3.3).
  3. **A reach margin that includes the stretch.** CS 3.2's 1.25W margin
     assumes the shipped knuckle size. `dkcatch` adds
     `0.8 * 1.375 * (Streak - 1) * W` and carries its own whole-page test,
     like CS 6 test 7.
- **Nesting is preserved.** A knuckle selected at touch t is selected at any
  higher touch, and its scale only grows, so catch(t1) ⊂ catch(t2) for
  t1 < t2. The prototype measured 0 pixels of a lower-touch catch outside a
  higher one, at touches 0.3, 0.5, 0.7 and 0.9 (`prototypes/crossing.ps`
  WHICH 3). Pressure bands and deposit levels both rely on this (2.3, 3.3).

### 2.2 Does the tooth stay shared when knuckles are oriented?

This was measured with #182's phi test (CS 3.3): two wide bands, one at 0°
and one at 60°, each rendered alone, compared over their overlap at
`/Weave 2.5 /Touch 0.6`, for seeds 3, 7, 11, 19 and 23.

| Catch | phi between the two bands | control (second band, different seed) |
|---|---|---|
| `pjcatch` (isotropic) | 1.00 at every seed | 0.05 to 0.33 |
| `dkcatch`, `/Streak 1.8` | 0.37 to 0.60 (mean 0.48) | -0.09 to 0.24 (mean 0.03) |
| `dkcatch`, `/Streak 1.25` | 0.40 to 0.60 (mean 0.51) | -0.10 to 0.21 (mean 0.03) |

Orientation, not stretch, lowers the correlation: even 1.25 barely moves it.
The tooth is still clearly shared (a mean of about 0.5 against about 0.03), but #182's
bar of phi > 0.5 at *every* seed is too strict for oriented catches. #234's
"shared surface" acceptance should be: mean phi over five seeds ≥ 0.4, and at
least 0.25 above the control's mean (open question Q4).

These numbers were measured at 144 dpi with `/Weave 2.5`, about 5 device
pixels per cell. drykit's surfaces are 0.35–0.9pt, which is 1–2 pixels per
cell at 144 dpi, so the same measurement there would be dominated by
antialiasing. #234's test must render at 4 or more device pixels per cell,
for example 720 dpi for `/Weave 0.4`, over a small region.

### 2.3 Named surfaces

#234 ships five surfaces. Each is a set of `pkcanvas` options plus a ground
tone that drykit keeps under its own prefix. Starting values come from the
references and the prototype; #234's specimen re-tunes them before tests pin
them:

| `/Surface` | `/Weave` [points] | `/Relief` | `/Mottle` [points] | Ground | Reference |
|---|---|---|---|---|---|
| `/slate` | 0.4 | 0.55 | 4 | near black, slight blue | C3 |
| `/greenboard` | 0.4 | 0.55 | 4 | dark grey-green | C1, C5, C7 |
| `/newsprint` | 0.35 | 0.4 | 3 | warm light grey | not yet sourced |
| `/drawing` | 0.6 | 0.65 | 6 | off-white | H1, H2, H4 |
| `/construction` | 0.9 | 0.75 | 8 | saturated colour, chosen by the caller | Y1, Y2 |

These are plain keys of the #182 surface. **Laid lines** (H1's vertical
pattern on drawing paper) cannot be added without changing `pjheight`, which
belongs to paintkit. They are deferred (open question Q3).

### 2.4 Ghostscript: fill adjust erases fine tooth

`prototypes/gs_filladjust.png` shows this. At `/Weave 0.7`, 216 dpi, gs
paints the oriented catch as **solid strokes**: every gap closes. The
default fill adjust dilates each fill and clip by part of a pixel, so gaps of
1–2 device pixels disappear. At `/Weave 3` the pattern survives but fattens.
`-dGraphicsAlphaBits=4` doesn't help. `0 0 .setfilladjust2` does restore
the texture.

The shipped `pjcatch` has the same problem: MODE 1 at `/Weave 0.7`, 216 dpi,
comes out almost solid in gs.

`.setfilladjust2` is a gs-internal operator, not PostScript. It is available
under gs's default `-dSAFER`: a single concatenated file, run as plain
`gs file.ps`, found it and rendered the texture. drykit calls it inside its
own `gsave`, guarded so pscat skips it:

```
/.setfilladjust2 where { pop 0 0 .setfilladjust2 } if
```

The gs check in #234's tests renders with that guard in place. CS 5 already
recorded the related 46%/38% ink difference at 72 dpi.

## 3. Core engine (`lib/drykit.ps`, prefix `dk-`)

`dk`, `dc`, `dy`, `dh` and `sg` have no `/name` use anywhere under `lib/`,
`examples/`, `gallery/`, `tests/`, `selftest/`, `site/` or `docs/` (scanned
for this document). Every key below follows #141: one meaning per key, and a
bracketed unit from `tests/capabilities.rs`'s closed list on every `@param`.
That test's unit check is extended from paintkit to drykit's files.

### 3.1 Calls

```
opts dksurface -     % names the surface; calls pkcanvas; paints nothing
opts dkstroke  -     % deposits along every subpath of the current path, in the current colour
proc dkbatch   -     % runs proc; its dkstroke calls render together, level-major (3.3)
```

The pigment is the current colour, as everywhere in paintkit. No key sets it.

`dksurface`:

| Key | Unit | Default | Meaning |
|---|---|---|---|
| `/Surface` | `[name]` | `/drawing` | One of the five in 2.3. Sets the defaults of the keys below. |
| `/Seed`, `/Weave`, `/Mottle`, `/Relief` | as `pkcanvas` | from `/Surface` | Passed through to `pkcanvas` unchanged, with paintkit's meanings. |
| `/Ground` | `[rgb array]` | from `/Surface` | The surface's own colour. It is drykit's default `/Under` (below). `dksurface` does not paint it: board and paper dressing belong to the medium files. |

`dkstroke`:

| Key | Unit | Default | Meaning (mechanism) |
|---|---|---|---|
| `/Width` | `[points]` | 2 | Contact width at the tip. |
| `/Touch` | `[0..1]` or `[proc]` | 0.8 | Pressure against the tooth, with #182's meaning (CS 2.2). A `{t -> touch}` procedure, t = 0..1 along each subpath, gives a pressure profile (M3). It is resolved once per walked stop (HANDOFF's "/Pressure inside a loop" gotcha). |
| `/Bands` | `[count]` | 4 | When `/Touch` is a procedure, the number of touch levels it is quantized to. One catch is built per band per level. This is #182's deferred Q6 (CS 8), solved by banding (3.3). |
| `/Pressure` | `[proc]` | unset | `{t -> mult}` width profile, paintkit's meaning: a landing blob, a tapered lift. |
| `/Wear` | `[fraction of /Width]` | 0 | How much the contact widens from start to end of a subpath as the tip flattens (Y6). |
| `/Tilt` | `[degrees]` | 0 | 0 draws with the tip. 90 lays the stick on its side: the mark widens toward `/Side`, and the edges of the patch press harder than its face (M5, C3). |
| `/Side` | `[points]` | 8 | Length of the stick in contact when it lies on its side. |
| `/Streak` | `[ratio]` | 1.4 | How much each knuckle is stretched along travel (M2). 1 gives round knuckles. |
| `/Levels` | `[count]` | 2 | Deposit levels (3.3), 1..4. |
| `/Strength` | `[0..1]` | 0.85 | How strongly the top level reads against `/Under`, with surfacekit's meaning. Chalk is about 0.8 (C1); compressed charcoal is about 0.95. |
| `/Under` | `[rgb array]` | the surface's `/Ground` | What lies beneath, with pkwet's meaning: lower levels lerp from it toward the pigment. drykit can't read the canvas back (HANDOFF), so it must be told. |
| `/Halo` | `[fraction of /Width]` | 0.25 | Extra width of the faintest level: dust beside the stroke (M6). |
| `/Load`, `/Dropout` | `[per /Width]` | 1, 0 | Tool-side contact chain, with pkdry's meaning: a stick that lifts off and lands again along the stroke, independent of the tooth. |
| `/Lanes` | `[count]` | 1 | Parallel lanes across the contact patch, each with its own contact chain: crayon striation (Y3) and split charcoal tails (H6). |
| `/Fallout` | `[count per points]` | 0 | Loose particles per point of stroke length (M6), placed with artkit's `scatter`. |
| `/Drift` | `[points]` | 0 | How far fallout drifts down canvas-space y: chalk on a vertical board falls. |
| `/Seed` | `[integer]` | from the position | Seed for the tool-side randomness (contact chain, lane offsets, fallout). By default it is derived from the first point's canvas-space position, so adding a stroke elsewhere never re-rolls this one. |

`/Spread` is deliberately absent: nothing here is a fraction of `/Width`
scattered across it. `/Density` is absent too: `scatter` and stipplekit
already give it meaning (marks per area), so deposit strength is
`/Strength`.

### 3.2 Determinism

drykit never calls `rand` or `srand`. Its tool-side generator is the
`pjrand` pattern (a Schrage LCG) under its own `dk-` state. Like `pkcanvas`
(CS 3.4), every call leaves the caller's random stream untouched, so
`--sweep-seed` varies a piece's composition and keeps its texture. The
surface takes no draws at all. `scatter` (fallout) takes a `/Seed` and
restores the caller's stream itself.

The default `/Seed` hashes the first point's canvas position, quantized to
0.01pt. A small real-number difference between pscat and gs can cross a
quantization step and re-roll that stroke's whole tool-side texture, not
just one knuckle. So any pscat-versus-gs comparison (test 6) pins `/Seed`.
The two interpreters' `rand` also differ: the prototype board, laid out
with `rand`, produced 1984 catches in pscat and 1964 in gs.

### 3.3 Levels, bands, and level-major order

One `dkstroke` call does the following:

1. **Walk** every subpath with `walkpath` (no new sampler). Resolve each
   stop's touch, width, angle and lane contacts once.
2. **Group** stops into runs of equal touch band.
3. **For each level k = 1..L**, faintest first:
   - **Colour:** lerp(`/Under`, pigment, `/Strength` × k/L).
   - **Touch:** band touch + (L − k) × 0.12. Fainter levels reach lower
     knuckles, and nesting puts each brighter level on top of the knuckles
     the fainter one already covered.
   - **Width:** the faintest level is widened by `/Halo`.
   - **Draw:** for each band, build `dkcatch` over that band's runs, then fill
     their ribbon geometry through it.

The order is **level-major across all of the call's subpaths**. Opaque
levels drawn stroke by stroke would let stroke B's dim level overpaint
stroke A's bright core where they cross. Drawing every dim level first
avoids that, and keeps the monotone "more chalk is brighter" look without
alpha.

**As built**, a batch record also keeps the CTM its stroke was given in,
so strokes drawn inside their own `gsave ... rotate ... grestore` render
in place when the batch ends (the clip used is the one in force then).
Each subpath is seeded from its own first point, so one call with two
subpaths and two calls in a batch draw identically. A second pigment in
one batch raises `dkbatch-mixes-pigments`. Within a pass, runs with the
same touch, streak and colour share one catch, chunked under `dkbudget`;
the catch takes a thinned centerline (about one stop per twice its
reach, the thinned stops' reach grown by the chord deviation), because
walkpath's pitch made short capsules revisit each cell up to 6 times.

**Across calls, opaque levels lighten crossings.** `prototypes/layering.ps`
draws two charcoal hatch layers crossing at 90°, each line with a dim level
and a dark core. Drawn as two calls, the second layer's dim level paints
over the first layer's dark core, so every crossing gets a grey break where
it should be darkest (`prototypes/layering.png`, left). Level-major across
both layers keeps the crossings dark (right). Tone built across calls is
the normal case: cross-hatching, re-stroked chalk lines (C4, M8), and
scribble fill. So the core adds a batch:

```
{ ... dkstroke calls ... } dkbatch -
```

Inside a batch, `dkstroke` runs step 1 only and appends the resolved stops
to a record. When the batch ends, all records are rendered level-major, then
band by band. Step 1 already builds those records, so a batch costs memory,
not extra catches. Batches don't nest: an inner `dkbatch` raises
`dkbatch-cannot-nest`.

Each pigment gets its own batch, in drawing order. If colours B and A shared
a batch, B's dim level would be drawn under A's core. Real wax or charcoal
in a later colour lies on top.

Ribbon geometry comes from round-capped strokes for constant width, and from
`pkribbon`'s outline for `/Pressure` and `/Wear`. **As built:** a varying
width is a union of a disc at every stop and a quadrilateral along every
segment instead, so the ink stays exactly within each segment's wider end,
the reach `dkcatch` is cut for. pkribbon is not called. drykit calls `pkribbon`
only for geometry, and all its own state is under `dk-`, so `pkribbon`'s
habit of clobbering `pk*` names cannot reach it (HANDOFF).

### 3.4 Effects built from the core

The core is the engine. Each effect below is a call pattern for a medium
file:

| Effect | Built as |
|---|---|
| Chalk landing and lift (C2, C3) | a `/Touch` profile that is high for the first ~10% and dips mid-stroke, plus a `/Pressure` lift taper |
| Side-of-chalk underline or shading | `/Tilt 80`, `/Side` 6–12pt |
| Dust halo and fallout (C5, C9) | `/Halo`, `/Fallout` and `/Drift` on chalk; a board-dressing routine adds ledge dust |
| Ghosting (C5) | `dkstroke` of old text at `/Strength` 0.08–0.15, then a haze layer |
| Eraser swipe (C6) | a wide, low-strength `dkstroke` in haze colour along an arc, `/Streak` about 3, `/Lanes` about 6 |
| Finger wipe (C5) | a `dkstroke` in board colour over haze (wiping removes dust) |
| Crayon striation and build-up (Y1, Y3) | `/Lanes` plus `/Streak` about 1.8. A turn raises touch through the motion generator's profile. |
| Crayon colour layering (not yet sourced) | two calls in different directions: the catches differ in orientation and lanes, so the first colour shows through the second's gaps |
| Charcoal hatching (H2, H4) | hatch line *paths* inked by `dkstroke`, with all the layers of one pigment in one `dkbatch`. hatchkit draws lines itself and has no path hook, so #241 adds a `/Mark {path}` hook to `lib/hatchkit.ps` (a small, separate change) rather than copying its geometry. |
| Smudge (H1, H5) | a wide `dkstroke` at high touch (fills the valleys), `/Strength` low, `/Under` = the caller's tone, directional `/Streak` |
| Kneaded-eraser lift (H8) | `dkstroke` in *paper* colour with `/Under` set to the charcoal tone beneath: paper colour through the peaks |

## 4. Stroke skeletons (#236)

### 4.1 Decision: a new, catalog-migrated glyph library

**`lib/strokeglyphs.ps`, prefix `sg-`.** handscript is not extended:

- **The catalog.** `build.rs` rejects `@kind: Type3Face`, so
  `lib/handscript.ps` stays in `LEGACY_FILES` and cannot carry catalog tags
  (HANDOFF). #236 requires catalog documentation.
- **Data, not ink.** handscript's glyphs reach the page through `show` and
  `BuildChar`, which paint. Dry media need the centerline as a *path* to
  hand to `dkstroke`.
- **Its jitter uses the caller's `rand`** (`jit`, read through `/J`). The
  prototype also showed that re-running `drawstroke` re-jitters, so a catch
  box and the ink came from different points
  (`prototypes/surface_cost.ps`, `dqjitter`). Skeleton jitter must be
  resolved once into data, from a private generator.

### 4.2 Format

Glyphs keep handscript's proven polyline shape, extended with metrics:

```
/name [ advance  [llx lly urx ury]  [x y x y ...]  [x y x y ...] ... ]
```

- **Units.** The design grid is 1000 per em, as in handscript: baseline 0,
  x-height 500, cap height 700, descender -250.
- **Strokes.** Each stroke is a flat `[x y ...]` polyline, smoothed into
  curves by the same quadratic midpoint rule as handscript's `drawstroke`.
  Stroke order and direction are data, matching how a writer draws the
  glyph: down-strokes go down, and crossbars are drawn after stems. #239's
  motion generators and the chalk landing profile both depend on it.
- **Pressure is not stored per point.** Landing and lift are the medium's
  `/Touch` and `/Pressure` profiles (3.4), so one skeleton serves pen,
  chalk, crayon and charcoal.
- **Lowercase.** It is imported from `HandScriptDict /CharDefs` at load
  (`@requires: (lib/handscript.ps) run`), so there is one copy, and
  handscript's own output stays byte-identical, as #236 requires.

### 4.3 Calls

- **`(text) x y opts sgpath -`** appends the jittered skeleton of `text` to
  the current path, with one subpath per glyph stroke. It paints nothing.
  - `/Size [points]`, `/Slant [degrees]`, `/Jitter [ratio]` (a fraction of
    the em, replacing glyph units).
  - `/Seed [integer]`: the glyph at index i uses seed + i. Editing later
    text never re-rolls earlier glyphs.
- **`(text) opts sgwidth -> w`** measures with fixed advances only, never
  ink. This is handscript's `hs-linecount` doctrine.
- **Growing symbols** (Σ Π ∫ √ ( ) [ ] { } |) are procedures, not fixed
  arrays: `height depth opts sggrow -> advance` appends a path sized to
  the content, for #237's box model.
- **Metrics for #237.** Each glyph carries a math class (ordinary, operator,
  relation, open, close, punctuation), its italic correction (0 for
  upright), and the axis height (250) as a dial.

`sgpath`, `sgwidth` and `sggrow` carry `@kind: Procedure` tags. The glyph
tables are `@internal` data. The file is tagged from its first commit, so it
never enters `LEGACY_FILES`.

## 5. Code location

| File | Prefix | Owner | Contents |
|---|---|---|---|
| `lib/drykit.ps` | `dk-` | #234 | `dksurface`, `dkstroke`, `dkcatch`, the private generator, surfaces |
| `lib/drychalk.ps` | `dc-` | #235 | chalk preset, board dressing, ghosting, erasing |
| `lib/drycrayon.ps` | `dy-` | #239 | crayon preset, crayon box palette, child motion generators |
| `lib/drycharcoal.ps` | `dh-` | #241 | vine, compressed and pencil presets, hatching, smudge, lift |
| `lib/strokeglyphs.ps` | `sg-` | #236 | skeleton glyphs and growing symbols |
| `lib/mathkit.ps` | `mk-` | #237 | TeX-subset layout to `sgpath` calls |

The core is confirmed as `lib/drykit.ps`, and **each medium gets its own
file**. #235, #239 and #241 run in parallel, so one shared file would
conflict, as paintkit has. Each medium file
`@requires: (lib/artkit.ps) run (lib/paintkit.ps) run (lib/drykit.ps) run`.
Media added later (pastel, graphite, conté) each get a `lib/dry*.ps` file
and a gallery ticket, as the epic already plans.

## 6. Performance budget

### 6.1 Measured

`prototypes/surface_cost.ps` renders a full 720 × 480 board: 12 lines of
handscript skeletons, about 600 glyphs, 1980 glyph strokes, each with a body
and a landing dab. Times are CPU (`user`) on the release pscat and gs
10.07.1, as the range over two runs. Wall time was unreliable under the
machine's load average of about 8:

| Case | Catches | Cells visited | pscat | gs |
|---|---|---|---|---|
| plain strokes (no catch) | 0 | 0 | 0.03s | 0.04s |
| `pjcatch`, `/Weave 0.7` | 1984 | 388k | 4.2s | 2.3s |
| oriented `dqcatch`, `/Weave 0.7` | 1984 | 388k | 8.5s | 4.0–4.9s |
| oriented, `/Weave 0.4`, thin pen, 2 levels | 3928 | 1.59M | 28.1–35.7s | 15.1–16.7s |

That works out to about 11 µs per cell for the isotropic catch and about
22 µs per cell oriented. The difference is the rotated-ellipse arithmetic.
Resolution doesn't matter (216 dpi costs the same, as CS 3.3 found): the cost
is interpretation. The swept-band estimate printed by the prototype for the
last row is 553k cells, about 2.9× fewer. That suggests about 10s in pscat
and 5s in gs for the core on a full board.

**That figure is an estimate.** The swept-band catch hasn't been built. The
measured bounding-box cost (28–36s) is already at the board budget before any
board dressing, and ghost text is roughly a second board of glyphs at low
strength. So part of D1's acceptance is to build the swept-band catch,
measure it on this board, and report the result against 6.2 before D2
starts.

**Measured in D1 (#234).** `dkcatch` is built, and `surface_cost.ps`
gained MODE 4 (one level through `dkcatch`) and MODE 5 (MODE 3's two
levels and landing dabs through `dkcatch`). The same case as the last row
above (`/TOUCH 0.82 /PEN 36 /GRAIN 0.4`), CPU `user` time, two runs each,
at 144 dpi (`--dpi 144`, gs `-r144`), load average about 3.5:

| Case | Catches | Cells | pscat | gs |
|---|---|---|---|---|
| oriented, bounding box (MODE 3, re-measured) | 3928 | 1.59M | 28.3s | 16.1s |
| `dkcatch`, two levels (MODE 5): the core board | 3928 | 1.44M | 14.4–14.5s | 7.2s |
| `dkcatch`, one level (MODE 4): a ghost-text proxy | 1984 | 0.66M | 7.1–7.2s | 3.5s |
| `portrait_cost.ps`: 3000 hatch strokes × 2 levels + 6 smudges | 6006 | 2.37M | 34.9–36.9s | 12.7–12.8s |

- **The 2.9× was not there in cells.** The swept band built 1.44M unique
  cells, 9% fewer than the bounding boxes and 2.6× the 553k estimate. The
  estimate's band was the stroke width plus 2.5 grain. A stretched
  knuckle reaches (0.88 grow Streak + 0.22) W from its cell centre, which
  is 0.74pt at `/Streak 1.8`, against 0.5pt. The estimate also left out
  the round end of each short glyph stroke, worth πR² apiece.
- **The time halved anyway.** `dkcatch` costs about 10 µs per cell in
  pscat, against the prototype's 18: one emission pass over a first-seen
  list, with each cell's height read once. gs is about 5 µs.
- **Against 6.2:** the core board plus a one-level ghost-text pass is
  21.6s in pscat and 10.7s in gs, leaving about 8s of the 30s for board
  dressing. The portrait proxy is 35–37s in pscat and 13s in gs, under 60s.
  The proxy uses 40pt strokes at 1.2pt width; it is not #242's portrait,
  and its 2.37M cells are about 1.6× the core board.
- **The anti-aliasing margin is not paid when rendering.** A pixel-exact
  reach test needs 1.5 device pixels more reach (0.75pt each side at 144
  dpi), because a knuckle that ends inside an edge pixel still shades it.
  That margin cost the board 60% more cells (2.31M, 24s in pscat), so it
  is `dkslack`, 0 by default, and only the reach test sets it.

**Measured in D2 (#234), through `dkstroke` and `dkbatch`.**
`surface_cost.ps` MODE 6 draws each glyph as one `dkbatch` of `dkstroke`
calls (two levels, the landing as a `/Touch` and `/Pressure` profile);
`/LEVELS 1` is the ghost-text proxy. `portrait_cost.ps` with
`/ENGINE true` draws all 3000 hatch strokes as `dkstroke` in one
`dkbatch`. Same machine and method as above:

| Case | Catches | Cells | pscat | gs |
|---|---|---|---|---|
| board core (MODE 6) | 2484 | 0.83M | 19.7–19.9s | 10.1–10.3s |
| ghost-text proxy (MODE 6, one level) | 1242 | 0.43M | 11.5–12.9s | 6.0–6.6s |
| **board core + ghost** | | | **31.2–32.8s** | **16.1–16.9s** |
| portrait proxy (`/ENGINE true`) | 35 | 1.76M | 39.8–41.9s | 27.4–27.9s |
| `examples/drykit_specimen.ps` (trimmed to fit) | | | 7.5s | 3.5s |

The portrait and the specimen are inside 6.2's budget in both
interpreters. The board core plus a one-level ghost pass is inside it in
gs and about 7% over in pscat, before board dressing: the engine adds the
walk, the per-stop resolution and the ink to D1's bare catches, and its
catches cost about 13 µs per cell in pscat. #283 has the breakdown and the
levers (chiefly a reach band cut per orientation). #238 is the real piece.

Two things scale beyond that:

- **Memo cap.** A full board at `/Weave 0.4` has about 2.2M cells, past
  `pjmemocap` (250000), so later cells are computed fresh. Measured fresh
  cost is about 2× memoized (CS 3.3).
- **Cells per catch.** `pjbudget` (60000 per catch) is not reached by glyph
  strokes (about 400 cells each). A side-of-chalk sweep 300pt long at 12pt
  width and `/Weave 0.4` is about 25k cells.

### 6.2 Budget

| Piece | pscat (release, 2x) | gs | Basis |
|---|---|---|---|
| full chalkboard (#238) | ≤ 30s | ≤ 30s | measured 28–36s with bbox catches; ~10s estimated with swept band; board dressing and ghost text on top |
| full portrait (#242) | ≤ 60s | ≤ 60s | ~3000 hatch strokes × 2 levels, plus smudges; about 2× the board in cells |
| child's drawing (#240) | ≤ 20s | ≤ 20s | fewer, larger strokes at coarser `/Weave` (0.9) |
| each specimen in `examples/` | ≤ 10s | ≤ 10s | specimens are smaller than pieces |
| each `tests/drykit*.rs` render | ≤ 2s under `--profile ci` | — | CI (#204) |
| all of `tests/drykit*.rs` together | ≤ 30s under `--profile ci` | — | CI (#204) |

CI renders no gallery pieces. It runs `scripts/selftest.sh`, `cargo test`
and a perf bench on `examples/sierpinski.ps` and `gallery/fern.ps`. The
site build (`scripts/build_site.sh`) renders the gallery to PNG only. The
budgets above that CI can't see are checked in each gallery PR's test plan.
A piece over budget raises the cost question in its PR, rather than quietly
coarsening the tooth (CS Q8's reasoning).

### 6.3 Export size

Every catch is a clip. PDF export shares one clip group across consecutive
elements with the same clip (#219, CS 3.3's slice S0, landed), and page
streams are Flate-compressed (#256). SVG pays
about 380 bytes per knuckle (CS 3.3). A full chalkboard is about 500k
knuckles, so **SVG export of a drykit piece is about 200 MB**. The docs say
so. The site and gallery build PNG only (`scripts/build_site.sh`), so the
gallery tickets are unaffected. A cheaper vector form is open question Q7.

## 7. Prototypes

All of these are in `docs/drymedia/prototypes/` and run from the repo root.
They read paintkit's internals and change nothing in it.

- **`surface_cost.ps`** draws the board in section 6, in four modes: plain,
  `pjcatch`, oriented, and oriented with two levels.
  `chalk_compare.jpg` compares all four with two real chalk crops.
  - The plain stroke reads as a font.
  - `pjcatch` reads as a perforated screen.
  - Oriented knuckles read as mostly solid strokes, with fine streaks
    along the travel direction and ragged edges.
  - Thin pen, fine tooth and two levels comes closest. It is still too
    bright, too even and too clean against C1 and C3.
- **`dqcatch.ps`** is the oriented emitter (2.1), as a stand-alone file.
- **`crossing.ps`** runs the phi and nesting measurements in 2.1 and 2.2.
- **`layering.ps`** draws two hatch layers as two calls and as one
  level-major call (`layering.png`), the evidence for `dkbatch` (3.3).
- **`gs_filladjust.png`** is the Ghostscript result in 2.4.

## 8. Test plan for #234

Self-tests next to the guards:

- `dksurface` rejects unknown surfaces and malformed keys.
- `dkstroke` rejects `/Touch` outside 0..1, a non-procedure `/Pressure`,
  `/Levels` outside 1..4, `/Bands` outside 1..16, and a call with no
  surface set.
- `dkstroke` takes no draws from the caller's stream.

Rust tests (`tests/drykit.rs`):

1. **Shared surface:** the 2.2 criterion, for two crossing `dkstroke`s.
2. **Nesting:** ink at touch t1 is a subset of ink at t2 > t1, for a filled
   band.
3. **Reach:** each stroke's own catch box against a whole-page box is
   pixel-identical (CS 6 test 7), over an extremes matrix: max `/Streak`,
   `/Tilt 90`, `/Wear 1`, an above-1 `/Pressure`, a degenerate subpath, and
   `/Fallout` with `/Drift`.
4. **Determinism:**
   - same seed, same pixels;
   - a mark drawn after the stroke from the caller's stream doesn't move
     when any drykit key changes;
   - adding a stroke elsewhere doesn't change this one (position seed).
5. **Level-major order:** two crossing strokes, once in one call and once
   as two calls inside a `dkbatch`. In both, no dim-level pixels sit over
   bright-level pixels at the crossing, and the two renders are
   pixel-identical.
6. **gs agreement:** with the 2.4 guard in place and `/Seed` pinned (3.2),
   pixels are compared at 288 dpi as in #253's test 8. This runs locally only (CI has no gs).
7. **Every documented parameter changes the render** (the pkbroad pattern),
   and every `@param` has a unit. The capabilities unit check is extended to
   drykit's files.
8. **Budget smoke test:** a 100-stroke render under `--profile ci` stays
   under 2s.

## 9. Slice plan

| Slice | Issue | What | Model | Thinking |
|---|---|---|---|---|
| D1 | #234 | `dkcatch` (oriented, swept band, reach), private generator, `dksurface`, tests 1–3, the gs guard. Acceptance includes measuring the swept-band board against 6.2. | opus | xhigh |
| D2 | #234 | `dkstroke` and `dkbatch`: walk, bands, levels, `/Pressure`/`/Wear`/`/Tilt`, contact chain and lanes, fallout through `scatter`, tests 4–8, the specimen (three pressures × three tilts × wear × five surfaces) | opus | high |
| G1 | #236 | `lib/strokeglyphs.ps`: format, `sgpath`/`sgwidth`/`sggrow`, imported lowercase; then caps, digits and Greek; then operators and growing symbols | opus | high |
| H0 | #241 (prep) | a `/Mark` path hook in `lib/hatchkit.ps`, byte-identical when unset | sonnet | medium |
| — | #235, #237, #239, #241 | as filed, on top of D2 and G1 | as filed | as filed |

D1 and G1 can start together once this design is approved. D2 depends on D1.

**Coordination with paintkit's open slices.** drykit reads but never edits
`lib/paintkit.ps`, so no drykit PR conflicts with paintkit's L-slices or
S-slices. The dependency is on `@internal` names (`pjheight`, `pjrange`,
`pjhash`, `pjCTM`, `pjP`, `pjW`). D1 pins that contract with a drykit test
that fails if any of them changes signature. Moving the `pj-` section into a
shared `lib/toothkit.ps` should be filed as a follow-up once this design is approved, to run after
paintkit's in-flight slices land (Q1).

## 10. Open questions for the maintainer

Each is phrased as a decision, with a recommendation.

- **Q1. Depend on paintkit's `@internal` `pj-` names, or extract them first?**
  Recommend depending on them now, pinned by a contract test (section 9),
  with extraction to `lib/toothkit.ps` as a follow-up issue. Extracting
  first would edit paintkit.ps while L2–L5 and S2–S5 are open, which is the
  conflict the epic set out to avoid.
- **Q2. Oriented knuckles in drykit only, or offered back to paintkit?**
  Recommend drykit only for now. A paintkit adopter (a dry brush on paper)
  can call `dkcatch` later. Canvas weave really is isotropic and
  alternating, so `pjcatch` stays right for canvas.
- **Q3. Laid lines on drawing paper (H1)?**
  Recommend deferring them. They need a directional term in `pjheight`,
  which is a paintkit change. Revisit after the #242 portrait if its stumped
  tones look too uniform.
- **Q4. The shared-tooth acceptance for oriented catches.**
  Recommend mean phi ≥ 0.4 over five seeds, and ≥ 0.25 above the control's
  mean, in place of #182's per-seed > 0.5 (2.2).
- **Q5. Should `/Touch` keep #182's meaning in drykit, or should dry media
  have their own pressure key?**
  Recommend keeping `/Touch`. It is the same physical quantity, contact
  against the same tooth. A `[proc]` form is added here first (3.1), and
  paintkit can adopt it later. `/Pressure` keeps paintkit's width-profile
  meaning.
- **Q6. Level-major order across calls?**
  Recommend `dkbatch` (3.3), in slice D2. The prototype showed that drawing
  layers in separate calls lightens every crossing between them. Because
  `dkstroke` already resolves stops into data, a batch is just a list of
  records, not a general deferred-drawing queue. Batches hold one pigment
  each and don't nest.
- **Q7. SVG size.**
  Recommend documenting it and publishing drykit pieces as PNG. A future
  slice could emit the catch as a single compound path per level, a
  `clipPath` reused across levels, but SVG's cost is the knuckle count
  itself.
- **Q8. Using a Ghostscript-internal operator.**
  Recommend the guarded `.setfilladjust2` (2.4). Without it, gs renders
  drykit marks as plain strokes at any practical tooth size. It is skipped
  where absent, so pscat and other interpreters are unaffected.
- **Q9. The reference gaps** (REFERENCES.md's last section). Recommend
  that each gallery ticket sources its own missing traits before its
  comparison. None of the gaps changes the core design.
