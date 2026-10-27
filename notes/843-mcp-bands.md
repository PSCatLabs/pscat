## `bands` in pscat-mcp (issue #843, 2026-10-26)

`render_postscript` and `start_render` take an optional `bands` (integer,
default 1), passed to the child as `pscat --bands N`. They share one input
schema, so `render_postscript` came free.

- **The cap is new: `--max-bands` / `PSCAT_MCP_MAX_BANDS`** (1..64, default
  4). The issue asked for "the server's existing per-call thread limit from
  #769", but #769 added no thread limit, only `--max-jobs` (jobs running at
  once) and `MAX_CALLS` (4 calls at once). So the cap is a new numeric option
  in `NUMERIC_OPTIONS`, which gives it the env variable for free (#756). With
  the default `--max-jobs 2`, two banded jobs take 8 threads; the worst case
  is (`--max-jobs` + 4) x `--max-bands`. The schema's `maximum` is the
  server's cap, and a larger value is refused (`invalid_argument`, naming
  the flag) rather than lowered, like every other out-of-range argument.
  `--max-bands 1` turns banded rendering off.
- **`bands: 1` is a plain render**, not `--bands 1`, so it keeps the
  `Lint:` block and a job's `pages_done`.
- **What `--bands` refuses, mapped** (`bands.rs` `check_options`): of
  `--svg`, `--pdf`, `--lint`, `--progress`, `--profile`, `--allow-write`
  only the format comes from a tool argument, so `bands > 1` with `svg` or
  `pdf` is refused up front. `--lint` and `--progress` are the server's own
  flags and are left off a banded run: no `Lint:` block, and `pages_done`
  stays 0 until the job ends. No tool passes `--profile` or
  `--allow-write`. More bands than the page's device rows (which
  `bands::run` refuses after the child starts) is refused before any job.
- **`--max-time`.** The child's `--max-time` (from `--timeout` or
  `--job-timeout`, less the margin) and `--max-steps` apply to each band;
  `--max-memory` to all of them; the first band to hit a limit stops the
  queue (#844). The server's hard kill still covers the whole child. With
  about 2 bands per thread queued, bands that each come close to the
  per-band time limit can add up past the kill, which then returns
  `timeout` without a partial page. Documented rather than changed: dividing
  the time limit between bands would make the per-band limit depend on the
  queue's order.
- **No `band_rows`.** `--band-rows R` stays CLI-only. The default split (2
  per thread, at least 8 rows) measured best in #844; a small R costs up to
  18x the CPU (one row per band), and a large K multiplies the per-band
  step and time budgets an MCP caller can spend. An agent has nothing to
  measure it by, and the image is the same either way.
- **Errors** read as for a plain render, with the partial page: the
  `PostScript error:` text gains a `pscat: band i/K failed:` line before the
  usual `%%[ Error: ... ]%%` (checked by hand).

Test: `tests/mcp.rs` `start_render_with_bands_matches_a_plain_render` (a
band-aware fbm2 colorimage, `bands: 4` against `bands: 1`, through
`start_render` and `render_postscript`, plus a control program that paints
differently when `/BandCount` is defined, so the test can't pass with
`bands` dropped) and `bands_refusals_name_the_argument`.
