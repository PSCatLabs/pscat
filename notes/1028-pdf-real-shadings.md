## pdf: shfill exports as a real PDF shading, not the ramp's average (issue #1028, 2026-11-05)

Before: the PDF recorder wrote every `shfill` as one flat fill in the
ramp's position-weighted average colour (a red-to-blue axial came out
`0.5 0 0.5 rg … f`). SVG already had native gradients; PNG was right.

**What it does now** (`PdfRecorder::shfill`, `src/pdf.rs`; the hook in
`Gfx::shfill`, `src/gfx.rs`):

- `shfill` is `q [gs] [bbox clip] <CTM> cm /ShN sh Q` with a `/Shading`
  resource on the page. The shading keeps its `/Coords` in user space; the
  CTM goes in the `cm`, on top of the page's own `1 0 0 -1 0 H` flip, so
  rotation and anisotropic scale are exact (an axial gradient is not the
  gradient between its transformed endpoints under a skew). Geometry under
  the raster's `SAFE_LEN` is rescaled with the inverse folded into the `cm`,
  as the raster does, so a 1e-7 Coords under `1e8 scale` is written as
  ordinary numbers. Geometry and `cm` use the shortest round-trip `f32`
  text (never an exponent, never `-0`).
- ShadingType 2 (axial) and 3 (radial) map one to one.
- **The Function is built from the same RGB stop list the raster and SVG
  paint** (`Shading::stops`), not from the original function dicts: one
  leg is FunctionType 2 `N 1`, more are FunctionType 3 over inline type 2
  legs. All three outputs then interpolate the same stops, which covers
  CMYK, `/Range`, gamma (`N != 1`) and nested stitching without a second
  evaluator. Stops on the straight line between their neighbours (within
  1e-4 per channel) are merged, and zero-width legs (a hard edge) are
  dropped so `/Bounds` stays strictly increasing. A densely sampled
  stitched ramp with a constant leg went from 64 legs to the curved ones
  only.
- Function and Shading objects are document-global, deduped by content,
  and numbered after the images, before `/Info`: a document with no
  `shfill` has every object number and byte it had before.
- `/Extend` is always `[true true]`, because the raster pads both ends
  (documented deviation in `Gfx::shfill`); the PDF must look like the PNG.
- `/BBox` becomes a clip on the device-space parallelogram (the region the
  raster fills), inside the element's `q` and before its `cm`, not a
  `/BBox` key: no SAFE_LEN rescale to remember, and viewers differ on a
  shading BBox under `sh`.
- `/Background` is ignored by `shfill` (PLRM) and not parsed.
- Alpha and blend go through the ExtGState like any paint (`ca` applies to
  `sh`).
- **tiny-skia's degenerate cases are recorded as the raster paints them**,
  decided with its own tolerance (`DEGENERATE_THRESHOLD`, 2^-15) on the same
  rescaled f32 numbers (Codex rounds 2–5). An axis that collapses (Coords
  distinct in f64 but one point in f32) is a solid fill in the last colour,
  not a zero-length shading that readers skip. Two (nearly) coincident circles
  of (nearly) one radius paint the first colour inside and the last outside,
  with a hard edge. PDF readers draw those circles as nothing (CoreGraphics)
  or as just the disk (gs), so the recorder writes a radial from the centre
  to twice the radius, at half scale with the `cm` doubling it back so it
  stays finite near f32::MAX, with a stitching function that steps at
  `/Bounds [0.5]`. A hard edge at either end of the axis gets a 1e-6 sliver
  leg, so the function starts and ends on the outermost stops' colours,
  which the raster pads with. A non-finite number is never written.
  Pre-existing and left alone: gs's own rendering of the equal-circle
  PostScript is a thin ring, so the raster's fallback already differs from
  gs there.

**Not mapped, because the interpreter has none of them in any output:**
ShadingType 1 and 4–7 rangecheck in `parse_shading_dict`, and there is no
`makepattern`/`setpattern` or `/Pattern` colour space (checked at run time
on this branch: `/makepattern where` is false, `[/Pattern] setcolorspace`
errors). So there is no PDF case left that falls back to a flat colour or
needs a raster image; adding those shading types or shading patterns is
new interpreter work that changes PNG output, proposed as follow-ups.

**Checks.** `tests/pdf.rs` parses the PDF for axial, radial, stitched
(hard edge + gamma leg), clipped, `/BBox` under rotation, alpha, tiny
coordinates, dedupe across pages, and no shading machinery in a plain
program; each gs-rasterises the PDF and compares it with pscat's own
canvas (mean block diff 0.27–1.1 of 255, limit 3; skipped without gs, as
in CI). `tests/wasm.rs`'s vector parity document gained a clipped
stitched radial, so WASM and CLI PDF bytes stay equal with shadings in.
PNG is byte-identical over 124 pages / 93 files (two-binary identity
check); a PDF sweep of every examples/ and gallery/ file with both
binaries changes only files that contain `/ShadingType`.
