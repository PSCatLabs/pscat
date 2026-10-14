## Evening Stream: the mass edges brushed with pkbrushedge (issue #382, 2026-10-14)

`gallery/evening_stream.ps` hid its ruled edges by hand (#347 asked for the
tool). With `pkbrushedge` landed, the hand-placed breakers that only existed to
break a ruled edge are replaced by it; the 2x still is re-rendered. Only the
`.ps`, the still and docs change; no `lib/` change.

- **What was brushed.**
  - *Stream banks* (`evbankedges`): the open `evleftedge` / `evrightedge`
    paths, meadow colour pulled out toward the water (`/Outward 0` and `180`),
    the water cut back in. Run after `evstream` and `evfall`, which repaint the
    whole stream rectangle under a clip and would otherwise erase it.
  - *The stream's head* (`evstreamhead`): the flat bright strip where the water
    meets the far shore, which no one had hidden by hand.
  - *The far shore* (`evfaredges`): the two `pktap` + `pkdab` rows along the
    meadows' far edge are gone; `pkbrushedge` with the wood's foot as `/Under`.
  - *The near banks' rims* (`evnear`): the bank colour pulled out over the
    meadow, or over the stream where it runs behind the bank's lower corner.
- **What stayed.** The grass pulled up along each rim and in tufts (it grows
  there), the autumn trees, the pines, the rocks and the bushes along the
  stream, all of which are part of the picture. The rim and stream-bank bush
  `pkdab` calls were thinned (`/Density` 2.5 to 1.2 on the near banks, 1.6 to
  0.8 on the stream banks, `/Scatter` a little lower) so they read as a few
  bushes, not a chain along the edge.
- **`/Under` is a position-dependent procedure in both bank cases.**
  - `evunderstream`: above the fall the water beside the banks is the dark
    pulled reflection of the wood, not the sky gradient, so it returns the water
    colour mixed 0.9 toward `evWOOD`; below the fall it is `evwaterat`. With
    plain `evwaterat` the cuts printed pale sky-coloured nicks into the meadow.
  - `evunderbank`: meadow, except where the stream runs behind the bank's lower
    inner corner (a crude straight-line test of the stream's edges; no path
    operator inside the procedure).
- **Pull-out colour matters.** `pkbrushedge` pulls out the *current* colour, so
  it is set to what is really painted at each edge: the meadow colour lit toward
  the glow along the far shore, and nearly plain meadow beside the stream
  (the lit meadow colour showed as yellow spikes).
- **Random state.** The three new edge procedures each start from their own
  `srand` (17, 19, 21), so the elements after them keep their marks. The
  exception is `evnear`: its `pkbrushedge` draws from the stream partway
  through the procedure and the bush density changed, so the rim bushes, tufts
  and grass of both near banks fall differently from the first still.
- **Review.** Three render rounds: first colour too bright, then a pale glitch
  at the stream's head from a sky-coloured `/Under`, then bushes thinned. The
  result is a clear improvement at 2x: the stream and shore edges read as brushed
  and the bank edge no longer looks cut out.
- **Reproducibility.** Two 2x renders are byte-identical; `--lint` is clean;
  Ghostscript renders the scene without errors (its rand stream differs).
- **Left alone on purpose** (separate tickets): the cloud lit side (#384) and
  the fall's `/Scatter`.
