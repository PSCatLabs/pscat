---
name: psart
description: Make generative art with the pscat PostScript interpreter — the artkit library (turtle, L-systems, palettes, path brushes, scatter, tiling, fractals), paint and dry media (watercolor, oil, chalk, crayon, charcoal), chalkboard TeX formulas, style packs and type, and the render-look-refine loop. Use when asked to create art, posters, generative pieces, paintings, chalkboards, or decorative graphics in PostScript.
---

# psart — making art with pscat

You have a full PostScript interpreter (`pscat` on your PATH), its
library (`lib/*.ps`, compiled into the binary, so `(lib/artkit.ps) run`
works from any directory), seven Type 3 display faces, and a font
catalog. Art here is *written*, rendered, looked at, and refined. The
`pscat` skill has the command-line reference; this one is the workflow
and the toolkit.

## The loop

1. Write the piece as a `.ps` file (start from the sketch at the end).
2. `pscat --page WxH --png out.png piece.ps` — then **look at the
   PNG**. Really look: composition, color, balance, whether the idea
   reads. (Over MCP: `render_postscript` returns the PNG inline.)
3. Refine and re-render. Small seeds, big differences: `N srand` at
   the top makes every run reproducible (same seed, same picture).
   `pscat piece.ps --sweep-seed 1:12 --contact-sheet grid.png` compares
   twelve seeds in one run.
4. Optional, if Ghostscript is installed: `gs` is the behavioural
   oracle, but it cannot find `lib/` on its own (pscat's copy is inside
   the binary), so this check needs a pscat checkout:
   `gs -dNOSAFER -I lib -dBATCH -dNOPAUSE -sDEVICE=png16m -g<W>x<H> -o gs.png piece.ps`.

This file is a hand-written tour and can drift. `pscat --capabilities`
(MCP: `describe_art_capabilities`) prints the real, structured inventory
of every font, Type 3 face, palette, template and major procedure this
build has, with each paintkit parameter's unit; prefer it over guessing a
name from prose.

## The toolkit (`(lib/artkit.ps) run`)

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
  `pitch {x y ang t sp atend ...} walkpath` adds normalized progress `t`
  (0..1 per subpath), spacing `sp` and an `atend` bitmask (1 = start,
  2 = end; the literal end always gets a call): what a tapered brush wants.
- **Shapes/text/layout**: `ngon`, `star`, `rrect`; `(s) cx y
  showctr`, `(s) w fitfont`; `x y w h cols rows {x y w h ...} grid`.
- **Scatter** (the area counterpart to `alongpath`): a region from
  `x y w h screct` or `scpath` (the current path; `clippath scpath` for
  the clip), then `region << /Mark {x y scale angle ...} ... >> scatter`.
  `/Count` or `/Density` (marks per square unit), `/Weight {x y -> 0..1}`
  (`noise2` composes into it), `/Scale [lo hi]`, `/Rotate [lo hi]`,
  `/MinSpacing` for blue noise, `/Seed` for a self-contained arrangement.
  Candidates outside are rejected, not clipped. Foliage, stars, debris,
  stipple, print specks: reach for this before a random placement loop.
- **Tiling**: `x0 y0 v1 v2 n1 n2 {x y ...} lattice` (any basis);
  `hex`/`tri` shapes; `hexgrid`/`trigrid`; `truchet` (a grid with each
  cell turned a random quarter, one motif becomes a maze). A `hex`/`tri`
  call inside a `hexgrid`/`trigrid` stamp needs its own `N dict begin ...
  end` (shared scratch names; see artkit.ps's tiling header).
- **Fractals**: `len angle turns scale depth edgefractal` (Koch-style,
  via `rlineto`); `[verts] turns scale depth edgepoly` around a closed
  polygon (clockwise vertices: bumps point out); presets `/koch`,
  `/quadkoch` via `/name fgen`. `x1 y1 x2 y2 x3 y3 depth {proc} gasket` and
  `x y w h depth {proc} carpet` subdivide areas; `proc` runs in your dict.

## Media and kits: read the reference you need

Each reference is a self-contained section; open only the one the piece
needs.

| You want | Library | Read |
|---|---|---|
| Brushes, oil, knife, fan, watercolor, wet-on-wet, loaded brushes, canvas weave, landscapes | `lib/paintkit.ps` | [references/paint.md](references/paint.md) |
| Chalkboards, crayon drawings, charcoal and hatching | `lib/drykit.ps`, `drychalk`, `drycrayon`, `drycharcoal` | [references/dry-media.md](references/dry-media.md) |
| Formulas on a board (TeX subset, aligned derivations, braces) | `lib/mathkit.ps` | [references/board-math.md](references/board-math.md) |
| Steampunk, psychedelic, sci-fi or cartoon motifs; Type 3 faces; the typewriter; a "released document" | `lib/styles/`, `lib/fonts/`, `lib/typewriter.ps`, `lib/released.ps` | [references/styles-and-type.md](references/styles-and-type.md) |

Also in the library, documented in each file's header and in
`--capabilities`: `surfacekit` (paper, canvas, print surfaces),
`etching`/`stipplekit`/`hatchkit`/`halftonekit` (print techniques),
`printkit` (photocopy and print defects), `fillkit`, `headline`/
`lettering`, `apparel` (garment mockups), `pagekit` (page templates).

## Type is material

- `pscat --fonts` lists what this install can find. The LaserWriter 35
  always resolve (Palatino, Bookman, ZapfChancery, Symbol,
  ZapfDingbats…). The ~35 display families (`/Bangers`,
  `/GreatVibes-Regular`, `/PressStart2P-Regular`, …) come from the font
  catalog, which a bare install may not have: **an unknown name silently
  becomes Helvetica**, so check `--fonts` before designing around a face.
- Type 3 program faces load from the library anywhere:
  `(lib/fonts/neon.ps) run` — /Neon, /Marquee, /Constellation,
  /Lapidary, /Circuitry, /Stitchwork, /Confetti.
- Text as geometry: `(word) true charpath` then fill/stroke/clip it, or
  walk it with `alongpath`.

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

Worked examples live in the repository's
[gallery](https://github.com/PSCatLabs/pscat/tree/main/gallery)
(`hortus.ps`: L-systems, alongpath blossoms and letterpress) and
[examples](https://github.com/PSCatLabs/pscat/tree/main/examples).
Adding a piece to the gallery is a contributor task: see `gallery/README.md`
in a checkout.
