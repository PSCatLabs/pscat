# #484: a directional pull's side edges staircased; /Smooth

## Cause

`pkwet`'s directional plan draws one seeded across-offset per non-core pass:
`(f * Offset) / Stretch * uniform(-1, 1)`, each draw independent. Each pass is
an identical copy of the mark (the stroke reseeds), and after #417 the canvas
specimen's pull has 29 passes a point apart along the pull. Neighbouring copies
therefore sit up to twice `Offset/Stretch` apart *across* the pull (about 5.6pt
at `/Stretch 10`, `/Offset 28`), and where each copy's edge shows, the side of
the stroke steps. Measured on a 31-pass pull (`/Offset 30 /Stretch 3`): the
sum of neighbour-to-neighbour steps is 105pt for independent draws.

A larger `/Stretch` (the existing damping knob) shrinks the wobble and the
steps with it: at `/Stretch 25` against the specimen's 10 (2x render, scratch,
not committed) the steps are about a point and the stroke loses its wobble, but
they are still steps. `/Smooth` keeps the wobble's size and changes its texture.

## The change (opt-in)

New `pkwet` key `/Smooth` [0..1], directional only (`pkwet-smooth-needs-a-direction`
otherwise, plus `-must-be-a-number`, `-must-not-be-a-procedure`, `-must-be-0-to-1`).
The draw is low-passed along the passes, one filter per side (a two-sided sweep's
sides each read their own neighbours):

    w = a*w + sqrt(1 - a^2) * d        (the first pass on a side takes d)

with `a = /Smooth`. The variance of a uniform draw is kept, so `/Stretch`
still sets the wobble's size. `/Smooth 1` holds the first draw (the pull
shears one way: the held draw is multiplied by a reach that grows with each
pass, so the pull slants). Still exactly one draw per non-core pass, so the random stream
after the call is the same with or without it (a test pins it). The default 0
never enters the new branch: nothing that omits the key changes.

Same 31-pass pull at `/Smooth 0.9`: 36pt of steps (-65%).

The canvas specimen picks 0.9 by eye from 0.8, 0.9, 0.97 (2x crops): 0.8 and
0.9 give a clean edge; 0.97 leans visibly.

## Why not the default

A default of, say, 0.9 would move every committed directional pull, among them
`gallery/alpine_lake.ps` and `gallery/evening_stream.ps`, which have other open
work. Opt-in leaves them byte-identical (`scripts/identity_check.sh`, see the PR).
Making it the default is a separate decision: #516 (related: #483, the default depth).

## Renders

Only `docs/dev/design/paintkit_canvas_demo.png` is regenerated. Before and after
of the pull panel, side by side:
`docs/dev/design/paintkit_canvas_pull_484_compare.png` (left the #417 render, right
this branch). The result is steps reduced, not removed: the upper stroke's right
edge still shows a jog where the smoothed drift turns. `scripts/identity_check.sh`
against the merge-base tree (default pick and `--loader lib/paintkit.ps`, with a
control that uses `/Smooth`, which the base ignores): 49 of 50 identical in each, the
one difference `examples/paintkit_canvas_demo.ps`. `gallery/alpine_lake.ps` and
`gallery/evening_stream.ps` are identical.
