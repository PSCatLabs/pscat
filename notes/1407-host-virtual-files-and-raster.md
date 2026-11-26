## Host API follow-ups: virtual files and a host raster under the CTM (#1407, 2026-11-25)

Built on `feature/0.8.0` (the host API #1410, transparency groups #1406, source
positions #1405). Two items #1403 left out, both for the next embedding
application (psanim#44 loads components from files, psanim#29 places layers).
The API is additive; the new public items are `Interp::add_virtual_file`,
`remove_virtual_file`, `draw_page`, `host::Sampling` and the four `host::MAX_*`
bounds.

### Virtual files

`add_virtual_file(name, bytes)` registers a named in-memory source that `run`,
`(name) (r) file`, `readreals` and `status` serve (`read_resource` is the one
function the first three share).

- **Looked at first.** A registered name answers before read roots, the disk
  and the embedded `lib/`, so a host can replace a library; any other name goes
  through the unchanged path (disk, embedded, `undefinedfilename`, and under
  read roots `invalidfileaccess` outside them). Tests pin each of these.
- **Not gated by read roots.** The roots confine disk reads; a virtual file is
  memory the host put there. `..` names are refused at registration (they can
  never match, since a lookup is exact), so a program cannot name its way
  around the roots to a disk file by a virtual name.
- **Names.** Exact match after one leading `./`. Absolute names are allowed
  (a host that serves `/etc/app/x.ps` from memory has a use for it) and
  shadow the disk like any other. `%` names (devices) and `..`, empty and NUL
  names are refused.
- **Bounds.** 64 MiB a file (`paths::MAX_LIB_BYTES`, the bound on a `lib/` file
  from disk), 4096 files, 512 MiB together (`paths::MAX_RESOURCE_BYTES`). Past
  one, `limitcheck` and nothing changes. The store holds `Rc<Vec<u8>>` (the host's buffer is moved in,
  not copied, so registering allocates nothing to charge), so serving a
  file does not copy it until the program's file object is made; that copy
  (one per `run`) is charged a step per KiB through `poll_limits`, with
  `check_alloc` before it, so a loop that loads a large file meets
  `--max-steps` and `--max-memory`. No limits: a lookup, and nothing when the
  store is empty.
- **Frames.** `run_file` is given the name the program wrote, so the error's
  `file`, a frame's `file`/`line` (#1405) and `currentfilename` name it; the
  test runs a procedure that fails in a virtual file.
- **Not done.** `filenameforall` lists disk and embedded names, not virtual
  ones; `(w) file`, `deletefile`, `renamefile` are the write policy's. A
  `wasm` binding (`pscat.addFile`) is not in the JS module.

### A host raster under the CTM

`draw_page(&Page, Sampling) -> Result<(), PsError>` maps the page's pixel grid
onto the unit square of user space, first row at `y = 1`, the mapping of
`w h 8 [w 0 0 -h 0 h] image`, so a host places it with `translate rotate scale`
run as PostScript. (The alternative, a matrix argument, would make a second way
to say the CTM; the unit square keeps the one `image` has and a 1:1 placement is
`w h scale`.)

- **Painting.** `Gfx::place_raster` fills the footprint with a tiny-skia
  `Pattern` shader over the page's pixmap (no copy of the pixels), with
  `setalpha` as the pattern's opacity, `setblendmode` as the blend, the f32
  pipeline the other fills use, anti-aliased edges, and the clip mask the other
  painters use. `Nearest` and `Bilinear` are the shader's filter quality. It
  paints `gfx.pixmap`, which is the layer inside an open group and the tile
  inside a `PaintProc`, so groups and patterns need no special case; a
  transparent pixel paints nothing. The 16-bit shadow needs no notice (it
  checks itself against the 8-bit page, as for `image`).
- **Why a polygon, not the unit square.** tiny-skia's anti-aliased fill
  panics on corners about 1e9 pixels away (`shfill_limited` met it), and a
  program can set any CTM before a host operator calls this. The footprint is
  cut to a guard rectangle around the canvas (`clip_polygon`, as `shfill`'s
  box), the shader keeps the true matrix, and a singular or non-finite matrix
  is a no-op, as for `image`. A test runs `1e9`, `1e30` and `2e9` scales.
- **SVG and PDF: embed.** Refusing would make a program that places a layer
  fail only under `--svg`/`--pdf`, so the raster is recorded through the same
  `svg_image`/`pdf_image` that record `image` (a straight-alpha PNG, an RGB
  image plus a soft mask, none when every pixel is opaque), which also carry the
  clip chain, alpha, blend and group. This is what separates it from
  `swap_canvas`, whose raster no recorder sees. The recorders copy the pixels,
  so past `MAX_EMBEDDED_RASTER_PIXELS` (16 Mpx, as the stencil composite's cap)
  it is `limitcheck` while either recorder is on, before anything is painted or
  recorded. A raster wholly off the page paints and records nothing.
- **Charging.** Work is `plan.area` (device pixels of the canvas, cut to the
  band's rows, that the footprint's box reaches, so a million-fold scale costs
  the canvas, not a million canvases) plus the page's pixels once per recorder,
  as a step per 1024, through `host_poll` before anything is painted, so a
  refused call leaves the canvas as it was. `check_alloc` covers the recorders'
  copies (12 bytes a pixel for SVG, 8 for PDF). Between runs `host_poll` is the
  interrupt flag only (the limits belong to the run that ended); the memory
  check applies.
- **Groups.** It paints the layer, like any mark. A host reading the canvas
  meanwhile sees the page below (`canvas_premultiplied_rgba8` is `page_pixmap`),
  and `endgroup` composites at the group's alpha and blend. A group ends with
  the run, so the tests place from a host operator.
- **Uncoloured patterns.** `image` is not skipped there (the tile keeps the
  raster's coverage and takes the pattern's colour), so neither is this.

### Known limits

- The PDF demultiply loop and the SVG PNG encode over a raster of up to 16 Mpx
  never look at the clock (`image`'s recorders poll once a row). The work is
  charged to the step cap up front, so only `--max-time` granularity is
  affected.
- `MAX_HOST_OPERATORS` (item 3 of the issue) is not touched.

### Identity and cost

Programs that use neither feature are unchanged: the only edits on their path
are a field in `Interp`, one `HashMap` lookup (skipped when empty) per `run` or
`(r) file`, and nothing in the step loop. See the PR body for the two-binary
identity run and the `benches/perf` figures.

### Review round 1 (Codex, PR #1432, head c25a15b6)

Six defects, each with a test that failed on that head before its fix (the
virtual-file ones are in `tests/host_virtual_files.rs` and
`tests/host_api_memory_1407.rs`, the raster ones in `tests/host_raster.rs`).

1. **Registration copied before it checked.** `add_virtual_file` took
   `impl Into<Vec<u8>>`, so a borrowed slice was copied (up to 64 MiB) before
   any bound was looked at. It now takes `impl Into<Cow<[u8]>>`: an owned `Vec`
   is moved in, a borrowed slice is checked (name, sizes, count, total), held
   to `--max-memory` with `check_alloc` and, inside a host operator, charged a
   step per KiB, and only then copied. (A `&str` is passed as bytes.)
2. **The store kept more than it counted.** The total counted lengths only.
   It now counts each entry's name length and its buffer's *capacity* (no
   `shrink_to_fit` on registration: that is itself a copy), refuses a buffer
   whose capacity passes the per-file bound, and names are limited to
   `MAX_VIRTUAL_NAME_BYTES` (4096).
3. **Uncoloured patterns.** The canvas shows the raster's coverage in the
   pattern's colour, so SVG and PDF now record a stencil of it, as for
   `imagemask`: an SVG `<mask>` of the alpha (`svg_stencil`, filled
   `currentColor`), and a PDF image mask. PDF's mask is one bit a pixel (alpha at
   least half): PDF allows no colour image in such a tile and no other form of
   mask there, so partial coverage is thresholded in PDF only.
4. **Banded fills.** The charge was band-restricted but the fill was not. The
   footprint polygon is now cut to the band's rows, which is exact (a row's
   coverage is the same cut at its edge as whole).
5. **No polls during the work.** `place_raster` takes an optional poll that
   looks at the interrupt flag and, while a program runs, the time and memory
   caps (a `LimitProbe` taken after the up-front charge). It is called between
   the recorders' copies and every million pixels inside them (the
   demultiply, the stencil build), and between strips of the fill, which goes
   strip by strip (whole pixel rows) only when something is being watched. The
   recorders are written after every copy exists, so a refusal leaves canvas
   and output untouched. The PNG encode and the zlib compression inside one SVG
   or PDF image are single library calls: polled before and after, not within.
6. **Huge-CTM cancellation.** Cutting a parallelogram whose corners are 1e20
   away loses its edges' positions (f64 has 16 digits). Whether the placement
   covers the canvas (with a pixel of margin) is now decided in the raster's
   own unit coordinates, where it is accurate, and a covering footprint is that
   rectangle. A footprint that only crosses the canvas at such a scale is still
   cut in f64, and where the edge falls is limited by the matrix's single
   precision, which nothing here can repair.

Read roots: virtual files bypass them by design (host-supplied memory, not the
disk). A program can read only the names the host registered, so `pscat-mcp`'s
sandbox and `--allow-read` are unaffected by registering files; the
registration is the host's grant of that one name. Documented in
`docs/guides/library.md` and on `add_virtual_file`.
