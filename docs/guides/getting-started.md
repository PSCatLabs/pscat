# Getting started

pscat is a PostScript interpreter that draws as it runs. This guide covers
building it, installing a ready-made bundle, and the command-line flags you
will use most. If PostScript itself is new to you, skim the
[site](https://jeromebanks.github.io/postscript_interpreter/) first; every
example below is a plain `.ps` text file.

## Build and run

You need a stable Rust toolchain. From a checkout of the repo:

```sh
cargo run -- --page 500x500 examples/sierpinski.ps      # watch it draw, live
cargo run -- --png out.png examples/sierpinski.ps       # or render it headlessly to a PNG
```

The `examples/` directory has three recursive fractals (Sierpinski triangle,
Koch snowflake, golden spiral), a test card written like a found file, and a
type specimen (`specimen.ps`). They render identically in pscat and
Ghostscript.

## Command-line flags

Every flag, with the most useful ones first:

```sh
cargo run -- --page 500x500 file.ps       # canvas size (default 612x792)
cargo run -- --speed 10 file.ps           # slower (steps per frame, default 100)
cargo run -- --png out.png file.ps        # headless render to PNG
cargo run -- --svg out.svg file.ps        # export SVG (also --pdf out.pdf)
cargo run -- --dpi 300 --png out.png file.ps   # print-resolution rendering
cargo run -- --spool jobs/                # act like a lab printer: render
                                          # each .ps/.eps dropped into jobs/
cargo run -- --halftone file.ps           # classic 45° halftone dots, like a
                                          # mono laser printer (window/PNG)
cargo run -- --lint --png out.png file.ps # self-check: blank page? unbalanced
                                          # gsave? stuff left on the stack?
cargo run -- --lint-strict file.ps        # ...and exit non-zero on any finding
                                          # (a run that errored is never "clean":
                                          # lint prints [run-error] and exits 1)
cargo run -- --selftest lib/paintkit.ps   # run a library's %%SelfTest blocks
./scripts/selftest.sh                     # every library's blocks, plus strict
                                          # lint over every rendering driver
cargo run -- file.ps --sweep-seed 1:12 \  # render 12 seeds, one grid PNG
    --contact-sheet grid.png
cargo run                                 # terminal REPL (headless canvas)
cargo run -- -i                           # REPL + live window: watch what you type draw
cargo run -- -i lib/handscript.ps         # ...with a library preloaded
cargo run -- -e '3 4 add ='               # evaluate a snippet
cargo test                                # language + pixel-level render tests
```

More on the less obvious ones:

- **Multi-page output.** `showpage` ends a page. The window keeps showing the
  finished page, and `--png` numbers the pages (`out-001.png`, ...).
  `--svg` and `--pdf` export the same paint pipeline. A PDF is a real
  document: `%%Title:` and `%%For:` header comments become its title and
  author metadata, so a multi-page piece opens in any reader or e-reader with
  a proper title.
- **Seed sweeps.** `--sweep-seed` renders a file once per value instead of
  once, so exploring seeds or a parameter is one command instead of many
  hand edits. It overrides every `srand` call, so found art with a hardcoded
  `N srand` line sweeps unmodified. (A restore of a value saved with `rrand`
  puts the saved stream back instead.) `--sweep NAME=...` predefines `/NAME`
  in `userdict` for a program that reads it. `--png` writes numbered
  per-frame files; `--contact-sheet PATH` composites every frame into one
  grid PNG instead (`--grid COLSxROWS` overrides the layout). Use either or
  both. `examples/sweep_demo.ps` is the specimen. Sweeps are CLI-only; the
  `pscat-mcp` server does not expose one.
- **Gradients.** `shfill` paints axial and radial gradients (`ShadingType`
  2 and 3, with exponential and stitching functions, so multi-stop ramps
  work). Raster and window output use real gradients, SVG uses native
  `<linearGradient>`/`<radialGradient>`, and PDF uses a flat average colour
  (a known gap). [artkit](artkit.md) has helpers that build the shading
  dictionaries for you; `examples/gradients.ps` is the specimen sheet.
- **The lab printer.** `--spool DIR` turns the window into the printer in the
  corner of the lab. It idles, watches the directory, and renders each
  `.ps`/`.eps` that lands there, page by page, each job in a fresh
  interpreter.
- **Halftone.** `--halftone` screens the raster like a mono laser printer's
  RIP: classic dots on a 45° lattice, with coverage tracking darkness.

## Installing pscat without the repo

You don't need a git checkout or a Rust toolchain just to *use* pscat.
`scripts/package_release.sh` builds `pscat` and `pscat-mcp` and bundles them
with their runtime assets (`lib/`, `fonts/catalog/`, `scripts/handwrite.sh`)
into a standalone directory. CI does this automatically and attaches the
result to a
[GitHub Release](https://github.com/jeromebanks/postscript_interpreter/releases)
whenever a version tag is pushed (`.github/workflows/release.yml`).

To use one:

```sh
tar xzf pscat-<version>-<os>-<arch>.tar.gz
export PATH="$PWD/pscat-<version>-<os>-<arch>:$PATH"
pscat --png out.png some_file.ps
```

`lib/artkit.ps` and the other libraries, and `fonts/catalog/`, resolve
automatically as long as they stay next to the `pscat` binary. That is the
point of the bundle layout. If you move the binary elsewhere, set
`PSCAT_ROOT` to wherever the bundle ended up (`src/paths.rs` has the full
resolution order, which also covers running from a `cargo build --release`
checkout with no configuration).

## Where next

- [Handwriting](handwriting.md): turn a string into a handwritten PNG.
- [Fonts](fonts.md): the font library and the 61-face catalog.
- [artkit](artkit.md): the generative-art toolkit.
- [All guides](README.md).
