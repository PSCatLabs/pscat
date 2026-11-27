## reversepath (issue #1266, 2026-11-26)

`reversepath` was undefined (found in the 2026-11-18 completeness audit). It is
now an operator with no operands: the current path walked the other way, each
subpath keeping its open or closed state. The usual use is a hole under the
nonzero rule: draw the inner circle, `reversepath`, `moveto` the outer circle's
start, draw it, `fill`.

Semantics, each pinned against gs 10.07.1 in `tests/reversepath.rs` (every
`pathforall` coordinate, curve control points included, and `currentpoint`):

- Subpaths come out last first; points within one are reversed; a curve's
  control points swap (`c1 c2 p` becomes `c2 c1 p_prev`).
- A closed subpath stays closed. The current point afterwards is the end of
  the last subpath written: the original start for an open one, the reversed
  subpath's own start (the original's last point) for a closed one.
- A `lineto`/`curveto` right after `closepath` starts a subpath at the closed
  one's start (as `pathforall` shows it), so it is its own subpath and gets a
  `moveto` in the result.
- A trailing lone `moveto` is dropped when the path has any other subpath, and
  kept when it is the whole path. That is gs's behaviour; the PLRM does not say.
- The empty path is not an error and still has no current point.

Implementation: `PsPath::reversed_limited` (src/gfx.rs). It finds the subpaths
in one pass, then writes the reversed copy through the path builders so
`current` and `subpath_start` come out as for any built path. Two reversals give
the original back, except that a trailing lone `moveto` is gone after the first
(and so the current point is the last drawn point, or the last closed subpath's
start, not that `moveto`); gs does the same, and the tests pin it. Under the limits it is built like `flattenpath`
(#1188): the copy and the subpath table are held against `--max-memory` before
they are made, the probe is looked at every 4096 segments in both passes (the copy counts
every segment it writes, movetos and closes too: a path of a million empty
subpaths is two million segments), and
a step is charged per 1024 segments written. The current path is replaced only
once the copy is whole, so a limit leaves it as it was.

Not done here: `ufill` and the other user-path operators (done in #1264, `notes/1264-hit-testing-and-user-paths.md`).
