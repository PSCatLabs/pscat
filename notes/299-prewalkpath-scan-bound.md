## pkribbon/pknib: bound the scans that run before walkpath (issue #299, 2026-10-14)

#110 put a source-segment bound (`wkmaxsegs`, 50,000) inside `wkmeasure`, so `walkpath` refuses a
pathological path in about 0.1 s. `pkribbon` (`pkscanclosed`) and `pknib` (an inline single-subpath
check) each ran their own `pathforall` over the whole path first, so the bound could not fire before
those scans had paid for every segment.

**The fix.** The bound moves into the one scan that is shared. `pkscanclosed` (lib/paintkit.ps) counts
moveto, lineto and closepath visits (the same three `wkmeasure` counts) and, past `wkmaxsegs`, runs
`cleartomark` and raises `walkpath-segment-count-exceeds-safety-limit`: same limit, same name, so a
caller sees one contract wherever the refusal comes from. `pknib`'s inline scan is replaced by a call to
`pkscanclosed` (the number of flags is the number of subpaths, and any true flag is a closepath), so the
guard lives in one place instead of being copied. `pkfeather` also calls `pkscanclosed` before its first
walk, so it is covered by the same change. `pkribbon` now writes `pkscanclosed /pkclosedflags exch def`
(was `/pkclosedflags pkscanclosed def`) so a refusal inside the scan does not leave the name on the stack.

**Why it cannot refuse a legal path.** The scan reads the path before `walkpath` flattens it, and a
curve counts once in the scan and many times flattened, so the scan's count is never above
`wkmeasure`'s. Nothing `walkpath` accepts is newly refused. Counting `curveto` in the scan was left out
for that reason.

**Measured** (release build, `usertime`, path construction excluded, refusal only; one run, ms):

| segments | pkribbon base / head | pknib base / head |
|---|---|---|
| 400,000 | 49 / 13 | 47 / 12 |
| 1,600,000 | 136 / 26 | 131 / 30 |

The head figures still grow a little with the path. That is not PostScript time: a bare `{ foo } stopped`
after building the same path costs 0 ms, so the residue is `pathforall` snapshotting the path before the
first callback, which no library code can avoid. Refusal no longer walks the path.

**Tests** (tests/paintkit.rs): `pre_walkpath_scans_refuse_a_huge_segment_count_before_walkpath` replaces
`walkpath` with a stub and requires pkribbon, pknib and pkfeather to raise the guard with a clean
operand stack and without reaching it. Deterministic, not timed (the #110 timing test already passed on
base, so it could not tell). Checked to fail on base, each preset on its own: with `lib/paintkit.ps` from `origin/main`, pkribbon,
pknib and pkfeather each get past their scan, call the stub and return without an error.
`pre_walkpath_scans_accept_exactly_wkmaxsegs_segments`: exactly `wkmaxsegs` visits still draw through
pkribbon and pknib, one more is refused.

**Not done.** No new top-level name (nothing for `ARTKIT_INTERNAL`). The other whole-path `pathforall`s in
paintkit are `pktrowelplane`'s (it flattens and scans the area outline for its extent, then clips to it;
it never calls `walkpath` on that path, so there is no #110 bound for it to be "before") and
`pkbrushedge`'s read-back of strokes that `pkfeather` itself produced (already bounded). A 400,000-segment
area outline runs `pktrowelplane` to completion in about 0.27 s (100,000: 0.09 s), unrefused. That is a
different question (a budget for area presets), filed as a follow-up.
