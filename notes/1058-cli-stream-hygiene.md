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
  `bands` and `subcmd` get them) with a writer that, on `BrokenPipe`, sets a
  process-wide flag and drops the text; `main` returns 141 (128 + SIGPIPE, what
  a shell reports for any tool a `SIGPIPE` killed) when the flag is set. It does
  not exit at the first failed write, because the exit paths that follow (the
  page, the flush of `--allow-write` files, which `SECURITY.md` promises on
  every exit) still have to run. For a PostScript program's own `print`/`=`,
  `Interp::write_stdout` records the `EPIPE` (`Interp::stdout_closed`); a
  headless run, a `--bands` band and the REPL then skip the `ioerror` report
  and set the same flag. The report is skipped only for the error that came
  from the failed write: `Interp::recover` clears the mark when a `stopped`
  catches an error, so a later, unrelated `ioerror` is still reported. The
  REPL also leaves at once, as nobody reads its prompts (Codex review, round 1).
  `SIGPIPE` is deliberately not reset to its default action: that would kill
  the process before the files are flushed. Not covered: `pscat fonts list |
  head` (`src/fontpacks/cli.rs` is in the library crate) and sweep frames,
  which still report an `ioerror`.
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
  One caveat: `writereals` and `writehexstring` check their whole size, then
  charge per chunk, so under `--bands` another band can spend the budget in
  between and the file is truncated and partly written before the
  `limitcheck` (the operators are outside the CLI; noted in `SECURITY.md`).
- **#1060, large program on stdin sometimes hangs: not a pscat bug.** The
  issue's Node `execFileSync` loop reproduced it here under CPU load (3 of 240
  runs at 70 KB; a second pass hung 8 of 400, load: 12 busy loops). Each hung
  `pscat` was sampled with `sample(1)`: all 8 were one thread parked in
  `read(0)` inside `read_source` (`read_to_end`), stdin a unix socket, so pscat
  was waiting for bytes or an EOF the writer never delivered; the interpreter
  had not started. The same Node harness with a plain Python reader of stdin
  (`sys.stdin.buffer.read()`) also hung (5 of 400), `cat` did not (0 of 400),
  and Python's `subprocess` as the writer never did (0 of 240, pscat 90 to 150
  KB). So it is Node's spawnSync stdin delivery racing a child that is slow to
  start, not pscat's read path, which is a plain `read_to_end` to EOF. No
  change made; the reporter's workaround (write the program to a file) is the
  right one.
