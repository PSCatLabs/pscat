# Paint media: `lib/paintkit.ps` (load after artkit)

Paths such as `examples/...`, `gallery/...` and `docs/...` name files in the
[pscat repository](https://github.com/PSCatLabs/pscat); an
installed pscat does not include them. Every `(lib/...) run` works anywhere:
the library is compiled into `pscat`.

```postscript partial
(lib/artkit.ps) run
(lib/paintkit.ps) run
```

Every preset takes one dict of artistic controls, paints in whatever
color you already set (there is no color key anywhere in the file), and
is deterministic under your own `N srand`. `pscat --capabilities` has
each one's real parameter list, with every parameter's unit in brackets
at the start of its description (`[points]`, `[degrees]`, `[count]`,
`[0..1]`, `[fraction of /Width]`, `[per /Width]`) — reach for it rather
than guessing. Two names are shared vocabulary: `/Spread` is always a
0..1 fraction of `/Width`, and `/Load` is always a contact rate per
`/Width` of travel. (Older code may pass `pkwet /Spread`, `pktrowel
/Load`, `pkdab /Spread` or `pkcrisscross /Spread`; those still work as
deprecated aliases for `/Offset`, `/Cover`, `/Scatter` and `/Lean`.)
Which tool to pick:

- **`pkribbon`** — the foundation: your path as a centerline, filled as
  a variable-width band. `/Pressure` (`pkflat`/`pktaper`/`pkbell`) is
  the whole point. Anything that should read as *one continuous
  stroke*.
- **`pknib`** — calligraphy. Width driven by the angle between the path
  and a fixed nib angle. One open subpath per call.
- **`pkdry`** — dry bristle. Scattered thin bristles broken into
  ink/no-ink runs; `/Load` vs `/Dropout` sets how dry.
- **`pkspray`** — aerosol. Discrete particles with a radial falloff,
  overspray mist, trigger-dwell bursts. Respects an active clip, so
  `charpath`+`clip` gives you a stencil for free. For clouds and mist
  use a fine speck: `/Speck` about 1–1.2 with a small `/Nozzle` (~8)
  and `/Density` 150–400 fuses the core into vapour, and a `pkwet` pass
  grades the fringe. At `/Speck` 2–3 the same counts give coarse lumpy
  bars that read as paint, not vapour.
- **`pkoil`** — loaded oil impasto. A solid base ribbon *plus* bristle
  ridges and a highlight/shadow lift. Use it when you want thickness
  and an unbroken mass. It is a vector suggestion of impasto: no fluid
  mixing, no physical paint thickness, no pigment simulation.

  ```postscript
  (lib/artkit.ps) run
  (lib/paintkit.ps) run
  1 srand
  0.55 0.35 0.2 setrgbcolor
  newpath 40 120 moveto 160 150 lineto 300 110 lineto
  << /Width 16 /Ridges 12 /Load 0.9 >> pkoil
  ```
- **`pktrowel`** — palette knife / trowel. A rigid flat blade dragged
  across the surface: broad angular masses, lengthwise scrape streaks,
  ridges piled at the blade edges, and **no base pass**, so whatever is
  underneath keeps showing through. Choose it over `pkoil` when you
  want the mark to look *placed by a tool* rather than brushed — large
  color masses, architectural planes, cliff faces, water, anything
  built from flat overlapping shapes. `/Cover` widens the deposit,
  `/Coverage` fills it, `/Viscosity` sets chatter vs chunk, `/Scrape`
  opens streaks, `/Angle` rakes the blade.
- **`pkfan`** — fan brush: foliage, evergreen branches, grasses, cloud
  feathering. Bristles leave a flattened ferrule and *splay* apart along
  the stroke (`/Splay`; at 0 it's `pkdry`'s parallel bristles). Many
  fine bristles + high `/Load` = soft feathering; few + low `/Load` +
  high `/Dropout` = separated bristles. `/Ragged` feathers the tips —
  leave it on, or the mark reads as a comb. **A single-point subpath is
  a pressed fan radiating about the point**: that's how you get foliage
  clusters and shrubs, one dab each.
- **`pkbroad`** — broad flat brush, the block-in tool: sky and water
  bands, reflections, broad masses. Its trick is that the brush **runs
  out** — `/Depletion` fades the mark from solid to broken along the
  stroke, `/Charge` is how loaded it started. `/Depletion 0` gives an
  even block-in. Reach for it over `pktrowel` when you want a brushed
  mass rather than a scraped one, and over `pkribbon` whenever the mark
  should look applied rather than drawn.
- **`pkliner`** — liner/detail brush: branches, trunks, grass stems,
  rigging, whiskers, shoreline accents. The fine marks you make *last*.
  Unlike every other preset here it **leaves the centerline you drew**
  (`/Waver`), which is what stops a branch reading as a ruled line;
  `/Taper` whips it from full width to a point, and `/Charge` +
  `/Depletion` break the tail into shortening dashes. A **single-point
  subpath is a pressed dot** — that's your speck, stamen, distant bird,
  sparkle on water. Reach for it over `pknib` whenever the mark should
  look painted rather than penned.
- **`pkdab`** — foliage/dab brush: bushes, leafy masses, clumps of
  highlight, any irregular vegetation texture. The path is a **spine**:
  every stop seeds a cluster of blobby dabs within `/Scatter` points, at
  `/Density` per `/Size` of travel (so `/Pitch` doesn't change
  coverage). One set of dab geometry is painted as a darker underlayer
  (`/Shadow`), the body in your current colour, and lighter highlight
  dabs (`/Highlight`) facing `/Light` (a fixed user-space angle, not
  relative to the stroke), so highlights always sit on the mass they
  light. A **single-point subpath is a standalone clump** (`/Clump`).
  For depth, call it several times: dark/large first, light/small last,
  each on its own tighter spine. It does not know what a bush is — the
  shape comes from your spines.
- **Loaded brushes: `/Colors`, `/ColorMix`, `/Light`** on `pkfan`,
  `pkbroad`, `pktrowel` (and `/Colors` per dab on `pkdab`) — one stroke in
  two or more colours, the way a real brush is loaded. `/Colors [[r g b] ...]`
  (1 to 8) runs from the head's left edge, looking along the travel, to its
  right; `/ColorMix` is 0 for hard bands and 1 for a smooth gradient
  (default 0.3), and `/Colors` replaces the current colour. **The load is
  brush-relative, so a stroke drawn backwards swaps its colours**: for an
  evergreen whose branches go out to both sides of the trunk, add `/Light
  degrees` (the direction the light comes from, as `pkdab`'s) and each
  subpath puts the first colour on whichever side faces it. Branches must be
  real strokes out from the trunk (`moveto`/`lineto`): a pressed fan or a dab
  is never flipped. A low `/ColorMix` (around 0.1)
  keeps the seam crisp, the stroke needs enough `/Width` to show both colours
  at page size, and the entries are evenly spaced, so repeating one
  (`[lit dark dark]`) gives it more of the head. `/HighlightColor` and `/ShadowColor`
  on `pkdab` and `pkoil` are the hues the lit and shaded layers mix toward
  instead of white and black (a warm yellow and a cool blue make a sunlit
  bush; `/Highlight` and `/Shadow` still say how far). A loaded mark goes
  inside a directional `pkwet` without any extra step: `pkwet` grades loaded
  colours itself, so a loaded reflection pulls down and fades. Never call
  `setrgbcolor` inside the pkwet procedure. `/Colours` is rejected, not
  ignored. `examples/paintkit_loading_demo.ps` shows all of it.

- **`pkwash`** / **`pkpaper`** — watercolor and its ground. The only
  presets needing pscat's `setalpha`; under plain `gs` they fall back
  to flattening against white and overlaps stop mixing, so verify real
  alpha output with `pscat --pdf`, not `gs file.ps`. `pkwash` multiplies
  each layer's `/Alpha` by the caller's `setalpha`, so
  `gsave 0.6 setalpha ... pkwash grestore` fades a wash, and `pkpaper`
  does the same for its ground and grain. `pkpaper` is a
  watercolor ground only; for paper, canvas or print surfaces in
  general use `lib/surfacekit.ps`.

  `pkpaper` takes the rectangle it covers first, then its options dict:
  `x y w h << ... >> pkpaper`. Call it with only the dict and it stops
  with an error naming the missing rectangle.

  ```postscript
  (lib/artkit.ps) run
  (lib/paintkit.ps) run
  0 0 612 792 << /Tone [0.97 0.955 0.92] /Grain 0.6 >> pkpaper   % x y w h opts
  showpage
  ```
- **`pkwet`** — not a brush: a *softener* that wraps any of the above.
  It re-runs your mark-drawing proc several times, each pass nudged
  further off the path and mixed further toward `/Under`, the color you
  declare is already on the canvas there — so the mark settles into the
  layer beneath instead of sitting on it. This is the wet-on-wet
  vocabulary: skies, clouds, mist, distant planes.

  ```postscript
  (lib/artkit.ps) run (lib/paintkit.ps) run
  { newpath 40 400 moveto 200 430 380 390 560 420 curveto
    << /Nozzle 22 /Density 70 >> pkspray }
  << /Soft 0.8 /Under [0.62 0.72 0.86] >> pkwet
  ```

  `/Soft` is the only knob you need, and `/Soft 0` is exactly a plain
  call. **Pair it with a broken or particulate brush** — `pkspray`,
  `pkdry`, or `pktrowel` at low `/Coverage`. A solid brush (`pkribbon`,
  `pkoil`) keeps its own hard silhouette and just gains a halo. Want a
  textured underlayer to read through mist? Use a low-`/Coverage` mark
  — the gaps do it, no transparency needed. Uses no alpha, so unlike
  `pkwash` it behaves the same under plain `gs`.

  `/Direction` (degrees) lines the passes up along one axis instead of
  scattering them: a reflection pulled down (`/Direction 270 /OneSided
  true`), a mountain foot lifted into mist (`90`, one-sided), a sky
  swept across (`0`, both ways); `/Offset` is how far, in points, the
  outermost pass travels, and `/Stretch` sets how narrow it stays
  across the axis (`/Smooth 0.9` keeps that wobble from stepping pass to
  pass, so a long pull's side edges read as a clean drift). Two rules: the wrapped proc must paint in the color
  it *inherits* (wrap a scene element by element, each with the
  `/Under` really beneath it), and the direction is in the user space
  you call pkwet in — put a reflection's `1 -1 scale` *inside* the proc
  so 270 is down the page. `examples/paintkit_wet_pull_demo.ps` shows
  all three.

  `/Under` can be a procedure `x y -> r g b` (with `/At [x y]`, required unless `/Box` is given)
  when the backdrop changes across the mark, such as a cloud over a graded
  sky: each pass is mixed toward the backdrop at `/At` plus its own
  displacement, so the pale/dark rim a single colour leaves goes away.
  It must be a pure function (no painting, no `pkwet`, no stack
  reliance) returning three numbers in 0..1; the answer is per pass, not
  per pixel (for loaded-colour marks `/PerFill true` beside `/At` asks it at each fill);
  for a tall mark give `/Box [x y w h] /Bands [cols rows]` instead
  of `/At` and it is asked per band (a cell no taller than about half
  `/Offset`; the mark runs (Layers - 1) x cells + 1 times, at most 64).
  `examples/paintkit_wet_under_demo.ps` is the side-by-side, and
  `examples/paintkit_wet_bands_demo.ps` the tall-mark one.

- **`pkcanvas` / `/Touch`** — a shared canvas under light strokes. Every
  other "less paint" key breaks one stroke up on its own; `/Touch` is the one
  that is anchored to the canvas. `<< /Seed 7 /Weave 2.5 >> pkcanvas` once,
  *first*, in the space the piece is painted in (before any `translate` or
  `scale`), sets a seeded weave of raised knuckles and paints nothing. Then
  `/Touch 0..1` on `pktrowel`, `pkdry`, `pkfan`, `pkbroad` or `pkliner` lands
  the mark only on the knuckles that pressure reaches: lower is lighter, 0
  deposits nothing, 1 is full contact (the same as unset). Because the surface
  is one, strokes that cross share it: give the heavier stroke the higher
  touch and lay it first (dry brush `0.7`, knife `0.55`), and the knife lands
  on a subset of the brush's knuckles. A touch with no `pkcanvas` is an error,
  `/Touch` takes no `rand` draws (turning it on moves nothing after it), it
  composes with `/Colors`, `/Load` and `/Coverage`, and inside a directional
  `pkwet` every pass catches on the same knuckles, so a pulled reflection
  streaks along the weave. `/Weave` is in canvas points, so supersampling
  with `2 2 scale` put *before* `pkcanvas` keeps the pattern.
  `examples/paintkit_canvas_demo.ps` shows the crossing strokes with and
  without the surface, a `/Touch` sweep, a coloured load and a wet pull.
  One touched stroke may reach at most 60000 weave cells: about a 612 pt square at the default `/Weave 2.5`, measured on the whole call's reach box (a long diagonal counts its bounding box, not its length). Past that, `pkdry`, `pktrowel`, `pkfan`, `pkbroad` and `pkliner` raise `limitcheck: pkX: /Touch is limited to 60000 weave cells ... this stroke reaches N cells`. Fix it by dropping `/Touch` on that mass, painting it as several strokes that each reach less, or calling `pkcanvas` with a larger `/Weave`. There is no automatic tiling.

- **Brush motions** — `pkcrisscross`, `pkscumblein`, `pkscumble`,
  `pktap`, `pkpull`, `pkzigzag`. Not brushes: *path builders*, the other
  half of a painting (the brushes are heads, these are the named wet-on-wet
  strokes). Each paints nothing and leaves an ordinary path, so it feeds
  any preset: `0 0 700 160 screct << /Count 90 /Length 50 >> pkcrisscross
  << /Width 30 >> pkbroad` is a sky block-in. Pairings: criss-cross →
  `pkbroad` (sky); scumble (`pkscumblein` for circles in a region,
  `pkscumble` for a chain of loops along a line) → `pkoil` or `pkdry`
  (clouds, bushes); `pktap` → `pkfan`, or `/Length 0` → `pkdab` (a clump
  per tap); `pkpull` → `pkbroad` with `/Direction -90` (reflection pulled
  down from a horizon) or `pkliner` with `/Direction 90` (grass pushed
  up, `/Bow` for blades, `/Scatter` so the roots do not sit on one ruled
  line); `pkzigzag` → `pkliner` or `pkbroad` (water).
  Two rules: **every motion replaces the current path** (so a guide, or
  the outline `scpath` leaves behind, is never painted — one motion, one
  paint, then build the next); and the region motions take an artkit
  region (`screct` or `scpath`) as the operand under the dict while the
  spine motions read the current path as the guide. They draw only from
  `frnd`, so they are deterministic under your `srand`.
  `examples/paintkit_motion_demo.ps` shows each beside its brush.
  **A lit side on a mass** (clouds, bushes, a tree crown): `pkscumblein`
  with `/Light <deg>` keeps only the circles along the edge that faces the
  light (`/Reach` how deep, `/Feather` how graded). Paint the body over the
  whole region first, then a half-light (`/Reach 0.4`) and a core
  (`/Reach 0.15`, smaller circles) in loads close to the body's value; the
  shadow side is `/Light` turned round. Specimen: the
  [massing demo](https://github.com/PSCatLabs/pscat/blob/main/docs/dev/design/paintkit_massing_demo.png)
  in the pscat repository.

- **Brushed edges** — `pkbrushedge` (and the motion under it, `pkfeather`).
  A filled mass (a meadow, a bank, a pond) has a ruled vector edge; fill it,
  then `MASS setrgbcolor massPath << /Under BEHIND >> pkbrushedge` pulls the
  mass's colour out across the edge and cuts the colour behind back in, so
  the edge reads as painted. Give it the closed outline you filled (holes
  and mirrored reflections work: the mass itself says which side is out) or
  just the visible crest, left open, whose out side is `/Outward` (default
  up). `/Under` is a colour, a pigment name or a procedure `x y -> r g b`
  (a graded sky or water); `/Slant` tilts the strokes (wind in a meadow, a
  dragged bank); `/Crisp 0.3` leaves stretches untouched (lost and found);
  `/Brush` swaps the brush (`{ << /Width 2 >> pkliner }` for grass blades).
  `examples/paintkit_brushedge_demo.ps` shows before and after.

**Seeding: repainting the same stroke.** One rule covers every preset: put
`N srand` immediately before the call and that call paints the same marks
every time. Do it before each stroke you will repaint (every frame of an
animation, a stroke you are still tuning). The number of `rand` draws a call
takes depends on its path (a 100 pt `pkdry` takes about 2300, a 600 pt one
about 22000), so without a `srand` per stroke, editing one stroke re-rolls
every stroke after it. What each preset does with the stream, counted by
asking for `rand` after a call (`notes/1414-paint-seeding-and-cost.md` has
the probe):

| Preset | Draws from `rand`? | `/Seed` |
| --- | --- | --- |
| `pkdry`, `pkspray`, `pkoil`, `pktrowel`, `pktrowelplane`, `pkfan`, `pkbroad`, `pkliner`, `pkdab`, `pkwet`, `pkfeather`, `pkbrushedge`, the brush motions (`pkcrisscross`, `pkscumblein`, `pkscumble`, `pktap`, `pkpull`, `pkzigzag`) | yes | **none: the key is silently ignored** (no error, identical output with or without it), so `srand` is the only way to pin them |
| `pkribbon`, `pknib` | no, unless `/Jitter` is above 0 | none |
| `pkwash`, `pkpaper` | one draw without `/Seed`, none with it | `/Seed N` pins this call alone: the same output under any `srand`, and no draw taken, so adding or removing the call shifts nothing after it |
| `pkcanvas` | no | `/Seed` (default 1, a constant): the weave is the same on every run unless you change it |

`/Touch` takes no draws either. So for a repainted frame: `N srand` before
each brush stroke, and `/Seed` (or a `srand`) on each wash and ground. Doing
both on a wash is harmless, just not needed.

```postscript partial
% the same stroke on every frame: reseed right before it
7 srand newpath 40 120 moveto 300 150 lineto << /Width 14 >> pkdry
% a wash pinned on its own: untouched by srand, takes no rand draw
newpath 40 40 moveto 300 40 lineto 300 100 lineto 40 100 lineto closepath
<< /Seed 3 /Alpha 0.3 >> pkwash
```

**What a stroke costs.** Measured with release `pscat` on a 1280 x 720 page
(1 pixel per point): CPU time of the whole run minus a run with the same
page and no strokes, best of 7, on an Apple M4 that other jobs were also
using, so read a figure as "about" and anything under 5 ms as small. Each
row is one stroke at `/Width 14` (`pkliner` 3, `pkdab` `/Size 12`, `pkspray`
`/Nozzle 14 /Density 80`) with every other key at its default, along a curve
about 100 pt and about 600 pt long:

| Preset | 100 pt stroke | 600 pt stroke |
| --- | --- | --- |
| `pkribbon` | 0.4 ms | 1 ms |
| `pkliner` | 0.7 ms | 2.5 ms |
| `pknib` | 0.7 ms | 7 ms |
| `pkbroad` | 2 ms | 6 ms |
| `pkdab` | 2 ms | 9 ms |
| `pktrowel` | 2 ms | 9 ms |
| `pkspray` | 1.5 ms | 5.5 ms |
| `pkoil` | 4.5 ms | 20 ms |
| `pkdry` | 12 ms | 65 ms |
| `pkfan` | 70 ms | 360 ms |
| `pkwet` around that `pkspray`, `/Soft 0.8` | 5 ms | 22 ms |

A stroke's cost follows its length and its bristle count, not the pixels it
covers: a 700 pt `pkdry` at `/Width 24` takes 76 ms with its 18 bristles and
420 ms at `/Bristles 100`, while the same 600 pt strokes at 144 dpi cost
`pkdry` 66 ms against 64, `pkoil` 23 against 22, `pkbroad` 9 against 6. `pkwet` runs its mark once per pass, about 4 times the plain
stroke here. Areas:

| Call | Cost |
| --- | --- |
| `pkwash` on a 200 x 150 / 640 x 360 / 1280 x 720 shape | 3.5 ms / 9 ms / 20 ms |
| `pkwash` on a full-frame shape under a 1280 x 60 or 200 x 150 clip | 14 to 21 ms: a clip does not make a wash cheaper |
| `pkpaper` over 200 x 150 / 1280 x 720 | 0.7 ms / 18 ms |
| `pkcanvas` set-up | 0.2 ms (the cost is in the `/Touch` strokes: a 600 pt `pkdry` goes from 65 to 120 ms) |
| `pktrowelplane` over 200 x 150 / 1280 x 720 | 50 ms / 1.5 to 1.7 s |
| `pkbrushedge` round a 100 x 60 / 300 x 200 mass | 15 to 25 ms / 65 to 70 ms |
| `pkcrisscross` sky 1280 x 300, then `pkbroad` | 45 to 55 ms |
| `pkscumblein` (60 circles, radius 20) over 400 x 200, then `pkoil` | 280 ms |

Budget the heavy ones first: `pkfan`, `pkdry` (and `/Touch` on it),
`pkscumblein` into `pkoil`, and `pktrowelplane` over a large area. The lever
on those is the number and length of strokes and the `/Bristles`, not how
much of the frame is painted. The harness is in
`notes/1414-paint-seeding-and-cost.md`.

**Painting a landscape?** Read the landscape cookbook first
([`docs/guides/landscape-cookbook.md`](https://github.com/PSCatLabs/pscat/blob/main/docs/guides/landscape-cookbook.md)): one
recipe per element (sky, clouds, mountains, mist, far forest, evergreens,
bushes, water, reflection, foreground bank, grass), each with the tools, the
order, the settings that matter, the usual failure with its fix and a complete
snippet that renders as it stands (360x240 pt: `pscat --page 360x240 --dpi 144
--png out.png recipe.ps`), plus the order a wet-on-wet landscape is painted in.
Copy a recipe and change the numbers rather than re-deriving the technique.

This section is the short orientation; `pscat --capabilities` is the
full inventory of paintkit's presets and parameters.
