## pktrowelplane: bound the area outline scan (issue #401, 2026-10-14)

`pktrowelplane` flattens the current path and scans it with its own `pathforall` for the area's
extent, then clips to it. It never calls `walkpath` on that path, so #110's `wkmaxsegs` did not
apply, and #299 (which put the bound inside `pkscanclosed`) left it alone for that reason: a
different question, a budget for area presets. This is that question answered.

**Decision: area presets share the limit and the error.** The scan counts moveto, lineto and
closepath visits of the flattened outline (the same three `wkmeasure` counts, on the same
flattened path `walkpath` would see) and, past `wkmaxsegs` (50,000), raises
`walkpath-segment-count-exceeds-safety-limit`. Same name and limit as every walkpath preset, so a
caller sees one contract wherever the refusal comes from. A separate budget for areas would be a
second knob with no better number behind it: a legitimate area outline is a few dozen segments,
and 50,000 is already three orders above any drawn one.

**Where the raise sits.** The scan runs inside a `gsave` that holds the flattened path. An error
raised from the `pathforall` callback would leave that gsave level behind and hand the caller back
a flattened path, breaking "leaves the caller's path as it was". So the callback only sets
`tplover` and calls `exit` (a plain `exit` out of `pathforall`, which loops; checked in pscat and
Ghostscript), the existing `grestore` runs, and the raise comes after it. No new top-level name
(`tpln` and `tplover` are `tpl-` scratch names like the rest of the procedure, so nothing for
`ARTKIT_INTERNAL`); the closepath callback carries its own increment.

**What it does not buy.** `flattenpath` and `pathforall`'s snapshot of the path still cost O(n)
before the first callback; the early `exit` saves the rest of the scan, the clip and the row loop.
The caller also already paid O(n) to build the path, which is why this was P3. Measured before the
change (issue): 100,000 segments 0.09 s, 400,000 about 0.27 s, no refusal.

**Why it cannot refuse a legal outline.** The scan reads the flattened path, as `wkmeasure` does,
so an outline of at most `wkmaxsegs` visits is the same set `walkpath` would accept. Exactly
`wkmaxsegs` visits still fill; one more is refused.

**Tests.** Self-tests (lib/paintkit.ps): `pktrowelplane-bounds-its-area-outline` (exact boundary,
closed and open outlines), `pktrowelplane-refuses-a-huge-outline-before-any-row` (stubs `pktrowel`
and requires exactly its one empty-path validation call, so only a refusal from the scan passes),
`pktrowelplane-refusal-leaves-the-caller-as-it-was` (gray and a curve in the path survive the
refusal). Rust (tests/paintkit.rs): `pktrowelplane_refuses_a_huge_outline_before_any_row`
(400,000-segment outline, clean operand stack, one `pktrowel` call, gray and current point kept) and
`pktrowelplane_accepts_exactly_wkmaxsegs_outline_segments`. Checked to fail on base: with
`origin/main`'s `lib/paintkit.ps` the two Rust tests fail and three self-test blocks fail.
Existing renders: `identity_check.sh --loader lib/paintkit.ps` byte-identical, with a control
(an over-limit outline) that differs.

**Also (#397).** The `pwsrand` rationale comment said an `rrand` / `srand` / restore idiom is not
equivalent because every `srand` is forced under `--sweep-seed`, so the restore resets the caller's
stream. #202 made a restore of an `rrand` value exempt, so that half is no longer true. The comment
now says what still holds: a literal seed (as /Seed would be) is still forced to the sweep's value,
so the local seed would stop being the wash's own (checked: `5 srand rand` and `9 srand rand` give
the same number under `--sweep-seed 77`). The Ghostscript-portability bullet is unchanged.
