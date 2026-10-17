# #630: tests/cli.rs `run()` flaked on BrokenPipe

`run()` wrote the program to pscat's stdin with `.expect("write stdin")`. A test
that passes bad arguments (`contact_sheet_without_a_sweep_axis_errors`) makes
pscat exit before it reads stdin, so the write raced the child's exit and
sometimes got EPIPE (seen on main CI for 3e2e5e0, ubuntu).

Fix: `common::feed_stdin(&mut Child, &[u8])` in `tests/common/mod.rs` writes
the bytes, closes stdin, and ignores `BrokenPipe` (any other error still
fails). `tests/cli.rs::run`, the six stdin sites in `tests/apparel.rs` and
`typewrite_reads_stdin` in `tests/subcommands.rs` use it; every caller still
asserts the exit status and stderr. `tests/mcp.rs` is left alone: it talks to a
long-lived server, where a broken pipe is a real failure.

`run_tolerates_a_child_that_exits_before_reading_stdin` makes the race certain
(about 1.2 MiB of stdin, more than a pipe buffer, against rejected arguments).
With the helper reverted to a plain `expect` it fails with `Broken pipe`.
