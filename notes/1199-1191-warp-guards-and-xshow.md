## warp: guards unwind by depth, and xshow, yshow and xyshow bend (issues #1199, #1191, 2026-11-12)

Two changes to `lib/warp.ps`, in one PR because both edit the show family's
guard.

### #1199: one guard, unwinding to the entry depths

`wp--try` (path painters) and `wp--guarded` (show family) cleared the operand
stack back to the nearest mark and never closed a dictionary. A path
transform that failed with its own `[` or `mark` open left the decorator's
internals on the stack; a closure warp (`wp-unroll` with a point at its
centre, `wp-polar`, `wp-lens`, `wp-swirl`, `wp-perspective`) that failed left
its scratch dictionary open, so the program's `y` resolved to the warp's.

Both now go through `wp--protect`, which is given `A = [saved n d k type]`:

- `n`, `d`: `count` and `countdictstack` at entry (below the call's own
  operands). On an error, `wp--rewind` pops back to `n` (a count, so a
  mark the warp pushed does not matter) and `end`s back to `d`, then the
  `saved` operands are pushed again, the gsave is undone and the error is
  re-raised with `stop`, so `$error` keeps its name and command.
- `k`, `type`: how many results the warp must leave and, for the path
  painters, that the one result is an array. `wp--shape` raises the undefined
  name `wp-warp-must-return-one-result` otherwise, inside the guard, so the
  same unwinding happens (the `{ 99 exch }` case, a transform that leaves
  nothing, a number).
- `A` is held by the executable array that runs the warp, not on the
  operand stack, where the warp can reach it. That array is built per call
  (`[ [ body exec A shape exec ] stopped A settle exec ]`); `wp--protect`
  is bound, so a decorated `exec` or `stopped` in the scope cannot change it.
- `clip` and `eoclip` now run the warp in a guard of its own, so the shape
  check sees the warp's result before it is wrapped with the saved path; one
  guard around both let a transform that popped records swallow the caller's
  operand into the wrapping array.
- Left alone, and documented in WARP.md: a warp's own unbalanced `gsave`
  (PostScript cannot query the gsave depth), and a warp that pops below its
  own operands (it ate the caller's, which nothing can return).

### #1191: xshow, yshow, xyshow (cshow needs no decorator)

- **Drawing.** `wp--positioned` cuts the text into glyphs with `wp--pieces`
  (the same cut as the rest of the family), builds `[piece dx dy]` records
  and, in `wp--guarded`, `charpath`s each glyph where the pen is, then moves
  the pen by the displacement with `moveto` in user space. The one path is
  filled once through the warp and the current point ends where the real
  operator leaves it. The displacements are user-space numbers and are not
  warped, as the glyph origins are not warped by the other operators.
- **Errors match the real operator without copying its checks.**
  `wp--dry-run` runs the real operator on the program's own string and
  displacements on the null device, from `0 0 moveto`, inside `stopped`.
  Anything it refuses (types, too few numbers, a non-number in the array, a
  bad encoded string, a missing font) goes to the real operator, which
  raises its error in gs's order and leaves the stack it leaves. A missing
  current point is checked separately, the same way. Type 3 text goes to the
  real operator too, as for the rest of the family.
- **Encoded number strings** are not decoded in PostScript. Decoding is 16
  representations (fixed point with any number of fraction bits, both byte
  orders, IEEE and native reals) plus the count rules; instead
  `wp--string-deltas` asks the real operator for the pen position after each
  prefix of the text on the null device, and takes the difference between
  neighbours as the glyph's displacement. That is quadratic in the text
  length (documented in WARP.md: use an array for long text), and the
  displacements carry the float noise of a user to device to user round
  trip (about 1e-13), which the tests do not see.
  Routing encoded strings to the real operator, unwarped, was the simpler
  option; it would have left those texts flat in an otherwise bent page.
- **cshow** paints nothing and moves nothing; the widths it reports are the
  font's, which a warp does not change; what its procedure shows goes
  through the decorated `show`. Tested (a decorator for it would only have
  copied the real operator).

### Tests (`tests/warp.rs`)

For both guards: `a_path_transform_that_fails_with_its_own_mark_open…`,
`a_closure_warp_that_fails_does_not_leave_its_scratch_dictionary_open`
(wp-unroll at its centre for the operators whose path has a point there,
and a closure warp that always fails for all eight), the wrong-result-shape
test, and `the_show_family_guard_unwinds_by_depth_too` (unroll at the first
point of a glyph, found by `charpath`, in every show operator). The
xshow family joins `SHOW_FAMILY` (identity warp, shift, current point and
stack against the plain operator, one entry with an encoded string), the Type 3
test, and the failing-operator test (errors against the real operator).
Also a Hangul case, a ripple, `cshow`.
