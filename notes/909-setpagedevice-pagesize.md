## setpagedevice /PageSize sizes the page (issue #909, 2026-10-29)

**The problem.** `<< /PageSize [640 400] >> setpagedevice` was `undefined`,
so the usual page-size preamble stopped a valid program before it painted
anything. `currentpagedevice` was missing too.

**The rule, highest first.** `--page` (MCP `width`/`height`, or any
`Interp::with_page`/`with_page_scaled` caller) > the program's
`setpagedevice /PageSize` > `%%BoundingBox` (#921) > letter. Code that
embeds pscat and picks a size is `PageSizeSource::Explicit`, so a program
cannot resize a page that a subcommand, a golden test or the wasm playground
sized. Only `Interp::new` (`Default`) and the CLI's `Options::new_interp`,
which records `--page`, the box or the default, let a program resize. An
explicit page keeps its size, and a `/PageSize` that differs gets one
`pscat: warning:` line.

**What it does, pinned against Ghostscript 10.07.1** (`src/ops/pagedevice.rs`):

| program | gs | pscat |
|---|---|---|
| `<< /PageSize [640 400] >> setpagedevice` | 640x400 page | same |
| `[595.28 841.89]` | 595x842, reports `[595.28 841.89]` | same |
| `[0 400]`, `[-10 400]`, `(ab)`, `[(a) 20]`, `[1e9 1e9]` | configurationerror | same |
| `[10 20 30]` | accepted | configurationerror (deliberately stricter) |
| `5 setpagedevice` | typecheck | same |
| marks, then `setpagedevice` (even `<< >>`) | page erased | same; the marks are not a page |
| `100 100 translate 0.5 setgray` then `setpagedevice` | CTM, gray, width reset | same (initgraphics) |
| `gsave` or `save`, `setpagedevice`, `grestore` or `restore` | old device back, erased; of another size, with the default matrix and no clip | same, and for `setgstate` |
| `setpagedevice` then `grestore` with nothing saved | back to letter | same (the job's initial state, #505) |
| `<< /Duplex true /Foo 3 >> setpagedevice` | ignored silently, not in `currentpagedevice` | ignored, one warning per key |

- **Other keys.** gs ignores keys it does not know, and so does the PLRM's
  default `/PolicyNotFound` 1. Refusing them would break the preambles this
  issue is about, so pscat ignores them too, but says so on stderr once per
  key (`pscat: warning: setpagedevice: /Duplex is not supported and was
  ignored (only /PageSize is)`), and the MCP server returns that line as a
  `Warnings:` item. `currentpagedevice` returns a new dictionary with only
  `/PageSize`, so `/Duplex known` is false. It does not claim to hold
  parameters that do nothing.
- **After marks.** The PLRM's setpagedevice runs `EndPage` with reason 2
  (device deactivation), which returns false by default, so the marks
  already painted are not transmitted. Then it does `erasepage` and
  `initgraphics`. pscat drops them the same way: the page is erased and
  does not count as a trailing page. A page already ended with `showpage` is
  kept at its own size.
- **The origin.** A page the program sizes starts at (0, 0), even when a
  bounding box had moved the origin. `<< >> setpagedevice` with no
  `/PageSize` keeps the size and the origin.

**How it works.** `gfx::PageDevice` (size in points, size in pixels, origin,
source) is an `Rc` held by `Gfx` and by every `GraphicsState`.
`Gfx::install_device` makes a new canvas when the size changes, erases it,
rebuilds the base CTM, and clears the ink flag and the lazy erase. After
any restore (`grestore`, `grestoreall`, `restore`, `setgstate`, a Type 3
glyph's restore), `Gfx::sync_device` reinstalls the restored state's device
if it is not the current one. That is an `Rc` pointer compare, so a run
that never calls `setpagedevice` does nothing new. `setpagedevice` keeps the
`clipsave` stack, as gs does, unless the canvas size changes: its masks were
made for the old canvas, so a resize drops them (gs would keep them; a
deviation). A `cliprestore` that falls back to the clip a level began with,
when that level began on another page device, gets that clip as gs keeps it
(`Gfx::fit_clip`): the old page's rectangle intersected with the chain's
device-space paths, rasterised on the new canvas. A level that began under
`nulldevice` gives back an empty clip, and the bottom level the job's initial
page. A restore refused for memory leaves the state on the kept device's
default CTM with no clip, and image rasterising only reads a clip mask of the
canvas's size. `currentpagedevice` under `nulldevice` is an empty dictionary,
as in gs.

**Everything that sized a buffer from the page now takes each page's size.**

- PDF: one `MediaBox` and flip `cm` per page. SVG: each page document has
  its own `width`/`height`.
- `--bands`: a band is now "band i of K" (`Gfx::set_band`), and its rows
  come from each page's own height, so a band keeps
  `floor(i*H/K)..floor((i+1)*H/K)` of a page of any size. `stitch` builds
  each page at its own size. When the page is resized inside a band,
  `/BandBBox` is redefined in userdict for the new page
  (`Interp::after_device_change`, with the same f32 arithmetic as the first
  definition). A banded render stays byte-identical to a plain one
  (`tests/pagedevice.rs`).
- A sweep's `--contact-sheet` on an explicit `--page` is still made, and its
  size checked, before any frame renders. On any other page the program may
  set the size, so the sheet is made with the first frame's size. A later frame of another size is an error, where
  before it would have panicked in `blit_cell`.
- The live window (a file run or `--interactive`) follows the new canvas,
  but the window keeps the size it opened with and stretches the page to
  fit it.

**Limits.** A page the program asks for is held to 8000 pixels a side, as
`--page` is. `pscat --max-pixels N` (new) caps it lower and checks the first
page too, before allocating it. It is refused with `--spool`, whose jobs make
their own canvases. Over a cap, `setpagedevice` raises `configurationerror` before it
allocates anything. With `--max-pixels` it also prints
`limits::pixel_cap_message`, so the MCP server, which now passes
`--max-pixels` to every `render_postscript`/`start_render` child, answers
`output_too_large` with the usual fields. The server's own check before the
child starts still covers the first page, which the child allocates before
the program runs. A resize is checked against `--max-memory` before the
new canvas is allocated (`Interp::check_alloc`), and so is a canvas a
restore brings back (`Gfx`'s memory budget: the restore keeps the current
device and the restoring operator raises `limitexceeded (memory)`). A Type 3 glyph that sets
the page device gets the old one back at its end, and `/BandBBox` follows.

**Not done.** No other page-device parameter (`/HWResolution`,
`/Orientation`, `/PageOffset`, `/BeginPage`/`/EndPage`, `/Policies`, media
selection). `--spool` jobs keep the canvas they were given (`Explicit`), and
the live window does not resize itself to a new page.
