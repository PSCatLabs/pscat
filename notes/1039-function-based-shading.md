## shfill: ShadingType 1 (function-based) and FunctionType 0 (sampled) in PNG, SVG and PDF (issue #1039, part 1, 2026-11-19)

Before this, `shfill` took ShadingType 2 and 3 only, with FunctionType 2 and
3. A Type 1 shading raised `rangecheck`, and so did any sampled function.
Types 4–7 (meshes) are still not implemented. They are the issue's second
half (follow-ups below).

**What a Type 1 shading is** (PLRM 4.9.3): colour is a function of a point
`(x, y)` in the shading's own space. `/Matrix` (default identity) maps that
space into user space. Only the `/Domain` rectangle (default `[0 1 0 1]`)
is painted, cut by `/BBox` when there is one. A two-input function needs
FunctionType 0 (sampled), because types 2 and 3 take one input. So the
sampled function came in the same change. It also works for axial and
radial shadings, and so does a `/Function` given as an array of
one-output functions.

**Semantics, settled with gs 10.07.1** (`{ ... } stopped` probes plus
`png16m` renders at 72 dpi):

- The 2x2 table `<ff0000 00ff00 0000ff ffffff>` under `/Matrix [100 0 0
  100 50 50]` gives corners (247,3,3), (3,247,0) and (3,0,247), and centre
  (127,127,128). The first input varies fastest in the table, and nothing
  outside the Domain square is painted.
- A 3x2 table: x = 0.25 lies halfway between the first two nodes (Encode
  defaults to `[0 Size-1]`). gs: (124,129,3) at (0.25,0.02), (126,129,33)
  at (0.25,0.25), (128,124,192) at (0.75,0.5). pscat evaluates exactly at
  each pixel centre. gs approximates in patches, so the two agree to a few
  levels (`tests/function_shading.rs`).
- Accepted by gs, and here too:
  - A DataSource shorter than the table: missing samples read as 0, and gs
    paints the white corner (0,3,3).
  - `/Size [1 2]` and `/Size [2 2.0]`.
  - `/Order 3`.
  - Stray `/Coords`, `/Extend` and `/Background` keys. `shfill` ignores
    Background.
  - A zero-area Domain, which paints nothing. So does a pair equal in f32
    (`notes/1333-shading-domain-equal-in-f32.md`).
- Errors, as gs reports them:
  - No `/Function`: `undefined`.
  - A one-input function on Type 1: `rangecheck`.
  - A Domain other than four numbers, or with a reversed pair: `rangecheck`.
  - A shading Domain not inside the sampled function's own Domain:
    `rangecheck` (from the agent review of PR #1322).
  - A Matrix: wrong length is `rangecheck`, a non-number in it is
    `typecheck`, and a singular one is `undefinedresult`.
  - Output count not matching the colour space: `rangecheck`.
  - In a sampled function, each of these is `rangecheck`:
    - no `/Range`;
    - BitsPerSample 7;
    - `/Size [0 2]` or a short Size;
    - `/Order 2`;
    - a short Encode or Decode;
    - a reversed function Domain or Range;
    - an array DataSource;
    - an array of the wrong number of functions.

**Gaps, measured:**

- **`/Order 3` is interpolated linearly.** gs does cubic interpolation.
  - With a 3x2 table, gs gives (127,145,57) at (0.25,0.5) where linear
    gives (127,127,64), and (128,139,201) at (0.75,0.5) where linear gives
    (128,124,192).
  - A 2x2 table reads the same both ways.
- **DataSource must be a string.** gs also reads a file. Here a file is a
  `rangecheck`.
- **FunctionType 4 (calculator) is not supported.** gs takes it in
  PostScript as an extension; the PLRM does not define it.
- A one-input sampled function on an axial or radial shading is sampled at
  the 64 even positions the ramp uses for any non-linear function. A table
  of many more nodes than that is smoothed.

The Domain and BBox quads are clipped to the layer in f64 before
tiny-skia fills them. Its anti-aliased fill panics on corners about 1e9
device pixels away (`/Matrix [2e9 0 0 2e9 0 0]`, `/BBox [-2e9 -2e9 2e9
2e9]`; gs paints both). A gradient with such a `/BBox` still panics the
same way in `paint_shfill`. That predates this change (0.7.0 does it too)
and is a follow-up.

**How it paints** (`Gfx::shfill_function_limited`, `function_layer`):

- **PNG.** A layer the size of the Domain parallelogram's box (cut by the
  BBox's and the page) holds the function's colour at each device pixel's
  centre. The device point goes back through the inverse of `/Matrix`
  times the CTM and is clamped to the Domain. The coverage of the Domain
  parallelogram and of the BBox is multiplied in, so the edges are
  anti-aliased as a fill's are. Then the layer goes through the same paint
  path as a gradient (`Gfx::paint_shfill`, factored out of `shfill_limited`
  unchanged): clip, `setalpha`, blend, strips and the scratch copy under
  the limits. The layer is built before anything is painted, with its work
  held against the step limit first, so a stopped `shfill` leaves the page
  as it was.
- **SVG.** SVG has no function shading, so the same layer is embedded as a
  PNG `<image>` at device resolution. The clip and opacity come from the
  usual wrappers, not from the image.
- **PDF.** A real `/ShadingType 1` with the shading's own `/Domain` and
  `/Matrix`. Its function is a FunctionType 0 table of the shading's RGB
  colours (Flate stream, 8 bits). The grid is 65 nodes a side, or a
  multiple of the source table's Size−1, plus one, so that a bilinear
  table comes back exactly (at most 257). It is RGB because that is how
  the raster converts DeviceCMYK. The BBox is a clip, as for a gradient.
  `assert_gs_matches` in `tests/pdf.rs` holds gs's render of the PDF to the
  PNG.

Gradients (types 2 and 3) are unchanged. The tail of `shfill_limited` moved
into `paint_shfill` as it was. The PDF writer's function list holds bytes
now, so it can carry a stream, and its shading emission is
`paint_shading`, shared by both kinds.

**Follow-ups:**

- ShadingType 4 and 5 (Gouraud triangle meshes) and 6 and 7 (Coons and
  tensor patches), in PNG, SVG (embedded image) and PDF (mesh streams).
- Cubic `/Order 3` for sampled functions, to match gs.
- A file DataSource.
- FunctionType 4, as gs accepts it.
- PatternType 2 (shading patterns).
