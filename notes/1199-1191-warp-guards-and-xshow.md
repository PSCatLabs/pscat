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
- **Encoded number strings** are read with `token`: since #1192 (PR #1195,
  merged while this was in flight) pscat scans token type 149, so
  `wp--numbers` turns the string into the array it holds (the real operator
  has already accepted it in the dry run). The scanner reads it only while
  the object format is not 0, so for a program that set `0 setobjectformat`
  the format is raised to 1 for the scan and put back (gs gates it the same
  way: see the runs below). The first version
  asked the real operator for the pen position after each prefix of the text
  and took differences (no decoder needed, but quadratic and with float
  noise); `token` is simpler, linear and exact. Routing encoded strings to
  the real operator, unwarped, would have left those texts flat in a bent
  page.
- **The dry run keeps no state on the operand stack.** `wp--dry-run` records
  the depth (`count`) before it runs the operator and `wp--dry-settle` pops
  back to it afterwards, so it does not depend on pscat dropping the operands
  of a failing operator (#1198 would change that, and gs leaves them). The
  operator and its copies sit in the array that runs it. The error-parity
  tests compare the decorator with pscat's own operator, whose behaviour on
  an error is "operands dropped"; gs leaves `(AB) [1]` (below).
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

### Ghostscript 10.07.1 runs behind the decisions

```
$ gs -q -dNODISPLAY -dNOPAUSE -dBATCH -dNOSAFER t.ps
{ 0 0 atan } stopped { $error /errorname get == $error /command get == } if
/undefinedresult
--atan--                        (pscat: /undefinedresult, atan)
9 { (AB) [1] xshow } stopped { $error /errorname get == } if pstack
/rangecheck                     (gs leaves  (AB) [1] 9 ; pscat drops (AB) [1], #1198)
[1]
(AB)
9
currentobjectformat ==   2
(\225\060\000\002\100\000\000\000\100\200\000\000) token pstack      % object format 2
true
[2.0 4.0]
()
0 setobjectformat   (same string) token pstack                              % object format 0
true
<the string read as text: a name token>
```

So `0 0 atan` is `undefinedresult` in gs too (the closure-warp failure the
tests rely on), `token` reads a 149 string as the number array `[2.0 4.0]`
and, with the object format at 0, does not (pscat's lexer gate is the same,
which is why `wp--numbers` raises the format around the scan). The decorated
operators match pscat's own operator, not gs's, on the operands left after an
error: the tests' contract is the native pscat operator, and #1198 is the
open question about making pscat leave them as gs does.

### Review round 1 (PR #1202)

- Executable operands are data. The decorators stored the text and the
  numbers with `def` and called them by name, so `(ab) {1 2} xshow` ran the
  array and `(ab) cvx [1 2] xshow` ran the string (the latter also broke
  widthshow, awidthshow and kshow on base). `cvlit` now on every stored
  operand (the glyph cut, `wp--awidth-path`, `wp--kshow`, `wp--positioned`,
  `wp--array-deltas`) and on the copies the dry run executes; the program's
  own operands are untouched when handed to `wp--or-real`.
- Errors routed to the real operator named `systemdict`, then `[`, as the
  offending command: pscat reports the last name it executed. The decorator
  code now uses `//systemdict`, `//true` and `//false`, and the dry run builds
  its arrays with `array astore`, not `[ ]`. The error tests compare the
  command as well as the error name. One difference stays: a kshow procedure
  that leaves no current point raises `nocurrentpoint` with command `kshow`
  from the decorator, where pscat's real kshow reports the last name it ran
  (`newpath`); gs names kshow.
- `wp--numbers` restores the object format when `token` raises.
