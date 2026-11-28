## host API: dropping the 16-bit fill shadow (issue #1506, 2026-11-28)

psanim found that `Interp::swap_canvas` -> `Gfx::swap_pixmap` calls `Deep::clear()`:
swapping the canvas out and straight back with no pixel changed throws the 16-bit
fill shadow (`src/deep.rs`) away, and painting after it rounds from the 8-bit
pixels, up to one level off the same painting with no swap. Nothing documented it.
psanim worked around it by swapping out and back before each step.

**Reproduced on the base** (`cd67023f`, `tests/host_fill_shadow_1506.rs`, 64x64 black
page, white at alpha 0.02):
- (a) 5 fills, swap out and back, 1 more fill: all 4096 pixels are 1 level off six
  fills in a row.
- (b) 1 fill, swap, one `show` of a 60 pt `W`: 37 pixels, 1 level (the issue's 36 is
  for its own glyph; the program is in the test).
`swap_canvas` is unchanged and pinned by these two tests.

**Public API (`pscat::Interp`, Experimental).**
`pub fn drop_fill_shadow(&mut self) -> bool`: clears the shadow of the canvas and
nothing else; `true` when there is a canvas, `false` inside a `PaintProc` with no page
group (nothing dropped). `src/page.rs` next to `swap_canvas`; `Gfx::drop_fill_shadow`
reuses `page_slot_mut`, the lookup `swap_pixmap` uses, so "which shadow" is the same
by construction:
- no group: the page's. With groups open: the page below the outermost group (the
  canvas `swap_canvas` exchanges), however deep the call is. A group layer's own
  shadow, and the layers of enclosing groups, are not touched.
- `nulldevice` group (no layer): the page itself.
- `PaintProc`: no page group opened before it -> `false`, nothing dropped (a group the
  PaintProc opens itself is the tile's); with one -> that group's page below, `true`.
- Pure on the shadow: unlike `swap_pixmap` it does not clear `pending_erase`, does not
  set `painted_since_page` or `dirty`. So after `showpage` the next mark still erases
  (the swap trick cancels it and paints on the old page, and `pages()` then counts
  a trailing page).
- Free: no step is charged (`steps_used` is counted under a limit only; the test sets
  one and asserts the count is real), nothing is allocated, the shadow's memory is
  freed (`tests/host_fill_shadow_memory_1506.rs`, own binary with the tracking
  allocator).

**What a later fill does** (measured, `what_feeds_the_shadow_and_what_does_not`): a
translucent `fill`, `rectfill`, a glyph, and Multiply even at alpha 1, differ after the
drop; strokes, images and an opaque Normal fill never do. 1000 fills of white at 0.002
reach 221 with the shadow; dropped before every fill they reach 5 (not the 128 that
`deep.rs` says an 8-bit page stalls at: that is a different arrangement).

**Item 3: carrying the shadow with a `Page`: measured, not implemented.**
- Cost: the shadow is 8 bytes a pixel once a row is composited into (rows are allocated
  lazily, so a fill over 10% of the rows holds 10%): 8.0 MB at 1000x1000 (the page is
  4.0 MB), 16.6 MB at 1920x1080, 32 MB at 2000x2000, 128 MB at 4000x4000. A `Page` that
  carried it is 3x today's. `Page` is `Clone` and `showpage`/`into_last_page`/`pages()`
  snapshots would carry it too, so it has to be opt-in (an `Option` filled only by a
  new swap variant), because `swap_canvas` must keep clearing it.
- Design if wanted: `Page { pixmap, scale, shadow: Option<Box<Deep>> }`; a new
  `swap_canvas_keeping_shadow` that takes the interpreter's `Deep` out with the old
  raster and installs the incoming page's. The bucket check (`deep.rs`) makes a carried
  shadow safe against a host that edited the 8-bit pixels in between, so correctness
  is not the risk. The risks are the footprint, a larger public surface (a second
  swap, `Page` growing a field every constructor must fill), and groups (which layer's
  shadow an open group would hand back).
- Why not now: psanim's problem is frames reached by playing in order versus by
  seeking, and a sought frame starts from 8-bit pixels read from disk; a shadow carried
  in memory does not reach it. Dropping at the same points on both paths (this PR)
  makes them agree. A follow-up is proposed only if a host needs an in-memory
  checkpoint to be the same page.

**Identity.** No rendering path changed: the only edits in `src/gfx.rs` and `src/page.rs`
are the new method and docs. See the PR for the base/head binary comparison.
