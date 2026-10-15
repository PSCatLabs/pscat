# #449: thin-stroke dropout under a photocopy

Follows #214, #466, #474 and #528 (`notes/214-released-look.md`, `notes/528-copy-paper-wave.md`).
T15 (`docs/dev/design/typewriter/refs/T15_jfk_release_copy.jpg`) is a bitonal
release copy whose heavy stems survive while the thin strokes are gone: the
arms of E and F, the bar of an `e`, the tops and bottoms of bowls. Ours only
thickened. Comparison: `notes/449-dropout-compare.png` (T15, generation 3
before and after, generation 1 before and after).

## What was wrong, and the constraint

`tInkStrike` (lib/typewriter.ps) zeroes `tE`, the per-glyph erosion that #477
gives a light strike, when `tCopyC` is above 0, and a copy's spread fattens every
stroke by up to 1.9x, so nothing thin is left to break. The honest fix is there:
an erosion of the copy's body, by the glyph's own outline. This issue was done in
`lib/released.ps` only (lib/typewriter.ps was being left alone while #467 is
open), so the glyph outline is not available and the page cannot read a stroke's
width (no read-back, no alpha).

## What shipped (`lib/released.ps`)

`rel-page /Dropout` (0 to 1, default 1; `@param` with a unit, guard
`rel-dropout-must-be-0-to-1`, a `%%SelfTest` line, seed slot 18).

- A hole cuts selectively without reading anything: a hole as wide as a stroke
  severs it, a narrower one only notches it. So for a copy (`/Generation` above 0)
  `rlOriginal` draws the content inside a clip of the sheet minus a field of
  small holes (`rlDropClip`), and the paper already painted shows through them.
  It is a clip and not paper-coloured paint, as stamps and weave holes are (the
  grain, creases, folds and lamp under a hole stay).
- The holes are taller than wide (half width 0.26 to 0.55pt, growing with the copy
  loss, so they stay under a stem's width; half height 1 to 2.2pt), tilted up to
  12 degrees, so they cut horizontal strokes (the thin ones of the face) and notch
  stems. One hole per cell of a 3.4 x 5.4pt grid at most, so none overlap (an
  overlap is solid under `eoclip`). The chance per cell is 1.2 x /Dropout x c^2
  (c = `1 - 0.6^n`: 0.4, 0.64, 0.78 for one, two, three copies), times
  `1 + 1.6 m` with m a slow noise (anisotropic, 90 x 30pt and 28 x 14pt), so a
  line loses its thin strokes unevenly: a word, then a clear run. An original has
  none.
- A redaction bar is content, so the holes would cut it. A plain black bar that
  `tw-type` draws itself (no `/Redactor`) is cut like the type (slits in the bar,
  no word leaks: the word is cut too), so use `/Redactor { rel-redact }`; it is the
  gallery memo's and the specimen's. `rlRedactDraw` records
  its operands, CTM and the bounds of the clip it was called under while the content runs (`rlRecOn`; the replay is clipped to those bounds, which a review found missing: a bar the content had clipped away was drawn whole), and `rlBarsAgain` draws
  the bars again whole after the clip is dropped (the generator is put back, so
  the stamps and notes that follow are the same). Other solid fills in the
  content get holes: dropout acts on whatever the content draws; `/Dropout 0`
  turns it off. A `rel-paper` inside the content (`/PageStart`) is clipped too,
  so the holes then show the sheet under it, not its own paper.
- `tests/released.rs`: `a_copy_breaks_its_thin_strokes_and_keeps_its_thick_ones`
  (hairlines in pieces, bars not), `dropout_leaves_an_original_alone_and_scales_with_its_strength`,
  `a_redaction_bar_stays_solid_under_dropout` (with a negative control that turns
  the redraw off). `a_copy_whitens_the_paper_and_the_type_gets_fatter` compares the
  copy's ink with `/Copies 3` by hand, so it passes `/Dropout 0`: the holes take
  ink away (190 against 217 dark pixels, 12.4% against its 12% bound) and the
  check is about spread and fill.

## Measurements

Strokes page (`strokes()` in the tests: 8 hairlines of 0.7pt and 6 bars of 3.2pt,
260pt long; 4 pixels to the point), dark pieces (4-connected, under 128):

| | hairlines (8) | bars (6) |
|---|---|---|
| gen 1, /Dropout 0 | 8 | 6 |
| gen 1 | 30 | 6 |
| gen 3, /Dropout 0 | 8 | 6 |
| gen 3 | 97 | 10 |
| gen 0, /Dropout 1 | 8 | 6 |

The specimen's third sheet against T15 (300 dpi crops, `metric.py` in scratch:
ink under 128, 8-connected pieces of 12 or more pixels, per 1000 ink pixels;
broken letters raise it; T15's own text is not the same text, so read it as a
direction): T15 7.4; generation 3 before 4.2, after 6.0; generation 1 before 4.1,
after 4.6. Ink coverage of the crop fell 0.1105 to 0.1023 (T15 0.056: its type is
smaller). Settings tried: 4.4 x 3.6pt cells and rounder 0.35 to 0.85pt holes (a
few round holes; at 4.6 pieces per 1000 they read as pin-holes, not breaks); tall
slits 0.3 to 0.55pt half width at K 0.5 (4.9), 1.0 (5.6), 1.2 with the stronger
noise (6.0, kept) and 1.5 (6.1: the per-cell cap of 0.95 saturates, no gain).

## Cost (fill counts)

A memo sheet (612 x 792) has 180 x 147 = 26,460 cells: one random draw each and a
noise evaluation for the ones that pass the pre-test. Counted (a counter wrapped
round `rlDrHole`): the gallery memo (one copy) has 5,084 holes of 8 vertices, and
the specimen (a 612 x 480 sheet at generation 1 and another at generation 3) 14,177
between them, each in one `eoclip` built once per page, not per glyph.
The paper field (53,856 rectfills) and the grain (15,246 `imagemask`) are
unchanged. No timing was done.

## Renders that changed

`scripts/identity_check.sh <base> --loader lib/released.ps --loader lib/typewriter.ps`
(control: a generation 3 page): `gallery/released_memo.ps` and
`examples/released_specimen.ps` differ, `examples/typewriter_specimen.ps` is
identical (the typewriter is untouched). Regenerated from a freshly built release
binary: `docs/dev/design/released_specimen.png` (150 dpi) and `_gs.png`,
`gallery/renders/released_memo.png` (144 dpi, 1224 x 1584),
`docs/dev/design/typewriter/release_vs_real.png`, `memo_vs_real.png` and
`blind_grid.png` (the key stays out of the repo, it is in the PR). PNG sizes
(bytes, before to after): specimen 1,499,373 to 1,501,988; its gs render
1,365,776 to 1,365,777; the memo 847,013 to 847,863; `release_vs_real.png`
1,558,382 to 1,553,178; `memo_vs_real.png` 810,408 to 810,426; `blind_grid.png`
160,968 to 160,987; the new `notes/449-dropout-compare.png` 847,241 and
`notes/449-memo-compare.png` (T14 above the gallery memo before and after, 300 dpi) 560,096. Ghostscript
clips are not antialiased, so its holes are fewer pixels than pscat's (2,278
pixels differ from before, against 11,403).

## Reading it

At the specimen's generation 3 the thin strokes now break: the bars of `e` and
`t`, the arms of `r` and `E`, in runs rather than evenly. Generation 1 shows a few.
It is still not T15: T15's face has thin horizontals and heavy stems by design,
and its toner is a dither, while ours breaks by random holes, so a thin stroke
is cut about as often as the holes land on it, not always. A hole also cuts a
stem now and then (in a first copy a stem is 1.2pt).

## Not done

- Glyph-level erosion on a copy, in `tInkStrike` (`tCopyC` above 0 sets `tE` to
  0): the outline-driven version that cuts every thin stroke and no stem. Needs
  lib/typewriter.ps; filed as #547 (p3).
- The gallery memo is one copy, so at 144 dpi its dropout is subtle (the
  specimen's third sheet shows it). Raising the memo's generation is a choice for
  the capstone (#531), not made here.
