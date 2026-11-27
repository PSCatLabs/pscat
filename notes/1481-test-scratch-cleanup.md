# #1481: test scratch directories clean up after themselves

## What shipped

- `common::scratch_dir` (`tests/common/mod.rs`) used to leave
  `<temp>/pscat-test-<pid>/` behind for ever. About 13,900 of them (about
  40 GB) filled an agent machine's disk during gates. Now, on unix:
  - the process removes its directory at exit with `libc::atexit` (a `Drop`
    in a `static` never runs at process exit). A process that is killed
    leaves its directory for the sweep. The handler checks the pid, never
    panics and ignores errors;
  - the first `scratch_dir` call starts a background thread (so a CPU-time
    test is not charged for it) that runs `sweep_stale_scratch`: it removes
    `pscat-test-<digits>` directories directly under the temp dir whose pid
    is gone (`kill(pid, 0)` says `ESRCH`; EPERM or an odd pid counts as
    alive), that were last changed at least an hour ago and hold no `.keep`
    file, at most 200 per process. It uses `symlink_metadata`, so a symlink
    or a file with that name is left alone, and `remove_dir_all` does not
    follow links inside;
  - `PSCAT_KEEP_TEST_TMP=1` (anything but empty or `0`) keeps the process's
    directories and writes `.keep` so a later sweep leaves them too.
- Directories are still outside the checkout, unique per test, present for the
  whole process (children can read them), and one process never touches
  another's: parallel test binaries each own their pid, and live pids are
  skipped.
- `tests/scratch_paths.rs` tests the sweep (dead pid removed; live, young,
  `.keep`, own pid kept; symlink, file, non-numeric names refused; the cap; a
  symlink inside a stale directory not followed), the exit cleanup and the
  keep variable, each against a private temp dir.

## Measured

`cargo test --profile ci --test patterns --test pdf --test limits --test
pdf_finish_memory_cli` with a private `TMPDIR`: before, 4 `pscat-test-*`
directories and about 165 MiB left; after, none. The writers:
`pdf_finish_memory_cli` (documents of 20 to 50 MiB, about 130 MiB per
process: `run` and the symlink test now delete their directory as soon as
they have read the file), `limits` (three 5 MiB `.bos` files, about 16 MiB),
`patterns` (shading SVGs of up to 10 MiB, about 16 MiB). A full run touches
about 120 binaries; the old leak was every run times those.

## Windows

`libc` is a unix-only dev-dependency, so Windows keeps the old behaviour (no
exit removal, no sweep). CI runners there are fresh machines.

## Other temp users

`scripts/` shell helpers (`check_package.sh`, `clean_install_test.sh`,
`font_pack_smoke.sh`, `plugin_install_check.sh`, ...) remove their `mktemp -d`
on exit; `identity_check.sh` keeps its renders only on a failure, on purpose.
`scripts/package_release.sh` leaves three tiny `mktemp -d` directories when it
exits early on a failed check (a cross-family path: proposed as a follow-up).
