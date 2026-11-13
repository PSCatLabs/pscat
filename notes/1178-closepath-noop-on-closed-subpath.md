## closepath on an already-closed subpath is a no-op (issue #1178, 2026-11-12)

PLRM 3rd ed. says `closepath` does nothing when the current subpath is already
closed (or there is no current point). pscat pushed a second `Close` segment,
so `pathforall` reported `z z` where Ghostscript reports `z`.

Fix: `PsPath::close` (src/gfx.rs) skips the push when the last segment is
already `Close`. A `lineto`/`curveto` after a close pushes a segment, so "last
segment is Close" is exactly "the current subpath is closed"; `moveto` pushes a
`Move`. The current point still returns to the subpath start. Every producer of
a close goes through `close`: the `closepath` operator, Type 1 glyph outlines,
`flattenpath` and the stroker's output.

What changes: a program that closes twice now emits one `Z` in SVG (was `ZZ`)
and one `h` in PDF content (was `h h`). Pixels do not change; the rasteriser
ignored the second close (tests/closepath_noop.rs renders both ways).

Tests: tests/closepath_noop.rs pins gs 10.07.1's traces and re-checks them
where gs is installed.
