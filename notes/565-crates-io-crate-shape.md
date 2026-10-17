## crates.io readiness: crate shape, features, public API, semver (issue #565, 2026-10-16)

Part of release epic #562. Makes `pscat` publishable and usable as a library;
the publish itself stays a manual owner step (#602). Nothing was published.

**Name.** `https://crates.io/api/v1/crates/pscat` answered "crate `pscat`
does not exist" on 2026-10-16, so the name is free. It is reserved only by
the first real publish.

**Crate shape.** One crate (lib + `pscat` + `pscat-mcp`), no workspace split.

**`window` feature.** Default on. `winit` and `softbuffer` are optional deps
behind it (still only on non-wasm targets). Without it `src/window_stub.rs`
replaces `src/window.rs`, with the same signatures, and returns an error
naming the fix. `main.rs` is unchanged, so `pscat file.ps`, `--interactive` and
`--spool` fail with that message, and every headless mode works. A downstream
crate with `default-features = false` has neither crate in
`cargo tree -e normal --target all`. The check also turns `pscat/window` on and
confirms winit then shows up, so a broken check cannot pass.

**cdylib.** Measured first. A scratch crate depending on the old
`crate-type = ["cdylib", "rlib"]` built `libpscat.dylib` (plus its codegen
units) in its own `target/debug/deps`. The library is rlib-only now, and
`scripts/build_wasm.sh` uses
`cargo rustc --lib --release --target wasm32-unknown-unknown --crate-type cdylib`.
The output path is the same, so `tests/wasm.rs`, `tests/site.rs` and the site
build are unchanged.

**Public API.** The supported surface is `Interp`, `Page` (new) and `PsError`,
in the visible modules `interp`, `page` and `error`, under
`#![deny(missing_docs)]`. Every other module is `#[doc(hidden)] pub`, so the
CLI, MCP server, wasm build and tests compile unchanged. On `Interp`, the
documented methods are `new`, `with_page`, `with_page_scaled`, `run_str`,
`run_source`, `begin_source`, `step_n`, `error_report`, `quit_requested`,
`set_seed_override`, `seed_override_fired`, `capture_stdout` and
`captured_stdout`, plus the new `pages`, `enable_svg`, `svg_pages`,
`enable_pdf` and `pdf_document`. These forward to `Gfx`, so `tiny_skia` does
not appear in the API. The rest (`push`/`pop*`, `load`/`define`, `gfx`/`gfx_mut`,
`lint*`, `break_permanent_dict_cycle`, ...) are hidden. `PsError` is
`#[non_exhaustive]` and every variant is documented; `PsError::from_name` is
hidden. `Page` wraps a `Pixmap` and has `width`, `height`, `rgba8` (straight
alpha), `to_png` and `save_png`. `to_png` is tiny-skia's `encode_png`, the
same call behind `pscat --png`. The CLI still uses its own PNG path, so golden
output cannot drift.
- Two `missing_docs` facts drove the layout, both checked in a scratch crate.
  The lint skips everything inside a `#[doc(hidden)]` module, and it also
  skips a `pub use` of an item from one. So supported items have to live in a
  visible module to be checked.
- rustdoc's `private_intra_doc_links` still fires inside hidden modules: 16
  doc links to private items were turned into plain code spans so that
  `-D warnings` passes.

**docs.rs.** `[package.metadata.docs.rs]` sets `no-default-features`, the
target `x86_64-unknown-linux-gnu` and `--cfg docsrs`. The window is hidden
anyway, so docs.rs doesn't build winit. The CI `docs` job runs
`RUSTDOCFLAGS="--cfg docsrs -D warnings" cargo doc --no-deps --no-default-features`,
and again with the defaults.

**Packaging checks.** `scripts/check_package.sh` runs these steps:
1. `cargo package`, then checks the `.crate` is under 10 MB.
2. Unpacks it outside the checkout and runs `cargo publish --dry-run --locked`
   there.
3. Builds a downstream `default-features = false` project from that tree (the
   `cargo tree` checks above), which renders PS to PNG through `Interp`/`Page`
   only, with `(lib/artkit.ps) run` from the embedded lib.
4. Runs `cargo install --path <tree>` with the defaults and renders with the
   installed binary.

The `crate (os)` CI job runs it on Linux, macOS and Windows, after
`cargo clippy --all-targets --no-default-features` and
`cargo test --profile ci --no-default-features`. The `packaging` job keeps the
licence allowlist check, and its old checkout-root dry run moved here.

**Semver.** The `semver` CI job runs `cargo semver-checks check-release
--baseline-rev <PR base>` on PRs. It is informational: `continue-on-error` is
on the step, so the check stays green and the findings go to the job summary.
There is no crates.io baseline before the first publish, and the version is
bumped at release time rather than per PR, so a blocking check would fail
every intentional API change. It flags this PR itself, because hiding
modules counts as removing public items. The policy is in
`docs/guides/library.md`: before 1.0 a minor release may break the API and a
patch release does not. After 0.6.0 is published, the release workflow (#602)
can run it blocking against crates.io.

**Not done.** No version bump (the release sets it, #602). No workspace split.
