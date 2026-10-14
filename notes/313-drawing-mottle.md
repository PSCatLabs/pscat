## drykit: `/drawing` paper's mottle scale (issue #313, 2026-10-14)

**Problem.** At drykit's `/drawing` defaults (`/Relief 0.65 /Mottle 6`), fine
charcoal hatching broke up in white patches several points across. The
references (H2, H4) break up as fine specks. #241 worked around it with
`/Relief 0.35` in the specimen, tests and the Lincoln piece, but a default
that needs an override is a trap for the next artist.

**Change.** `/drawing`'s `/Mottle` is 1.5 (was 6). `/Relief` stays 0.65.
The patches come from the slow mottle (scale `/Mottle`), so shrinking its
scale shrinks the patches to a few knuckles. `/Relief` is what ties the
tooth to the mottle, and crossing strokes share their tooth through it
(Q4, `notes/234-drykit-core.md`), so it stays. The issue offered a separate
`/charcoalpaper` surface; the default itself is fixed instead, so every
charcoal artist gets it and no surface or key was added.

**Measured** (Q4 mean phi over five seeds, streak 1.4, `tests/drykit.rs`
`crossing_strokes_share_the_tooth`; the bar is 0.4 and 0.25 above control):

| `/drawing` variant | mean phi |
|---|---|
| `/Relief 0.65 /Mottle 6` (old default) | 0.48 (0.46 in the test's own bands) |
| `/Relief 0.35 /Mottle 6` (the #241 workaround) | 0.34 fails |
| `/Relief 0.65 /Mottle 3` | 0.47, but patches still show |
| `/Relief 0.65 /Mottle 2` | 0.47 |
| `/Relief 0.65 /Mottle 1.5` (new default) | 0.46 here, 0.44 in the test |
| `/Relief 0.65 /Mottle 1` | 0.40, at the bar |
| `/Relief 0.5 /Mottle 1.5` | 0.38 fails |

The first six rows are from a scratch harness that mirrors the test's
bands. The test itself, with `--nocapture`, gives 0.44 for the new default.
1.5 and 2 hatch alike on Lincoln; 1.5 has the cleaner swatches, and 1.0 is
too close to the bar.
`docs/drymedia/mottle_compare_313.png` (built by
`docs/drymedia/prototypes/mottle_compare_313.py`) shows the old default, the
old workaround and the new default under the same hatching, beside H2, and
Lincoln's face before and after.

**New test.** `drawing_paper_breaks_up_in_specks_not_patches` fills a flat
catch at touch 0.6 and measures the spread of ink coverage across 3pt
blocks. The default's is 0.14, the old `/Mottle 6`'s 0.25 (five seeds); the
test requires under 0.7 times.

**Reach test threshold.** `dkstroke_reaches_every_knuckle_its_ink_touches`
failed by one pixel on the new paper (`tilt 90 on a 10pt side`, seed 3).
It was an anti-aliasing difference between the band's clip and the
whole-page clip (49.6 of 255 against a threshold of 40), not a missed
knuckle: a missed knuckle uncovers whole pixels, and the one-cell-short
control differs in hundreds. The finer mottle raises that noise (max about
25 before, about 50 now). `missed` now counts differences over 80.

**Renders that changed** (found with `scripts/identity_check.sh --loader
lib/drykit.ps`, control a `dkcatch` fill on `/drawing`; the other six
drykit-family files stayed byte-identical): `gallery/lincoln_charcoal.ps`
(still `gallery/renders/lincoln_charcoal.png`, 2x), `examples/drycharcoal_specimen.ps`
(`docs/drycharcoal_specimen.png`), `examples/drykit_specimen.ps`
(`docs/drymedia/drykit_specimen.png` and `_gs.png`). The specimen and
Lincoln now use the defaults, with `/Relief 0.35` dropped. Lincoln's mean
luma is 207.2 before and 208.1 after: the tone is unchanged and the grain
is a little finer and more even. Smudged cells in the specimen show a few
more white flecks.

**Timings** (CPU `user`, `--dpi 144`, load 3 to 6, before then after):
Lincoln 2x 23.6 s then 23.4 s; drycharcoal specimen 4.5 s then 4.5 s;
drykit specimen 9.7 s then 9.6 s in gs and 3.9 s in pscat. The mottle's
scale has no cost of its own.

**Not changed.** The other four surfaces keep their mottle (slate 4,
newsprint 3, construction 8). Crayon's coarse paper is meant to be patchy.
The mottle lattice repeats every 256 times `/Mottle`, 384 pt at 1.5. The
knuckle hash repeats every 256 cells (154 pt at `/Weave 0.6`) already, so
this adds nothing visible.
