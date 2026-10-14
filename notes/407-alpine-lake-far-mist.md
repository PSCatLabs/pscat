## Alpine Lake: the far range's mist as banks of haze (issue #407, 2026-10-14)

#206 turned the near range's foot from gravel into haze and left the far range's mist band
(`lkmist`, element 2, at y=330) as it was: one curve of 1pt spray at `/Density 150` under an
isotropic `pkwet /Soft 1`. At 2x it read as a granular streak across the far range. The
cookbook calls that grain correct for vapour, so this is taste, not a defect. All looks were
judged at 2x against a fresh render of `main`, which matched the committed still byte for byte.

**What changed.** The band is now four banks of haze: filled lenses (`lkwisp`) of different
lengths, heights and thickness, with gaps between them, plus a sparse 0.6pt spray
(`/Density 60`) along the old curve. They sit inside one two-sided directional `pkwet`
(`/Direction 90 /Offset 7 /Layers 8 /Stretch 8`, not one-sided), so each bank fades up into the
range and down toward the near one. The core colour (`lkMC`) is `lkMIST` mixed 0.45 toward
`lkFAR`, and the reflection's part-21 pull now replays in that colour. Nested in the
reflection's 6-layer pull that is 48 passes, under pkwet's 64x budget. (A 13-layer version
raised `pkwet-too-many-passes` and dropped the rest of the scene, so the budget fails loudly.)
The `lkmd` density switch, which only one call used, is gone.

**Variants tried at 2x** (in order):
- One full-width filled band with a wavering edge, `/Offset 14 /Layers 10`, core 0.3 toward the
  range: haze rather than gravel, but it read as a ruled stripe across the picture. Its outer
  passes are nearly `lkFAR` and opaque, so they flattened the streaked shadow planes into a light
  bar. Using `lkMIST` for the core made a white line. Pulling the reflection sharp so the band
  could go to `/Offset 18 /Layers 16` made the stripe wider.
- The same band at 2pt thick with a dimmer core (0.55), `/Offset 10`: quieter, still ruled.
- Spray only (no band) at `/Density 150` under the two-sided lift: the stacked passes fuse into the
  same flat stripe.
- The band as four `pkdry` strokes: a wide streaky bar that covered the planes.
- Banks with gaps (shipped): the gaps let the shadow planes and the fine grain show through,
  and the banks' varied length and height read as drifting mist, not a rule. `/Offset 10
  /Layers 10` was a little softer and wider; 7/8 kept the banks more distinct.

**Containment.** `lkmist` reseeds itself, so only the band and its reflection should move. A
pixel diff of the old and new 2x stills agrees: only 2x rows 406 to 522 (the band) and 935 to 1049
(its reflection, mostly under the reflected near range) changed.

**Timing.** At 1x, `--png --page 760x560`, user time: 26.5s before, 26.3s after, which is within
noise. Ghostscript (`gs -dNOSAFER`) still renders it without error, and the banks show in its
render too.

## Not done

- `gallery/README.md`'s Alpine Lake entry is left alone, since #251 is rewriting that file (#435
  tracks the stale wording).
