# #450: copier softening of stamps, notes and redaction bars

Follows #214, #449 and #528 (`notes/214-released-look.md`, `notes/449-thin-stroke-dropout.md`).
In T14 (`docs/dev/design/typewriter/refs/T14_nsam55_marks.jpg`) the stamps, the
handwriting and the marker's bar are soft where a copier has handled them: the
edge fades over a pixel or two and the ink bleeds a little. Ours were crisp
(clip holes only). Before and after: `notes/450-memo-compare.png` (T14 above the
gallery memo's marks, generation 1) and `notes/450-specimen-compare.png`
(generation 3).

## What shipped (`lib/released.ps` only)

- `rel-page /Soften` (0 to 1, default 1; guard `rel-soften-must-be-0-to-1`) and a
  `/Soften` on `rel-stamp` and `rel-note` (guards `rel-stamp-soften-...`,
  `rel-note-soften-...`). The softness that is drawn is `s`:
  - inside `rel-page`: `clamp(/Soften x 1.25 x c)`, c the copy loss
    (`1 - 0.6^n`: 0.4, 0.64, 0.78 for 1, 2, 3 copies), so s is 0.5, 0.8, 0.98.
    An original has c = 0, s = 0: none of the new code runs, and its render is
    byte-identical (`typewriter_specimen.ps` and every generation-0 page).
  - a stamp's or note's own `/Soften` is s, so a standalone `rel-stamp` can be soft.
  - `rel-redact` takes the page's (it is called from the content).
- The blur has no alpha and no read-back to work with, so it is a fringe of five
  stepped rings under the crisp mark (`rlSoR`, `rlSoA`): the mark's outline
  stroked at five widths, widest first, each in the colour that ink of a growing
  share (6, 14, 27, 44, 66 percent) makes over the paper, i.e. the ink mixed toward
  the paper's tone (`rlTone rlCol`, so the copy's grey is in it). Antialiasing
  turns that into a gradient about the radius wide. The radius is
  `s x (0.2 + 0.055 size)` pt for a stamp, `s x (0.1 + 0.012 size)` for a note,
  `s x (0.3 + 0.08 size)` for a bar (size in points; the bar's is kept short so that
    its fringe stops about at the baseline of the line above, see below).
  - stamp (`rlStampRings`): the text as `charpath` (a character at a time, so the
    tracking matches `ashow`) and the frame, inside the stamp's own hole clip, so a
    void stays a void. It closes thin cracks in the frame, as a copier does.
  - note: five `hs-write` passes with a wider `/Pen` in the ring colours, under
    the pen's own pass, which is lightened by `0.2 s` (a blurred line gives up
    darkness; without it a 0.5pt pen line doubled in weight and read as a
    marker). The same `/Seed`, so the line is the same line.
  - bar (`rlBarRings`): the bar's path is kept as a procedure (`rlPathProc`) for
    both of its passes, every ring is stroked over both passes, then the passes
    are filled. (Stroking one pass's rings after the other's fill would put a
    pale outline inside the black.)
- No ring draws a random number, so the generator stream (and the caller's
  `rand`, untouched as before) is exactly as it was; the stamp's holes, the bar's
  ragged edge and the note's wobble are the same as before.
- Mechanism decision: dither (a stencil or a field of dots) was the alternative,
  and the plan was to prototype it. It was not built: stepped rings are smooth
  under antialiasing where a one-bit stencil is pixel noise (pscat samples an
  `imagemask` nearest-neighbour), and they cost 5 strokes a mark, not thousands of
  clip subpaths. The cost is that a ring is opaque and lightens the type it
  crosses, which dither would not. Measured (a render of the base tree against
  the head, 300 dpi, pixels more than 40 levels lighter in the head; the margin
  notes excluded, since the note's own pass is lightened on purpose): 38 of 8.4M
  pixels on the memo and 226 of 16.9M on the three specimen sheets. A first
  version with a bigger bar radius (`0.4 + 0.14 size`) lifted 624 on the specimen,
  clustered along the bar's top edge where its fringe reached the line above (the
  baseline of "building" lost its serifs): the bar's radius was cut to
  `0.3 + 0.08 size` for that, which stops about at that baseline.
- The stamp's clip is the box plus 3pt plus the fringe's radius (it was a fixed 3pt),
  so a large stamp's outer ring is not cut off straight (checked at 60pt, generation 3).

## Measured

10 to 90 percent edge width of the bar (`rel-redact`, 11.5pt type, straight bar,
300 dpi, no haze or speckle; the mean over a 30pt run):

| | /Soften 0 | /Soften 1 |
|---|---|---|
| generation 1 | 0.37pt | 0.62pt |
| generation 3 | 0.35pt | 1.0pt |

T14's own bar edge is about 6 px against a 22 px cap height (0.3 of the cap
height: the marker's slope is in it); ours at generation 3 is 1.0pt against an 8pt
cap height (0.125), at generation 1 0.08. So T14's bar is blurrier than ours by a
factor of two or more; the bar's radius is held back because a larger one
erases the type of the line above (above). T14's stamp edges are nearer 0.14 of
the cap height, which the stamps' generation 1 to 3 range covers.

CPU time (user + sys, same binary, base tree against head, three runs):
specimen 6.4 s to 6.3-6.7 s, memo 3.26 s to 3.28-3.34 s (+2 percent at most).

## Renders that changed

`scripts/identity_check.sh <base> --loader lib/released.ps --loader
lib/typewriter.ps` (control: a generation 1 page with a stamp):
`gallery/released_memo.ps` and `examples/released_specimen.ps` differ;
`examples/typewriter_specimen.ps` is identical. Regenerated:
`docs/dev/design/released_specimen.png` (150 dpi) and `_gs.png`,
`gallery/renders/released_memo.png` (2x), `docs/dev/design/typewriter/release_vs_real.png`
and `memo_vs_real.png`. `blind_grid.png` is typed lines only and did not change.
Ghostscript has the same rings (they are ordinary strokes); its antialiasing is
the device's, so the gradient is coarser there. The gs test still passes.

## Not done

- A per-mark `/Soften` on a redaction bar: it follows the page.
- Ink bleed along the type's own strokes is the typewriter's (#538 and the `/Copies`
  spread); this issue is only the marks.
