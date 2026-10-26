# 781: 16-bit samples, `colorimage` multi-source, `/MultipleDataSources`

Found by #776 (its image drivers pinned all three as refusals). Implemented in
`src/image.rs` and `src/ops/image.rs`; the tests are `tests/image_planes.rs`
and the drivers `selftest/drivers/image_{formats,sources,planes}.ps`.

## What now works

- `/BitsPerComponent 16` and, a PLRM Level 2 depth that was also missing, `12`,
  in `image` (operand and dict form), `colorimage` and ImageType 3 DataDicts.
  `sample()` already read any depth MSB-first, so this was the operators'
  `1|2|4|8` check, now `valid_bits` (1, 2, 4, 8, 12, 16). Imagemasks stay 1 bit.
- `colorimage` with `multi true`: `ncomp` sources are on the stack, the first
  (deepest) is the first component (red, cyan, gray).
  `ncomp` must still be 1, 3 or 4 (`rangecheck` otherwise, as gs).
- `/MultipleDataSources true`: `DataSource` is a literal array of one source per
  component of the current colour space. gs errors, and so do we: a wrong
  length is `rangecheck`; `true` with a non-array or a procedure, or `false`
  with a literal array, is `typecheck`.
- It all composes: Decode, Indexed (an array of one), `/Interpolate`, and an
  ImageType 3 `DataDict` with separate sources.

## The output decision: 8 bits everywhere

Every output is 8-bit RGB (`rgb_at` is the only sample-to-colour step), so a
16-bit sample is reduced to 8 bits, identically in PNG, SVG and PDF. We did
not write `/BitsPerComponent 16` into the PDF although PDF 1.5 allows it: the
PDF image is already a converted RGB image (Indexed, CMYK and Decode are
resolved), the PNG raster is 8-bit, and a 16-bit PDF alongside an 8-bit PNG
would make the formats disagree. The cost is a tiny tonal-resolution loss in
PDF that the PNG and SVG share. `tests/image_planes.rs` pins PNG, SVG and PDF
equal to the matching 8-bit program (samples of the form `n*257` scale to `n`).
PDF output was also rendered by gs and matched the PNG exactly.

## Read order

Each component is an `ImageCtx` "plane" (single component, same size and
depth). `fill_planes` repeatedly asks the plane holding the fewest bytes, the
lowest component on a tie. For row procedures that is red 0, green 0, blue 0,
red 1, ...: the same order gs calls them (checked with a logging program).
String and file sources fill at once. A plane that completes drains its filter
chain at once, so a second `currentfile /ASCIIHexDecode filter` begins after
the first one's `>`. Premature EOD reads as zero, like a short single source.

## Against Ghostscript 10.07

Pixel-identical (max difference 0-1 level) on every Gray and RGB cell: 16-bit
interleaved, 16-bit multi, 8-bit multi, MDS dict, three `currentfile
readhexstring` procedures (the issue's real-world case), three inline
ASCIIHex filters, 12-bit gray. Differences, none ours:

- **CMYK** differs at every depth, 8-bit included: pscat's CMYK-to-RGB is the
  naive formula, gs's is not. 12-bit CMYK equals the equivalent 8-bit CMYK
  exactly (`tests/image_planes.rs`), so wide samples add no CMYK error.
- **Indexed, no `/Decode`**: gs 10.07 paints palette entry 0 for every sample
  (it falls back to Decode `[0 1]`); pscat defaults Indexed to
  `[0 2^n-1]` (the rule PDF states; the PostScript dict form lists Decode as
  required, so neither is wrong), as before this change. With an explicit `/Decode` gs agrees,
  except at 16 bits, where it loses the low bit of the index.
- **ImageType 3** left cell: gs paints black where pscat paints the image,
  with or without `/MultipleDataSources` (so not this change).
- **One file object for all planes** (`currentfile currentfile currentfile`):
  gs treats it as one plane and paints it gray; pscat reads the planes
  one after another. Nobody writes this; not matched.

## Not done

InterleaveType 1/2 (#765) still `limitcheck`; their probe stays in
`image_masks.ps`.

## Identity check

`scripts/identity_check.sh --base-bin ... --head-bin ... --expect-diff`:
see the PR. Only the cells that started to draw differ (the 16-bit cell on
each page of `image_formats.ps`, the multi-source cells on pages 2 and 3 of
`image_sources.ps`); `image_planes.ps` is new and so not compared.
