# #416: the handscript 2 has a flat base, not a curly one

Found while working #298. handscript's `/two` was
`[420 [70 550 180 700 330 620 300 420 70 30 360 40]]`. Under the midpoint rule
(see `drawstroke`) the interior point `(70 30)`, the bottom-left corner, is a
control point, so the diagonal and the base slur into one soft curve that
reads as a curl, not a stroke along the baseline.

## What changed

`lib/handscript.ps`: the corner point is written twice, the same technique as
the 7 (#298) and the 4 (#351):

    /two [420 [70 550 180 700 330 620 300 420 70 30 70 30 360 40]] def

The advance stays 420, so no line's layout moves. Each copy of the corner draws
its own jitter, so the corner is a sharp point that shivers slightly, which
reads as pen pressure and not as a defect (checked in the board-size renders in the panel).

Variants rendered side by side at 90pt, with and without jitter: the plain
doubled corner (kept); a straighter diagonal (a lower second-to-last control
point); an extra mid-diagonal point; an extra mid-base point. All the flat-base
forms read as a 2; the plain doubled corner is the smallest change that does it,
so it won (maintainer: a clear improvement, not perfection).

## Consequences

- Two more jitter draws per 2: in `hs-write` output every glyph after a 2
  draws differently (field_notes: the axis labels and the caption beside them).
  strokeglyphs and chalkboard pieces seed per stroke, so there only the 2 moves.
- `lib/strokeglyphs.ps` copies handscript's glyphs at load (`sginit`) and has
  no `/two` override, so its 2, the derived superscript and subscript 2 and
  mathkit's `^2` / `_2` follow for free. strokeglyphs.ps and mathkit.ps are
  untouched. The 4 is untouched (#351's open 4 is still the strokeglyphs 4).
- Left alone: `examples/handwriting.ps` and `gallery/hundred_lines.ps` each
  carry their own inlined copy of the old glyph (they do not load the library),
  so their renders do not change.

## Renders regenerated

`scripts/identity_check.sh` against a detached merge-base tree, one binary,
with a "2 22" handscript control (differed in both runs, as it must): the
default pick (39 files that load paintkit) and `--loader lib/handscript.ps`
(also covers strokeglyphs, mathkit and drychalk; 10 files). Both were needed:
the default pick found only the chalkboard and the mathkit specimen. Differs:

- `gallery/renders/chalkboard_lecture.png` (`--page 1440x850 --dpi 144`)
- `gallery/renders/field_notes.png` (`--page 700x600 --dpi 144`)
- `docs/drymedia/mathkit_specimen.png` (`--page 1000x1960 --dpi 144`)
- `docs/drymedia/strokeglyphs_specimen.png` (`--page 900x1600`)
- `docs/drymedia/strokeglyphs_subscripts_greek_specimen.png`
  (`--page 900x600 --dpi 144`)
- `docs/drymedia/strokeglyphs_board_math_specimen.png`
  (`--page 900x1100 --dpi 144`)
- `docs/drymedia/lecture_compare_238.png`, rebuilt with
  `docs/drymedia/prototypes/lecture_compare.py` from the new still (the base
  tree reproduced the committed file first).

Identical: crayon_drawing, drychalk_specimen, every strokeglyphs selftest
driver page, all paintkit/drykit files. The base tree reproduced every
committed PNG byte for byte before each was regenerated. None of the changed
renders has a committed Ghostscript twin; a gs run of a handscript 2 (with
`-dNOSAFER`) exits 0.

`docs/two_compare_416.png` is the before/after panel: every changed spot, left
is before, right is after. Built by `notes/416-compare-panel.py`.

Historical panels (`specimen_polish_298_compare.png`, `four_compare_351.png`)
are left as they were.

The mathkit specimen is also touched by #340 (#440); whichever lands second
regenerates it from the merged tree.
