# Warp: non-affine paths and decorated drawing

[`lib/warp.ps`](../../lib/warp.ps) bends PostScript. The language's own
transforms (`translate`, `scale`, `rotate`, `concat`) are affine: they map
straight lines to straight lines and can never bend an edge. Warp gets
around that by treating a path as data:

1. `wp-path` flattens the current path and reads it back with `pathforall`
   as records: `[/m x y]` for a moveto, `[/l x y]` for a lineto, `[/z]` for
   a closepath.
2. `wp-subdivide` splits every edge, closing edges included, into steps no
   longer than a given length, so there are points to bend.
3. A **point warp**, any quotation `( x y -- x' y' )`, is lifted with
   `wp-lift` and mapped over the records with `fp-map`.
4. `wp-emit` rebuilds the path, and you fill or stroke it as usual.

Because warps are plain quotations, `fp-compose` chains them and their
parameters are captured data. `wp-warppath` runs steps 1–4 on the current
path in one call.

```postscript
(lib/functional.ps) run
(lib/aspect.ps) run
(lib/warp.ps) run
/Helvetica-Bold findfont 40 scalefont setfont
newpath 10 30 moveto (BENT) true charpath
50 50 120 0.4 wp-lens 10 6 wp-ripple fp-compose 2 wp-warppath fill
showpage
```

## Point warps

Each constructor returns a quotation `( x y -- x' y' )`. Coordinates are in
whatever user space the path is read in.

| Constructor | Effect |
|---|---|
| `dx dy wp-shift` | translate |
| `sx sy wp-scale` | scale about the origin |
| `amplitude wavelength wp-ripple` | y moves by amplitude × sin(360 x / wavelength) |
| `cx cy r strength wp-lens` | bulge (strength > 0) or pinch (< 0) inside radius r. Keep strength below 0.5, or the disc folds over itself at its rim. |
| `cx cy r degrees wp-swirl` | twist by up to `degrees` at the centre, falling to nothing at radius r |
| `cx cy r0 degrees-per-unit wp-polar` | a horizontal strip becomes a ring: x runs clockwise from twelve o'clock, y outward from radius r0 |
| `x0 y0 x1 y1 x2 y2 x3 y3 w h wp-perspective` | projective map of the rectangle 0 0 w h onto any quadrilateral, corners counter-clockwise from bottom left |

A projective map keeps straight lines straight, so a perspective-only warp
needs no subdivision: pass a step longer than anything you draw, such as
`1e5`. The curving warps need a step of a few units at the size they are
drawn; smaller steps cost proportionally more points.

Warps that need names open their own scratch dictionary, so they never
define anything in the program they are warping.

## Path transforms

A **path transform** maps a whole record array, `( records -- records )`.
`warp wp-points` lifts a point warp into one. Path transforms compose with
`fp-compose` like anything else, and can do what a point warp cannot.
`cx cy k wp-unroll` is the inverse of `wp-polar`: angle becomes x and
radius becomes y. It keeps each path on the side of the twelve o'clock cut
where it starts, so a glyph that straddles the cut is not torn across the
page.

## Running an unmodified program through a warp

`wp-run` runs a file so everything it paints is bent:

```postscript
(lib/functional.ps) run
(lib/aspect.ps) run
(lib/warp.ps) run
gsave 0.4 dup scale
  (gallery/golden_bloom.ps) 350 350 300 0.4 wp-lens 3 wp-run
grestore
showpage
```

| Procedure | Inputs |
|---|---|
| `wp-run` | `program point-warp step --` |
| `wp-run-path` | `program path-transform step --` |
| `wp-decorators` | `path-transform step -- decorators` |
| `wp-run-decorated` | `program decorators --` |

`program` is a file name, or a procedure to execute in the same scope.

`wp-decorators` builds a dictionary of decorators, quotations
`( target -- wrapper )` in the sense of `aop-wrap-def`, keyed by operator
name:
- `fill`, `eofill`, `clip` and `eoclip` warp the current path, then call
  the real operator.
- `stroke` first converts the stroke to its outline with `strokepath`, so
  line weight and dashes bend with the shape, then fills that.
- `rectfill`, `rectstroke` and `rectclip` do the same for rectangles, with
  the operator given four numbers.
- `show` draws the string's glyph outlines (`charpath`) through the warp
  and leaves the current point where `show` would.

The coordinate space a warp works in is the one in force when
`wp-decorators` runs, so place the program with `translate`/`scale` before
calling `wp-run`.

`wp-run-decorated` opens a fresh dictionary and installs each decorator
with `aop-wrap-def` **before** the program is read. That order matters:
`bind` only replaces names that resolve to operators, and inside the scope
`fill` resolves to the decorated procedure, so even programs that bind
their procedures paint through it. Inside the scope `showpage`, `copypage`
and `erasepage` do nothing. When the program ends, by returning or by an
error, every dictionary it left open is closed. An error is then re-raised
with `stop`.

You can edit the decorator dictionary before running. *Blueprint of a
Rose* (`gallery/blueprint_rose.ps`) nulls the colour operators, pins the
line width and traces fills as outlines.

A decorator that needs a real operator must capture it, with `bind` or with
`systemdict /name get`. Inside the scope, the operator's name refers to the
decorated version, so a decorator that calls it by name calls itself
forever.

## Limits

- Images, `shfill` and other smooth shadings pass through unwarped.
- The array forms of `rectfill`, `rectstroke` and `rectclip` are not
  decorated, and neither are `ashow`, `widthshow`, `kshow`, `xshow` or
  `glyphshow`.
- A program that fetches operators straight from `systemdict`, or that
  `initmatrix`/`setmatrix`es away the placement, escapes the warp.
- Flattening makes curves into short lines. `setflat` controls how fine
  they are.
- Cost grows with the number of points: about 0.1–0.7 s for the gallery
  pieces built on this.
