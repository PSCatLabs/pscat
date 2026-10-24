## Interpreter limits and fuzzing (issue #604, 2026-10-23)

**What shipped**
- `Limits { max_steps, max_time, max_memory }` (`src/limits.rs`), applied with
  `Interp::set_limits`. Default: none. `Limits::untrusted()` is the MCP preset.
- A limit stops the run with `PsError::StepsExceeded` / `TimeExceeded` / `MemoryExceeded` (`PsError::limit()` names the `Limit`). A program cannot
  catch it with `stopped`, and it is not recorded in `$error`.
- CLI: `--max-steps N`, `--max-time SECS`, `--max-memory MIB` (0 = off).
  Refused with `--spool` (it builds its own interpreters), and by the
  runs-alone self-test modes.
- MCP: `--max-steps`, `--max-memory` flags, on by default; the child's time
  limit is `--timeout` less a quarter, at most 5 s, so the render keeps the partial page
  and the call fails with `limit_exceeded` + `limit` instead of the `timeout`
  kill (still there as the backstop).
- `fuzz/` (cargo-fuzz: `lexer`, `interp`), seeds, a replay test, and a weekly
  `fuzz` workflow that opens an issue on a crash.
- Fixed: `N packedarray` with a huge `N` panicked (`capacity overflow`).

**Design**
- Steps: zero cost per step. `step_n_limited` caps the budget it passes to
  `step_n_inner` by the steps left. Loop iterations are also counted, from
  inside `next_item` (`tick`), because `{ } loop` and `1 1 N { } for` never
  return to `step_n`; that decrement-and-branch is the only cost an unlimited
  run pays.
- Time and memory are read once per `step_n` call (every 512 steps under
  `run_source` when either is set).
- Memory: a counting global allocator (`src/memtrack.rs`) counts live process
  heap bytes always (one relaxed atomic per alloc/free, in the `pscat` binary
  only); the limit is growth since `set_limits`. A memory limit without the
  allocator installed is an error, not a no-op. `array`, `string` and `aload`
  check a request against the limit before allocating.
- Counting always, not only while a limit is on, so a block allocated before
  tracking started and freed after cannot make the counter wrap.

**Defaults (MCP) and why**: measured every gallery piece (144 dpi) and example
(72 dpi) in a release build. Heaviest: `alpine_lake.ps`, 2.3 billion steps, 38 s
CPU; peak resident 0.42 GiB (`lincoln_charcoal.ps`). Steps 25 billion (about
ten times), memory 1024 MiB (2.4 times the heaviest resident peak, which
includes the canvas; a hosted service will want less), time 55 s. At about 60
million steps a second the clock binds long before the step cap; the cap is the
machine-independent bound.

**Not done / follow-ups**: limits for `--spool` and the subcommands; fuzz targets
for the font loaders and image decoders; a PDF reader target (no reader yet);
interruptible long operators (a whole-file read still runs to completion).
