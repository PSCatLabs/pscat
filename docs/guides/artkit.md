# artkit: the generative-art toolkit

`lib/artkit.ps` is the toolkit most pscat art is built on: seeded random
numbers, mood palettes, turtle graphics, L-systems, shapes, layout and text
helpers, and a set of "brushes" that walk along a path. Everything is
deterministic under `srand`, and everything runs unchanged in Ghostscript.

## A first example

Load the library with `run`, then call its words like any other operators.
This draws a turtle-grown plant and sets a line of text along a curve:

```postscript
(lib/artkit.ps) run
newpath 100 400 90 thome                        % a turtle...
(F) << (F) 0 get (F[+F]F[-F]F) >> 4 lsys        % ...an L-system...
4.4 22 ldraw stroke                             % ...a plant.
newpath 60 100 moveto 500 200 550 300 300 380 curveto
(text can walk along any path now) pathtext     % type on a curve
```

## What is in it

- **Random and colour.** Seeded random helpers, and eight mood palettes with
  colour mixing.
- **Turtle graphics and L-systems.** A turtle with a pose stack, and an
  L-system engine that grows strings of drawing commands.
- **Shapes, layout and text.** Shape primitives, layout helpers, and text
  helpers.
- **Path brushes.** These are built on `pathforall`.
  - `alongpath` stamps anything at even arc-length along any path, including
    `charpath` text.
  - `pathtext` sets type along a curve, each glyph rotated to the tangent.
    `ctext` and `ctextctr` build on it for the circular case (seals, coins,
    "ring of type" compositions); `ctextctr` also centres the string on the
    circle.
  - `walkpath` is a richer centreline sampler. Alongside the stamp's position
    and tangent it reports normalized progress along the current subpath, the
    arc-length spacing since the last stop, and start and end flags. It
    always makes a call at each subpath's literal endpoint, which
    `alongpath`'s plain pitch-stepping can't promise. Painterly brushes such
    as variable-width ribbons and tapered ends build on it (see the
    [painting guide](painting.md)).
- **Tiling.** A general `lattice` walk, `hex` and `tri` shape primitives,
  `hexgrid` and `trigrid` drivers for the two other regular tessellations,
  and `truchet`: a single motif at a random quarter-turn per cell, the
  classic trick that turns a regular grid into a flowing, maze-like pattern
  (see the gallery's *Woven Labyrinth*).
- **Hyperbolic geometry.** A model of the Poincaré disk. `hpoint` and `hpolar`
  place points by hyperbolic radius and angle. `horthocircle`, `hgeo` and
  `hpoly` build individual geodesics and closed geodesic polygons (circular
  arcs orthogonal to the unit disk, or diameters through the origin).
  `hreflect` reflects across one (circle inversion), and `httile` runs the
  reflections breadth-first to generate a regular `{p,q}` tessellation of the
  whole disk from one fundamental polygon (see *Infinite Descent*, a `{7,3}`
  tiling).
- **Fractals.** `edgefractal` and `edgepoly` are a Koch-style edge-replacement
  curve generalized to any turn-delta generator (presets `/koch` and
  `/quadkoch` in `FractalGens`, retrieved with `fgen`), walked around any
  closed polygon. `gasket` and `carpet` are Sierpinski-style recursive area
  subdivision for a triangle and a square (see *Recursive Peaks*, gasket-faceted
  mountains under a carpet-driven starfield).
- **Noise and flow fields.** `noise2` is 2D gradient (Perlin) noise off a
  `srand`-shuffled permutation table. `curl2` turns any scalar field into a
  unit-vector flow by taking its normalized perpendicular gradient. `advect`
  traces a particle through a vector field as a sequence of `lineto`s (see
  *Lodestone*, iron filings curling around a magnetic stone).
- **Scatter.** The area-shaped counterpart to `alongpath`. `screct` and
  `scpath` build a region from a rectangle or from the current path (flattened
  and closed the way `fill` sees it; `clippath scpath` captures the clip
  region). `scin` and `scarea` answer containment and area. `scatter` places a
  caller-supplied mark across the region by fixed count or by density, with a
  `/Weight` procedure for non-uniform distributions, seeded scale and rotation
  variation, exact `/MinSpacing` enforcement, and a hard deposit budget (see
  *Firefly Census*, a night meadow in which every mark on the page is
  scattered rather than placed by hand).
- **Gradients.** `gradfn`, `axialsh`, `radialsh` and `gradfill` build shading
  dictionaries from a plain array of `[r g b]` colours; see
  `examples/gradients.ps`.

The gallery pieces named above live in `gallery/`; the
[gallery](https://jeromebanks.github.io/postscript_interpreter/gallery.html)
shows them all.

## Where to go from here

`gallery/hortus.ps` is the worked example: three L-system plants grown by
turtle, with blossoms stamped along each plant's own path.

The `psart` skill ([`.claude/skills/psart/SKILL.md`](../../.claude/skills/psart/SKILL.md))
teaches the whole workflow to any agent: the render-look-refine loop, the
toolkit, the style packs, type as material, and the composition habits that
keep pieces good.

Next:

- [Style packs and page templates](style-packs-and-templates.md): four
  ready-made aesthetics and five document templates built on artkit.
- [Painting](painting.md): variable-width brushes, wash and watercolor.
- [Charts and plots](charts.md): mathematical plotting and chart drawing.
