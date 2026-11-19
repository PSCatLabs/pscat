## Three tests that flaked on a loaded Mac (issue #1118, 2026-11-18)

Three tests failed locally, and never in CI, while this machine was busy:
`tests/mcp.rs::concurrent_replies_never_interleave`,
`tests/subcommands.rs::typewrite_refuses_an_output_name_it_cannot_pick_a_format_for`
and `tests/mcp.rs::review_sheet_works_under_the_smallest_timeout`. They had
two different causes. Both reproduce on an untouched `feature/0.7.1`.

**Pipe deadlocks (the first two).** On this Mac a pipe held 512 bytes, not
the usual 16-64 KB: the kernel falls back to that when system pipe memory is
exhausted (measured with a non-blocking write loop on `os.pipe()`; a
long-running stray `pscat` process was holding it). A test that is correct on
a normal pipe then deadlocks:

- `concurrent_replies_never_interleave` writes about 3 KB of requests before
  it reads anything. The server answers each `ping` as it reads it, so it
  blocks writing a reply nobody reads, stops reading stdin, and the test blocks
  writing stdin. The 60 s watchdog then killed the server. Fix: the `Server`
  harness reads the server's stdout on a thread and the tests take lines from a
  channel, so no test can block the server's output. This covers every test in
  `tests/mcp.rs`. A closed stdout now fails with "pscat-mcp closed its stdout"
  rather than a JSON parse error on an empty line.
- `typewrite_refuses_...` polled `try_wait` for the child to exit and read
  stderr only afterwards. The refusal prints its usage text, 2841 bytes, so the
  child blocked in its write, never exited, and the test blamed "typewrite
  blocked reading stdin". Fix: read stderr on a thread while waiting.

**First run of a fresh binary (the third).** `review_sheet` runs the sibling
`pscat` inside the server's 1 s budget. On macOS the first run of a freshly
built binary waits 1-8 s for the system's security assessment, using no CPU
(`time` shows 0.00 s user and 8 s real), so a test run right after a build
timed out. Fix: the test runs `pscat --version` first, outside the budget.
What is left is plain contention against a 1 s clock (3 misses in 40 runs
with 16 busy processes), so the test asks again after a `timeout` reply, up to
five times; any other reply, or five misses, fails it.

Not touched: `tests/cli.rs::a_repl_stops_when_its_stdout_reader_goes`, which
flaked twice on CI (issue comment). It is a different test and was not
investigated here.
