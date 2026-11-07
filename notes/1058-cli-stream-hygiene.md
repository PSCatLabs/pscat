## CLI stream hygiene: wrote report, EPIPE, window notice, bands + writes (issues #1058, #1059, #1035, #1045, 2026-11-06)

Five reports from the ArtColony batch, one CLI PR (#1060 is only investigated, see below).

- **#1058, the `pscat: wrote FILE` report is on stderr.** It was `println!`
  to stdout, glued to a program's own output (`xpscat: wrote w.pdf`). Now
  `eprintln!` in the flag CLI (`--png/--svg/--pdf`, `--contact-sheet`) and in
  `pscat chalk` and `pscat sheet`, matching `pscat typewrite`, which already
  reported on stderr. `docs/STABILITY.md` already calls stderr text
  reword-able and does not promise the stdout line. MCP drops the line from
  child stderr (`warnings_content`) and never read it from stdout.
- **#1059, a closed stdout is not a crash.** Rust ignores `SIGPIPE`, so a write
  to a closed pipe fails with `EPIPE`; `println!` turns that into a panic.
  `main.rs` now shadows `println!`/`print!` (defined before the `mod` lines, so
  `bands` and `subcmd` get them) with a writer that exits 141 (128 + SIGPIPE,
  what a shell reports for any tool a `SIGPIPE` killed) on `BrokenPipe` and
  still panics on other errors. For a PostScript program's own `print`/`=`,
  `Interp::write_stdout` records the `EPIPE` (`Interp::stdout_closed`); the
  headless run then skips the `ioerror` report, still writes the PNG/SVG/PDF
  and flushes `--allow-write` files, and exits 141. `SIGPIPE` is deliberately
  not reset to its default action: that would kill the process before the files
  are flushed. Not covered: `pscat fonts list | head` (`src/fontpacks/cli.rs`
  is in the library crate) and sweep frames, which still report an `ioerror`.
- **#1035, a notice before a window nobody sees.** A plain file run (or `-`)
  with no output flag opens a window. If neither stdin nor stdout is a terminal
  (an agent's shell, a script), `pscat` first prints
  `pscat: opening a window; pass --headless, --png/--svg/--pdf or -e for batch use`
  to stderr. A notice rather than a headless fallback, because a desktop
  launcher also has no terminal and does want the window. Not for `--spool`,
  `--interactive` or the REPL. The test is a pure function
  (`window_without_terminal`) unit-tested over the four combinations; an
  integration test would open a window.
- **#1045, `--allow-write` with `--bands`.** Each band is a full run of the
  program, so each now gets the same write roots (`Options::apply_write_policy`,
  shared with a plain run) and every band counts its writes in one
  `Arc<AtomicU64>` (`Interp::share_write_count`). `charge_write_bytes` does its
  check and add in one `fetch_update`, so two bands cannot both pass a check
  only one fits under. A band also flushes its open files at the end and
  fails if a write was lost, as a plain run does. Writes from bands are
  unordered and repeated once per band: a program should write from band 0
  (`BandIndex 0 eq`) or write a temporary name and `renamefile` it. Opening an
  existing file for `(w)` truncates it, so two bands writing identical bytes
  to one file end with that content. Documented in `--help`, the getting-started
  guide, the `pscat` skill's bands reference and `SECURITY.md`.
- **#1060, large program on stdin sometimes hangs.** Investigated, no pscat
  cause found; see the PR. Left open.
