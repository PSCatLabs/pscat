# Printmaking and etching

These libraries make marks the way printmakers do: engraved lines, hatching,
stipple, halftone screens, woodcut and linocut cuts, and the paper texture
under the ink. All of them draw with ordinary `moveto`, `lineto` and `stroke`,
so they render to PNG, SVG and PDF alike. Most take a region (the current clip,
or a region built with artkit's `screct` or `scpath`; see the
[artkit guide](artkit.md)) and an options dictionary.

## Etching: a photograph as an engraving

`lib/etching.ps` turns a photograph into
a line engraving — the reproduction technique old newspapers and books
actually used before halftone screens, not edge detection. `et-dims`
reads a JPEG's own SOF marker (segment-skipping, not byte-scanning) to
get its width/height/component count without decoding it; `et-draw`
opens the same file through `/DCTDecode` — a general filter in this
interpreter, so a PS program can `readstring` decoded samples directly
without going through the `image` operator — and hatches it: parallel
lines whose stroke width tracks local darkness, plus a perpendicular
crosshatch pass in the deep shadows. Both honor the file's own EXIF
Orientation tag, so a portrait phone photo sizes and
renders right-side up rather than sideways. Draws with ordinary
`moveto`/`lineto`/`stroke`, so it renders to `--png`/`--svg`/`--pdf`
alike. `pscat photo-etch` wraps it end to end (photo path in, PNG/
SVG/PDF out — no PostScript required); `examples/etching_demo.ps` is a
specimen sheet.
`et-tonemap` returns the photo's tones as data instead: a
grid of box-averaged darkness, row 0 at the top. The charcoal portrait
`gallery/lincoln_charcoal.ps` hatches to such a grid.

```postscript
(lib/etching.ps) run
<< /Photo (photo.jpg) /PageWidth 320 /PageHeight 240 >> et-draw showpage
```

## Hatching

`lib/hatchkit.ps` is a reusable hatching
and cross-hatching library: `hatch` fills whatever region is currently
clipped with a family of parallel line strokes, so the caller supplies
the region and any tone-driving callback rather than this library
doing image analysis or tone extraction (`lib/etching.ps` already
covers that end-to-end). It never reimplements point-in-polygon or
edge-crossing tests -- lines are drawn well past the region's
boundary and left for the graphics state's own `clip` to cut down, so
`hatch` clips to concave and self-intersecting paths exactly as well
as convex ones. Single or multi-angle (`/Angles`, layered per call --
two calls at 90 degrees apart is the classic cross-hatch), seeded
width/wobble/dropout/length variation, and an optional per-sample
`/Density` callback that drives width buckets and a no-ink cutoff --
clamped into `[0,1]` regardless of what the callback returns, so a
misbehaving callback can only change tone, never spawn unbounded
geometry. `/MaxLines` and `/MaxSamples` reject a pathological
`/Spacing` before anything is drawn, the same up-front-reject shape
`scatter`'s `/Budget` uses. `examples/hatching.ps` is a three-panel
specimen sheet: flat shading, a `/Density`-driven tonal band that
reads as a curved, lit sphere despite every stroke being straight, and
layered cross-hatching built from two and three `hatch` calls over the
same region.

```postscript
(lib/hatchkit.ps) run
newpath 40 40 moveto 260 40 lineto 260 260 lineto 40 260 lineto closepath clip
<< /Angle 30 /Spacing 4 /Seed 1 >> hatch
```

## Stippling

`lib/stipplekit.ps` is a thin layer over artkit's `scatter`: one operator,
`stipple`, places dots (or any mark you supply) across a region by density,
constant or driven by a callback for tonal stippling.

## Halftone screens

`lib/halftonekit.ps` covers print
screens: `halftone` fills whatever region is currently clipped with a
regular dot, line, or cross-line lattice, one mark per cell sized by a
tone (a number, or a `{x y -> w}` callback clamped into `[0,1]`), with
a per-layer `/Offset` for controlled misregistration -- one call per
ink, each with its own color, angle, and shift, is a two-plate
risograph spread. Frequency is cells-per-inch, dots scale by
square-root (area-proportional, the print-correct curve), and a single
deterministic `/MaxCells` budget rejects pathological lattices before
anything is drawn. No random draws anywhere, so fixed options
reproduce identically with no seed to manage. `examples/halftone.ps`
is a four-panel specimen: a dot ramp, a line screen, a cross screen,
and a misregistered second plate.

```postscript
(lib/halftonekit.ps) run
newpath 40 40 moveto 200 40 lineto 200 200 lineto 40 200 lineto closepath clip
<< /Screen /dot /Frequency 9 /Tone 0.5 >> halftone
```

## Paper and surface textures

`lib/surfacekit.ps` is a library of
deterministic, seeded surface textures meant to sit under other marks
the way a real surface sits under ink: `grain` (paper grain), `fiber`
(paper fibers), and `scuff` (scratches and scuffs) are thin wrappers
over `scatter`, the same "no new placement engine" choice
`lib/stipplekit.ps` made; `misreg` (print/registration imperfections)
is too, drawing nested lightening rings instead of a single mark;
`weave` (canvas weave) is its own grid-and-`scin` loop instead, since a
basket weave is a regular structure, not a random one, with its own
two independent pre-flight budgets mirroring `hatch`'s own
`/MaxLines`/`/MaxSamples` shape. New `/Color`/`/Strength` options lerp
each mark from paper-white toward a given color rather than toward
transparent (gs has no PostScript-callable alpha operator). A caller
needing an exact edge on a non-rectangular region clips the ambient
graphics state first, the same idiom `hatch` itself relies on.
`examples/surfacekit.ps` is a six-panel specimen sheet: one per preset,
plus a sixth demonstrating that exact-edge clip.

```postscript
(lib/artkit.ps) run
(lib/surfacekit.ps) run
0 0 200 200 screct << /Density 0.4 /Scale [0.4 1.4] /Seed 3 >> grain
```

## Woodcut, linocut and engraving presets

`lib/printkit.ps` composes `hatch`,
`scatter`, and (optionally) `grain` into three printmaking presets --
`woodcut`, `linocut`, `engraving` -- over a shared options dict
(`/Scale`, `/Density`, `/Roughness`, `/Seed`, `/Color`, `/Budget`,
`/Paper`, `/Angle`). Unlike the libraries above, which take a `region opts NAME`, each
preset takes the *current path* directly and manages its own `clip`
internally -- no pre-clip, no `screct`/`scpath` region object, needed
from the caller -- since rebuilding an exact `clip`-able path back out
of a stored region's flattened edge soup would be real machinery for
no benefit over the path the caller already has. `woodcut` is one
directional hatch pass (the grain) at high wobble/dropout/trim plus a
scatter of small jittered chip marks; `linocut` is one bold, low-wobble
pass plus a sparse scatter of a few large, more deliberate marks;
`engraving` is a single `hatch` call sweeping three angles 60 degrees
apart at fine spacing and minimal wobble -- no chip marks. `/Paper
true` layers a subtle `surfacekit` `grain` pass under the ink, inside
the same clip. `examples/printkit.ps` is a four-panel specimen sheet
(one per preset, plus a `/Paper true` panel); the gallery piece
*Nightfall, Three Cuts* uses all three in one scene, each silhouette
inked solid first and then cut a second time with a lighter `/Color`
for the moonlit-highlight look real relief printing gets from gouging
ink away.

```postscript
(lib/artkit.ps) run
(lib/hatchkit.ps) run
(lib/surfacekit.ps) run
(lib/printkit.ps) run
newpath 40 40 moveto 260 40 lineto 260 260 lineto 40 260 lineto closepath
<< /Seed 5 >> woodcut
```

## See also

The gallery's *Out of Register* is a three-plate risograph sunset built entirely
from `halftonekit`, and *Nightfall, Three Cuts* puts the three printkit presets
in one scene. The [gallery](https://jeromebanks.github.io/postscript_interpreter/gallery.html)
shows both.
