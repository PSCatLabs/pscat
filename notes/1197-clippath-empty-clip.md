## `clippath` after a clip with no area (issue #1197, 2026-11-12)

## Decision

`newpath 0 0 10 10 rectclip 50 50 10 10 rectclip clippath pathbbox` raised
`nocurrentpoint`: an empty intersection left `clippath` an empty path.
Ghostscript 10.07.1 never gives an empty path there, so neither does pscat.
Every case was run through gs first (`tests/clippath_empty.rs` pins the
lines and re-runs gs where it is installed).

## What gs does (pinned)

- **No area** (disjoint rectangles from `rectclip`, `clip` or `eoclip`;
  `newpath clip`; a lone-`moveto` clip; rectangles that only touch along an
  edge; any clip made after a zero-area clip): one `moveto` at the device
  origin. In default user space that is `m 0 792`, and `pathbbox` is
  `0 792 0 792`.
- **A zero-area rectangle** (`rectclip` with a zero width or height) that
  lies wholly inside the clip it was made under (page included, edges
  allowed) stays a box: its own four-point path, in the device order it was
  made with (a later CTM change only re-expresses it). `rectclip`
  normalises a negative width on a zero-area rect (`10 20 -5 0` starts at
  `5 20`); a negative height is kept (`10 50 0 -30` keeps its order); a
  path built by hand and `clip`ped is kept as given. Off the page, or
  leaving its parent, it is empty.
- `eoclip` with a path that cancels itself leaves gs's `clippath` as the
  original (two-subpath) path, so emptiness is decided from the
  rectangles, never from the raster mask.
- `initclip` after an empty clip is the page rectangle again.

## What changed

- `Gfx::set_path_to_clip` ends by swapping a path with no line, curve or
  close for the device-origin `moveto`. A rectangle chain whose
  intersection has no area goes through `degenerate_chain_boundary`.
- `PsPath::as_axis_aligned_rect` accepts a zero-area box, so a degenerate
  clip stays on the exact rectangle path instead of the mask trace.
- `rectclip` normalises the width of a zero-area rect before building its
  path (the corner order above).
- With no clip, `clippath` lists the page corners in gs's order (device
  (0,0), (0,h), (w,h), (w,0)) through its own `page_clip_path`; `rect_path`
  (used by `shfill` and clip refits too) is unchanged.

## Not done

- A zero-height line that partly overlaps its parent: gs gives a 1-pixel
  strip (it works on integer device pixels), pscat says empty.
- A nested non-degenerate `rectclip` lists its corners in the order the
  rectangle was given in gs; pscat lists the intersection in device order.
  Bounding boxes agree. (The same difference existed before.)
- A single clip with area that extends past the page is passed through
  unclamped (documented in `set_path_to_clip`; gs clamps it).
- A rotated CTM: gs makes a zero-area `rectclip` empty there; pscat keeps
  the box.
- After a device refit (`fit_clip`) a zero-area chain rectangle becomes an
  empty clip.
