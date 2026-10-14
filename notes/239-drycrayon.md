## drycrayon: crayon preset and child-drawing motions (issue #239, 2026-10-13)

New `lib/drycrayon.ps` (`dy-`, catalog-tagged, 12 self-test blocks), on
`dkstroke`, `dkbatch` and `dksurface`. It does not edit `lib/drykit.ps`.

**Public:**
- `dypaper`: crayon paper. It is `dksurface` with `/construction` at
  `/Weave 1.4 /Relief 0.65 /Mottle 8` on a warm white `/Ground`. Any
  dksurface key overrides.
- `dybox`, `dycolors` and `dycolor`: a 16-crayon box as named rgb arrays,
  in box order.
- `dycrayon`: the preset.
- Six motions: `dywobble`, `dyscribble`, `dyloop`, `dyfigure`, `dyrays`
  and `dygrass`.

### How dycrayon is built

Each subpath is drawn as two one-level `dkstroke` calls, inside one
`dkbatch`:

- a **rim**: the full contact width, at lower touch (0.16 below for a new
  point, 0.06 for a worn flat);
- a **core**: 0.5 to 0.85 of the width, at the stroke's touch.

A rounded tip presses hardest at its centre, so the rim gives the ragged,
speckled edge and the core the dense middle. Both levels are raised by
0.12, which is where dkstroke's faint level sits. So a crayon at a
given `/Touch` covers the tooth a two-level dkstroke at that `/Touch`
covers, and `/Touch` keeps drykit's meaning.

- **Why one level, not drykit's two.** Wax is opaque and leaves no dust,
  so it has no tone ramp. Two levels cost 35% more: the specimen took
  11.3s against 8.2s. They gave pale flecks that read as tint, not paper.
- **Why no lanes.** `/Lanes` with `/Dropout` read as rectangular bricks,
  not striation (first probe).
- **Pressure profile.** `/Touch` may be a procedure, and it is the base.
  On top of it come:
  - `/Landing`, on the core only;
  - `/Fade` and `/Uneven`, on both;
  - `/Buildup`, the full amount on the core and half on the rim;
  - `/Pass`, +0.06 per pass after the first;
  - `/Over`, −0.08, because wax slides on wax.

  The build-up comes from the subpath's own walk, at the rim's pitch. A
  stop's turning is summed over three stops, divided by 120°, and spread
  0.7 to its neighbours. The profile is a closure per subpath, so dycrayon
  calls dkstroke once per subpath and per layer.
- **Landing and build-up press the core only (build-up the rim by half).**
  On both, a pressed spot became a solid round cap: a "bullet end" at
  every scribble turn. On the core alone the edge stays ragged.
- **Tip wear.** `/Worn` 0..1 scales the line from 0.8 to 1.2 × `/Width`
  and the core from 0.5 to 0.85 of it. `/Wear` is drykit's: widening
  along each subpath.
- **Batches.** dycrayon opens a `dkbatch` unless one is open. It detects
  one by `userdict /dkBatch`, drykit's record. That dependency is new and
  unpinned by drykit, so the `drycrayon-drykit-contract` self-test pins it.
  One colour's passes render the same in a batch or as separate calls:
  0.7% of pixels differ by more than 60, all of them anti-aliasing.
  That's because every crayon level is one full-colour drykit level. The
  advisor's concern that pass 2's faint level would grey pass 1's core
  only applies to two-level strokes.
- **`/Over`** sets drykit's `/Under`. Where a stroke runs past the colour
  beneath onto bare paper, it is tinted toward that colour by
  1 − `/Strength`, which is 5% at the default. The specimen's layering
  panel runs past on purpose.

### Tuning against the crops

The fills were measured as white fraction inside a scribble fill. The
threshold is pigment + 0.6 × (paper − pigment), with the pigment taken
as the box's 15th percentile for the crops:

| | white fraction |
|---|---|
| Y2 pink (light), crayon_fill_ground | 0.31-0.36 |
| Y1 grey (denser), crayon_fill_streaks | 0.15 |
| Y2 heavy black | about 0.10 (mostly pass seams) |
| dycrayon, /Touch 0.6 | 0.34-0.38 |
| dycrayon, /Touch 0.72 | 0.23-0.26 |
| dycrayon, /Touch 0.88 | 0.17-0.18 (seams at /Spacing 5) |

Surfaces tried (`/Weave /Relief /Mottle`):

| Surface | Result |
|---|---|
| drawing paper, 0.6 | flecks too fine, 10.5s for one small page |
| construction paper, 0.9 / 0.75 / 8 | mottle punched big empty holes |
| 1.8 to 2.4 | knuckles read as separate dashes, not wax with holes |
| **1.4 / 0.65 / 8** | chosen |

`docs/drymedia/crayon_compare.png` puts the crops beside the render at
the same scale. The panel is CC0: the rhino-drawing crops only.
`crayon_compare_figure.png` puts the tadpole beside `dyfigure`. It is CC
BY-SA 3.0. `docs/drymedia/prototypes/crayon_compare.py` builds both.

**Still different from the crops** (filed as #306):

- The holes are finer and more regular.
- A heavy press is flat colour.
- The passes read as separate capsules.

A pale thin-wax rim (strength 0.65 to 0.72, higher touch) was tried for
the heavy-press grain. It drew a smooth halo around every pass, so it
was reverted.

### Motions

They follow #184's contract (lib/paintkit.ps, "Brush motions"):

- each replaces the current path;
- they paint nothing;
- they draw only from `frnd`, with a fixed number of draws per slot.

Randomness whose amount depends on a slot's length, such as the tremor
along a line, comes from a private Schrage generator (`dyrand`) seeded
by one of the slot's draws. Tested: ray starts don't move when
`/Length` changes, and a second wobbly line doesn't move when the
first's `/Overshoot` does. Key names follow #141: `/Lean`, `/Scatter`,
`/Wander`, `/Direction` and `/Offset` keep paintkit's meanings, and
there's no `/Spread` or `/Density`. `dyloop`'s gap is `/Gap` [degrees],
and `dyrays`' is `/Offset` [points], so no key has two units. Guards
reject any non-number, a packed procedure included. `/Touch`'s
procedure check accepts `packedarraytype`, and real gs exercises it in
`ghostscript_takes_packed_procedures_and_runs_every_motion`.

### Measured (CPU `user`, at 2x, i.e. 144 dpi; load average about 7)

| Render | pscat | gs | Budget |
|---|---|---|---|
| `examples/drycrayon_specimen.ps` | 8.4s | 4.8s | 10s |
| `examples/drycrayon_motion_demo.ps` | 4.5s | 4.6s | 10s |
| slowest `tests/drycrayon.rs` render, the CI scene, `--profile ci` | 0.55s alone | | 2s |
| `tests/drycrayon.rs`, 12 tests | about 3s | | 30s |

gs agrees on fixed-path crayon, block for block: a mean block
difference of 0.18, against 0.67 for another seed. The motion demo's
composition differs in gs, because gs's `rand` differs from pscat's
(DRYMEDIA.md 3.2).
