## identity_check.sh --base-bin/--head-bin (issue #335, 2026-10-14)

Friction from the #323 and #338 runs: `scripts/identity_check.sh` rendered both
trees with one binary, so proving a `src/` change renders byte-identically
needed a hand-written wrapper that picked the binary by working directory, and
`--control` proved nothing in that setup.

What changed:

- `identity_check.sh BASE_WORKTREE --base-bin PATH --head-bin PATH`: the base
  tree renders with the first, the head tree with the second. Each job in
  `jobs.nul` now carries its own binary (6 NUL fields, `xargs -n 6`), so there
  is no per-tree branching in the render driver. Both flags or neither; not
  with the positional BIN; neither is built; a missing or non-executable path
  is exit 64. Byte-identical binaries get a warning. The default mode is
  unchanged (one binary, built if not given).
- `--control` in this mode is a driver that uses behaviour only the head
  binary has (for #338: `1 (one) def`, which the old binary rejects, so its
  canvas is blank there). It must differ between the trees, or exit 2, same as
  before; only the wording says "binary".
- `BASE_WORKTREE` must still be a separate tree (the `BASE = ROOT` guard
  stays): the base binary is built from a base worktree anyway, and a file new
  on the branch must stay `new`, not be rendered by the old binary.
- `scripts/build_base_bin.sh BASE_WORKTREE OUT_BIN` builds the release pscat
  inside the base tree and copies it out, because a shared target dir would let
  the head build overwrite `target/release/pscat`.
- `scripts/bench_compare.sh BASE_BIN HEAD_BIN FILE [N]`: interleaved rounds of
  base, head and base-again (the identical-binary control), child CPU time via
  perl `times`, min and median per arm. 37 lines, so it stayed in. Each round
  goes to stderr as it finishes, so a bench stopped early leaves a partial
  sample; CPU ticks are 10 ms, so use a file that takes a few hundred ms or
  more.
- `tests/identity_check.rs`: wrapper "binaries" log their cwd and exec the real
  pscat, so the tests assert the pairing (the base wrapper only ever ran in the
  base tree), the control differing (exit 0), the control not differing (exit
  2), a differing page (exit 1), the usage errors (exit 64) and the unchanged
  positional mode. The scratch dir falls back to `/tmp` when `TMPDIR` is inside
  the repo (the script refuses a scratch dir inside either tree).
- work-issue skill: SKILL.md step 5, pitfalls.md checklist item 4 (plus the
  stop-a-bench-early line from the issue) and finish.md's `EXTRA_WORKTREES`
  comment now name this as the standard proof for a `src/` change.

Not done: the issue's `--base-bin` could default the base binary to a build of
the base tree. Left explicit, since a cold build is minutes and belongs in a
background command the caller controls. Related: #331.

Real run (not just the wrapper tests): base worktree at `3d58985^`, base binary
from `build_base_bin.sh`, head binary built in the branch (`3d58985` plus these
scripts), control `/s 1 (one) def` then a `rectfill`, `--loader lib/artkit.ps`:
84 of 84 page renders identical across 68 files in 42 s, and the control differed
(the old binary stops at `def`).
