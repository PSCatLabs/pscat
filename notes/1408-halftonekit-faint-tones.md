# 1408: halftonekit line/cross screens fade smoothly at tiny tones

**Bug.** A `/Screen /line` or `/cross` halftone drew its rules at width
`/MaxWidth * tone`, skipping only a tone of exactly 0. pscat strokes
anything thinner than 0.1 device pixel at exactly 0.1 (`device_line_width`,
`stroke_params` in `src/gfx.rs`), so at `/Tone 0.001` (width 0.008 px) the
rules were drawn at 0.1 px: mean ink 0.91% on the issue's repro where 0.07%
is proportional, and a tone fading to 0 popped from nothing to a visible
screen the moment it left 0 (found by a psanim animation: a 5-level step in
the mean sky colour in one frame).

**Fix (`lib/halftonekit.ps` only).** Below `hfthresh` = 0.1 device pixel
(measured across the rule with `dtransform`, so it follows the CTM, a
non-uniform scale and `--dpi`), the rule keeps that width and gets shorter:
a butt-capped dash centered on its cell, `pitch * width / hfthresh` long,
which has the area the full rule's width would have. Rules at or above the
limit take the old code path unchanged. Dot screens are untouched.

**Alternatives measured** (mean ink over a 400x400 clip, 72 dpi, defaults):

| tone | before line | after line | after cross |
|---|---|---|---|
| 0.0005 | 0.00906 | 0.00033 | 0.00066 |
| 0.001 | 0.00906 | 0.00066 | 0.00131 |
| 0.003 | 0.00906 | 0.00200 | 0.00395 |
| 0.01 | 0.00906 | 0.00668 | 0.01329 |
| 0.03 | 0.02246 | 0.02246 | 0.04449 |
| 0.1 | 0.07341 | 0.07341 | 0.14184 |

Threshold 1 device pixel (the issue's wording) was slightly smoother (ink/tone
a flat 0.71 down to 0.0005) but changes every tone below 0.125 at the
defaults and any deliberate thin `/MaxWidth` screen (a `/MaxWidth 0.5` rule
at tone 1 would turn into dashes). 0.1 fixes the cliff and leaves every other
render byte-identical. The price is a gentle slope change at the switch
(line ink/tone 0.67 below, 0.75 above), never a drop; a test sweeps it.

**Why shorten, not fade alpha.** `setalpha` is a pscat extension and
halftonekit is documented to run under Ghostscript; `dtransform`,
`setlinecap` and a butt-capped stroke are core. Pinned by
`ghostscript_accepts_faint_line_and_cross_tones`.

**Evidence.** `scripts/identity_check.sh --loader lib/halftonekit.ps`: only
`gallery/out_of_register.ps` differs (471 of 2.6M pixels at 144 dpi, max 19
levels; its ridge feet fade to 0). Flooring that callback below the switch
tone makes the two libraries render identically, so the difference is only
the faint region. `gallery/renders/*.png` are not regenerated here.

**Known limits (follow-ups).** `hfthresh` mirrors the renderer's floor; if
`src/gfx.rs` changes it, change both. Under a strongly non-uniform CTM the
pen floor uses the pen's largest stretch while `hfpixel` measures across the
rule, so a small residual plateau can remain. PDF/SVG viewers enforce their
own minimum line width, so faint tones can still look too dark there.
