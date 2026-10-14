## Evening Stream: the meadow's light on its edges, and the stream behind the banks (issue #455, 2026-10-14)

#382 brushed Evening Stream's edges with one pull-out colour each: a compromise
lit-meadow mix along the far shore, nearly plain meadow beside the stream. With
`pkbrushedge /Mass` (#426) and region-list `/Under` (#456) the edges can carry
the meadow's light, and the near banks can follow the stream exactly. Only
`gallery/evening_stream.ps`, the 2x still, the gallery technique text and one
sentence of `docs/guides/painting.md` change; no `lib/` change. `evcloud` is
untouched.

![before and after, full](455-before-after-full.png)
![before (top) and after (bottom), the far shore and the stream's upper banks](455-before-after-shore.png)

- **`evmeadowat`** (x y -> r g b), the new `/Mass` procedure: plain meadow lit
  with the yellow of its light by a share that is largest right on the far
  shore, fades to nothing 60 pt toward the viewer, and is scaled by the glow
  (strongest at the sun's x, a quarter of that toward the corners). It draws
  nothing random. Used by the far shore (`evfaredges`) and both stream banks
  (`evbankedges`), so the shore catches the light under the glow and the banks
  pick it up only at their far ends, then match plain meadow (no yellow
  spikes, which is why #382 had settled for a dull single colour).
- **`evunderbank` is gone.** The near banks' rim is `/Under [ [ /evstreampath
  load /evwaterat load ] evMEADOW ]`: the stream's own fill picks water, the
  meadow is the default, so the cuts follow the stream's outline exactly (the
  straight-line guess was a crude pair of lines). The rim corners differ by
  about 600 px each; the bush, tuft and grass marks of the banks did not move.
- **Why the glow factor.** The meadow's own strokes are graded in y only, so a
  y-only `/Mass` would be one colour along the (horizontal) far shore, which is
  no better than before. The x term is what the issue's "lit toward the glow"
  needs. It lives in the edge colour only, not in the meadow fill, which keeps
  the diff to the edges: the edge's light is a touch of grass catching the glow.
- **Calibration.** Round 1 took the meadow's own tone as painted (about 20% lit
  at the shore, from sampling the old still): a bare change. Round 2 pushed it a
  little. Round 3 (kept): about 30% lit, times the glow factor 0.25 to 2.15, which
  at the stream's mouth is about 65% (the strokes are thin, charge 0.8 and
  depleting, so they read far less lit than their colour).
- **Random state.** One procedural `/Mass` stroke is one `/Brush` call, so
  `evfaredges` now draws differently. The stream and the fall are painted next
  and start from whatever state that leaves, so `evfaredges` ends by resetting
  to the state the old one left (`2133854749 srand`, read off the old file with
  `rrand`): the water, the reflection and the fall are byte-identical.
  `evbankedges` is followed by `evstreamhead`, which reseeds.
- **Pixels** (2x, against the committed still): 24 004 pixels differ, all in
  the far-shore band (y 244 to 256 pt), along the two stream banks, and at the
  near banks' lower rim corners. 0 differ in the four cloud boxes (each cloud's
  extent plus 10 pt) or in the whole sky above y = 262, in the stream's interior,
  the fall, the cabin, the trees, and both near-bank bodies.
- **Reproducibility.** Two 2x renders are byte-identical; `--lint` is clean;
  Ghostscript renders the scene without errors (correctness only).
