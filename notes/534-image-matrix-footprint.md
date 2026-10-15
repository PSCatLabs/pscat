## `image`/`imagemask` honour any invertible ImageMatrix (issue #534, 2026-10-15)

Found in #474: `imagemask` with `[1 0 0 -1 0 h]` and no `w h scale` painted one
cell. The rasterizer (`src/image.rs`) clipped every image to the user-space
unit square, which is the image's footprint only for the usual
`w h scale` + `[w 0 0 ±h …]` idiom. Per the PLRM the footprint is
ImageMatrix⁻¹ of the source rectangle `[0,w]×[0,h]`, whatever the matrix.
`image` and `colorimage` (Level 1 and dict forms) had the same bug; the SVG and
PDF recorders already used CTM ∘ ImageMatrix⁻¹ and were right.

### What changed

- `ImageCtx::rasterize` takes its device bounding box from the footprint
  (ImageMatrix⁻¹ of the source corners), and a pixel is painted when its
  centre maps to a sample inside `[0,w)×[0,h)`.
- **The standard idiom keeps its old path exactly.** When the matrix maps the
  unit square's corners exactly onto the source rectangle's corners
  (`unit_footprint`: `[w 0 0 ±h …]`, mirrored or transposed), the old
  unit-square bbox and its half-open `[0,1)²` test stay. That test decides
  pixel centres lying exactly on a flipped image's top edge, so dropping it
  would move edge rows in committed renders. Every tracked use of
  `image`/`imagemask`/`colorimage` (`examples/postcard.ps`, `lib/released.ps`'s
  haze grain, `lib/fillkit.ps`, `lib/drychalk.ps`) is this idiom.
- A singular ImageMatrix is `undefinedresult` at the operator, before any data
  is read (`check_invertible` in `src/ops/image.rs`), as in gs. pscat used to paint sample (0,0)
  over the unit square. One gs quirk is matched too: Level 1 `image` and
  `colorimage` with a zero width or height return before the matrix check, while
  `imagemask` and the dict form still raise.

### Pinned against Ghostscript 10.07.1

72 dpi, 100×100 page, `png16m`, no anti-aliasing. With the fix pscat matches gs
pixel for pixel on: `[1 0 0 -1 0 8]` after `10 10 scale`; `[0.1 0 0 -0.1 0 8]`
with no scale; `[1 0 0 1 0 0]` (unflipped); a translated source cut at the
origin (`[1 0 0 -1 -2 8]`); a skewed non-unit matrix; the current colour;
polarity false; the dict form of `imagemask`; and the same for 8-bit `image`
(scaled, unscaled, transposed, dict form) and 3-component `colorimage`. Under a
rotated CTM the unscaled form renders the same pixels as the rotated standard
idiom; both differ from gs by one edge pixel (3700 vs 3701 inked), which
predates this change. `tests/images.rs` pins the gs ink counts and checks each
unscaled form is pixel-identical to its standard-idiom twin.

Not changed: at half-pixel-aligned edges the standard idiom's coverage rule
differs from gs's by a row or column (e.g. `10.5 10.5 translate`); that is the
existing pixel-centre rule, and changing it would move committed renders.

### Identity

`scripts/identity_check.sh --base-bin/--head-bin` over all 195 tracked
`.ps`/`.eps` files plus `--gallery-still`, with the `[1 0 0 -1 0 8]` probe as
the must-differ control: 212 of 212 page renders identical across 195
files, `alpine_lake` equal to its committed still in both trees, the control
differs, and the usual 8 files skipped (they fail identically in both trees).
No committed render changes.
