## Square caps draw no zero-length subpath under an ordinary CTM (issue #1215, 2026-11-14)

Under a similarity CTM the stroke is tiny-skia's own on the device path, and
tiny-skia draws a square for a zero-length subpath under square caps (about
0.39 px of ink for a `.4` pen). gs 10.07.1 draws nothing, for `moveto
closepath` and for `moveto lineto` to the same point. #1203 (#1196) had
already dropped these on its pen-space path (`stroke_space`); the device path
was left alone.

gs 10.07.1, `false setstrokeadjust`, 100x100 at 72 dpi, `-dGraphicsAlphaBits=4`,
ink summed over the page, a `.4` pen unless noted:

| program | butt | round | square |
|---|---|---|---|
| `50 50 moveto closepath` | 0 | 0.267 | 0 |
| `50 50 moveto 50 50 lineto` | 0 | 0.267 | 0 |
| `50 50 moveto 50 50 lineto closepath` | 0 | 0.267 | 0 |
| `0 setlinewidth`, `moveto closepath` | 0 | 0.267 | 0 |
| `4 setlinewidth`, `moveto closepath` | 0 | 14.15 | 0 |
| `[0 3] 0 setdash`, `moveto closepath` | 0 | 0.267 | 0 |
| `[0 3] 0 setdash`, a 20-unit line | 0.933 | 1.867 | 1.867 |
| 4 wide, two dots plus a 10-unit line | 40 | 82.4 | 56 (the line alone, 14 x 4) |

So a dot is dropped one subpath at a time, a real subpath's zero-length
*dashes* stay (7 of them on the 20-unit line, as in pscat), and a dashed point
subpath stays empty. (Butt-capped zero-length dashes paint 0.933 in gs, which
looks like gs's fill adjustment; pscat paints them as it always did, and this
change does not touch dashes.)

**Fix.** `Gfx::stroke_dropped` gives the current path less its point
subpaths under square caps (`None`, so no clone, when there are none or the
cap is not square). `stroke` and `strokepath` take it before anything else,
so the raster (device path or pen space), the SVG `d` data and the PDF path
all see the same subpaths. The rule moved out of `stroke_space`, which now
takes the path to map; it used to sit behind `stroke_space`'s early returns,
which is why the hairline (`0 setlinewidth`) and similarity cases missed it.
`PsPath::without_point_subpaths` returns `None` when nothing is dropped.
SVG needs the explicit drop: its spec strokes a zero-length subpath under
`stroke-linecap: square`, so a browser would paint the square.

Not changed: a hairline round dot reads 0.79 of ink in pscat and 0.27 in gs
(a one-pixel dot against gs's fainter one); a separate, pre-existing
difference.

**Tests** (`tests/stroke_anisotropic.rs`, last section): the four spellings of
a point subpath under none, a uniform scale and a rotation, at widths 0, .4
and 4: square caps paint nothing in the raster, the SVG and the PDF, and in
gs; butt caps paint nothing; round caps keep the dot (compared with gs at 4
wide); a mixed path loses only the dots (SVG `d`, PDF content, and the PDF
read back by gs); `strokepath` leaves nothing for a square-capped point and a
butt-capped one, and an outline for a round one; zero-length dashes of a real
line still draw as in gs, and a dashed point stays empty.

**Codex round 1 (PR #1220), two fixes.**

- *Points that collapse.* gs rounds every device point to 1/256 px, so a
  `25 25 moveto 25.000001 25 lineto` (or a step of .001) is a zero-length line
  there and paints nothing under square caps, under any CTM (a step of .003 or
  more is a line, caps and all). pscat's f32 points stay distinct, so the drop
  now compares device points on that grid. After mapping into pen space the
  path is filtered again for identical points, the old `stroke_space` filter,
  for points the mapping itself collapses.
- *A `lineto` after `closepath`.* It begins a new subpath at the closed point
  (as `pathforall` reports it), but `subpaths()` split only at a `moveto`, so
  `25 25 moveto 65 25 lineto closepath 25 25 lineto` kept a square cap at the
  closed point (bbox 23..65 against gs's 25..65). `subpaths()` now also ends a
  subpath at a segment that follows a close, and a kept subpath whose
  predecessor was dropped gets its start point back.
- Not changed: a degenerate *leading segment* of a longer subpath
  (`25 25 moveto 25 25 lineto 60 40 lineto`, square caps) makes tiny-skia add
  a cap at the start that gs does not draw. That predates this issue.
