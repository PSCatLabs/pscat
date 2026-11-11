# #701: limits for --spool and the subcommands

`--max-steps`, `--max-time` and `--max-memory` (#604) applied to plain renders
and the MCP server only. They now reach `--spool` and the five tool
subcommands (`handwrite`, `typewrite`, `photo-etch`, `chalk`, `sheet`).

## Per run, not per page

The main CLI's unit is one interpreter run: a sweep frame, a `--bands` band
(steps and time; `--max-memory` is shared by the bands) and a `--spool` job each build a fresh `Interp` and get the whole budget
(`Interp::set_limits` before `begin_source`, which restarts the step count and
the clock). The subcommands follow that:

| Command | A run is | Budget |
|---|---|---|
| `--spool` | one job | whole limits per job, memory baseline taken after the previous job is freed |
| `handwrite`, `typewrite --height fit`, `photo-etch` | the measuring pass, then the render | each pass gets the whole limits; worst case about 2x `--max-time` |
| `chalk` | the whole board (its measuring pass is inside the program) | one budget |
| `sheet` | one cell | whole limits per cell; a sheet's worst case is cells x limit |

A page is not a run: a multi-page `typewrite` document is one run. The memory
limit counts heap growth since the interpreter was set up, so `sheet`'s
accumulated canvas and `handwrite`'s first pass do not count against the next.

A subcommand that passes a limit prints the interpreter's usual
`%%[ Error: limitexceeded (steps) ... ]%%` and exits non-zero; `chalk` writes
no file (as for a TeX error), `sheet` still draws the failed cell as far as it
got (as for any failed cell).

## Code

- `main.rs`: `limit_flag` and `is_limit_flag` hold the one parser for the three
  flags (the flag CLI and the subcommands share it, so `0 = no limit` and the
  ranges match). The `--max-steps` help says whole-file reads charge one step
  per KiB (from #702).
- `subcmd/mod.rs`: `Confine { read_roots, limits }` replaces the bare
  `read_roots` argument of `measure` and `render`; `take_limit_flag` is the
  subcommand parsers' one-line hook.
- `chalk.rs`: `render_with_limits` and `run_with_limits` (additive; `render`
  and `run` are unchanged).
- `window.rs`: `run_spool_with_limits` (additive; `run_spool` is unchanged).
  `start_job` frees the previous job's interpreter before `set_limits`. A
  window job runs `steps_per_frame` per frame, so `--max-time` counts that
  pace; documented in `--help` and SECURITY.md.
- `mcp_server.rs`: `handwrite` takes `interpreter_args` (limits, read roots,
  `--exit-with-parent`) like every other child, and a stop is `limit_exceeded`
  through `failed_run`, not `handwrite_failed`. Before, only `--timeout`'s kill
  bounded it.

## `--max-pixels` routes into `pscat handwrite`

#711 asked that `--max-pixels` reach every route into `pscat handwrite`. There
is one: the MCP `handwrite` tool, which passes it (`scripts/handwrite.sh` forwards
its arguments untouched). The other tools run the flag CLI, not `pscat
handwrite`, and check pixels in the server. Nothing to add.

## Tests

`tests/subcommand_limits.rs` stands a library of `{ } loop` files in for
`lib/` (`$PSCAT_LIB`): the subcommands' arguments cannot carry PostScript, so
that is the way to give them a real runaway. A watchdog kills the child and
fails the test if a limit stops working. `tests/mcp.rs` does the same through
the server for `handwrite`. `window.rs` unit-tests the spool path
(`poll_spool` to `tick`) without a window.

## Not done

- `review_sheet` could now share `pscat sheet`'s path, but the server wants one
  `--timeout` budget across cells, which `pscat sheet` does not have.
