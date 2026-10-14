## hatch gains an optional /Mark path hook (issue #271, 2026-10-13)

Slice H0 of the drykit design (`docs/DRYMEDIA.md` 3.4 and 9; epic #232), prep for
#241 (charcoal hatching). drykit's `dkstroke` has to ink the hatch lines, with
every layer of one pigment inside one `dkbatch`, so `hatch` can no longer be the
thing that strokes them.

**Interface.** `<< ... /Mark { ... } >> hatch`:

- Each line's segment, after `/Wobble`, `/Trim` and `/Dropout` have decided it,
  is appended to the current path instead of being stroked. `/Mark` runs once at
  the end, with that path current, inside `hatch`'s own `gsave`.
- `hatchwidths` is bound to an array of the line widths, one per appended
  segment, in append order. With `/Density` a segment is one constant-bucket run
  (the same split as stroking). It is `def`ed in the dictionary current when
  `hatch` was called, like every other name `hatch` binds.
- `/Mark` must be a procedure (array with `xcheck`), else
  `hatch-mark-must-be-a-procedure`; `null` means unset. It is kept in a
  1-element array like `/Density`. The check is in the up-front operand block
  (#259), so it precedes the pre-flight loop, `srand` and `gsave`: a rejected
  call leaves the caller's random stream and graphics state alone and never runs
  the value (tested for a number, string, literal name, executable name,
  operator and literal array).
- `@param: /Mark [proc] ...` carries the `[proc]` unit.

**Design calls (the issue and `DRYMEDIA.md` leave them open):**

1. *Widths travel as the name `hatchwidths`, not on the operand stack.* The
   acceptance criterion names `/Mark { stroke }` as the control, which has to
   work unchanged, so nothing may be left on the stack for the hook. The
   alternative (pass the array and require `pop`) was rejected for that reason.
   #241 reads `hatchwidths` inside its hook and gives each width to `dkstroke`.
2. *`hatch` does not set a line width in Mark mode.* `/Mark` sees the caller's own
   linewidth. `/Mark { stroke }` therefore matches the default render exactly
   only when the caller's linewidth equals `/Width` and there is one angle and
   no `/Density`; with a ranged `/Width` or `/Density` the widths differ by
   design (that is what `hatchwidths` is for). The "apart from the width handling
   the design specifies" clause of the issue is this.
3. *`newpath` inside the `gsave`, in Mark mode only.* The issue says "append to
   the current path"; but the current path on entry is the caller's region
   outline (`<path> clip << ... >> hatch`), and `/Mark { stroke }` would ink
   it too. `pathbbox` has already read it by then, and `grestore` gives it back,
   so a second layered `hatch` still defaults `/BBox` from the same path.
4. *`/Mark` runs after the seed restore.* `hseeded { hsaved srand } if` comes
   before the hook, so a `rand` the hook calls is the caller's next `rand`
   and is not wiped by the restore. The hook runs before `grestore` so the
   collected path is current. The hook must not touch `h`-prefixed names
   (documented, as for `/Density`).
5. *Runs even with no surviving line* (`/Dropout 1`): an empty path and an empty
   `hatchwidths`. A `dkbatch` caller wants one call per `hatch`, not a
   conditional one.

**Implementation.** The three `setlinewidth newpath moveto lineto stroke` sites
(plain line; each density run; the last run) now compute the width and the four
coordinates in the same order as before and call one helper, `hkline`. With
`/Mark` unset it executes exactly the old operators in the old order. With it set
it appends `moveto lineto` and pushes the width via `hkpushw` (a 64-slot array
that doubles; amortized O(1) per line, not a copy per append). Every `rand` call
(`hkfrnd`) is still made at the same point, before the coordinates, so the
draw count cannot depend on `/Mark`.

**Measured.**

- *Random draws.* `tests/hatchkit.rs::mark_does_not_change_the_number_of_random_draws`
  seeds the caller (`7 srand`), calls `hatch` with no `/Seed` (a seeded call
  restores the stream, which would make the check vacuous), turns on every roll
  site (`/Angles` with two angles, ranged `/Width`, `/Wobble`, `/Dropout`,
  `/Trim`), and compares the next `rand`. The same comparison runs with
  `/Density` for the run-split sites. A control that changes `/Dropout` does move
  the next `rand`. Mutation check: making the ranged-width roll conditional on
  `hmarkon` makes the test fail, then reverted.
- *Same geometry.* One angle, `/Width 1`, caller linewidth 1: the whole page is
  equal to the default render under `/Mark { stroke }`, for two option sets
  (with wobble, dropout and trim; and plain). Multi-angle with ranged widths:
  ink within 50 percent (crossings anti-alias differently in one stroke than in
  separate ones).
- *Byte identity with `/Mark` unset.* See the PR for the file list; method as in
  `notes/259-hatch-options-operand-guards.md`.

**Tests.** A `%%SelfTest: hatch-mark-hook` block in `lib/hatchkit.ps`, and seven
tests in `tests/hatchkit.rs` (draw count, same geometry, once-and-path-not-region,
one width per line including the array growth past 64 and `/Density` runs, empty
case, restored stream, guard and untouched stream).

**Not changed / follow-ups.** `lib/artkit.ps` untouched (#110 is editing it).
`printkit` does not forward `/Mark`; #241 calls `hatch` itself.

**Byte identity, measured.** Merge-base `5d5bad6` (the branch was rebased onto it
first), a base worktree there, the same binary for both trees, each render run
from its own tree root, PNG only (SVG is nondeterministic, #261). Twelve files:
`examples/{etching_demo,halftone,hatching,printkit,stippling,surfacekit}.ps`,
`gallery/{nightfall_triptych,out_of_register,varsity_backs}.ps`,
`docs/drymedia/prototypes/layering.ps` and the artkit and paintkit selftest
drivers. That is every file in the repo that calls `hatch` or loads a library
that does, 32 PNGs: 0 differ under `cmp`, all exit codes 0. `scripts/identity_check.sh`
covers paintkit loaders only, so this was a hand loop.
Negative control: changing the candidate-centering divisor in `hatch` (`2 div`
to `3 div` for `/hkslack`) made `examples/hatching.ps`, `examples/printkit.ps` and
`gallery/nightfall_triptych.ps` differ, then reverted. `/Mark { stroke }` control:
`examples/hatching.ps` with `/Mark { stroke }` injected into every `hatch` call
renders differently (about 24 percent of the raw PNG bytes; the density panel's
widths and the caller linewidth, as designed). `examples/printkit.ps` builds its
options dict inside printkit, so it has no call to inject into.
