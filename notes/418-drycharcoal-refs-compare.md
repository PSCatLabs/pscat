## drycharcoal_refs_compare.png rebuilt from a committed script (issue #418, 2026-10-14)

**Problem.** `docs/dev/design/drymedia/drycharcoal_refs_compare.png` (#241) had no
build recipe in the repo, and after #313 changed `/drawing` paper's mottle its
right-hand crops (H2, H4 rows above all) showed the old, patchy surface. The
#241 crops were at about 8 px per point with a finer grain than a 576 dpi pscat
render, so the old file could not be reproduced.

**Change.** New `docs/dev/design/drymedia/prototypes/drycharcoal_refs_compare.py`,
in the shape of `mottle_compare_313.py`. It renders `examples/drycharcoal_specimen.ps`
once at 576 dpi (8 px/pt) and crops it in the specimen's own point coordinates
(the `.ps` source's, before its `0 -70 translate`), so a change to the specimen
or to drykit only needs a re-run. The five rows and their media are the old panel's:

| Row | Reference crop | Specimen crop | px/pt |
|---|---|---|---|
| H2 | `charcoal_hatching_on_tone` | the pencil hatch cells at values 0.2, 0.4, 0.6 | 8 (native) |
| H4 | `charcoal_fine_hatching` | the sphere hatched to a value procedure | 3 |
| H1 | `charcoal_stumped_tone` | the smudge cell at 0.6 | 8 (native) |
| H6 | `charcoal_stick_strokes`, its middle contour | vine and compressed `dhstroke` rows | 2.5 |
| H7 | `charcoal_compressed_mass` | compressed mass, smudge, kneaded and hard lifts | 3 |

The old panel mixed scales without saying so; each caption now states its px/pt.
The three H2 cells are pasted side by side (the old panel kept the empty paper
between them).

**Faithfulness check.** The worktree's pscat at `--dpi 144` reproduces the
committed `drycharcoal_specimen.png` byte for byte, and the 576 dpi render, box-
downsampled to 144 dpi, differs from it by a mean of 0.56 of 255 per channel, so
the crops show the approved look at a finer sampling, not a different one.

**Before and after.**
- before: `docs/dev/design/drymedia/drycharcoal_refs_compare.png` at `origin/main`
  (1750 x 1900, crops of the pre-#313 specimen)
- after: the same path, 1768 x 1856, from the script above. H2 and H4 now show the
  finer speckle of `/Mottle 1.5`.
- reference crops are unchanged.

Not changed: `lib/`, `src/`, the specimen or any gallery render.
`docs/dev/design/drymedia/REFERENCES.md` gains a line naming the script and the
panel's licence (it includes H6, CC BY-SA 4.0). Historical notes (#241, #313)
still say the panel had no recipe; they are left as written.
