## The page comes from %%BoundingBox when --page is not given (issue #921, 2026-10-28)

**The problem.** Nothing read `%%BoundingBox`, so a 400x300 drawing rendered
on a 612x792 letter page with the art in its lower-left corner, and every
render of such a file needed `--page WxH` by hand.

**The rule, highest first.** `--page` (MCP `width`/`height`) > the program's
`setpagedevice /PageSize` (#909, not implemented yet) > the bounding box >
letter. The box is read:

- by default only for an EPSF header (first line `%!PS-Adobe-x.y EPSF-x.y`);
- for any header with `--bbox` (MCP `bbox: true`).

`--bbox` beside `--page` (or `bbox` beside `width`/`height`) is refused rather
than silently dropped, as is `--bbox` with `--spool` or `--interactive`.

**Why only EPSF by default.** These were settled by the specs and by Ghostscript
10.07, measured here:

| file | gs | gs -dEPSCrop | pscat now |
|---|---|---|---|
| `%!PS-Adobe-3.0`, box 0 0 400 300 | 612x792 | 612x792 | 612x792 (400x300 with `--bbox`) |
| `... EPSF-3.0`, box 100 50 500 350 | 612x792 | 400x300 | 400x300, origin moved |
| EPSF, HiRes 10.5 10.5 210.25 110.75 | | 200x100 | 200x100 |
| EPSF, HiRes 0 0 100.6 50.4 | | 101x50 | 101x50 |
| EPSF, box `(atend)` + trailer 0 0 123 45 | | 612x792 | 123x45 |
| EPSF, box `0 0 abc 300` | | typecheck | warning, 612x792 |
| EPSF, box `50 50 50 300` | | configurationerror | warning, 612x792 |

- The DSC spec (Adobe TN 5001) defines `%%BoundingBox` as the box enclosing
  the document's marks, not its media; the media size is `%%DocumentMedia` or
  `setpagedevice`. So a letter document with 1-inch margins says
  `72 72 540 720`, and cropping it would change the page.
- The EPSF spec (TN 5002) requires the box in an EPS file. An importing
  program places and crops the graphic by it, which is what EPS viewers
  and converters do.
- Ghostscript's `-dEPSCrop` acts only on an EPSF header. A plain file
  keeps letter even with the flag.

Gallery manifests already say `page` equals the box (`scripts/gallery.py`),
so for the 12 tracked EPSF files the CLI now draws what the gallery shows.

**How it works.** `src/bbox.rs` (`page_box`) reads the header comments: the
lines after the first, up to `%%EndComments` or the first line that is not a
comment. Lines may end in CR, LF or CRLF. The first occurrence of each comment
wins. `(atend)` is looked up after the last `%%Trailer`, so a document embedded
before the trailer cannot supply it. A usable `%%HiResBoundingBox` wins, as in
gs. Each side is rounded to the nearest point, as gs does (it does not take
the ceiling). Values may be real numbers, and the origin may be negative. A box
that is malformed, empty, inverted, or larger than 8000 points produces a
`pscat: warning:` line, and the run goes on with the default page. For an MCP
call that line comes back as a `Warnings:` item.

The origin goes into the device's base CTM (`Gfx::set_page_origin`), so
`defaultmatrix`, `initmatrix`, `initgraphics` and the reset after `showpage`
all keep it. `clippath pathbbox` returns the box. `--bands` adds the origin
to `/BandBBox`, which is in default user space. SVG and PDF record device
coordinates, so their pages need nothing extra.

**Leaving room for #909.** `Gfx` records where the size came from
(`PageSizeSource::{Default, BoundingBox, Explicit}`). `setpagedevice
/PageSize` should replace a `Default` or `BoundingBox` page and reset the
origin to (0, 0). It should leave an `Explicit` page alone.

**Where it applies.** It covers the flag CLI's file, stdin and `-e` runs,
`--bands`, the sweeps, and the window run of a file. It also covers MCP
`render_postscript` and `start_render`, which share `render_plan`. For those
two the server works out the same page, so the pixel cap checks the canvas
the child will make.

It does not cover `--spool`, `--interactive` preludes, `pscat sheet`, the
`contact_sheet` MCP tool, or the subcommands, which make their own pages.
DOS binary EPS headers (`C5D0D3C6`) are not read either.
