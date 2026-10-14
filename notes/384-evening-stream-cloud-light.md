## Evening Stream: the clouds' light from `pkscumblein /Light` (issue #384, 2026-10-14)

`gallery/evening_stream.ps` lit its clouds from below with a trick (#187): the
whole cloud scrubbed in the light colour, then the body over it shifted up 0.3 of
its height, leaving a crescent. `pkscumblein /Light /Reach /Feather` (#345) does
that natively, and the landscape cookbook's recipe 2 already used the native
form, so the source piece no longer matched its recipe. Only the `.ps`, the
2x still and docs change; no `lib/` change.

![before and after](384-cloud-light-compare.png)

Left, the committed still; right, the new one. Below, the two left clouds close up.

- **New `evcloud`.** The body is scrubbed over the whole outline (no shift) and
  worked wet into the sky (`/Soft 0.6 /Under` the sky behind it), as before. Then
  `pkscumblein /Light 270` twice: a half-light (`/Reach 0.3 /Feather 0.9`, counts
  x3, a load between body and the glow rose) and its core (`/Reach 0.11
  /Feather 1`, circles 0.6x, a thinner brush, a load from the rose toward the
  rim colour). Both are worked into the body (`pkwet /Soft 0.4 /Under` body), as
  in the recipe, not into the sky.
- **Taller clouds.** The shifted copy had made every cloud about 0.3 taller than
  its outline. With no shift the clouds came out squat, so `evch` went from 0.28
  to 0.36 of the width.
- **Value and hue.** First try (the recipe's keys, a load mixed toward the pale
  rim colour) was too bright and cream: the light read as a pasted-on band and
  the band was so deep it swallowed the cloud. Shallower reach and a load mixed
  toward the rose of the glow (`evROSE`) kept it warm and close to the body.
  Rim colour mixed in directly turned grey-brown over the purple body.
- **Random state.** Every `evcloud` starts from its own `srand`, and `evridge`
  reseeds next, so nothing downstream moves. Checked: against the previous still,
  zero pixels differ outside the four cloud boxes.
- **Review.** Four render rounds (too bright; too deep and muddy; warm but ringed;
  the accepted one). A fifth, a paler core, broke into a hairline of light with a
  brown halo and was dropped.
- **Reproducibility.** Two 2x renders are byte-identical; `--lint` is clean;
  Ghostscript renders the scene without errors (correctness only; its rand
  stream differs).
- **Left alone** (separate tickets): the waterfall `/Scatter`, and #433, the lit
  band's stepped feather and even saturation along each cloud's length.
- **No halo.** A 2.5x crop of the right-hand cloud's underside shows nothing on the
  sky below the outline: the softening is into the body.
