## Light the far wood's crest (issue #383, 2026-10-14)

Recipe 5 (the far forest) painted its front row's crest with no lit side;
#345 gave `pkscumblein` `/Light`, `/Reach` and `/Feather`, and #347 had
been reworking the recipe's edge first. Recipe 5 now lights the crest the
way recipes 2 and 7 light a mass. Docs only: `lib/` is untouched, and only
`docs/guides/landscape/forest.png` was regenerated (the test rewrites all
eleven; the other ten came out byte-identical).

**What painters do.** A wood against a glowing sky is backlit (contre-jour):
a dark mass with a thin, broken light along its top edge where the glow
catches the leaves, and nothing inside. The brightest values sit on the
outline, the interior keeps its value, and distant planes turn hazy and
warm. Two mistakes to avoid: lighting the whole mass (it reads as haze and
fuses with the back row) and lighting evenly all along the edge. So the
light is two shallow passes, in loads close to the body.

**What was built.** After the front row's tapped crest, the mass is a
region (`0.55 crest` closed down to the waterline, `scpath`) and two
`pkscumblein /Light 90` passes with a small `pkdab` clump on each circle:
a half-light (`/Reach 0.17 /Feather 0.8`, a load 40% from the body toward
the glow) and a core (`/Reach 0.07 /Feather 1`, smaller circles and dabs,
80%). The glow is the body mixed 35% toward a pale, slightly warm
`[0.80 0.82 0.74]`, close to the sky's value but not its cool hue. The back
row takes no pass: it is the paler, hazier plane.

**What only showed in renders.**

- First draft mixed toward the sky the wrong way round (`SKY LINE 0.25
  mix3` is 75% sky) and the front row came out lighter than the back row.
- The region's depth is its whole height (about 22 pt), so where the crest
  dips to 9 pt even `/Reach 0.28` lit the whole front row. 0.17 and 0.07 keep it a rim.
- `pkdab /Size 1.9 /Scatter 1.8` painted grey bubbles; 1.4/1 and 1.1/0.8
  paint a clump about 4 pt across. A thin band needs hundreds of circles
  because `/Count` is over the whole region (700 and 900 here).
- The tapped crest's dabs (`/Scatter 4`) stand above the fill's edge, so a
  region on the fill's edge put the light under a dark rim. A scaled crest
  (`0.64 crest`) fixed it but lifts high humps 3.6 pt and low dips 1.4 pt,
  and breaks the recipe's "mass, crest and spires agree" rule, so the lit
  passes run under `gsave 0 2 translate`, a constant lift.
- The first drafts left the colour set to the light's load, so the spires
  and the shore band came out light. The lit passes now sit in a `gsave`,
  and `grestore` puts the body colour back; the `LINE aload pop setrgbcolor`
  line before the spires is belt and braces.
- The lit passes use random draws, so the spires land in different places
  after than before. That is the stream moving, not a change to the spires.

**Cost.** 0.46 s of CPU before, 0.72 s after (CLI, user time); budget 2 s.
The test's negative check (`PSCAT_TEST_BUDGET_SCALE=0.001`) still fails the
budget test as it should. The snippet runs under Ghostscript with clean
stderr (spires heavier, no antialiasing, as for the other recipes).

**Deferred.** Nothing new. The Evening Stream recipe-2 mismatch is #384.
