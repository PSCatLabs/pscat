# #1343: SVG pattern defs are written once

Follow-up to `notes/1300-patterns-across-contexts.md` ("Gaps"). A page
pattern P used on the page and inside another tile T's `PaintProc` wrote
P's captured defs (its clips, with ids `t<P>_...`) into the page's defs
twice: once when the page used P, and again inside T's captured defs,
which the page copies in whole. Two siblings T1 and T2 that both used P
repeated them too. The copies were identical, so viewers drew the same,
but a repeated id is invalid SVG.

## Fix

`SvgTile` keeps its own defs (`defs`) apart from the captured defs of the
tiles it paints with (`imports`, by tile id). A recorder that first
paints with a tile imports that tile's own defs and its imports, each
once by id (`SvgRecorder::imported`), in either order and across any
nesting. The page writes imported chunks ahead of its own defs, and a new
page starts empty (`erase`). The `<clipPath>` and `<pattern>` a context
writes for a tile stay in that context's own defs (their ids carry the
context's prefix, so they never collide).

No render changes: references are by `url(#id)`, and SVG does not care
where in `<defs>` a def sits, so only the order of the defs text moved.

## Verification

`tests/patterns.rs` (`..._writes_its_defs_once`): page then tile, tile
then page, two sibling tiles, three levels, made-inside; each asserts
every `id` is unique and every `url(#...)` names a defined id.

Base (`f2b7c325`) against this branch, eleven programs (the new tests'
and the 1300 pictures, plus `examples/patterns.ps`): PNG and PDF are byte
for byte identical on all eleven, and resvg's raster of the SVG is
identical in every pixel (0 differing bytes of 612x792 RGBA each). The
base SVG repeated ids in five of them (1 to 2 each) and the head in none.
`scripts/identity_check.sh --base-bin/--head-bin` on `examples/patterns.ps`
and the image drivers: 20 of 20 pages identical.
