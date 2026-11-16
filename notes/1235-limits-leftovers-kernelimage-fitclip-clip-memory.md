## Four leftovers of #1219 (issue #1235, 2026-11-16)

#1219 (`notes/1219-limits-clip-shfill-type1.md`) left four paths. Items 1, 3 and 4 shipped in #1238; item 2 (`/BBox` shfill) followed #1040.

Measured on a release build of `main` (cc661d3) and of this branch, this
machine, 72 dpi unless stated.

| Item | Before | After |
|---|---|---|
| `kernelimage`, 8000 x 8000, every pixel retired, `--max-time 1` only | 1.27 GB RSS | 63 MB RSS, stops at 1.0 s |
| the same at 3000 x 3000, `--max-time 20` | 742 MB RSS, 8.7 s | 64 MB RSS, 5.5 s |
| `cliprestore` refitting a clip of 100,000 page-sized curves, `--max-time 0.1` | ran its 0.5 s, blamed on the next operator | stops inside `cliprestore` |
| `clip` of 100,000 curves under `--max-memory`, smallest cap that finishes, 72 dpi | 494 MiB | 31 MiB |
| the same at 300 dpi | 828 MiB | 159 MiB |
| the same at 300 dpi, no limit (the plain run) | 4.5 s, 629 MB | not changed (the rasteriser is compiled without the limits) |

The issue says 650 MB for the clip at 300 dpi and that `--max-memory 1024`
refuses it. On this machine the process's peak RSS was 629 MB and the
tracked heap needed 828 MiB, so 1024 still just passed; the margin was thin
(the MCP server holds its own page buffers in the same count).

### 1. `kernelimage` retired list

With `--max-memory` the list was already capped (`retire_cap`). With only
`--max-time` it had nothing to size it by. Now any limit (`Interp::limits_set`:
steps, time or memory) holds it to `RETIRE_CEILING_BYTES` (256 MiB) at the
cost a pixel really has: 16 bytes in the kernel's list and two objects in the
interpreter's, 112 bytes with the 48-byte `Object`: 2.4 million pixels (a
1500 x 1500 image all retired stays compiled, 1700 x 1700 goes to the
interpreter). A memory cap
holds it to the smaller of its own room and that. Past it the existing
`Stop::Cancelled` path makes the interpreter do the whole image from the
same procedure: the same pixels. With no limit set nothing changes.

The issue proposed about 16 million (256 MB of `(u32, u32)`). The
interpreter's list is the larger part of the cost, so the ceiling counts it.
Checked: none of the four gallery pieces that use `kernelimage` retires a
pixel at 72 or 300 dpi (`mirror_garden`, `mandelbulb`, `dreaming_network`,
`mochi_in_oils`), so no corpus render goes slower. An image that retires
more than 2.4 million pixels under a limit now runs entirely in the
interpreter; 3000 x 3000 all retired is faster that way (5.5 s against 8.7 s),
since the compiled run only discovered the retirements.

### 3. `fit_clip` polled with no probe

`Gfx::cliprestore` and `fit_clip` take the probe (`Interp::do_cliprestore`
builds it), and `fitted_coverage` turns only a plain `limitcheck` (no mask
can be made: no clip, as before) into "no clip"; a limit is an error. The
clip is assigned only on success, so a refused `cliprestore` leaves the clip,
the `clipsave` stack and everything else as they were. The work is charged
like `clip`'s, a step per 1024.

### 4. The polled clip route's memory

The edges were the memory: for 100,000 curves at 300 dpi, 32.1 million at 16
bytes in a vector that grew to 2^25 (537 MB), and the band walk's index and
last-band arrays at 8 bytes an edge (257 MB). A page-sized accumulator is
`(w + 2) x h` cells at 5 bytes, 41 MB at 300 dpi letter.

Under limits `rasterize_body::<true>` now switches (`Stream::Auto`) once the
edge list is longer than the accumulator that would replace it (and at least
65,536 edges, and at most 2^25 cells): the edges so far are laid into the
page-sized accumulator and every later one is laid as it is flattened. It is
the same arithmetic in the same order, which is why the output does not
change (`lay_edge`):

- each edge is cut at the same band boundaries as before, because
  `accumulate` restarts its x at the top of each band: one walk down the
  page would round differently. `accumulate_at` is `accumulate` with the
  band's rows at their place in the big buffer; the unlimited body calls the
  old function, unchanged;
- each edge is laid into all its bands before the next, so a cell sums its
  edges in edge order, as it did band by band;
- `band_range` is the walk's own (moved out of `BandWalk::new`), NaN edges
  to every band included;
- the work count is the same (`chords + visits`), so `steps_used` agrees.

A unit test (`the_band_walk_paints_exactly_what_every_band_visiting_every_edge_does`)
streams from the first edge, the 2nd, the 500th and the 1500th and checks
the runs, the mask bytes and the work against the banded walk, with a clip
mask and both fill rules.

The step limit is held against the whole work before anything is painted in
the banded route; a streamed run knows its work only after laying, so it is
held then (and the polls stop it on the way). Nothing is painted either way:
the rasteriser writes the mask, the caller composites after it returns.

Applies to every limited `fill` too (same function): a 100,000-curve fill
at 72 dpi needed 494 MiB under a cap and needs 31. The existing
`limits_memory.rs` assertions that `fill` and `clip` of that path are
refused at 100 MiB therefore became "finish at 100 MiB", and their purpose
(the edge list is policed as it grows) moved to a page past 2^25 cells
(6000 x 6000, scaled so the path covers it), where the list is still kept.

The streaming route is a little slower than the banded one on this worst
case (100,000 curves clipped, no memory pressure): 1.54 s against 1.37 s at
72 dpi and 6.6 s against 6.3 s at 300 dpi, since a 41 MB accumulator misses
the cache that a 1 MB band does not. Nothing ordinary reaches it: 65,536
edges and more edges than the accumulator has bytes.

Not done: the unlimited run (`rasterize_body::<false>`) still keeps every
edge. It has no limit to hit and its output is the corpus's, so it was left
exactly as compiled before. A follow-up could stream it for a page of a few
million cells.

### 2. `shfill` with `/BBox` (PR after #1040)

With no `/BBox` the paint region is the page rectangle and strips of whole
rows paint the same pixels as the one fill (#1219). A `/BBox` is a
parallelogram with anti-aliased edges, so a strip cannot cut the path. Instead
the path's coverage is made once as a mask (`Mask::fill_path`, the same
rasteriser `fill_path` runs), and each strip is a rectangle of the box's
columns filled through it (`fill_rect` with that mask), so the shader runs
only where the box is. Under the limits and over `POLLED_BBOX_SHFILL_WORK`
(2^28 units of `shfill_work` for the box, about 0.07 s; a two-stop ramp needs
a 16-million-pixel box), nothing else changes: the page copy, the pending
erase, the SVG and PDF records are all kept for after the last strip, so a
limit still leaves all of them as they were. Below it, or with no limits, it
is the one `fill_path` as before.

Pixel identity, checked first as the issue asked: a randomised comparison of
`fill_path` against the mask and strips (rotated boxes, alpha, Multiply,
200-stop gradient) was byte-identical in every case with no clip. With a clip,
the strips multiply the box's coverage and the clip into one 8-bit mask where
the pipeline rounds twice, and 23 edge pixels in all differed by one level across
the 12 clipped cases (max difference 1). So with a clip the polled route is
a documented deviation of at most one level on edge pixels, as #1219's polled
clip is; below the threshold, which no gallery piece reaches, nothing differs.
The 2550 x 3300 box with and without a clip, at a heavy ramp, came out
byte-identical to the one fill (`bb.ps`, `bbclip.ps` in the PR).

`/Extend`'s transparent end stops (#1040) are part of the ramp, so `nstops`
counts them. The two-root layer of a cone (`two_root_layer`) was painted in
strips already, and the box then fills through it as a pattern: that part is
cheap, so the cone case stops inside `shfill` in the layer with or without
this change; the test pins the box over it. Measured on 4000 x 4000 with a
1026-stop ramp, a box over the page: 1.1 s in one fill that never read the
clock; with the strips it stops at the limit.

### Tests

- `tests/limits.rs`, `one_operator::issue_1235`:
  `a_clip_refitted_by_cliprestore_stops_inside_cliprestore` and
  `a_step_limit_refuses_a_refit_and_leaves_the_clip_alone` (a black fill after
  the refusal paints every pixel: the clip is the page, not the refit). Both
  fail with the probe replaced by `NONE`.
- `tests/limits_memory.rs`: the kernelimage case (4000 x 4000 all retired,
  only a 2 s clock, a sampler thread: peak growth under 300 MiB; 1.5 GB
  without the ceiling) and the clip and fill cases above (the 300 dpi clip
  of 100,000 curves finishes under a 400 MiB cap; with `Stream::Never` the
  100 MiB cases are refused as they were).
- `src/coverage.rs`: the streamed-against-banded unit test above.
