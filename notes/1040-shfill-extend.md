## shfill: /Extend honoured in PNG, SVG and PDF (issue #1040, 2026-11-16)

Before: `/Extend` was validated for shape and then ignored. Every shading
padded both ends (`[true true]`), and the PDF wrote `/Extend [true true]` so
it would match the PNG. The PLRM default for a missing key is `[false
false]`, and Ghostscript honours it, so a gradient bar painted past its ends
and a radial highlight flooded its whole clip.

**Semantics, settled with gs 10.07.1** (a grid of axial, nested radial,
cone radial and shrinking radial under each of the four `/Extend`s, plus
the degenerate radials):

- A missing key is `[false false]`. Each end is independent.
- Past an end that does not extend, the page is untouched (axial: the half
  plane beyond that point; nested radial: inside the first circle when the
  circles grow, outside the second; the other way round when they shrink).
- A radial whose circles are not nested (a cone, `|c1 - c0| > |r1 - r0|`)
  can put a point on two circles. gs paints the larger t that is in range,
  so with the end not extended a point whose larger t is past the end takes
  its smaller t. It is never a hole.
- One circle twice (`[50 50 20 50 50 20]`): a disk of C0 if the start
  extends, else nothing; nothing outside the circle under any `/Extend`.
  Nearly one circle (tiny-skia's 2^-15 tolerance) is the same, except that
  circles shrinking by a hair paint C1 when the end extends. pscat used to
  paint C0 inside and C1 outside in every case.

**Raster** (`Gfx::shfill_limited`, `src/gfx.rs`). An end that does not
extend gets a transparent stop at its own position (`extend_ramp`), after
`raster_stops` so the 1024-stop resampling cannot blur the step. With the
zero-width step tiny-skia's gradient takes its general stage, which has no
clamp: below 0 the first stop, from 1 on the last, so nothing is painted
past the end. A transparent stop costs tiny-skia its opaque fast path
(about twice the instructions for a page-filling gradient), so
`shfill_ends_reached` keeps the old opaque shader for an end whose far side
the painted region cannot reach. The region is the page's pixel centres, the
`/BBox` and the clip paths' boxes. An axial's t is affine, so the corners
decide it; nested circles test corners and box against the disks; a cone is
taken to reach both ends. A page-filling axial or radial whose ends lie
outside the page retires the same instruction count as before (300 dpi
letter, ten `shfill`s: within 0.6%). One that reaches past an end paints
differently and costs more: +36% for a radial whose corners leave the end
circle, +104% for an axis across mid-page, and +193% for a cone with its
two-root layer.

The cone with its end not extended paints from `two_root_layer`. That is a
page-size layer: first the circles run backwards (tiny-skia's larger root of
that gradient is the smaller root of the shading), then the shading itself
over it, then the layer through a `Pattern` shader into the unchanged paint
path (strips, scratch copy, mask, alpha, blend). Its work is charged three
times, it is `check_alloc`ed, and it is painted in polled strips under the
limits. A degenerate radial hands tiny-skia `[inside, transparent]` as its
stops (`degenerate_radial_inside`), and nothing at all when there is no
inside colour.

**PDF** (`PdfRecorder::shfill`). It writes the shading's own `/Extend`. The
degenerate-radial fallback records gs's disk (centre to radius, one colour,
`/Extend [false false]`, or nothing). The collapsed-axis solid fill is
recorded only when the end extends, since otherwise the raster's last stop
is transparent.

**SVG** (`extended_for_svg`, `src/svg.rs`). SVG pads with the outermost
stop, and two stops at a terminal offset are read differently by different
renderers. So the gradient is lengthened in its own t past each unextended
end (an axis a whole length each way; a radial back to the circle of
radius 0, or one t forward), with a transparent run from a zero-width step
at the real end. Both ends extended writes exactly what it did before.
Checked in Chrome and in WebKit (`qlmanage`) against the PNG on the grid.
The remaining gap was SVG's own (it paints only a point's larger root, so a
cone with its end not extended showed a hole where PNG and PDF paint the
smaller root); closed by #1370 (`notes/1370-svg-cone-smaller-root.md`).

**Corpus.** Only `gallery/varsity_backs.ps` and `examples/gradients.ps` relied
on the old padding (130 pages compared). Rendered to the spec, both looked
broken to the perceptual reviewer (round 1, FAIL), though both matched gs:
- the jersey's last vertical shading stopped in a hard seam across the body;
- the specimen's radial panels left white corners and a hard-edged hole.

Both files now ask for the extension they rely on (`dup /Extend [true true]
put`), which also makes gs paint them as intended.
- `gradients.ps` renders byte-identical to before.
- `varsity_backs.ps` differs only in anti-aliased clip-edge pixels: the
  earlier unextended layers no longer paint there. Against gs it is the
  closest it has been.

artkit's `axialsh`/`radialsh` keep the spec default; its header and guide
show `dup /Extend [true true] put` for padding.
