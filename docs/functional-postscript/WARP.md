# Warp: non-affine paths and decorated drawing

[`lib/warp.ps`](../../lib/warp.ps) bends PostScript. The language's own
transforms (`translate`, `scale`, `rotate`, `concat`) are affine: they map
straight lines to straight lines and can never bend an edge. Warp gets
around that by treating a path as data:

1. `wp-path` flattens the current path and reads it back with `pathforall`
   as records: `[/m x y]` for a moveto, `[/l x y]` for a lineto, `[/z]` for
   a closepath.
2. `wp-subdivide` splits every edge, closing edges included, into steps no
   longer than a given length, so there are points to bend. A closing edge
   becomes `[/l x y]` records followed by its `[/z]`.
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
The program sees the caller's operand stack as it was below `program`, so
a procedure can take operands and leave results.

`wp-decorators` builds a dictionary of decorators, quotations
`( target -- wrapper )` in the sense of `aop-wrap-def`, keyed by operator
name:
- `fill`, `eofill`, `clip` and `eoclip` warp the current path, then call
  the real operator. An open subpath is closed first, as these operators
  close it, so its closing edge bends too.
- `clip` and `eoclip` then put the original, unwarped path back, as the real
  operators leave the current path in place. Painting it afterwards warps
  it once.
- `stroke` first converts the stroke to its outline with `strokepath`, so
  line weight and dashes bend with the shape, then fills that.
- `rectfill`, `rectstroke` and `rectclip` do the same for rectangles, with
  the operator given four numbers.
- `show` draws the string's glyph outlines (`charpath`) through the warp.
  Like the real `show`, it keeps the current path and only moves the
  current point to where `show` would leave it.
- `ashow`, `widthshow`, `awidthshow`, `kshow` and `glyphshow` do the same,
  glyph by glyph, with each operator's spacing: `ashow`'s extra advance
  after every glyph, `widthshow`'s after the given character code, both
  for `awidthshow`. `kshow` calls its procedure between each pair of
  glyphs with their two codes on the operand stack, the current point
  after the first glyph, and the caller's operand and dictionary stacks
  below them, so a procedure that moves the current point or sets a font
  works as it does without the warp. `glyphshow` draws the named glyph.
  The text is cut into glyphs as the interpreter cuts it: one byte each,
  or one UTF-8 sequence each in a Unicode-mode font (the CJK and Thai
  faces).
- `xshow`, `yshow` and `xyshow` place each glyph where the program's numbers
  put it (in user space, so the displacements are not warped, but the glyph
  outlines drawn there are), then fill the outlines through the warp. The
  numbers are an array or an encoded number string. Whether the real
  operator would accept the operands is decided by running it, on the null
  device, from a made-up current point: if it raises an error, the real
  operator runs on the program's operands and raises it, in its order and
  with the operand stack it leaves, so there is no second copy of its checks
  here. An encoded number string is not decoded in PostScript: the same
  operator, run on each prefix of the text on the null device, says how far
  the pen moves after each glyph. `cshow` has no decorator: it paints
  nothing and moves nothing, and the widths it reports are the font's, which
  a warp does not change; the `show` calls in its procedure are the
  decorated ones, so that text bends.
- Text in a Type 3 font (the faces in `lib/fonts/`, `lib/hangul.ps`,
  `lib/handscript.ps`) is shown by the real operator, since `charpath`
  traces no Type 3 glyph. Its glyph procedures paint with the decorated
  `fill` and `stroke`, so it bends too. `kshow` makes that choice glyph by
  glyph, as its procedure may change the font.
- A show operator given bad operands, or run with no current point, hands
  them to the real operator, which raises its usual error and leaves the
  operand stack as it would without the warp. A warp that fails while a
  glyph is drawn leaves no `gsave` or glyph path behind.
- A warp that fails while `fill`, `eofill`, `stroke`, `clip`, `eoclip`,
  `rectfill`, `rectstroke` or `rectclip` redraws a path leaves the program
  where the call found it: the operand stack (a rect operator's `x y w h`
  are put back, as the real operators leave their operands on an error),
  the current path, the CTM, the dictionary stack and the `gsave` depth.
  The warp's own error is re-raised with its name and offending command
  (`undefinedresult`, `div`). The show family has the same guard, with one
  difference: a failing show operator, like the real one, does not give back
  its string. Both guards put the stacks back by depth, not to the nearest
  mark: they note how deep the operand and dictionary stacks were when the
  call began, and on an error pop and `end` back to those depths. So a
  transform that fails with its own `mark` or `[` open, or a closure warp
  (`wp-polar`, `wp-lens`, `wp-swirl`, `wp-perspective`, `wp-unroll`) that
  fails with its scratch dictionary open, leaves nothing behind, and the
  program's own names resolve as before. A warp that returns anything but
  the one result it should (a path transform that leaves two items, none, or
  something other than an array) is an error too, raised as the undefined
  name `wp-warp-must-return-one-result`, after the same unwinding. What a
  guard cannot undo is a `gsave` the warp itself opened and then failed
  with: PostScript cannot query the gsave depth, so that one level stays. A
  warp that pops items it was not given (below its own operands) has eaten
  the caller's operands, which no guard can return. A rect operator without
  four numbers on top of the stack (too few operands, a wrong type, the
  array form) goes to the real operator, which raises its own error or draws
  unwarped.

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
  decorated.
- A warp that opens a `gsave` of its own and then fails leaves that one
  level, as above.
- `xshow`, `yshow` and `xyshow` with an encoded number string run the real
  operator once per glyph on the null device to find the displacements, so
  they take time proportional to the square of the text length. Use an array
  for long text.
- Type 3 text bends only where its glyph procedures paint with names the
  warp decorates: a procedure that `bind`s `fill` before the warp is
  installed, or paints with `imagemask`, draws unwarped.
- `glyphshow` in a Unicode-mode font draws its glyph unwarped: those fonts
  look glyphs up by code, not through the `Encoding` the decorator uses to
  reach a glyph by name.
- A program that fetches operators straight from `systemdict`, or that
  `initmatrix`/`setmatrix`es away the placement, escapes the warp.
- Flattening makes curves into short lines. `setflat` controls how fine
  they are.
- Cost grows with the number of points: about 0.1–0.7 s for the gallery
  pieces built on this.
