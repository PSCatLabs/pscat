## pkfeather / pkbrushedge skip edges hidden inside overlapping blobs (issue #381, 2026-10-14)

#347 shipped with a documented limit: a mass built from several overlapping
closed blobs (a cloud bank, a bush of ellipses) had each blob's edge brushed,
including the stretches lying inside the union. The inward pass painted
backdrop-coloured nicks in the middle of the mass.

**What changed** (lib/paintkit.ps, `pkfeather` only; `pkbrushedge` gets it
through its two calls):

- With two or more closed subpaths (`mfmulti`), each brushed slot on a closed
  subpath asks the whole-path region (the one the side test already built)
  about a pair of points `/Spacing / 12` either side of the stroke's anchor.
  Both in the mass means the edge is hidden there, and the stroke is not
  emitted; but before calling it hidden the pair is asked once more at a
  sixty-fourth of the step, because a hole thinner than the step puts both
  points in the mass while its boundary is plainly visible (Codex round 1:
  an 80 x 0.1 hole in a square lost most of its strokes). A hidden edge stays
  inside however small the step. The slot's four `frnd` draws and its `mfk` count are still taken,
  so the strokes that remain are the ones the edge would have had and the
  contract (one draw per call, four per slot) is unchanged.
- The subpath's side vote, which a hidden stretch gave no vote to (and
  wasted 40 halvings trying), now tries a both-in probe once more at a
  sixty-fourth of the step in the multi case, then gives up, and votes over 28
  places instead of 7 (a both-out probe, a thin mass, still halves as before). The issue's
  other symptom (the vote flipping a whole blob) came from a blob mostly
  swallowed by another: few of its 7 fixed probes were on visible edge. A
  bar inside a circle with just its end sticking out starts at an angle where
  all seven first probes land on hidden edge, and is in the self-test; with
  the old vote it brushes the end inward.
- One closed subpath never probes: it is its own whole mass, so every
  existing single-outline use renders byte-identically
  (`scripts/identity_check.sh --loader lib/paintkit.ps`: 43 of 43 pages
  identical across 34 files, including the brushedge specimen's field and
  pond, which has two closed subpaths; the control, overlapping ellipses
  through `pkbrushedge`, differs as it must).

**Cost.** Two `siinside` calls per brushed slot of a multi-closed path, four
when it is hidden (the second look at a sixty-fourth of the step), twice for
`pkbrushedge` (inward and outward). Measured against the old code on a
six-ellipse bank at 260x130 (`pkbrushedge`, spacing 3): 1.14 s CPU before,
0.78 s after. It is cheaper because the hidden strokes were each a brush
call, and the old vote paid up to 80 region probes per hidden probe. The
`pkfeather` motion alone, which paints nothing: 0.46 s before, 0.45 s after.
(A first version halved the step nine times on every hidden slot: 2.65 s,
more than twice the old cost. One extra look is what the thin-hole case
needs.)

**Not done / limits.**

- A single closed subpath that overlaps itself (a figure of eight) is not
  tested: it has no second subpath to hide behind.
- Open subpaths are never skipped. They are part of the region (as in #347)
  but a crest drawn over a mass is the caller's decision.
- A hole thinner than `/Spacing / 768` (0.004 pt at the default; 0.0006 device
  pixels at 144 dpi) is read as a hidden edge: both looks fall in the mass
  around it, so its boundary gets no strokes, where the old code halved the
  step 40 times and kept them (Codex round 2: an 80 x 0.001 slit in a square).
  Nothing a render can show distinguishes such a hole from none, a third look
  would only move the threshold, and every look costs a probe on every
  hidden slot.
- A stroke anchored exactly on a vertex where the outline turns sharply can,
  rarely, see both probe points in the mass (the offset leaves the segment's
  normal) and be skipped. One slot, not a stretch.
- Blobs wound opposite ways cancel under the nonzero rule where they
  overlap, so that overlap is a hole and its edge is brushed into it.
  This is the rule `fill` paints with, so it is what the eye sees.
- The specimen (`docs/paintkit_brushedge_demo.png`) and the cookbook's
  bank thumbnail are untouched: neither overlaps (all eleven cookbook
  recipes render byte-identically, base against head, 360x240 at 144 dpi).
  `docs/paintkit_brushedge_overlap.png` is the before/after of a six-blob
  bank (the same driver with the old library and the new).
