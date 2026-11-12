## CI installs Ghostscript and requires it (issue #985, 2026-11-11)

**Why.** 44 test files ran `Command::new("gs")` and, when it was missing,
`eprintln!`ed and returned. libtest hides that output for a passing test and
CI installed no gs, so no log could say whether the parity checks ran.

**What changed.**
- `tests/common/mod.rs`: `gs_available()` is the one skip rule. gs runs: true.
  gs missing: a `SKIPPED (gs not installed): <test>` line written straight to
  stderr (not `eprintln!`, so capture cannot hide it), then false. gs missing
  and `PSCAT_REQUIRE_GS` set (non-empty, not `0`): panic.
  `PSCAT_GS_LOG=<file>` appends `ran <test>` / `skipped <test>`.
  `gs_decision` is the pure part, covered by `tests/gs_helper.rs`.
- All 44 files use it; each local `gs_available` and ad-hoc probe is gone.
- `.github/workflows/ci.yml`: the macOS `test` job (`brew install ghostscript`)
  and the Linux `cross-platform` job (apt) log whether the runner image ships
  gs, install it, log `gs --version`, set `PSCAT_REQUIRE_GS=1` on the test step
  and print the ran/skipped counts (`scripts/gs_parity_summary.sh`) to the job
  summary. Windows skips visibly (binary is `gswin64c`); the `packaging` job's
  `--no-default-features` run also skips visibly.

**Not converted here** (held PRs add their own probes; switch them to
`common::gs_available()` after they merge): see the PR description.
