## hatch checks its options before binding them (issue #259, 2026-10-13)

`hatch` in `lib/hatchkit.ps` had the same auto-execution hazard #123 fixed in
artkit's `scatter` (PR #262): it bound its options operand and each option
value with `def`, then type-checked them through the name. A name bound to a
procedure, an operator or an executable name runs on its next bare
reference, so a malformed value ran, and a raw `stackunderflow` (from
`known`) replaced a named guard.

**Measured first** (a probe that passes a side-effecting procedure, an
operator and an executable name in every slot, with `/tgot true def` as the
side effect). On `origin/main`, with a procedure in the slot:

- the options operand itself: ran, then raw `stackunderflow` on `known`;
- `/Angle /Angles /Spacing /Width /Wobble /Dropout /Trim /DensityThreshold
  /MaxLines /MaxSamples`: ran;
- an element inside `/BBox`, `/Angles`, `/Width` or `/Trim`: ran (this is the
  hatch version of the "hand-built region whose `/BBox` holds a procedure"
  residual #123 recorded);
- worst were the cases that ended with no error at all: a procedure that
  leaves a number (`/Spacing { 1 }`) was accepted, hatch drew normally, and
  the procedure had been run once per bare reference;
- `/Seed` never ran but failed with a raw `cvi` typecheck; `/BBox` as a
  procedure failed on `def`/`length`;
- operators and executable names failed with a raw error naming a scratch
  variable (`hopts`, `hkspacing`, `hang`).

After the fix none of those runs the value, and each raises its own guard.
`/Density` still runs: it is the one callable slot, already held in a
1-element array.

**Fix (the #123 pattern).** `hatch` checks each operand while it is on the
operand stack, then `def`s it. Three internal helpers, none of which can run
what they inspect: `hkisnum`, `hkliteralarray` (an array that is not
executable) and `hkallnums` (`forall` pushes elements without running them).

New guard names: `hatch-opts-must-be-a-dict`, `hatch-bbox-must-be-four-numbers`,
`hatch-angle-must-be-a-number`, `hatch-angles-must-be-an-array-of-numbers`,
`hatch-spacing-must-be-a-number`, `hatch-width-must-be-a-number-or-a-pair`,
`hatch-wobble-must-be-a-number`, `hatch-dropout-must-be-a-number`,
`hatch-trim-must-be-two-numbers`, `hatch-density-threshold-must-be-a-number`,
`hatch-seed-must-be-a-number`, `hatch-maxlines-must-be-a-number`,
`hatch-maxsamples-must-be-a-number`. The existing names
(`hatch-bbox-must-have-four-elements`, `hatch-trim-must-be-ordered-fractions-in-0-1`,
`hatch-density-must-be-callable` and the range guards) are unchanged.
Every check precedes the pre-flight loop, `srand` and `gsave`, so a rejected
call leaves the caller's random stream and graphics state alone (`/Seed` is
checked with the other options rather than where it is read).

**Behaviour change.** Newly rejected, all with a named guard instead of a raw
error or silent acceptance: an executable array where an array of numbers
belongs, a non-dict options operand, a wrong-length `/Width` or `/Trim`
(was a raw `rangecheck`), a non-number in `/Angles`/`/BBox`. A well-formed call
is untouched. Renders are byte-identical: 13 files that load hatchkit or one of its
dependants (`examples/{hatching,printkit,surfacekit,stippling,halftone,etching_demo}.ps`,
`gallery/{nightfall_triptych,out_of_register,varsity_backs}.ps`, two
`docs/drymedia/prototypes`, the artkit and paintkit selftest drivers) gave 29
PNGs from a base worktree (merge-base `b2e56e3`) and from this branch, same
binary, each run from its own tree root: 0 differ under `cmp`. Negative
control: changing the candidate-centering divisor in the branch's `hatch`
(`2 div` to `3 div` for `/hkslack`) made `examples/hatching.ps`,
`examples/printkit.ps` and `gallery/nightfall_triptych.ps` differ (the three
files that actually call `hatch`), then reverted. PNG only: SVG output is
nondeterministic (#261).

**For #271 (a `/Mark` hook).** Add the new option's check in the block at the
top of `hatch`, same shape: read it with `hkgetdef` (or `known`/`get`), keep
the value on the stack, `dup <check> not { hatch-<name>-... } if`, then `def`.
If the hook is meant to run, keep it in a 1-element array like `/Density`
and require `xcheck`.

**Tests.** Three `%%SelfTest` blocks in `lib/hatchkit.ps` (the library is now
listed in `selftest/libraries.txt`) and five tests in `tests/hatchkit.rs`;
each asserts the guard name and that the side effect never ran. They fail
against the `origin/main` library (4 of the 5 Rust tests, all 3 blocks).

**Not changed / follow-ups.** `scatter`'s own `/BBox` residual and `scarea`'s
raw `typecheck` live in `lib/artkit.ps`, which another change (#110) is
editing; they remain open on #259's body. Options that dependants
(`printkit`, `surfacekit`, ...) forward to `hatch` were not audited.
