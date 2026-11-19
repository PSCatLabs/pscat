## The closed-stdout tests in tests/cli.rs (issues #1294, #1201, 2026-11-18)

Three tests in `tests/cli.rs` check what pscat does when the reader of its
stdout has gone: `a_closed_stdout_ends_quietly`,
`a_repl_stops_when_its_stdout_reader_goes` and
`only_the_error_from_the_closed_stdout_is_silenced`.

**The race.** A test that spawns the child with a piped stdout and drops its
read end afterwards races the child. If the child writes into the pipe buffer
before the drop, it never sees `EPIPE`. A REPL then writes its banner and
prompt, finds nothing wrong, and waits on its open stdin until the test
deadline kills it. That is the CI flake of #1294 (`crate (ubuntu-latest)` ->
"Tests without default features", runs 43158341398 and 43151418309). A short
`--capabilities` would exit 0 instead of 141 for the same reason (reproduced
here with a delay, not seen on CI). The other two tests did not hang only
because the child usually started later than the test's drop, or kept
writing: that is timing, not safety.

**Already fixed for one test.** #1186 (eb44371, 2026-11-12 21:55 PST) moved
`a_repl_stops_when_its_stdout_reader_goes` to close-before-spawn. The CI
failures of that test all predate it (runs 43151418309, 43158341398 and
43171090022, and 43366642609 on the #1186 branch itself), and no later failed
CI run names the test. #1294 had no further race to find in that test.

**This change (#1201 and the rest of #1294).** One helper,
`spawn_with_closed_stdout(args, stdin)`, makes the pipe with
`std::io::pipe()`, drops the read end, and only then spawns. All three tests use
it, so each sees `EPIPE` on the child's first write whatever the pipe's size
(it is the 512-byte kind on a starved Mac, 64 KB on Linux) and however fast the
child runs. The 200000-iteration loops stay: they still test that a program
stops quietly, but no longer depend on out-writing the pipe.

**Checks (2026-11-18, this Mac).** Each of the three tests, run 50 times as the
built test binary: 150/150 on a debug build, 150/150 on the ci profile, 150/150
under 8 busy processes, and without default features 150/150 plus 60/60 under 8
busy processes. The old pattern with a 200 ms delay between `spawn` and the drop:
`--capabilities` exited 0 instead of 141, and the REPL test hit its deadline once
the binary was warm. On a freshly built binary the child started slower than the
delay, so the old pattern passed twice (macOS delays the first run of a new
binary). The delay edit to `tests/cli.rs` was a scratch change, reverted before
committing; the helper has no such window.
