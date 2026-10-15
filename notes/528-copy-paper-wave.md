# #528: a copy's haze mottle is a crumpled-paper wave, not soft blocks

Follows #466 and #474 (`notes/474-copy-paper-haze-grain.md`). The mottle's
strength already matched T14 (48px std 0.0088 against 0.0087); its shape did
not.

## What was wrong

Two things, and neither is the ridged noise #474 tried:

1. The 14pt and 6pt value-noise octaves sat on one axis-aligned lattice.
   Smoothstep value noise is flat at its lattice points, so the field had
   plateaus the size of the spacing, on a grid. At a contrast stretch of 0.80 to
   1.00 the "before" panel of `notes/474-grain-compare.png` shows the grid.
2. The grain's squares (16pt in #474) paint half of the cells in the grey the
   field has at the square's centre. Any structure in the field smaller than the
   square is flattened in those cells and comes back as 16pt blocks. That is why
   #474's one octave of ridged noise showed no wave: the grain erased it.

A throwaway probe (an exaggerated diagonal sine in `rlHzMot`, 10pt period)
showed the 16pt blocks directly.

## What shipped (`lib/released.ps`)

- `rlHzOcts`, `rlHzInit`, `rlHzFb`: the two axis-aligned octaves are replaced by
  four octaves of value noise (spacings 48, 24, 12 and 9pt, weights 0.10, 0.10,
  0.15, 0.11), each on a lattice turned about the sheet's centre (35, 25, 40 and
  -25 degrees) and stretched 1.5 to 1.6 along its tilt, so no octave has a grid
  and the paper has long soft ridges. Nothing is finer than 9pt (see the grain
  below); the blotches' amplitude fell from 0.22 to 0.10 to keep the std.
- `rlHzCrease` (in `rlHazeGrain`, before the stencils): about 6 creases per
  100,000 square points (30 on a memo sheet), each a run of 4pt segments 45 to
  155pt long, tilted -35 degrees by about 18 (one in five at any tilt), each
  segment a darker line with a lighter one beside it, in the paper field's grey
  plus or minus a depth of 0.045 (at /Haze 0.6; it scales with /Haze). They sit
  under the grain, which paints half the cells over them, so they are as grainy
  as the paper and half as deep as drawn. T14's creases run mostly top left to
  bottom right.
- `rlGrT` 16 to 8: the squares are one byte wide. `rlGrA` 0.013 to 0.0145
  (smaller squares, and the new field, left the 6px grain at 0.0076; it is
  0.0085 again). The stencil reuse arithmetic of #474 holds: a square steps a
  byte, the tile row is 128 bytes, so no two squares share bits within 1024pt.
- `tests/released.rs`: `a_copys_mottle_is_at_the_scans_strength_and_has_creases`
  (the 48px mottle of a copy in 0.005..0.014; the creases draw pixels and leave
  the mean alone).

## Measurements (300 dpi, bare-paper windows)

A bare 612 x 480 copy sheet (`/Generation 1 /Age 0.6`, no content), `grain.py`
measure, whole interior:

| | mean | grain 6px | mottle 48px |
|---|---|---|---|
| T14 | 0.914 | 0.0084 | 0.0087 |
| before | 0.899 | 0.0081 | 0.0085 |
| after | 0.901 | 0.0085 | 0.0091 |

Grain by block size (2, 4, 6, 8, 12 px): T14 0.0049, 0.0073, 0.0084, 0.0093,
0.0104; before 0.0045, 0.0068, 0.0081, 0.0087, 0.0092; after 0.0048, 0.0071,
0.0085, 0.0091, 0.0097. The grain's shape is nearer T14 at every size (the
mid-size end of the curve is the wave).

Not matched: the band-pass energy at 1 to 2pt. In a clean (ink-free) corner of
T14 the difference-of-Gaussians std at sigma 4 to 8 px and 8 to 16 px is 0.0034
and 0.0032; the render is 0.0020 to 0.0022 and 0.0019, before and after alike.
The paper field is laid in 3pt cells (12.5px at 300 dpi), so the render has no
structure between the grain (1pt) and the cell. Octaves finer than 12pt put the
cell steps on show; creases are the only fine detail the render can carry.

## Variants tried and dropped

- Ridged anisotropic noise (`exp(-(n/w)^2)` of tilted noise, centred): thin
  diagonal bands at the zero crossings, but wide and patchy and the 48px std
  doubled; it read as stripes of different tone, not creases.
- Directional-derivative (emboss) octaves of the same noise: a convincing
  diagonal relief at 1.5pt cells, but it needs the fine cell (4x the fills) and
  at 3pt cells it showed the cell grid; amplitudes had to be tuned per octave
  (the derivative scales as 1 / spacing).
- Voronoi facets (flat or tilted per-site values, with a darker crease at the
  cell edges, soft-blended edges): straight-edged crumple facets, which read as
  a quilt of patches, with too much fine-scale energy (grain 0.0104).
- Laying the field in 1.5pt cells: smoother clouds but 4x the fills (about
  215,000 for a memo) and no visible gain in the wave. Not done.
- 16pt squares with the new field: slightly angular edges at the stretch; 8pt
  looks smoother. 8pt is kept.

## Cost (fill counts)

For a memo sheet (612 x 792): the paper field is unchanged (3pt cells, 204 x 264
= 53,856 rectfills). The grain was 1,950 field evaluations and 3,900 `imagemask`
calls (16pt squares); it is now 7,623 field evaluations and 15,246 `imagemask`
calls (8pt). The creases are about 1,760 strokes (586 segments, 3 strokes each,
one field evaluation per segment). The noise is 4 value-noise evaluations per
field evaluation instead of 2. The pre-#474 grain was about 58,000 fills.

## Renders that changed

`scripts/identity_check.sh --loader lib/released.ps` (control: a hazed copy
page): exactly `examples/released_specimen.ps` and `gallery/released_memo.ps`
differ, as in #466 and #474. A specimen with every `/Haze` set to 0 is
byte-identical in both trees (so originals and white copies did not move).
Regenerated from a fresh release binary with their committed commands:
`docs/dev/design/released_specimen.png` and `_gs.png`,
`gallery/renders/released_memo.png`, and, by their scripts, `release_vs_real.png`,
`blind_grid.png` and `memo_vs_real.png`. The blind grid's shuffle is fixed; the
tool prints its key and the key stays out of the repo (it is in the PR).
`notes/528-wave-compare.png` is T14 against the specimen's copy sheet before and
after, plain and contrast-stretched (0.80 to 1.00, blurred 2px), for the
text corner (as `notes/474-grain-compare.png`) and a bare patch with a crease;
`notes/528-memo-compare.png` is the gallery memo before and after, at half size.

PNG sizes (bytes, before to after): specimen 1,467,775 to 1,497,959; its gs
render 1,325,778 to 1,365,045; `gallery/renders/released_memo.png` 827,186 to
845,695; `release_vs_real.png` 1,547,036 to 1,557,934; `memo_vs_real.png` 803,736
to 810,112; `blind_grid.png` 160,682 to 160,968; the new
`notes/528-wave-compare.png` is 303,782 and `notes/528-memo-compare.png` 485,465.

## Reading it

At plain contrast the two copies are hard to tell apart, and T14's own creases
are faint there too; the after sheet shows a few diagonal creases at 300 dpi.
Under the stretch the grid of soft blocks is gone and the paper has the cloudy,
creased look of T14, with less fine crumple than T14 has (above).

## Not done

- The 1 to 2pt crumple of T14's paper (the band above): it needs a finer field
  (1.5pt cells, 4x the fills) or a stroke-based fine texture; filed as a p3.
