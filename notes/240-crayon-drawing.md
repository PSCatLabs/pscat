## Gallery: a young child's crayon drawing (issue #240, 2026-10-13)

`gallery/crayon_drawing.ps`, "My Family and Our House": a six-year-old's picture
on printer paper, 792x612 pt, built from `lib/drycrayon.ps` (#239) and
`lib/strokeglyphs.ps` (#236). No library changes. Rendered still:
`gallery/renders/crayon_drawing.png` (2x, as the gallery convention says).
The reference panel is `docs/drymedia/crayon_drawing_compare.png`, built by
`docs/drymedia/prototypes/crayon_drawing_compare.py`; the gs render is
`docs/drymedia/crayon_drawing_gs.png`.

### Composition, and what it copies from the references

Checked against Y4 (tadpole) and Y5 (the 6-year-old's rhino picture) before
and after drawing, not from memory:

- **Bands at the top and bottom, air between** (Y5): the sky is blue scribble
  along the top and the grass yellow-green along the bottom; the paper
  between them is the air. The sun, house, tree and figures sit on the
  ground edge, no perspective, the figures nearly as tall as the house.
- **Colour in side-by-side patches with white seams** (Y5, Y2): `cdband`
  lays a band as several `dyscribble` patches of random width and direction
  (within 10 degrees), each running past its edges, so seams and ragged
  ends come for free. A single full-width scribble read as ruled stripes in
  the first probe.
- **Outlines last** (Y5): every fill is down before any black outline.
- **Pressure** (Y4): the outlines are one firm width (3.6 to 4.4 pt);
  `dycrayon`'s own landing/fade does the rest. Dots (apples, doorknob,
  flower centres) are tiny `dyloop`s pressed hard.
- **The sun** is oversized (radius 74 pt of a 612 pt page), its disc yellow
  scribble over the blue band with the blue showing through the gaps, its ring
  and rays `dyloop` and `dyrays` with large `/Wobble`, `/Gap` and `/Lean`.
  It has a smile, as suns do.
- **Faces and spill** (the advisor's pass after the first render): every
  figure has two dots and a smile (`cdface`, head radius and centre from
  dyfigure's own geometry: 0.13 h at 0.87 h up for a stick figure, 0.3 h at
  0.7 h for a tadpole). Each fill is a polygon with its corners knocked up
  to 5 pt off the outline's (`cdpolyj`) and `/Overrun 8`, so colour spills
  past some edges and stops short of others. Flowers vary in petal count
  (5 to 7), ring radius, petal size and starting angle. The paper is
  `/Ground [1 1 1]`, printer white, so drykit's idea of what lies beneath
  the wax matches what is painted.
- **The name**, JOSIE, then a 6, are `sgpath` capitals inked with the crayon
  one letter at a time (`cdletter`): each its own size (47 to 52 pt), lean
  (-6 to +5 degrees), baseline (10 to 14 pt) and colour, with a gap that is
  never the same twice, and the S written backwards (`-1 1 scale` about the
  letter, shifted right by its `sgwidth`). `sgpath` needs `handscript.ps`
  loaded first. Each letter has a fixed `/Seed`, so gs builds the same
  skeleton.

### Numbers

| | pscat | gs |
|---|---|---|
| render at 2x (`--dpi 144`, 1584x1224), CPU `user` | 8.7 s (13.2 s before #332 landed; the machine ran at load average 12 to 27, so wall time swung up to 25 s) | 5.2 s |
| `tests/crayon_drawing.rs`, `--profile ci`, 1x plus gs | about 15 s for the pair (a whole piece, so over the 2 s per-render line for `tests/drykit*.rs`, which is for specimen-scale renders) | |
| DRYMEDIA.md 6.2 budget for this piece | 20 s | 20 s |

The 2x-scale convention (`2 2 scale` prepended, `--page 1584x1224`) gives the
same pixels as `--dpi 144`: 0 of 1,938,816 pixels differ by more than 40, so
drykit's device-space tooth is not shifted by the prepended scale.

### Where it still differs from the references

- The grain is finer and the holes more irregular in Y5; ours are the
  capsule-shaped passes and regular holes already filed as #306. The panel
  shows it side by side, row by row.
- Y5's fills overlap (colour over colour); ours mostly sit side by side, as
  the child's do in Y5. Only the sun uses `dycrayon`'s `/Over`.
- REFERENCES.md still lists "the sky as a band at the top" as not yet
  sourced: Y5's top band is a tree canopy, so the sky band is by analogy,
  not copied. The row was left as it was.
- gs's `rand` differs from pscat's (DRYMEDIA.md 3.2), so the motions land
  differently in gs: the same drawing, laid out a little differently.
  The still is the pscat render.

### Wiring

`gallery/show.sh` (all three arrays), `gallery/README.md`, a card in
`site/gallery.html` (the PNG comes from `build_site.sh`'s wildcard copy of
`gallery/renders/*.png`), `HANDOFF.md`, and `tests/crayon_drawing.rs`
(builds in pscat with the parts where the composition puts them, and runs in
gs when it is installed).
