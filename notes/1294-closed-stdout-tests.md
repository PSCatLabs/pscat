## The closed-stdout tests in tests/cli.rs (issues #1294, #1201, 2026-11-18)

Three tests in `tests/cli.rs` check what pscat does when the reader of its
stdout has gone: `a_closed_stdout_ends_quietly`,
`a_repl_stops_when_its_stdout_reader_goes` and
`only_the_error_from_the_closed_stdout_is_silenced`.

**The race.** A test that spawns the child with a piped stdout and drops its
read end afterwards races the child. If the child writes its output into the
pipe buffer before the drop, it never sees `EPIPE`. A REPL then writes its
banner and prompt, finds nothing wrong, and waits on its open stdin until the
test deadline kills it (the CI flake of #1294: `crate (ubuntu-latest)` ->
"Tests without default features", runs 43158341398 and 43151418309). A short
`--capabilities` would exit 0 instead of 141. The two other tests escaped only
because they print far more than a pipe holds.

**Already fixed for one test.** #1186 (eb44371, 2026-11-13 UTC) moved
`a_repl_stops_when_its_stdout_reader_goes` to close-before-spawn. Both CI
flakes in #1294 are from 2026-11-12, before that commit, and no later failed CI
run names the test. #1294 had no further race to find.

**This change (#1201 and the rest of #1294).** One helper,
`spawn_with_closed_stdout(args, stdin)`, makes the pipe with
`std::io::pipe()`, drops the read end, and only then spawns. All three tests use
it, so each sees `EPIPE` on the child's first write whatever the pipe's size
(it is the 512-byte kind on a starved Mac, 64 KB on Linux) and however fast the
child runs. The 200000-iteration loops stay: they still test that a program
stops quietly, but no longer depend on out-writing the pipe.

**Checks.** Each test, run 50 times as the built test binary (with and without
default features, some under load), plus a scratch copy with the old pattern
and a 200 ms delay between `spawn` and the drop, to see the old race fail and
the helper survive the same delay. Results are in the PR.
