# Charts and plots

Two small libraries draw data, and neither needs [artkit](artkit.md) or the
other: `lib/graph.ps` for mathematical plots and `lib/dataviz.ps` for charts.
Both render to PNG, SVG and PDF like any other PostScript.

## Function plots and surfaces: graph.ps

`lib/graph.ps` covers mathematical plotting
instead of freehand generative art: `setframe` maps a data-space
domain onto a device-space viewport, `plotfn`/`plotparam`/`plotpolar`
sample a function/parametric curve/polar curve into the current path,
and `axes` draws a bordered, ticked frame around them. A 3D side adds
`setview` (an azimuth/elevation camera), `project3`, and `plotsurface`
— a `z=f(x,y)` wireframe mesh, row by row and column by column, with
no hidden-surface removal (a deliberate scope cut; the gallery's *Ripple Range* shows a cheap
per-row-occlusion trick that works for a single swept height field without
needing a general solver). `examples/graphing.ps` is a four-quadrant
specimen sheet exercising all four drivers.

```postscript
(lib/graph.ps) run
0 -1.2 6.283185 1.2 60 60 480 360 setframe   % domain -> viewport
newpath 0 6.283185 200 { sin } plotfn stroke % y = sin(x)
newpath 5 4 6 axes stroke                    % border + ticks
```

## Bar, line, area, scatter and pie charts: dataviz.ps

`lib/dataviz.ps` covers chart-drawing instead of
mathematical plotting: `setdvframe` maps a value domain and category
count onto a device viewport, and `barchart`/`linechart`/`areachart`
read it directly — a per-element color callback (`{i v -> r g b}`,
same shape as artkit's `grid`) lets every bar or wedge pick its own
color, called before that element's path is built so a misbehaving
callback can't silently drop it. `scatterchart` has its own continuous
2D frame (`setscatterframe`, the same 8-arg shape as `graph.ps`'s
`setframe`); `piechart` draws pie wedges or, given a nonzero inner
radius, donuts — clockwise from 12 o'clock, the usual chart-library
convention. `dvaxes` decorates the categorical frame with a bordered,
ticked frame the same way `graph.ps`'s `axes` does. `examples/dataviz.ps` is a six-panel
specimen sheet; the gallery's Field Notes pairs a bar chart and a line
chart on one shared category axis (weekly sightings against a
temperature trend) alongside a species-mix donut.

```postscript
(lib/dataviz.ps) run
0 10 60 60 480 360 setdvframe                  % value range -> viewport
newpath [3 7 2 9 5] 0.25 { pop dvcolor } barchart
newpath 5 4 6 dvaxes stroke                     % border + ticks
```

## Paper-ready figures

Both libraries have plain counterparts for papers and documents, distinct from
the stylized gallery pieces: `examples/graph_paper.ps` (a single framed
`plotfn` figure with axes and labels) and `examples/dataviz_paper.ps` (bar,
line, pie and scatter panels on one white page).

## Interactive version

The site's [charts page](https://jeromebanks.github.io/postscript_interpreter/charts.html)
scopes the playground to these two libraries: pick a preset, edit the data or
function, and watch it render live.

## For agents

The `psviz` skill ([`.claude/skills/psviz/SKILL.md`](../../.claude/skills/psviz/SKILL.md))
teaches an agent the calling conventions and copy-paste templates, so a
data-visualization request goes straight to `graph.ps` and `dataviz.ps`
instead of the generative-art loop.
