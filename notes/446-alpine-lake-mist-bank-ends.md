## Alpine Lake: the far mist banks' ends dissolve sideways (issue #446, 2026-10-14)

#407 made the far range's mist four lens-shaped banks of haze under one two-sided directional
`pkwet`. Two leftovers were filed from it: some banks end in a blunt rounded nose, and where a bank
crosses a shadow plane the plane's streaks are flattened. This takes the first and leaves the second.
All looks were judged at 2x against a fresh render of `main`, which matched the committed still byte
for byte.

**Why the ends were blunt.** A directional `pkwet` moves each pass only along `/Direction` (here
up and down) plus a seeded wobble across it of `Offset / Stretch` (under 1pt at `/Stretch 8`). Every
pass of a lens therefore has its tip at the same x, and the 8 stacked, graded passes read as one
vertical cut with the core colour at its middle. The outline of one lens is not the cause.

**What changed.** `lkmist` now runs two sibling `pkwet`s inside the same checkpoint: first a
horizontal one (`/Direction 0 /Stretch 4 /Offset 24 /Layers 6`) over the four lenses alone, then the
#407 vertical one over lenses plus spray, unchanged. The horizontal passes smear each lens sideways
in graded colour, so the tips land 0 to 24pt apart and each bank trails off into the range instead of
ending. The vertical one, laid second, covers the horizontal one's core, so the banks' bodies keep
their #407 look. The four lens calls moved into `lkbanks` so both calls share them. They are
siblings, not nested, so the reflection's chain is still 6 x 8 = 48 passes, under pkwet's 64x budget.

**Variants tried at 2x** (in order):
- A needle-tipped lens (flat tangents at the ends, smoothstep rise): the passes stack into
  hairlines, a ladder of 1px streaks at each end, and the body gets thinner. Same ladder with
  `/Stretch 1`. A step of 1.75pt between passes divided by a taper slope is how long the ladder is,
  and it is unavoidable while every tip shares an x.
- `/Stretch 1` on the existing vertical call (the minimum: `/Stretch 0.5` raises
  `pkwet-stretch-must-be-at-least-1`): the ends stagger a little, not enough.
- The sibling horizontal wet (shipped), `/Offset 24 /Layers 6`: banks end in a faint trail, the gaps
  between them stay.
- The same at `/Offset 40` (and `/Offset 32 /Stretch 2`): longer trails, but the first two banks
  begin to join into one stripe, which loses the drifting-banks read #407 approved.

**Containment.** `lkmist` reseeds itself, so only the band and its reflection move. A pixel diff of the
old and new 2x stills agrees: only rows 406 to 509 (the band) and 946 to 1048 (its reflection) changed.
Every other row is identical. Ghostscript (`gs -dNOSAFER`) renders the file without error and the
banks show in its render.

## Not done

- **Shadow planes under a bank.** One idea was checked on paper and not built: a procedural `/Under`
  with `/Box` and `/Bands`, so cells over a plane mix toward the plane's shade. The outer passes stay
  opaque, so that would turn a light bar into a flat dark one without keeping any streak, the plane
  edges are diagonal so column cells would leave vertical seams, and each cell reruns the pass. The
  real fixes are a broken (low-coverage) fill for the bank's outer layers, which `pkwet` cannot do
  for a plain `fill`, or banks that thin where they cross a plane. Filed as a follow-up.
- The manifest's technique text still holds ("banks of haze lifted both ways by a directional
  `pkwet`"), so it is unchanged.
