## `--lint` no longer calls an errored run "clean" (issue #369, 2026-10-14)

**Bug.** After a program raised an error, `pscat --lint` printed the
`%%[ Error: ... ]%%` report and then `pscat: lint: clean`. `report_lint`
judged only the finished interpreter state (blank page, gsave balance,
leaks) and never saw whether the run had succeeded. An errored snippet
with a clean-looking state (`-e pop`, or an error before anything leaked)
therefore read as fine, and agents trust that verdict line.

**The exit code was already right.** `finish_headless` exits 1 whenever the
run errored (`ok && !lint_failed`), under plain `--lint` and `--lint-strict`
alike. So this is a stderr-only fix and no caller's exit status changes:
`scripts/selftest.sh` and CI's PS-native step (`--lint-strict` over
`selftest/drivers/*.ps`) were already failing on a driver that errored.
New tests pin that exit code so it cannot regress.

**Fix.** `report_lint` takes `ok`. When the run errored it prints
`pscat: lint: [run-error] the run ended in an error ...` and never `clean`;
it still prints the real findings after an error (a blank page after a crash
is still worth seeing). The `--lint-strict` footer ("N finding(s) — failing
because --lint-strict was given") counts real findings only and is omitted
when the error is the only thing wrong, since the error fails the run on its
own.

**pscat-mcp.** `render_postscript` runs `--lint` and already leads with a
`PostScript error:` block for a failed run, so `split_lint_findings` drops
the `[run-error]` line (as it drops `clean`) instead of repeating the verdict
under `Lint:`. The two strings are kept in step by hand: `pscat_mcp` is a
separate binary and cannot see `main.rs`'s constant.

**Rendering is untouched** (nothing in this change touches the rasteriser or
the interpreter). Proof: `scripts/identity_check.sh --base-bin/--head-bin`,
see the PR.

**Deliberately unchanged.** `pscat::lint::check` (the library API used by
`tests/landscape_cookbook.rs`) still takes no run status: those tests
`panic!` on a failed `run_source` before linting, so they cannot reach the
bug. Only the CLI verdict line needed it.
