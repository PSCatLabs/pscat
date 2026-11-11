## `--lint` with `--bands` (issue #847, 2026-10-26)

`--lint` and `--lint-strict` used to be refused with `--bands`, so a banded
render script had to drop linting, and `pscat-mcp` left `--lint` off banded
calls (#843). Both work now.

### What lint observes

| check | reads | per band? |
|---|---|---|
| `page-count` (`%%Pages:` against emitted pages) | the source header, `gfx.pages().len()` | same in every band: `stitch` already refuses bands that emit different page counts |
| `gsave-imbalance` | graphics-state depth at the end | same, unless the program branches on `/BandIndex` |
| `stack-leak` (operand stack, dictionary stack) | the stacks at the end | same, unless the program branches on `/BandIndex` |
| `blank-page` | the page **pixels** and the per-page ink flag | **differs**: a band-aware program skips work outside `/BandBBox`, and image rasterization skips other bands' rows, so band 0's canvas (or its ink flag) can be empty when the page is not, e.g. a white sky band over painted ground |
| `run-error` | whether the run ended in an error | any band can fail |

So "lint is the same in every band, lint band 0" is true of the behaviour
checks and false of `blank-page`. Linting band 0's `Interp` whole would call a
band-aware program's page blank whenever its top rows are empty.

### What it does

- Band 0 runs `lint::check_band` (page count, gsave, stacks) in its own
  thread, since the `Interp` cannot leave it, and returns the findings
  (`BandOutput::lint`). `lint::scan_declared_pages` reads the same source.
- Every band returns its per-page ink flags (`BandOutput::had_ink`). After
  stitching, `lint::blank_page_findings` runs on the stitched pages with
  each page's flag OR'ed across the bands that ran: what a plain run sees.
- `run-error` is raised when any band failed, and lint prints after the
  `band i/N failed` report, so "see the report above" holds.
- One report, after the run, like #877's one stderr. It opens with
  `pscat: lint: note: --bands: ... band 1 of K (the top band) only ...`. The
  note is not a finding: `--lint-strict`'s footer doesn't count it and a note
  alone never fails a run.
- `--lint-strict` fails the banded run on findings (exit 1), as a plain run.

Known gap, in `--help`, the guides and the skill: a program that leaks, or
leaves a `gsave` open, only in a band other than 0 (a branch on `/BandIndex`)
is not caught. Linting every band would need a per-band report and a dedupe
rule; not done until someone asks.

### pscat-mcp

`render_run` passes `--lint` again for banded calls, so the `Lint:` block
returns. `split_lint_findings` keeps the `note:` line only when there are
findings for it to explain, so a clean banded render (or one whose only lint
line is the dropped `run-error`) stays silent, as a plain one does. Tests:
`a_banded_render_has_a_lint_block_only_when_there_are_findings`, and a unit
test of the splitter.

### Tests

`tests/bands.rs`: `lint_with_bands_*` (once and which band; blank page once;
blankness judged on the stitched page; band 0 findings once, strict exit; a
leak in another band unseen; an error in a lower band is not clean).
`refuses_what_it_cannot_honour` no longer lists `--lint`.
