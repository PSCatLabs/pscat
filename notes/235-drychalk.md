## drychalk: the chalk preset and board dressing (issue #235, 2026-10-13)

The chalk medium over drykit (design: `docs/DRYMEDIA.md`). New
`lib/drychalk.ps` (`dc-`, catalog-tagged) requires artkit, paintkit and
drykit. It does not edit `lib/drykit.ps`, and it calls no drykit internals
except `dkstroke`, `dkbatch` and `dksurface`.

**Public procedures:**
- **`dcchalk`** treats each subpath of the current path as one chalk
  stroke and draws it through `dkstroke`, inside a `dkbatch`, or inside the
  caller's batch when one is open. Each stroke gets:
  - a landing heavier and wider than its body, a tired middle, and a lift
    that thins to a floor and fades. The floor means the stroke never ends
    in a point, which was the open D2 item about the `/Pressure` lift;
  - its own variation, from a private Schrage generator seeded by its
    position: touch by up to a band, width by up to a tenth, and strength
    one of three steps;
  - a dab for a dot: a short drag along the dot's own direction, never a
    clean disc;
  - dust: seeded specks beside the stroke, drifting down the board, drawn
    as one fill per call (`dcdust`).

  With `/Tilt`, the stick lies on its side and the mark is drawn as lanes.
  The two edge lanes are narrow and press a band harder. The face lanes
  between them press two bands lighter, overlap each other, and have
  their ends staggered. The result is C3's two bright edges over a
  granular face, and a ragged end rather than D2's three round bumps.
- **`dcchalkcolor`** sets white, or a soft yellow, pink, blue, green or
  orange.
- **`dcboard`** (`x y w h opts`) dresses a slate or a green board, and
  calls `dksurface` with `/Weave 0.7`. It draws:
  - the board's tone;
  - a dust film as one `colorimage`: value-noise clouds, eraser-swipe
    bands, lighter smudges, darker wipes, and dust that thickens at the
    ledge;
  - a grain of fine specks;
  - `/Ghost` writing, drawn as one faint, broken, wider chalk level, then
    erased under half the swipes;
  - felt streaks along each swipe;
  - the film laid back inside every wipe, so a wipe cuts the ghost;
  - ledge specks and an optional `/Tray`.

**Tuned against the photographs** (`docs/drymedia/chalk_compare_235.jpg`
puts five real crops beside the renders). Measured with PIL on
`chalk_crypto_letters` against a render of the same words at the same
scale on the same board tone. Brightness is given as a fraction of the way
from the board to white.

| | real (C1) | drychalk | first prototype |
|---|---|---|---|
| stroke core | 0.42 (with photo blur) to 0.48 (peak) | 0.52 | 0.58, and near white in `chalk_compare.jpg` |
| spread of stroke brightness | 0.06 | 0.05 | 0.01 |
| stroke width / cap height | 1/15 to 1/20 | about 1/16 at the specimen's sizes | |

Board tones read 0.23 to 0.30 in the photos, haze included. The bare
tones are set darker, and the film brings them up. Pixel values are in
the PR.

Steps that changed the look, in order:
1. **Weave 0.4 to 0.7 on boards.** At 2x a 0.4pt tooth is below a pixel,
   and its catch costs 2.25 times the cells. At 0.7 the break-up reads,
   and the specimen took a third of the time.
2. **Three levels and no wide halo.** A wide faint level reads as a grey
   outline, not dust (the advisor flagged this before the build, and a
   render confirmed it). A body-plus-narrow-bright-core design was tried
   and dropped: at writing size it read as outlined letters.
3. **Ghost writing as a single faint level**, with no plain-stroke smear.
   A plain smear in one flat colour showed as a bold silhouette wherever
   the haze under it differed.
4. **Haze range grows with /Haze squared.** A heavily erased board (C6)
   is milky. A lightly dusted one stays dark.

**Cost** (CPU `user`, release build, load average 3 to 9 on a shared machine):

| | pscat | gs | budget |
|---|---|---|---|
| `examples/drychalk_specimen.ps` at 2x | 8.5–9.0s | 4.1–4.5s | 10s |
| `tests/drychalk.rs` slowest render | 0.22s (a small board, `--profile ci`) | | 2s |

Three findings, each now handled in drychalk:
- **A board's names doubled gs's chalk cost** (#307). Ghostscript
  dictionaries are open hash tables. Loading artkit, paintkit and drykit
  leaves userdict about 80% full, and dcboard's scratch names pushed it to
  88%. Every operator lookup that misses userdict then got slower: one
  side stroke went from 0.21s to 0.44s, and the specimen took 14.8s in
  gs. drychalk's public procedures now run inside a private 1200-slot dict (about 500 names land in it on the specimen)
  (`dcscratch`), and gs takes 4.5s. drykit's and paintkit's own names
  still pay this cost.
- **drykit `/Fallout` in gs** (#308). It makes one artkit `scatter` call
  per 20pt piece. On this specimen gs took 26s with it and 10s without
  it. drychalk draws its own dust as one fill.
- **The field is evaluated on a coarse grid.** `dcfield` runs every 4
  samples, and the samples between are bilinear. Building the field took
  0.55s before and takes 0.09s now.

**Tests** (`tests/drychalk.rs`, 13, about 4s):
- every `dcchalk` and `dcboard` key changes the render, and the
  advertised keys equal the tested keys;
- determinism, and a word written elsewhere re-rolls nothing;
- the landing is brighter than the body (79 against 70) and the lift is
  thinner (38 lit pixels against 79), but never empty;
- a side stroke's edges are brighter than its face (156 against 146);
- the ghost is faint (+3 over the board) and fresh chalk is not (+90);
- the ledge band is brighter than with `/Ledge 0`, and wipes darken the
  board;
- the CI budget;
- gs block agreement on a dressed board with chalk (0.60, against 4.97
  for another seed), local only;
- a packed `/Ghost` under real gs, local only;
- the tray is drawn below the board, and `/Landing` presses dots harder
  (both from Codex's review of PR #310, as is the check that a later
  `dksurface` drops dcboard's tone).

Self-tests cover every guard and the caller's `rand` stream. A packed
`/Ghost` runs under `true setpacking`.

**Open, for the maintainer's eye:**
- **The glyph shapes are handscript's.** They read neater than a
  professor's hand (C1). That is the skeleton data (#236), not the medium.
- **Side strokes still end in a rounded rectangle**, not the chisel's
  square end. The lanes are round-capped drykit strokes.
- **The haze field is sampled every 6 device pixels**, bilinear from a
  grid four times coarser. It can't be seen at normal contrast, but a
  contrast stretch shows the blocks.
- **The before panel's handscript font comes out bolder in gs**, because
  its rand-driven pen width differs there. The chalk itself agrees.
