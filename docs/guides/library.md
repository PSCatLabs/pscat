# pscat as a Rust library

The `pscat` crate is the interpreter behind the command-line tool. A Rust
program can depend on it to render PostScript to PNG, SVG or PDF without
running the `pscat` binary. The API reference is on
[docs.rs/pscat](https://docs.rs/pscat); this page covers setup, features and
the stability policy.

## Add the dependency

```toml
[dependencies]
pscat = { version = "0.6", default-features = false }
```

`default-features = false` turns off the `window` feature, which is the live
window behind `pscat file.ps`, `--interactive` and `--spool`. A library has no
use for it, and leaving it on pulls `winit` and `softbuffer` (and, on Linux,
their X11/Wayland stack) into your build.

## Render a program

```rust
use pscat::Interp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A 300x200-point page at 144 dpi: 600x400 pixels.
    let mut interp = Interp::with_page_scaled(300, 200, 2.0).ok_or("bad page size")?;
    if let Err(e) = interp.run_str(
        "(lib/artkit.ps) run
         0.2 0.4 0.9 setrgbcolor 20 20 260 160 rectfill
         showpage",
    ) {
        return Err(interp.error_report(&e).into());
    }
    for (i, page) in interp.pages().iter().enumerate() {
        page.save_png(format!("page-{}.png", i + 1))?;
    }
    Ok(())
}
```

- `Interp::pages()` returns one `Page` per `showpage`, plus the canvas if the
  program drew after its last `showpage`, the same pages `pscat --png` writes.
  A `Page` gives you `width()`, `height()`, `rgba8()` (straight RGBA bytes),
  `to_png()` and `save_png()`.
- For SVG or PDF, call `interp.enable_svg()` or `interp.enable_pdf()` before
  running, then read `interp.svg_pages()` or `interp.pdf_document()`.
- The PostScript libraries in `lib/` (artkit, graph, dataviz and the rest) are
  compiled into the crate, so `(lib/artkit.ps) run` works with nothing on disk.
  The search order for files a program loads is in
  [Getting started](getting-started.md#where-pscat-finds-its-files).
- Errors are `pscat::PsError`, named after the PLRM errors (`undefined`,
  `typecheck`, ...). `interp.error_report(&e)` gives the
  `%%[ Error: ...; OffendingCommand: ... ]%%` line the CLI prints.
- An `Interp` is not `Send`. Make one per thread, or one per document; they
  are independent.

## Supported builds

| Build | Command | What you get |
|---|---|---|
| CLI, full | `cargo install pscat` | `pscat` with the live window, and `pscat-mcp` |
| CLI, headless | `cargo install pscat --no-default-features` | everything except the window modes, which print an error |
| Library | `pscat = { version = "0.6", default-features = false }` | no `winit`/`softbuffer` in your dependency graph |
| Browser | `scripts/build_wasm.sh` | the wasm32 module ([pscat in the browser](browser.md)) |

CI builds and tests the first three on Linux, macOS and Windows against the
packaged crate (`scripts/check_package.sh`). The minimum supported Rust
version is the `rust-version` in `Cargo.toml` (1.88).

## Stability

The supported API is what docs.rs shows: `Interp`, `Page` and `PsError`. The
crate has other public modules that the binaries, the browser build and the
test suite use; they are hidden from the documentation and can change in any
release.

Before 1.0, a minor release (0.6 to 0.7) may change the supported API and a
patch release (0.6.0 to 0.6.1) does not. New `PsError` variants can arrive in
any minor release (the enum is `#[non_exhaustive]`). Rendering output can
change between releases as the interpreter gets closer to Ghostscript; pin an
exact version if you compare images byte for byte.

Pull requests run `cargo-semver-checks` against their base commit and report
API changes in the CI job summary, so the release notes know which releases
need the minor bump.
