## shfill: Gouraud mesh shadings, ShadingType 4 and 5, in PNG, SVG and PDF (issue #1325, part 1, 2026-11-20)

Before this, `shfill` took ShadingType 1, 2 and 3; a mesh (types 4–7) was
a `rangecheck`. Now the two Gouraud triangle meshes paint:

- Type 4: free-form Gouraud triangles.
- Type 5: lattice-form Gouraud triangles.

Types 6 (Coons patches) and 7 (tensor-product patches) are part 2, a
separate PR kept apart for size; see the section at the end.

The code is `src/mesh.rs`. `shading.rs` only dispatches to it
(`ParsedShading::Mesh`), and `Gfx::shfill_mesh_limited` is the paint path.

**Reading the data.** `/DataSource` may be any of:

- an array of numbers, used as they are;
- a string;
- a file. It is read to its end, so an inline `currentfile
  /ASCIIHexDecode filter` stops at its EOD and the program goes on after
  it.

Packed data (a string or a file) is read MSB first, through `/Decode`.
Rules settled with gs 10.07.1 on a real device: png16m renders and
`stopped` probes. `nullpage` skips mesh decoding and reports no data
errors.

- **Byte alignment.** Each Type 4 or 5 vertex starts on a byte. A packing that runs on unaligned is garbage or a
  rangecheck in gs.
- **Type 4 flags.**
  - A flag of 1 makes the triangle (vb, vc, vd); a flag of 2 makes (va, vc,
    vd).
  - A flag of 0 starts a triangle. The flags of its next two vertices are
    read but not used.
  - A first vertex flagged 1 or 2 also starts a triangle. gs accepts it
    and paints the triangle, plus some stray pixels pscat does not copy.
- **Errors, as gs reports them:**
  - `/DataSource` missing: rangecheck.
  - `/DataSource` of another type, or a non-number in the array: typecheck.
  - Packed data without `/BitsPerFlag` (not needed for Type 5),
    `/BitsPerCoordinate` or `/BitsPerComponent`: `undefined`.
  - Packed data with a bit count gs does not take: rangecheck (`8.0` is
    accepted).
  - `/Decode` of a length other than 4+2n: rangecheck. Here n is the number
    of colour values: one t with a `/Function`, otherwise the colour
    space's components.
  - A vertex, triangle or Type 5 row cut short: rangecheck.
  - A Type 4 flag over 2, or not an integer: rangecheck.
  - Type 5 with no `/VerticesPerRow`: `undefined`. Below 2 or fractional:
    rangecheck. Not an integer type: typecheck.
  - Type 5 with no vertices: rangecheck.
  - A `/Function` that is not one-input, or whose outputs do not match the
    colour space: rangecheck.
- **Accepted:**
  - Empty Type 4 data, which paints nothing.
  - A single Type 5 row, which also paints nothing.
  - Packed data with no `/Decode`. gs paints nothing, and so does pscat.
- **Error timing.** Errors are raised when `shfill` decodes the data,
  before anything is painted. gs may paint the triangles before a bad one;
  pscat paints nothing.

**Painting** (`mesh_layer`):

- **Triangles.** A Type 5 cell is split along its (1,0)–(0,1) diagonal,
  as gs splits it. Later triangles paint over earlier ones.
- **Colour.** A pixel takes the colour of the last triangle that holds its
  centre. The colour is interpolated in the shading's own components and
  converted per pixel, never interpolated in RGB. With a `/Function`, t is
  interpolated and then read from the function's RGB ramp (`build_stops`,
  the same stops an axial shading uses).
- **Coverage.** Each pixel has a 4x4 grid of samples, OR-ed over every
  triangle. Shared edges therefore have no seams: a constant-colour lattice
  or fan is exactly that colour inside (`shared_edges_leave_no_seams`).
  Only the mesh's outer edge is anti-aliased. A pixel at that edge whose
  centre no triangle holds takes the nearest triangle's colour.
- **Differences from gs.** gs does not anti-alias the edge, and paints
  every pixel the edge touches. It also subdivides triangles by its own
  smoothness tolerance, so its interior is a few levels off pscat's exact
  interpolation. Away from the edge, the whole-page mean difference from
  gs is under 1 level.
- **Painting the layer.** It is built before anything is painted. Its work
  is held against the step limit first: each triangle, plus each pixel of
  its box inside the layer. That way a crafted mesh of huge overlapping
  triangles is stopped up front, and the page is left as it was. The
  layer is then painted by `paint_shfill` like a Type 1 layer, so the
  clip, `setalpha` and blend modes apply.

**SVG** embeds the layer as a PNG `<image>`, as for Type 1.

**PDF** writes a real mesh stream in DeviceRGB:

- Coordinates are 32 bits over the mesh's box, colours 16 bits.
- Triangles become Type 4, a lattice Type 5.
- With a `/Function`, each vertex carries its position on the ramp, and the
  ramp's stops are the shading's function, as for an axial shading.
- The PDF shading list now holds a `ShadingObj`: body, an optional
  function, and a stream.
- `assert_gs_matches` compares gs's raster of each PDF with the canvas.
  The mean block difference is 0.6–1.2.

**Codex round 1 on #1376:**

- **Raw file DataSource and the limits.** A raw file DataSource is now read
  under the limits. `parse_shading_dict` takes the limit probe. Each 64 KiB
  chunk's growth of the buffer, and the final copy, are held against
  `--max-memory` before they are taken. The bytes read are charged as
  steps (one a KiB) as reading goes, and again by the operator on every
  path, even when nothing is decoded.
- **Decoding is charged.** Each vertex costs 64 units, which also covers
  the bounds scan, and the limits are looked at every 4096 vertices.
  `shfill` returns this work on every early exit, so a large mesh wholly
  off the page, shaded in a loop, answers to `--max-steps`. The same holds
  in `shading_layer` and `makepattern`.
- **PDF function input.** With a Function, the PDF now writes each vertex's
  position on the ramp unclamped, over the range the vertices span. The
  function is then flat outside the ramp (a stitching function whose
  Domain is that range), so a reader interpolates first and clips after,
  as the raster does. Before, a vertex with t = -1 was written as 0, and
  the PDF blended purple where the raster is red.

**Codex round 2 on #1376 (limit accounting on hostile input, by class):**

- **Copying the DataSource.** Every copy is charged on every path, as
  `MeshShading::source_work`, one step a KiB:
  - a string's bytes;
  - a file's bytes, read and then copied;
  - an array's numbers, converted at 8 units each.

  Each copy is checked against the step and memory limits before it is
  taken. So a 4 MB string with no `/Decode`, or an array under
  `nulldevice`, shaded in a loop, stops at `--max-steps`.
- **Vector growth.** Each decode vector is reserved once with
  `try_reserve_exact`, at the most the data can hold, after the memory
  check, so pushing never doubles past the allowance. The file buffer
  grows geometrically, but each real growth is checked first. The layer's
  buffers are allocated once, at their checked size.
- **Painting work.** Work is counted as it is done, not estimated up front.
  `mesh_layer` adds each triangle's work, its box inside the layer, before
  painting it. It holds the running total, plus the work the caller has
  already done, against the step limit every 1 Mi units. The layer is not
  yet on the page, so a hostile mesh stops at the limit and the page is
  untouched. The same loop serves every type.

**Shading patterns (#1378).** After #1367 merged, a mesh is a shading
pattern too (`makepattern` PatternType 2 with a mesh `/Shading`):

- `shading_layer` decodes it under the probe and paints it with
  `mesh_layer` in pattern space.
- Its work is charged on top of the layer's base before anything is
  painted, and its coverage buffers are held against the memory limit, so
  under `fill`, `stroke` and `imagemask` it answers to `--max-steps` and
  `--max-memory` (`a_mesh_pattern_layer_answers_to_the_limits`).
- SVG embeds the layer over the mesh's box, as for a Type 1 pattern.
- PDF writes the mesh stream as the pattern's shading, with `/BBox` and
  `/Background` (`PdfRecorder::mesh_obj`, shared with `shfill`).
- `makepattern` decodes the data once when SVG or PDF is being recorded,
  so a data error is raised there in those modes. Otherwise it is raised
  at the first fill.

**Gaps:**


- **CMYK in the PDF.** The PDF blends RGB vertex colours, but the raster
  converts CMYK per pixel, so a CMYK mesh's interior differs a little in
  the PDF. pscat's naive CMYK conversion is not gs's anyway.
- **Array data and `/Decode`.** An array DataSource ignores `/Decode`,
  while gs seems to apply part of it. Producers do not write that.
- **Left from #1039.** These are not in this change: cubic `/Order 3` for
  sampled functions, a file DataSource for FunctionType 0, and
  FunctionType 4.

## Part 2: patch meshes, ShadingType 6 and 7 (2026-11-20)

Same reader, painter and PDF hook as part 1; settled with gs 10.07.1 the
same way.

- **Data.** Each patch starts on a byte. Point order is the PLRM's: p00 p01
  p02 p03 p13 p23 p33 p32 p31 p30 p20 p10, then for Type 7 p11 p12 p22 p21,
  then the corner colours c00 c03 c33 c30 (`p[i][j]`, i along u).
  - Flag k (1–3) takes the previous patch's boundary points 3k to 3k+3 as
    its first edge and its corner colours k and k+1; the patch then gives
    8 points (12 for Type 7) and 2 colours. Pinned against gs renders for
    flags 1, 2 and 3.
  - A Coons patch gets its four interior points from the PDF spec's
    formula, so both types are tensor patches from then on.
- **Errors.** A first patch that is flagged, a flag over 3, and a patch cut
  short are rangechecks. Empty data paints nothing.
- **Painting.** A patch is tessellated on its surface S(u,v), with steps of
  at most 2 device pixels and at most 256 a side, into the same triangles
  as part 1; its colour is bilinear in (u,v). Larger v paints over smaller,
  then larger u, as the PDF spec orders a folded patch.
- **Limits.** A patch's work is counted triangle by triangle as
  `mesh_layer` paints its tessellation, with the same loop and polling as
  part 1. An estimate from the control points' box was not an upper bound
  (Codex round 1 on #1377): a tensor patch whose cells all cross the page
  made about 8.7 million pixel visits against an estimate of 20,000, and
  now stops at the limit (`a_patch_with_overlapping_cells_stops_at_the_step_limit`).
  A few KB of packed 1-bit patches that would tessellate into hundreds of
  millions of triangles stops too (`a_huge_packed_patch_mesh_is_stopped_promptly`).
  The patch vector is reserved exactly, as the others are.
- **Limits, from Codex round 1 on #1376.** The patch decoder counts and
  polls each patch as the triangle decoders do. A patch's corner `t`
  values join the PDF's unclamped ramp range.
- **PDF.** Patches are written as a Type 7 mesh (Coons ones already in
  tensor form), 16 points and 4 colours each.
