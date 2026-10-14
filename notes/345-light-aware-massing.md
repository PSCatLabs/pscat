## Light-aware massing: a lit side on a scumbled region (issue #345, 2026-10-14)

Found while painting Evening Stream (#187): a scumbled cloud or bush had no
tool that knew where the light was on the *mass*. `pkdab /Light` lights
each dab, which at cloud scale reads as a leopard print, and every circle of
`pkscumblein` was painted alike, so #187 faked a lit underside by scrubbing
the cloud in its light, then the body over it shifted up by 0.3 of its
height. That trick works only for light from straight above or below.

**What was built.** `pkscumblein` takes three new keys:

- `/Light [degrees]`: where the light comes from, counterclockwise from +x,
  the same convention as `pktrowel`/`pkfan`/`pkbroad`. Set, the motion keeps
  only the circles on the region's lit side.
- `/Reach [fraction of the region's depth along the light]`, default 0.3:
  how deep the lit band goes.
- `/Feather [0..1]`, default 0.5: how much of the band is a graded border.

A circle is lit when a step of `Reach * depth * (1 - Feather * u)` from its
centre toward the light lands outside the region (one `scin`), where `u` is
the cell's existing phase draw. That is #187's shifted copy, generalised to
any angle and done per circle. Every lump of the outline gets its own lit
cap, because the test is "near the edge facing the light", not "far along
the light's axis".

**Departure from the issue's wording.** The issue suggested grading by
distance *along the light axis* across the region. That lights a straight
band across the whole region, and a concave lump on the shadow side of the
axis would get no light at all. Distance to the light-facing edge is what a
painter models (each clump of a bush has its own lit top), and it is what
the #187 workaround already did for straight-up light.

**Design calls.**

- *A filter on the motion, not a painting wrapper.* Motions paint nothing,
  so a "lit colour" cannot live in `pkscumblein`. The artist paints the
  body pass over the whole region, then the same call with `/Light` in a
  lighter load. A shadow side is `/Light` turned round with a darker load,
  so there is no `/Side` key.
- *No new draws.* The dither for `/Feather` reuses the cell's phase draw
  (`mcph`, uniform and independent of where the circle lands). Unset, the
  code path is the old one, byte-identical. Set, the circles kept are a
  subset of the unlit call's under the same seed. The cost is that circle
  start phase and lit-ness are correlated. That is invisible: the phase is
  only where a closed circle starts.
- *`/Count` keeps its meaning* (circles over the whole region), so a lit
  pass that keeps 30% of the region holds about 30% of `/Count`. The
  recipes ask for more on a lit pass.
- *Depth is the bounding box's chord through its centre along the light*
  (`min(w/|cos|, h/|sin|)`), not the box projected on the light's axis. The
  first draft used the projection, and a wide, low bush lit at 135 degrees
  came out lit over half its area, because the projection is
  `(w + h) * 0.707`, nearly twice the real depth.
- `/Reach` or `/Feather` without `/Light` is rejected
  (`pkscumblein-reach-needs-light`), as `pktrowel` rejects `/Light` without
  `/Colors`: those keys would silently do nothing.

**Painting it (from looking at how painters model masses).** A tree or
bush mass reads as one value, with its light and dark as variations on it.
The usual mistakes are lighting one side too much and scattering highlight
spots over the mass. So the specimen and recipes use two lit passes: a
deep, graded half-light (`/Reach 0.4 /Feather 0.8-0.9`, a load between the
body and the light), then a shallow core (`/Reach 0.15-0.2`) in loads close
to the body's value. Two things only showed up in renders:

- A thin lit band holds few circles, and a lone circle under `pkbroad`
  paints as a ring. The core pass uses smaller circles (and more of them).
- Lit circles inside the body's `pkwet` leave the displaced passes' rings
  on top of the body. Work the lights in their own `pkwet` with `/Under`
  set to the body colour and a lower `/Soft`, since the body is what is
  under them.

**Specimen.** `examples/paintkit_massing_demo.ps`, rendered at 2x to
`docs/paintkit_massing_demo.png`. Before and after for an evening cloud
lit from below, a bush lit from the upper left with a shadow side (pkdab
clumps on the circles), and a cumulus lit from a low sun on the left (the
angle the old trick could not do), plus a row showing which circles a lit
pass keeps at three `/Reach`/`/Feather` settings.

**Cookbook.** Recipe 2 (Clouds) uses the native form in place of the
shifted copy. Recipe 7 (Bushes) is now a massed region (pkscumblein
circles into pkdab clumps) with a shadow side and a lit side, replacing the
three spine layers that painted a flat "sausage". The #345 row is gone from
"Tools not yet landed". Only `clouds.png` and `bushes.png` were
regenerated.

**Tests.** `%%SelfTest` blocks for the guards and for the geometry (lit
circles stay in the band facing the light, from above and from below, and
form a proper subset of the unlit call under the same seed). A gs check
for the specimen. `scripts/identity_check.sh --loader lib/paintkit.ps`
proves existing renders are byte-identical.

**Deferred.**

- The far wood's crest (recipe 5) could use the same pass. #347 is reworking
  that recipe's edge, so it is left for after #347: #383.
- Evening Stream (#348) still uses the shifted-copy trick, so the cookbook's
  recipe 2 no longer matches its source piece: #384.
- `/Count` caps the lit passes. Because it counts circles over the whole
  region and stops at 5000, a dense core pass on a large mass hits the cap
  before the band is dense (the specimen already asks for 1000 to 1200 on a
  180-pt lump). A band-relative count would lift that. No issue filed:
  nothing has hit the cap yet.
