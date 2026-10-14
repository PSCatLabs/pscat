---
name: psart
description: Make generative art with the pscat PostScript interpreter — the artkit library (turtle, L-systems, palettes, path brushes), the font catalog, and the render-look-refine loop. Use when asked to create art, posters, generative pieces, or decorative graphics in PostScript.
---

# psart — making art with pscat

You have a full PostScript interpreter (`cargo build --release`;
binary `target/release/pscat`), a curated font catalog, seven Type 3
display faces, and `lib/artkit.ps` — a generative-art toolkit. Art
here is *written*, rendered, looked at, and refined.

## The loop

1. Write the piece as a `.ps` file (start from a gallery piece or the
   sketch below).
2. `pscat --page WxH --png out.png piece.ps` — then **look at the
   PNG**. Really look: composition, color, balance, whether the idea
   reads.
3. Refine and re-render. Small seeds, big differences: `N srand` at
   the top makes every run reproducible (same seed, same picture —
   the house doctrine).
4. When it's done, check it in Ghostscript too:
   `gs -dBATCH -dNOPAUSE -sDEVICE=png16m -g<W>x<H> -o gs.png piece.ps`

This file is a hand-written tour, not the source of truth — it can and
does drift (see CAPABILITIES.md). `pscat --capabilities` prints the
real, structured inventory of every font, Type 3 face, palette,
template, and major procedure this build actually has installed;
prefer it over guessing a name from prose below.

## The toolkit (`(lib/artkit.ps) run` from the repo root)

- **Random**: `n chance` (0..n-1), `j jit` (-j..j; integer j gives an
  integer, real j gives a real, e.g. `1.5 jit`), `frnd` (0..1),
  `arr oneof`. All flow from `srand`.
- **Palettes**: `/dusk /ember /tide /meadow /carnival /parchment
  /nocturne /stone` — `/name palpick` → r g b; `/name pal` → the
  five-color array. `[rgb] [rgb] t mix3` lerps; `[rgb] k shade`
  darkens (k<1) or lifts toward white (k>1).
- **Turtle**: `x y heading thome`, `d fd` (draws), `d hop` (pen up),
  `a tl` / `a tr` (turn; NOT lt/rt — those are comparisons),
  `tpush`/`tpop` (pose stack). Build path, then `stroke` yourself.
- **L-systems**: `(axiom) << (F) 0 get (F[+F]F[-F]F) >> depth lsys`
  → expanded string; `(str) step angle ldraw` drives the turtle
  (F draw, f hop, + left, - right, [ ] push/pop; other letters are
  free no-op symbols).
- **Path brushes** (the superpower): `pitch {x y ang ...} alongpath`
  stamps a proc at even arc-length along the **current path** —
  including text via `charpath`. `(string) pathtext` sets type along
  any path, glyph by glyph, rotated to the tangent. Wrap stamps in
  `gsave newpath ... grestore` so the walked path survives.
  `pitch {x y ang t sp atend ...} walkpath` is the same walk with more
  to build a mark from: normalized progress `t` (0..1 per subpath),
  spacing `sp` since the last stop, and an `atend` bitmask (1 = start,
  2 = end — a subpath always gets one guaranteed call at its literal
  end, even off-pitch). This is what a variable-width/tapered brush
  wants instead of plain `alongpath`.
- **Shapes/text/layout**: `ngon`, `star`, `rrect`; `(s) cx y
  showctr`, `(s) w fitfont`; `x y w h cols rows {x y w h ...} grid`.
- **Scatter** (the area-shaped counterpart to `alongpath`): build a
  region with `x y w h screct` or `scpath` (the current path,
  flattened and closed as `fill` sees it — `clippath scpath` for the
  clip region), then `region << /Mark {x y scale angle ...} ... >>
  scatter`. `/Count` or `/Density` (marks per square unit of the
  region's own area), `/Weight {x y -> 0..1}` for a non-uniform
  distribution (`noise2` composes straight into it), `/Scale [lo hi]`
  and `/Rotate [lo hi]` for per-mark variation, `/MinSpacing` for
  blue-noise separation, `/Seed` for an arrangement that reproduces
  regardless of what drew before it (and doesn't disturb what draws
  after). Candidates outside the region are rejected, not clipped, so
  density means what it says. `scplaced` holds the count afterwards;
  `x y region scin` and `region scarea` are there on their own.
  Foliage, stars, debris, stipple, print specks — reach for this
  before writing another random placement loop.
- **Tiling**: `x0 y0 v1 v2 n1 n2 {x y ...} lattice` (the general point
  walk — any basis, oblique included); `hex`/`tri` shapes; `hexgrid`/
  `trigrid` (the other two regular tessellations, built on `lattice`);
  `truchet` — walks a square grid like `grid` but rotates each cell a
  random quarter-turn first, so one motif reads as a flowing maze
  (`gallery/woven_labyrinth.ps` is the worked example). Calling
  `hex`/`tri` from inside a `hexgrid`/`trigrid` stamp (the obvious
  thing to do) needs the inner call wrapped in its own `N dict begin
  ... end` — they share scratch names, same as `grid`/`ngon` already
  do; artkit.ps's tiling-section header has the details.
- **Fractals**: `len angle turns scale depth edgefractal` (Koch-style
  edge replacement — draws via `rlineto`, any turn-delta generator);
  `[verts] turns scale depth edgepoly` walks it around a closed
  polygon (vertices clockwise, so the bumps point outward — see its
  header). `/koch` and `/quadkoch` are presets in `FractalGens`,
  retrieved with `/name fgen` → `turns scale`. `x1 y1 x2 y2 x3 y3 depth
  {proc} gasket` and `x y w h depth {proc} carpet` are Sierpinski-style
  recursive area subdivision (triangle and square); both drive the
  walk with an explicit stack array rather than recursing, so `proc`
  always runs in your own dict — a plain `/n n 1 add def` counter
  works the same as it does with `truchet`.

## Paint media (`lib/paintkit.ps`, load after artkit)

```postscript
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
  and an unbroken mass.
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
  to flattening against white and overlaps stop mixing.
- **`pkwet`** — not a brush: a *softener* that wraps any of the above.
  It re-runs your mark-drawing proc several times, each pass nudged
  further off the path and mixed further toward `/Under`, the color you
  declare is already on the canvas there — so the mark settles into the
  layer beneath instead of sitting on it. This is the wet-on-wet
  vocabulary: skies, clouds, mist, distant planes.

  ```postscript
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
  across the axis. Two rules: the wrapped proc must paint in the color
  it *inherits* (wrap a scene element by element, each with the
  `/Under` really beneath it), and the direction is in the user space
  you call pkwet in — put a reflection's `1 -1 scale` *inside* the proc
  so 270 is down the page. `examples/paintkit_wet_pull_demo.ps` shows
  all three.

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
  up, `/Bow` for blades); `pkzigzag` → `pkliner` or `pkbroad` (water).
  Two rules: **every motion replaces the current path** (so a guide, or
  the outline `scpath` leaves behind, is never painted — one motion, one
  paint, then build the next); and the region motions take an artkit
  region (`screct` or `scpath`) as the operand under the dict while the
  spine motions read the current path as the guide. They draw only from
  `frnd`, so they are deterministic under your `srand`.
  `examples/paintkit_motion_demo.ps` shows each beside its brush.

(A fuller paintkit tour, and an audit of the parameter docs, is issue
#99 — this section is the short orientation, not that sweep.)

## Dry media: chalk (`lib/drychalk.ps`, after artkit, paintkit and drykit)

For a chalkboard, use `lib/drychalk.ps`, not a pen stroke in a chalk
colour (that is `gallery/hundred_lines.ps`'s flat look). Load
`lib/handscript.ps` and `lib/strokeglyphs.ps` too for handwriting: `sgpath`
gives skeleton paths, and `dcchalk` inks them.

```postscript
(lib/artkit.ps) run (lib/paintkit.ps) run (lib/drykit.ps) run
(lib/handscript.ps) run (lib/strokeglyphs.ps) run (lib/drychalk.ps) run
40 40 520 300 << /Surface /greenboard /Tray 12
    /Ghost { (last lecture) 60 250 << /Size 30 >> sgpath } >> dcboard
/white dcchalkcolor
newpath (fresh notes) 60 180 << /Size 32 >> sgpath << /Width 1.8 >> dcchalk
newpath 58 170 moveto 260 172 lineto << /Tilt 85 /Side 5 >> dcchalk
```

- **`dcboard`** (`x y w h opts`) paints the board and makes it the
  surface. It draws:
  - a dust haze that is never flat;
  - `/Ghost`, a path procedure for the last lecture's writing, drawn
    erased;
  - eraser `/Swipes`, darker `/Wipes`, lighter `/Smudges`;
  - `/Ledge` dust and an optional `/Tray`.

  Call it once per board, before the chalk.
- **`dcchalk`** treats each subpath as one stroke. It lands heavy, tires in
  the middle, lifts thin, sheds dust and differs a little from its
  neighbours. `/Tilt` 80-90 with `/Side` gives broad side-of-chalk
  underlines and shading.
- **`/name dcchalkcolor`** picks the chalk: `/white /yellow /pink /blue
  /green /orange`.
- Real chalk is grey, not white: the defaults (`/Strength 0.55`) were
  measured against photographs. Raising it gives the fake-font look.
- Cost: a 42pt line is about 0.7s at 2x, so keep a board to about a dozen
  lines. `examples/drychalk_specimen.ps` shows everything.

## Dry media: crayon (`lib/drycrayon.ps`)

Chalk, crayon and charcoal sit on `lib/drykit.ps` (design:
`docs/DRYMEDIA.md`): a stick catches only on the surface's tooth, so
light pressure breaks into streaks along the stroke. Crayon is a medium
file over it. Load artkit, paintkit, drykit, then drycrayon:

```postscript
(lib/artkit.ps) run (lib/paintkit.ps) run (lib/drykit.ps) run (lib/drycrayon.ps) run
<< /Seed 4 >> dypaper                       % crayon paper; paints nothing
0.97 0.96 0.93 setrgbcolor clippath fill    % so paint the paper yourself
/blue dycolor                               % the 16-crayon box (dybox, dycolors)
newpath 40 40 moveto 200 80 lineto << /Touch 0.7 >> dycrayon
```

- **`dycrayon`** inks the current path in the current colour: a dense core
  in a ragged rim, a child's pressure (a press on landing, a fade, an
  uneven wander), and wax build-up where the stroke turns. `/Touch` is
  drykit's pressure: 0.6 leaves about a third of the paper showing in a
  scribble fill, 0.9 is nearly solid. A later pass over the same area is
  `/Pass 2`, `3`, ...; a colour over another takes `/Over [r g b of the one
  beneath]`, which lets it show through. `/Worn` 0 is a new point, 1 a
  worn flat (wider). drykit can't read the page back, so passes and
  layering are declared, not detected.
- **Child motions** build paths and paint nothing, with paintkit's motion
  contract (each replaces the current path and draws only from `frnd`):
  `dywobble` (shaky, overshooting lines), `region opts dyscribble`
  (back-and-forth fill past the edges, with gaps), `cx cy r opts dyloop`
  (a circle that doesn't close), `x y h opts dyfigure` (a stick figure, or
  `/Tadpole true`, with `/Hair`), `cx cy r opts dyrays` (sun rays),
  `dygrass` (blades along a line, `/Zigzag true` for one continuous line).
  Each feeds `dycrayon`: `40 300 200 60 screct << >> dyscribble << >> dycrayon`.
- Cost: every mark is a clip of tooth knuckles, so a big scribble fill
  costs about a second at 2x. Publish PNG, not SVG (SVG carries every
  knuckle). `examples/drycrayon_specimen.ps` and
  `examples/drycrayon_motion_demo.ps` show everything.

## Charcoal (`lib/drycharcoal.ps`, on drykit)

```
(lib/artkit.ps) run (lib/paintkit.ps) run (lib/drykit.ps) run
(lib/hatchkit.ps) run (lib/drycharcoal.ps) run
<< /Surface /drawing /Relief 0.35 >> dksurface   % less mottle suits charcoal
0.11 0.10 0.10 setrgbcolor                        % the pigment is the current colour
```

- `opts dhstroke` — the current path in `/Medium /vine` (grey, dusty),
  `/compressed` (dark, dense) or `/pencil` (sharp; hard grades break into
  dots). `/Grade` 0 (hard) to 1 (soft). Any other `dkstroke` key overrides
  the preset.
- `opts dhhatch` — hatches the current path's region to `/Value` (0..1, or
  `{x y -> value}`): up to `/Layers` crossing layers of hand strokes, all in
  one `dkbatch` so crossings stay dark. It clips to the region and cannot run
  inside a `dkbatch`. Hatch a region's layers in **one** call: two calls are
  two batches, and at `/Levels 2` or more the second one's faint level greys
  the first's crossings.
- `opts dhsmudge` — the path is a stump's track: a wide, soft stroke that
  fills the tooth, leaving `/Value` between paper and the current colour.
  drykit cannot read the page, so give `/Under` (the tone beneath) when it
  is not paper. Over hatching, a moderate `/Touch` lets the hatch show.
- `opts dhlift` — an eraser's track: `/Eraser /kneaded` (grey, soft, never
  back to paper) or `/hard` (a narrow, crisp line). `/Under` defaults to the
  current colour.
- `opts dhsketch` — faint construction lines, laid `/Passes` times, each
  shifted by up to `/Shift`; draw the drawing over them.
- None of them draws from your `rand` stream. **Cost:** hatching costs about
  0.07 ms per square point per layer in release pscat at 2x (a 120 x 120 pt
  square is 1.3 s at one layer, 4 s at four), so keep hatched areas modest.
  In Ghostscript, any `scatter` (vine charcoal's fallout) makes the drykit
  work after it about 2x slower (#311), so draw vine strokes late.
  `examples/drycharcoal_specimen.ps` shows all of it.

## Style packs (`lib/styles/`, load after artkit)

Four motif libraries, one per aesthetic — each registers three
palettes into artkit's `Palettes` and adds path builders (you paint
them) and painted stamps (self-contained). Every file's header lists
its full API; `examples/style_<name>.ps` is a worked specimen poster
for each.

- **steampunk** — `gear` (path, bored hub), `rivet`, `pipe`, `gauge`,
  `plateframe`; palettes /brass /verdigris /boiler; `/spmetal` dial
  picks the stamps' metal. Pair with Rye + SpecialElite.
- **psychedelic** — `rays`, `blob` (wobble circle — nest and cycle
  colors), `spiral`, `wavy`, `kaleido` (n-fold repeat about a
  center), `t rainbow`; /acid /blacklight /sherbet. Monoton on an
  `arcn` path via `pathtext` is the move.
- **scifi** — `glowstroke` (the neon seller: halo/mid/core),
  `starfield`, `planet` + `planetring` (`/sfworld` dial),
  `hudcorners`, `reticle`, `hexfield`, `gridfloor` (synthwave
  perspective floor — glowstroke it); /void /hologram /synthwave.
  Orbitron, Audiowide, VT323, PressStart2P.
- **toon** — the cel-cartoon look: `celfill` (flat fill + fat ink
  outline, the foundation), `burst`, `bubble` (speech, tail toward a
  point), `speedlines`, `dotfill` (halftone in the current path, path
  survives), `eye`, `dripbox` (slime title slab); /saturday
  /latenight /pastelpop; `/tnink` dial sets the line color. Bangers,
  ComicNeue, PermanentMarker.

Scratch prefixes sp-/py-/sf-/tn- join artkit's reserved list — don't
reuse them in your own procs.

## Type is material

- `pscat --fonts` lists everything. The standard 35 all resolve
  (Palatino, Bookman, ZapfChancery, Symbol, ZapfDingbats…) plus ~35
  display families: `/GreatVibes-Regular`, `/Bangers`,
  `/UnifrakturMaguntia`, `/PressStart2P-Regular`, `/Creepster-Regular`…
- The Type 3 program-faces (`lib/fonts/*.ps`, load with `(file) run`)
  — /Neon, /Marquee, /Constellation, /Lapidary, /Circuitry,
  /Stitchwork, /Confetti — take dials through their scratch dicts
  (each file's header documents them).
- Text as geometry: `(word) true charpath` then fill/stroke/clip
  it — or walk it with `alongpath`.

## Composition habits that keep pieces good

- Pick a palette and stay in it; one accent color earns more than six.
- Ground first (`clippath fill`), then depth back-to-front; overdraw
  (glow, halo, shadow) is layered strokes, no alpha needed.
- Odd counts, asymmetry, and a little `jit` beat perfect grids.
- Big shapes read; a thousand tiny marks are texture, not subject.
- Deterministic art is debuggable art: seed everything, and when a
  region misbehaves, render just that region at `--page` scale.

## A sketch to start from

```postscript
%!PS-Adobe-3.0
(lib/artkit.ps) run
11 srand
/dusk pal 0 get aload pop setrgbcolor clippath fill   % ground
newpath 306 200 90 thome
(F) << (F) 0 get (FF-[-F+F+F]+[+F-F-F]) >> 3 lsys 6 22.5 ldraw
/dusk pal 4 get aload pop setrgbcolor 1.2 setlinewidth stroke
/Palatino-Italic findfont 18 scalefont setfont
/dusk pal 3 get aload pop setrgbcolor
(grown, not drawn) 306 60 showctr
showpage
```

`gallery/hortus.ps` is the worked example (herbarium plate:
L-systems + alongpath blossoms + letterpress); `gallery/README.md`
is the brief for gallery-quality pieces, and `examples/` has the
font showcases.
