# #487: test temp files are unique per call and per process

## What shipped

- `common::scratch_dir(tag)` in `tests/common/mod.rs` returns a new,
  already created directory `<temp>/pscat-test-<pid>/<tag>-<n>`. The pid
  separates processes (two `cargo test` runs, two worktrees); the counter
  separates calls inside one process (tests run on parallel threads). A file
  goes under it with `.join(name)`.
- Every `std::env::temp_dir()` use under `tests/` now goes through it. Two
  groups needed it: paths with a fixed name (`pscat_corpus_*.png` in
  `corpus.rs`, `pscat_gs_*.png` in `golden.rs`, `pscat_pdftest_*` in
  `pdf.rs`, `pscat_minisquare.pfa` in `type1.rs`, `etching_test_*.jpg`,
  `apparel_fill_*`, paintkit's oil-pressure file), and paths that had the pid
  but not the call (any two tests in one binary that picked the same tag). The
  local counters in `strokepath.rs` and `pathbbox.rs` are gone.
- `tests/scratch_paths.rs` fails if a test file (outside `common/`) mentions
  `temp_dir(` or a `"/tmp` literal outside a comment, so a new test cannot
  quietly bring a shared path back.
- If `TMPDIR` points inside the checkout (`target/`), the helper uses `/tmp`,
  which `identity_check.sh` needs (it refuses a scratch dir inside a tree).

## Left alone

- `scripts/` helpers already use `mktemp -d`; no fixed names.
- `src/spool.rs` and `src/bin/pscat_mcp.rs` tests already add the pid (and a
  counter).
- `tests/files.rs` writes `target/pscat-test-fixture-<pid>-<n>.ps` on purpose
  (a relative path under the worktree's own `target/`).
- No cleanup was added: each site keeps the cleanup it had.

## Check

Two `cargo test --profile ci` runs started at the same moment (18:24:33), in
two worktrees at the same commit, both finished green at 18:29:24: 1746 passed,
0 failed, 2 ignored each. The gs-backed tests (`corpus`, `golden`, `pdf`,
`type1`, `etching`) ran with `--nocapture` and printed no "skipping" line.
