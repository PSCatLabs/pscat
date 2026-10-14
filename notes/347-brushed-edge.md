## Brushed edges for filled masses: pkbrushedge and pkfeather (issue #347, 2026-10-14)

Every big mass in a landscape starts as a filled path with a ruled vector
edge. Evening Stream (#187, PR #348) hid those edges by hand (dabs, bushes,
grass), one composition decision per edge. This adds the tool.

**What shipped** (lib/paintkit.ps, new "Brushed edges" section after the
motions; nothing existing changed):

- `opts pkfeather` — a brush motion. The current path is a mass's edge; it is
  replaced with short strokes across it along the local normal. `/Toward
  /out | /in | /both`, `/Length`, `/LengthJitter`, `/Overlap` (the share of a
  stroke on its starting side), `/Spacing`, `/Wander`, `/Lean`, `/Slant` (a
  fixed tilt from the normal), `/Bow`, `/Crisp` + `/CrispRun` + `/CrispSeed`
  (lost and found: stretches left untouched), `/Outward` (for open subpaths).
- `opts pkbrushedge` — the one-call edge. Inward cuts in `/Under` (colour,
  pigment name, or procedure `x y -> r g b`, asked per stroke) first, then the
  outward strokes in the current colour, each through `/Brush` (default a
  small pkbroad that runs dry: `/Width 5 /Charge 0.8 /Depletion 0.9 /Edge 0.6
  /Grain 10`). `/InLength`, `/InSpacing`. The caller's path, colour and
  graphics state survive.

**Design calls.**

- *Which side is out* is decided once per subpath. A closepath'd subpath is
  a mass outline: seven probe points just left of the walk are tested against
  the whole path as a nonzero region (artkit `scpath`/`siinside`), majority
  wins. Per-subpath signed area was the first plan and is wrong at holes (a
  hole's own interior is the hole, so its strokes would go into the mass);
  the region test also makes mirrored CTMs free. An open subpath (a visible
  crest) faces `/Outward` by its summed normal, so the crest can be drawn
  either way. Closure comes from `pkscanclosed`, not coincident endpoints.
- *Lost and found.* Painters' edges are not broken evenly all along; `/Crisp`
  is named for what the artist sees (stretches left crisp), not "lost", which
  in painting means the opposite (a dissolved edge). The first version ran a
  per-slot two-state walk on `frnd`; in `pkbrushedge` the two passes then had
  different crisp stretches, so only about Crisp squared of the edge was
  really untouched (review). The stretches are now a function of arc length
  and `/CrispSeed` alone (a small private hash, `mfu`), and the wrapper gives
  both passes one seed.
- *Order and density of the two passes.* First prototypes painted the outward
  pass, then an inward pass as dense as it: the inward strokes' inner ends
  ruled a pale band just inside the edge. The inward pass is now first,
  half as dense (`/InSpacing 2`), and mirrors `/Overlap`, so each cut covers
  the stretch of the normal an outward stroke would and only its tip nicks
  the mass; the outward strokes land on top and interlock with the nicks.
- *Default brush.* Compared pkbroad, pkdry, pkliner, pkfan, pkoil and pktrowel
  at the cookbook scale. Thin brushes (width 2-3) read as a hairy fringe;
  pkoil as a scalloped rope; pkfan and pktrowel too faint. A 5 pt pkbroad that
  runs dry, short strokes (`/Length 8`) and a wide lean (`/Lean 30`) read as a
  broken painted edge. pkliner at width 2 is the grass-blade variant
  (`/Brush`).
- *`/Under` takes pkwet's forms* (issue #141, one meaning per key) plus a
  bare pigment name, and needs no `/At`: each inward stroke asks the
  procedure at its own start.
- Errors mid-call (a bad key, a backdrop procedure's bad answer) unwind every
  gsave the wrapper opened (`pegsave` counts them) before passing on.

**Cookbook.** Recipe 10 (Bank) now brushes its edge with `pkbrushedge`
instead of tapped dabs, and paints the lip before it so the lip is broken
with the rest of the rim. Recipe 5 (Forest) keeps its tapped crest, which is
the right break for foliage; its note points non-foliage masses at recipe 10.
The "Tools not yet landed" row for #347 is gone.

**Checks.** `%%SelfTest` block: a guard per key, a safety limit, and behaviour
(a ring wound both ways and under `1 -1 scale` must have every stroke end
outside the mass; an open crest drawn right to left brushes up). Flipping
the side test fails it. `scripts/identity_check.sh --loader
lib/paintkit.ps`: every existing render byte-identical. The specimen renders
in Ghostscript too.

**Not done.** No `eofill` rule (the region is nonzero, like `fill`); an
outline mixing open and closed subpaths puts the open ones into the region
too. One non-overlapping outline per call: a mass built from overlapping
closed blobs has edges hidden inside the union, and those get brushed too
(the inward pass would nick the middle of the mass). Brush the union's
outline, or each blob in its own call where its edge shows. Evening Stream (PR #348) is not edited here.
