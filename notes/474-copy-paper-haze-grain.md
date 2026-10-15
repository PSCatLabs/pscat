# #474: a copy's paper grain at the scans' strength

Follows #466 (`notes/466-copy-paper-haze.md`). That note's grain was about a
third of the real scans'; denser flecks were hundreds of thousands of opaque
fills, so it was deferred.

## What shipped

- `rlHazeGrain` in `lib/released.ps` no longer fills flecks. It paints two
  stencils with `imagemask`: a lighter speckle and a darker one, a quarter of
  the cells each, in the grey the paper field has at the centre of a 16pt
  square plus or minus 0.013 (at `/Haze 0.6`; the amplitude scales with
  `/Haze`, so the near-white third sheet at 0.2 keeps a faint grain). Cells are
  1pt. The stencil bits come from two 64 x 128 byte random tiles; every stencil
  row is a random tile row at a random byte shift, and the squares along a
  row step through it a square's own bytes (2) at a time, so no two squares
  share bits within 1024pt.
  Seeded by `rlSeed 17` and `rlU`, so pscat and gs draw the same page.
- The grey of a square comes from the new `rlFieldSet` (the paper field's
  colour at a point), which is the body of `rlPaperField`'s cell loop moved
  into a procedure. The grain therefore sits on the lamp falloff, the edge
  darkening, the lid and the mottle instead of overpainting them with one
  fixed grey (a first sketch with a global grey would have flattened all of
  those). `/Haze 0` and `/Generation 0` renders are byte-identical to before
  (a specimen with every `/Haze` set to 0 compared in both trees).
- `docs/dev/design/typewriter/tools/grain.py` measures a scan or a render:
  bare-paper 48px windows, the RMS over 6px blocks of the block deviation
  (grain), the std of the windows' means (mottle). `tests/released.rs` has the
  same measure (`the_haze_grain_is_at_the_strength_of_the_scans`).

## Measurements (300 dpi, `grain.py`)

The method changed from #466's (which gave 0.0035 for the specimen sheet where
this gives 0.0046), so every figure here is from `grain.py`.

| | mean | grain 6px | mottle 48px |
|---|---|---|---|
| T14 | 0.914 | 0.0084 | 0.0087 |
| T9 | 0.864 | 0.0074 | 0.0031 |
| T1 (a 1914 letter) | 0.844 | 0.0087 | 0.0090 |
| specimen sheet 1, before | 0.904 | 0.0046 | 0.0088 |
| specimen sheet 1, after | 0.903 | 0.0086 | 0.0088 |

By block size (2, 4, 6, 8, 12 px) T14 is 0.0049, 0.0073, 0.0084, 0.0093, 0.0104
and the render is 0.0048, 0.0073, 0.0086, 0.0092, 0.0098: the spatial shape of
the grain matches, not only its level. 0.5pt cells (white noise at 300 dpi)
gave 0.0059 at 2px against T14's 0.0049 and a flatter curve, and 1.0pt cells
at 0.013 matched best.

The blind grid's strips (240 dpi, JPEG q85) keep it: bare-paper 6px blocks are
RMS 0.0124 and 0.0122 in the two render strips (A, D), 0.0127, 0.0100 and 0.0130
in the real ones.

Stencil reuse (found in review): a first version took 12pt squares, 1.5 bytes
of cells, and stepped 1 byte per square while each read 2, so a square's first
4 cells copied its left neighbour's last 4. A horizontal autocorrelation of the
high-passed grain (9px box removed, bare-paper windows, 300 dpi) showed it: at
4pt 0.298, against 0.092 now; the smooth residue of the field is 0.06 to 0.09
at 8 to 24pt, 0.015 at 64pt and 0.004 at 384pt. Squares are now 16pt (2 whole
bytes), the byte step is the bytes a square reads, and the tile row is 128 bytes
so it does not wrap within 1024pt.

Ghostscript (correctness only): the gs specimen has the same grain (0.0109 at
150 dpi in both renderers on sheet 1) and mean.

## Cost

Fill counts for the grain of one sheet: the memo (612 x 792) was
`0.6 * 612 * 792 * 0.2` = about 58,000 `rectfill`s; it is now 1,950 field
evaluations and 3,900 `imagemask` calls (39 x 50 squares, two stencils). A
specimen sheet is about 1,100 squares. Matching the grain with fills would have
been several times the old count (every fleck is a fraction of a pixel).

PNG sizes grow, because pixel-level noise does not compress (bytes, before to after):
specimen 1,304,798 to 1,467,775; its gs render 1,180,990 to 1,325,778;
`gallery/renders/released_memo.png` 773,991 to 827,186; `release_vs_real.png`
1,545,374 to 1,547,036; `memo_vs_real.png` 695,435 to 803,736; `blind_grid.png`
154,371 to 160,682; the new `notes/474-grain-compare.png` is 199,199. 0.5pt cells made the specimen 2.2 MB, which is why the cells
are 1pt.

## Renders that changed

`scripts/identity_check.sh --loader lib/released.ps` (control: a hazed copy
page): exactly `examples/released_specimen.ps` and `gallery/released_memo.ps`
differ, as in #466. Regenerated from a fresh release binary with their
committed commands: `docs/dev/design/released_specimen.png` and `_gs.png`,
`gallery/renders/released_memo.png`, and, by their scripts,
`release_vs_real.png`, `blind_grid.png` and `memo_vs_real.png`. The blind grid's
shuffle is fixed; the tool prints its key and the key stays out of the repo (it
is in the PR). `notes/474-grain-compare.png` is T14 against the specimen's copy sheet
before and after, plain and contrast-stretched.

## Not done

- The crumpled-paper wave of T14's mottle (smooth blobs here): #528. One
  ridged anisotropic octave was tried and dropped (48px std rose to 0.0107, mean
  fell, no wave showed).
