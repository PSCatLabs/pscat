## Resource resolver and embedded lib/: no checkout needed after install (issue #598, 2026-10-16)

Part of release epic #562 (split from #564; the design came out of the
#564 comment thread).

**What was built.**
- `build.rs` writes `$OUT_DIR/embedded_lib.rs`, an `include_bytes!` index of
  every `lib/**/*.ps` (fonts/ and styles/ included, PNG specimens not).
- `src/paths.rs` is now a resolver over *names*, not root paths:
  `paths::open(name)` returns the bytes and the `Backend` that served them.
  `Roots` holds the disk locations and is injectable, so unit tests pin the
  order without touching the env. The order is: `$PSCAT_LIB` (lib/ only) →
  `$PSCAT_HOME`/`$PSCAT_ROOT` → cwd → per-user data dir → exe dir →
  embedded → checkout (debug-assertion builds only). It is documented once,
  for users, in `docs/guides/getting-started.md`.
- The callers moved onto it: `run`/`file` (`ops/file.rs`), the catalog
  (`paths::catalog_dir`, so `--fonts`/`--capabilities`), `selftest`'s
  `% @requires:` (ancestors first, as before, then the resolver),
  `pscat-mcp`'s `repo_root`.
- Wasm gets the embedded backend for free: `(lib/artkit.ps) run` now works
  in the playground build (`tests/wasm.rs` checks it). Size: the `.wasm` goes
  from 5.86 MB to 7.79 MB raw, 2.90 MB to 3.45 MB gzipped.
- `PSCAT_TRACE_RESOURCES=1` prints `pscat: resource <name> <- <backend>`
  for each file. `scripts/clean_install_test.sh` (CI job `clean-install`,
  macOS + Linux) runs `cargo install --path` with temp `HOME`/`CARGO_HOME`,
  renders the mathkit/drychalk/typewriter specimens from an empty dir
  outside the checkout, and requires every trace line to say `embedded`
  (plus at least one line per example, so broken tracing can't pass it). It
  also checks that `cargo package --list` still carries `lib/*.ps`.
  `scripts/package_release.sh`'s bundle check now asserts `exe-dir` the same
  way: with an embedded copy, "it ran" no longer proved the bundle's lib/
  was found.

**Decisions.**
- **Checkout heuristics are gone from release builds, as the issue says**
  (`cfg(debug_assertions)`; `profile.ci` keeps them, so the suite is
  unchanged). Fallout: `target/release/pscat` run outside the checkout no
  longer finds the checkout's `fonts/catalog/` or a fresher `lib/`.
  `handwrite.sh`, `typewrite.sh`, `photo_etch.sh`, `build_site.sh` and
  `build_font_gallery.sh` now export `PSCAT_HOME` to their own tree, so
  they behave as before. Tools that `cd` to the checkout (`identity_check`,
  `gallery/show.sh`, the perf bench) were never affected: cwd comes first.
- **`pscat-mcp` keeps one checkout fallback in release builds**: three
  levels above its exe, used only to find `scripts/handwrite.sh`, so the
  documented `claude mcp add pscat -- $PWD/target/release/pscat-mcp`
  keeps working. It goes away with #600 (script ports).
- **`$PSCAT_HOME` is the new name; `$PSCAT_ROOT` is an alias** at the same
  step, so existing setups keep working. It comes before the cwd (as
  `$PSCAT_ROOT` always did) because setting it is an explicit choice.
- **Per-user data dir before the exe dir and the embedded copy**, as the
  issue orders it. A stale `lib/` left in `~/.local/share/pscat` would
  shadow the binary's copy; the trace shows that. #599 decides what lives
  there (font packs).

**Deferred.** Font packs and `pscat fonts install` (#599); porting the shell
scripts so the MCP server needs only `pscat` (#600); extending
`clean-install` to Windows and making it a required check (#601).
