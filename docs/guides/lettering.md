# Lettering, headlines and apparel

Display type is the point of these libraries: big, characterful letters filled
with colour and texture, laid out as a headline, and printed on shirts and
jerseys. They all build on [artkit](artkit.md) and on the installed outline
fonts (see the [fonts guide](fonts.md)); load artkit first.

## Display lettering: lettering.ps

`lib/lettering.ps` turns an
installed outline face into large "groovy" lettering: `psyletter`
takes `(text) x y << options >>` and draws the glyph outlines
(`charpath`) filled with a multicolor interior clipped to the letters --
`/Treatment` `/solid`, `/transition` (a `shfill` gradient), `/patches`
(seeded irregular color pools), or `/mottled` (patches plus worn
flecks) -- under a dark contour. `/Font` (default `/PermanentMarker`),
`/Size` or `/Width`, `/Align`, `/Palette` (an artkit palette name or an
array of `[r g b]`), `/Seed`, `/Wear`, `/Outline`, `/OutlineWidth` are
the controls. A seed changes only the texture, never metrics or
placement; a missing face is an error (`lettering-font-not-found`), not
a silent Helvetica. `psyletterpath` exposes just the geometry.
`examples/lettering.ps` is the specimen. With `lib/fillkit.ps` loaded,
`/Treatment` also takes `/plasma`, `/clouds`, `/diffclouds` and `/conical`
(see `fillkit` below); `/Fill << ... >>` forwards any fillkit option.

```postscript
(lib/artkit.ps) run
(lib/lettering.ps) run
(Real) 306 300 << /Size 220 /Treatment /mottled /Seed 7 >> psyletter
```

## GIMP-style fills: fillkit.ps

`lib/fillkit.ps` reproduces GIMP's fill/render algorithms as
seeded fills over a box, meant to sit inside a clip: `plasmafill`
(diamond-square plasma; random RGB like GIMP, or blended through a
`/Palette`; `/Turbulence`), `solidnoisefill` (Perlin fBm, `/Turbulent`),
`diffcloudsfill` (|noise - noise|), and `gradshapefill` (`/bilinear`
`/square` `/conical` `/spiral`), all with `/RGBNoise`, `/HSVNoise` and
`/Spread` grain. Each is one small `colorimage` under the caller's clip, so
the shape stays vector in SVG/PDF; `/Resolution /device` samples once per device pixel (capped at 4096 per side; a
numeric `/Resolution` stays capped at 512).
`examples/fillkit.ps` is the specimen.

## Shirts: apparel.ps

`lib/apparel.ps` builds apparel graphics on top of those two: one
config dict describes a front, back and optional left/right sleeve print area
(physical inches, DPI, margin, phrase, face, treatment, seed), and exports each
piece at its physical size. The finished real-world config is
`examples/keepin_it_real_shirt.ps` (design record: `docs/archive/keepin_it_real_shirt.md`,
exports in `art/keepin-it-real/`). `scripts/apparel_export.sh examples/apparel_shirt.ps out/` writes each piece
as PNG/SVG/PDF plus a proof sheet and `manifest.json`. Artwork is exported
with `--transparent` (PNG alpha, SVG without a white backdrop; PDF never had
one). `pscat --page 1400x760 examples/apparel_shirt.ps` is the proof sheet;
`examples/apparel_second.ps` shows a different design from configuration
alone.

## Headline layout: headline.ps

`lib/headline.ps` lays out
one dominant run with smaller supporting runs tucked around it, from
*measured ink bounds* (not advance widths -- ascenders, descenders and a
rising capital are accounted for). `hllayout` takes a region and an
array of run dicts and returns a layout; `hldraw` draws it;
`hlrunink`, `hlcharink` and `hlink` read the measured boxes back. A run
is sized by `/Size`, `/FitWidth`, `/FitHeight`, `/FitBox`,
`/FitRegion` or `/SizeOf` (a ratio of another run), and positioned by
`/At` -- a point, or a point in an *earlier run's* ink box or one glyph
range of it -- plus `/Anchor` and `/Offset`. `/Text` may contain `\n`
line breaks (`/Justify`, `/Leading`). A run that would leave its region,
sit too close to what it `/Avoid`s (`/Clearance`), or fit smaller than
`/MinSize` raises a named error (`headline-run-outside-region`,
`headline-runs-collide`, `headline-fit-below-minimum-size`) instead of
overflowing silently. `/Draw` takes a per-line procedure, so
`psyletter` can render the same layout. `examples/headline.ps` is the
specimen.

```postscript
(lib/artkit.ps) run
(lib/headline.ps) run
[ 40 40 860 400 ] [
  << /Name /real /Text (Real) /Font /AlfaSlabOne /FitBox [ 780 300 ] /At [ 60 60 ] >>
  << /Name /keep /Text (Keepin' it) /Font /Helvetica-Bold /SizeOf [ /real 0.17 ]
     /At << /To /real /Chars [ 1 3 ] /H 0 /V 1 >> /Anchor [ /left /bottom ]
     /Offset [ 8 18 ] /Avoid [ [ /real 0 1 ] [ /real 1 3 ] [ /real 3 4 ] ]
     /Clearance 10 >>
] hllayout hldraw
```

## Jersey lettering: jersey.ps

`lib/jersey.ps` lays
out the classic athletic-jersey graphic: a team-like name on an arch above
a large player number, with an optional caption, fitted into a print
rectangle. `jerseylayout` takes an options dict (`/Name`, `/Number` as
text so `00` survives, `/Rect`, `/Margin`, fonts, `/Radius`, colours or a
`/Preset /light`/`/dark`) and returns a layout of measured *ink* boxes;
`jerseydraw` paints it (outline first, then fill) and `jersey` does both.
The arch is placed by glyph midpoint with the ink centred on the axis
(`jerseyarch` is the reusable path builder); the name is sized down until
its end letters stay within `/MaxTilt` and the number stays dominant, and
impossible geometry raises a named error (`jersey-name-too-wide`,
`jersey-name-too-tilted`, `jersey-does-not-fit`, ...) instead of clipping.
`examples/jersey.ps` is the specimen. Team-jersey lettering is also
available: `/Outline2` adds a second outer outline band (fill, thin accent,
dark outer), `/Shadow` a solid block shadow, `/Tracking` wide-tracked
capitals and `/NameWidth` a name narrower than the number;
`gallery/varsity_backs.ps` shows them on two jersey backs.

```postscript
(lib/artkit.ps) run
(lib/jersey.ps) run
<< /Name (Cleveland Steamers) /Number (69)
   /NameFont /AlfaSlabOne /NumberFont /AlfaSlabOne
   /Rect [ 36 36 576 684 ] /Margin 12 /Preset /light >> jersey
```
