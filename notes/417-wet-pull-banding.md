# #417: the canvas specimen's untouched wet pull banded as it faded

The bottom-right panel of `examples/paintkit_canvas_demo.ps` (render:
`docs/dev/design/paintkit_canvas_demo.png`, 2x) pulls a knife stroke down with
a directional `pkwet`. The fade showed horizontal steps.

## Cause

Not the specimen's keys and not the weave: the pull. The wrapped proc reseeds
(`5 srand`), so every pass is an identical copy of the stroke, and the library's
default depth for a directional call is the one that keeps same-side copies
2pt apart (`/Offset 28 /OneSided true` -> 15 layers, step 28/14 = 2pt). At 2x
that is a hard-edged band of one flat colour every 4 device pixels, with about
10 levels of colour change between neighbours. `pkwet`'s own header calls 2pt
"where the echoes start to show"; for a pull that is meant to read as a
gradient it is still too coarse.

## What was tried (the pull panel only; 2x, scratch dir outside the repo)

| variant | steps | result |
|---|---|---|
| base: `/Offset 28`, default depth 15 | 2.0pt | banded |
| `/Offset 28 /Layers 16` (the old cap) | 1.9pt | same |
| `/Offset 15`, 9 default layers | 1.9pt | same, and a short pull |
| `/Offset 15 /Layers 16` (1pt, old cap) | 1.0pt | better, faint steps, a stubby 15pt pull |
| `/Offset 20 /Layers 16` | 1.3pt | banded |
| no `5 srand` inside the stroke | 2.0pt | ragged side edges, same bands |
| `/Offset 28 /Layers 22` (cap lifted) | 1.3pt | mostly smooth, faint steps |
| `/Offset 28 /Layers 29` (cap lifted) | 1.0pt | smooth |

Nothing under the old cap of 16 gets both a long pull and 1pt steps, so the
fix is in the library, as the one number that stood in the way.

## The change

`lib/paintkit.ps`, `pkwet`: the cap on an explicit `/Layers` with `/Direction`
goes from 16 to 32 (error name `pkwet-layers-must-be-1-to-32`; the isotropic
cap is still 6). The default depth is untouched, still
`ceil(Offset/2)+1` capped at 16, and the 64-run chain budget is untouched. So:

- Only a call that names `/Layers` above 16 sees anything, and those raised an
  error before: no committed file can depend on the old behaviour.
- Every other render is byte-identical. `scripts/identity_check.sh` (default
  pick and `--loader lib/paintkit.ps`) shows only the canvas specimen differing;
  see the PR.

The specimen asks for `/Layers 29` (1pt). It is the only call site changed.
Both strokes of the panel move (the touched one too: more passes, so a longer
graded tail). Nothing outside that panel changes.

## Not done

- Moving the default to 1pt steps would deepen every directional pull in the
  gallery (alpine_lake, evening_stream and others) and move their renders. A
  keyed opt-in costs nothing, and the default is a separate decision.
- The pull's left and right edges stagger (the `/Stretch` wobble, one draw per
  pass, now 28 of them). They read as a worn edge, not as banding.
