# 1356: `pscat lib list` reads the marker with the checked opener

`list` used to call `read_to_string` on `lib/<name>/.pscat-lib`. A FIFO
there blocked it for good, and a symlink to `/dev/zero` made it allocate
without bound (found in Codex round 2 on #1331).

`read_marker` in `src/subcmd/lib_pkg.rs` now opens the marker with
`open_regular` (O_NOFOLLOW | O_NONBLOCK, regular-file check on the handle),
the opener `install` uses for what it copies, and reads at most
`MARKER_MAX_BYTES` (16 KiB; a marker is one line, the phrase plus the source
path). Anything else (missing, link, FIFO, device, over the cap, not UTF-8)
lists as "not made by pscat lib install", as a hand-made directory does.

Tests: `tests/lib_pkg.rs` runs `pscat lib list` as a child under a 10 s
watchdog (`output_within`, which kills the child and panics) with a FIFO
marker and with a `/dev/zero` link; both failed before the change. A unit
test pins the cap.

Left alone: `install` and `remove` only stat the marker (`is_file()`, which
follows a link to a regular file) and never read it.
