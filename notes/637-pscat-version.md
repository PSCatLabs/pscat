# 637: pscat --version

`pscat --version` / `pscat -V` print `pscat X.Y.Z` (from `CARGO_PKG_VERSION`)
to stdout and exit 0, handled in `parse_args` beside `--help`, and are listed
in the usage text. `pscat-mcp` already had `-V`/`--version` (it prints
`pscat-mcp X.Y.Z`); it had no test, so `tests/mcp.rs` now pins both spellings
and that the flag exits without serving stdio.

`pscat fonts install --version TAG` is unaffected: `fonts` dispatches before
`parse_args`, so its own `--version TAG` option never reaches this arm.

Deferred (owner paths, so not edited here): use `--version` in
`scripts/package_release.sh`'s can-it-run probe and the outsider smoke test.
