# #515: `currentrgbcolor` rounds its readback like gs (2026-10-14)

Follow-up to `notes/505-grestore-bottom-currentgray.md` (which left
`currentrgbcolor` alone) and `notes/513-readback-operators.md`.

gs prints `0.3 0.6 0.9 setrgbcolor currentrgbcolor` as `0.3 0.6 0.9`; pscat
printed the widened f32s `0.30000001192092896 0.6000000238418579
0.8999999761581421`. `currentrgbcolor` now pushes each component through
`f32_readback` (`src/ops/graphics.rs`), as `currentgray`,
`currenthsbcolor`, `currentcmykcolor` and `currentcolor` already do.

A read-then-set round trip still lands on the same `f32`, so
`currentrgbcolor setrgbcolor` changes nothing (tested in
`tests/grestore_boundary.rs`). What does change is arithmetic on the value:
`lib/paintkit.ps`, `lib/drykit.ps`, `lib/drychalk.ps` and
`lib/drycharcoal.ps` do lerps on `[ currentrgbcolor ]`, and the rounded
inputs can differ from the widened ones by a few f32 ulps.

Pinned in `tests/grestore_boundary.rs` (`[0.3 0.6 0.9]`, and
`[0.123 0.456 0.789]`, both checked against gs 10.07.1).

## Identity and regenerated renders

`scripts/identity_check.sh --base-bin/--head-bin` over every tracked
`.ps`/`.eps` (195 files, 212 pages, examples and drivers at 72 dpi,
gallery at 144 dpi), with `--gallery-still` and a control that turns the
readback into a green or red fill. 211 of 212 pages were identical; 8 files
fail identically in both trees (libraries that need a driver, and
`tw_proto.ps`) and were not compared. The only page that differs is
`gallery/alpine_lake.ps`. A second pass at 144 dpi over the non-library
files (examples, drivers, docs prototypes, site) found nothing else. The
eleven landscape-cookbook snippets render byte-identically in both
binaries too (their thumbnails are not in the `.ps` corpus, so they were
checked separately). `evening_stream` is identical.

`gallery/renders/alpine_lake.png` (2x, README command: `2 2 scale`
prepended, `--page 1520x1120`) changes in **one pixel, one channel, by one
level**: (759, 1016) of 1520x1120, G 111 to 110 (RGB 104,111,136 to
104,110,136). The base binary's render by the README method equals the
committed still, so the regeneration pulls in nothing else.
