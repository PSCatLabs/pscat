# #351: the strokeglyphs digit 4 no longer reads as a lightning bolt

Found while working #238. Handscript's `/four` is `[430 [280 700 70 250 380 260] [300 480 300 20]]`:
under the midpoint rule the first stroke's interior point `(70 250)` is a
control point, so the diagonal and the crossbar melt into one curved hook
that never reaches `(70 250)`, and the stem is a separate short stroke at a
different x. At board size that is a ϟ.

## What changed

`lib/strokeglyphs.ps` gets a `/four` entry in `sgsrc`. `sginit` copies
handscript's glyphs first and then `sgsrc`, so the entry replaces the copy.
`lib/handscript.ps` is untouched: `hs-write` and every handscript render keep
the old 4 (the existing "hs-write renders identically with strokeglyphs
loaded" test still holds).

    /four [430 [280 700 50 230 50 230 390 230] [280 700 280 10]]

A closed 4. Stroke one is the diagonal and the crossbar, with the corner at
`(50 230)` written twice so it is sharp (see "Doubled points make corners" in
notes/236). Stroke two is the stem, from the diagonal's top `(280 700)` down
to the baseline. The crossbar runs on to `x = 390`, past the stem. The
advance stays 430, so no line's layout moves: every pixel that differs in the
renders is inside a 4.

The superscript and subscript 4s are derived from the base glyph in `sginit`
(notes/339), so `x⁴`, `₄` and mathkit's `^4` / `_4` follow for free.

A `%%SelfTest: digit-four-is-a-closed-four` pins the advance, the bbox and
the crossbar-past-the-stem property.

## Evidence

`scripts/identity_check.sh --loader lib/strokeglyphs.ps` against a detached
`origin/main` worktree, one binary for both trees: of the nine files that load
strokeglyphs, exactly these differ, and the rest are byte-identical.

- DIFFERS: `gallery/chalkboard_lecture.ps`, `examples/mathkit_specimen.ps`,
  `examples/strokeglyphs.ps`, `examples/strokeglyphs_subscripts_greek.ps`,
  `selftest/drivers/mathkit.ps` page 1.
- identical: `gallery/crayon_drawing.ps` (it writes only the capitals of a
  name), `examples/drychalk_specimen.ps`,
  `examples/strokeglyphs_board_math.ps`, every other driver page.

The base tree reproduced each committed PNG byte for byte with the commands
in the files' headers, so regenerating them with the same commands adds no
unrelated change. Regenerated:

- `gallery/renders/chalkboard_lecture.png` (`--page 1440x850 --dpi 144`)
- `docs/drymedia/mathkit_specimen.png` (`--page 1000x1960 --dpi 144`)
- `docs/drymedia/strokeglyphs_specimen.png` (`--page 900x1600`)
- `docs/drymedia/strokeglyphs_subscripts_greek_specimen.png` (`--page 900x600 --dpi 144`)
- `docs/drymedia/lecture_compare_238.png`, rebuilt with
  `prototypes/lecture_compare.py` from the new still (its inputs are in the
  repo; the base tree's output matched the committed file exactly, so only
  the new still changes in it).

The Ghostscript renders (`crayon_drawing_gs.png`, `drychalk_specimen_gs.png`)
come from files whose pscat render did not change, so they were left. The
chalkboard has no committed gs render.

`docs/drymedia/four_compare_351.png` is the before/after panel: every 4 that
changed, left is before, right is after.

Timings with the release binary, two renders in parallel: chalkboard 9 s at
144 dpi (the same before and after), mathkit specimen 1 s, the two strokeglyphs
specimens under 1 s. Nothing moved: the new 4 has the same point count as the
old one.

## Not done

The 4 is the only glyph changed. The 7 (`[70 700 360 700 180 20]`) has the
same melted-corner look in the chalkboard crops ("1748" reads as "1)48"), but
that is a different glyph and a different issue.
