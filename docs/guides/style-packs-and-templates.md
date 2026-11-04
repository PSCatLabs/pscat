# Style packs and page templates

Two libraries sit on top of [artkit](artkit.md) and give you finished-looking
results with little code: **style packs**, which add a whole aesthetic, and
**page templates**, which lay out a document for you. Both need artkit loaded
first.

## Style packs

Four **style packs** (`lib/styles/`) sit on top of [artkit](artkit.md), one
per aesthetic, each adding three palettes and a drawer of motifs:
`steampunk.ps` (gears, rivets, pipework, pressure gauges, riveted
plate frames), `psychedelic.ps` (sunburst rays, wobble-ring blobs,
spirals, kaleidoscope repeats, hue-wheel color), `scifi.ps`
(starfields, ringed planets, HUD chrome, hex shields, the synthwave
grid floor, and `glowstroke` to sell it all), and `toon.ps` (the
cel-cartoon look: `celfill` flats under fat ink, speech bubbles,
action bursts, speed lines, halftone `dotfill`, dripping title
slabs). Load artkit, then the pack; `examples/style_*.ps` are the
four specimen posters.

<p><img src="../../lib/styles/specimen_steampunk.png" width="23%" alt="Aether &amp; Brass — the steampunk specimen"/>
<img src="../../lib/styles/specimen_psychedelic.png" width="23%" alt="Turn On The Sun — the psychedelic specimen"/>
<img src="../../lib/styles/specimen_scifi.png" width="23%" alt="Outer Reaches — the sci-fi specimen"/>
<img src="../../lib/styles/specimen_toon.png" width="23%" alt="Splat! — the toon specimen"/></p>

## Page templates

`lib/pagekit.ps` depends on `artkit.ps`, as the style packs do, leaning on `tfblock` for
paragraph flow, `showctr`/`rrect` for layout, and `Palettes`/`pal` for
color. Five parameterized document templates --
`pgcard`/`pgletter`/`pgcertificate`/`pginvitation`/`pgposter` -- each
take `x y w h dict pgNAME` (the usual bottom-left-plus-size
convention) and a content dict of optional keys (title, body copy,
signature, and a few template-specific fields), so an agent fills in a
message without making layout decisions itself; every template
returns whatever body text didn't fit, the same leftover contract
`tfblock` already establishes. Two palettes, `/vellum` and
`/marigold`, join artkit's eight, registered the same way a style
pack registers its own.
`examples/template_*.ps` is one specimen per template.
`pgcertificate` anchors its title and signature block to the inner rule
and shares the space between them equally among the gaps around the
awardee and the citation, so a short citation does not leave an empty
band; a long title or awardee shrinks to fit and its slot shrinks with it.

```postscript
(lib/artkit.ps) run
(lib/pagekit.ps) run
40 40 532 500
<< /Awardee (Ada Lovelace) /Body (For contributions to computing.)
   /Presenter (C. Babbage) /Date (1843-10-12) >>
pgcertificate pop showpage
```
