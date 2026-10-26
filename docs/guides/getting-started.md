# Getting started

pscat is a PostScript interpreter that draws as it runs. This guide covers
building it, installing a ready-made bundle, and the command-line flags you
will use most. If PostScript itself is new to you, skim the
[site](https://pscatlabs.github.io/pscat/) first; every
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
cargo run -- --bands 4 --png out.png file.ps   # render on 4 threads, 4 bands
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
cargo run -- --allow-write cache/ f.ps    # let f.ps write files in cache/
cargo run -- --version                    # print "pscat X.Y.Z" (also -V; pscat-mcp too)
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
- **Review sheets.** `pscat sheet -o sheet.png --page 384x384 --dpi 96,9
  a.ps b.ps` lays several files out as labelled rows, each at several sizes:
  every cell at its own true pixel size, centred in its column, with the row
  label (the file name, or `--label TEXT`) above and a `WxH px` caption
  below. A `--dpi` entry such as `9x4` adds an exact, unsmoothed 4x
  enlargement of the 9 dpi render. `--page` and `--dpi` apply to the files
  after them, so one sheet can mix page sizes. Labels can be accented Latin,
  Korean, Japanese, Chinese or Thai (the face follows the script; Korean,
  Japanese and Thai need the `cjk-thai` font pack). Where `--contact-sheet` can
  only grid same-size sweep frames, this builds icon-size comparisons and
  candidate review sheets without an imaging library. Four more controls
  (#706). `--sweep-seed 1:12` or `--sweep NAME=10,20,40` before a file turns it
  into one cell per seed or value, each captioned `seed=7` or `NAME=20`, with
  the flag CLI's own sweep grammar. A `--dpi` cell takes a mat, `9@#222`: it is
  drawn on a transparent page, set on a dark (or `white`) tile padded 10 px,
  to judge an icon on both backgrounds. `+` stacks cells in one column
  (`'96,9+9@#222'`: a 96 dpi cell, then the 9 dpi render over its dark-mat
  copy), so small cells stop taking a wide column each. `--cols N` wraps a long
  row onto more lines, for sweeps and sheets that would pass 8000 px. Quote a
  `--dpi` list that has `#` or `+`. `pscat sheet --help` has the rest.
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
- **Writing files.** A program writes nothing to disk unless you name a
  directory with `--allow-write DIR` (repeatable). Then `(DIR/f) (w) file`
  (or `(a)` to append), `writestring`, `writehexstring`, `renamefile` and
  `deletefile` work inside it; any path outside it, a `..`, or a symlink
  leading out is `invalidfileaccess`. A relative name is relative to the
  current directory. `(%stdout) (w) file` and `(%stderr) (w) file` need no
  flag. `filenameforall` lists files where `run` would look for them.
  `pscat-mcp` never lets a program write.
- **Saving numeric arrays.** `array (path) writereals` writes the numbers as
  packed little-endian binary64 (8 bytes each), and `(path) n readreals` reads
  `n` of them back into a new array, bit for bit (`-0.0`, infinities and NaN
  included). It is far faster than printing the numbers (about 10x to read, and much more to write) and
  `run`ning the text, and exact where text is not (`inf` has no literal).
  `array (path) /f32 writereals` and `(path) n /f32 readreals` use binary32
  (4 bytes each; lossy on the way in, exact on the way out). Writing needs
  `--allow-write` like `(w) file`; reading follows the `(r) file` rules.
  Integers are written as reals; any other element is `typecheck` (before the
  file is touched). A file shorter than `n` values is `rangecheck`; extra bytes
  are ignored. `readreals` reads the whole file, then keeps the first `n`
  values, so for a huge file it is as costly as `(path) (r) file`. Both honour
  `--max-steps`, `--max-time` and `--max-memory`.
- **Parallel bands.** `--bands N` (1 to 64) renders a PNG as N horizontal
  bands on N threads and stitches them into one page; the PNG is byte for
  byte the plain `--png` render. Each band is a full run of the program with
  `/BandIndex` (0 is the top band), `/BandCount` and `/BandBBox
  [llx lly urx ury]` (the band's area in default user space, points) defined
  in `userdict`. A program that reads them can skip the work outside its band:
  an expensive `colorimage` procedure computes only the image rows that reach
  `BandBBox`, plus a row of margin. That is where the speed comes from. A
  program that doesn't read them still renders correctly, but every band runs
  all of it, so it takes as long as a plain render or longer; only image
  rasterization skips the other bands' rows. Band `i` keeps device rows
  `floor(i*H/N)` to `floor((i+1)*H/N)`. `clippath` and the rest of the
  graphics state still see the whole page. Every page of a multi-page program
  is banded, and every band must emit the same number of pages. PNG only: it
  refuses `--svg`, `--pdf`, `--lint`, sweeps, `--progress` and
  `--allow-write`, since every band would write the same file. `--max-steps`
  and `--max-time` apply to each band on its own. `--max-memory` counts the
  whole process, so it caps all bands together, and each band holds its own
  canvas and pages (N times the memory of a plain render). Only the top band's
  stdout is printed; an error is reported once, from the first band that hit
  it, and the stitched partial page is still written. A piece seeded from
  `realtime` or `usertime` can tear at the seams, since the bands read
  different clocks.
- **Halftone.** `--halftone` screens the raster like a mono laser printer's
  RIP: classic dots on a 45° lattice, with coverage tracking darkness.

## Installing pscat without the repo

With a Rust toolchain, install from [crates.io](https://crates.io/crates/pscat):

```sh
cargo install pscat                          # pscat and pscat-mcp, with the live window
cargo install pscat --no-default-features    # headless: PNG/SVG/PDF only, no window
pscat fonts install                          # the font catalog, as font packs
```

You don't need a git checkout or a Rust toolchain just to *use* pscat. Each
[GitHub Release](https://github.com/PSCatLabs/pscat/releases)
carries a ready-built archive per platform, made by
`scripts/package_release.sh` in the release workflow
(`.github/workflows/release.yml`):

| Platform | Archive |
|---|---|
| macOS, Apple silicon | `pscat-<version>-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `pscat-<version>-x86_64-apple-darwin.tar.gz` |
| Linux x86_64 (static, headless) | `pscat-<version>-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 (static, headless) | `pscat-<version>-aarch64-unknown-linux-musl.tar.gz` |
| Windows x86_64 | `pscat-<version>-x86_64-pc-windows-msvc.zip` |

Each holds `pscat` and `pscat-mcp`, their runtime libraries (`lib/`), the
licences (`LICENSE-MIT`, `LICENSE-APACHE`, `LICENSE-OFL`,
`THIRD_PARTY_NOTICES.md`) and the licences of the Rust libraries linked into
the binaries (`THIRD_PARTY_LICENSES.html`). `SHA256SUMS` on the release lists
every archive's checksum. The handwriting, typewriter and photo-etching tools
are subcommands of `pscat` itself (`pscat handwrite`, `pscat typewrite`,
`pscat photo-etch`), so they come with the binary.

To use one:

```sh
shasum -a 256 -c SHA256SUMS --ignore-missing     # optional: check the download
tar xzf pscat-<version>-<target>.tar.gz
export PATH="$PWD/pscat-<version>-<target>:$PATH"
pscat --png out.png some_file.ps
pscat fonts install                               # the font catalog, as font packs
```

Releases published since provenance was added (#635; not 0.6.0) also carry
a signed build-provenance attestation for every archive. To check that an archive was built by pscat's release workflow
from a tagged commit (needs the [GitHub CLI](https://cli.github.com)):

```sh
gh attestation verify pscat-<version>-<target>.tar.gz --repo PSCatLabs/pscat
```

`pscat --version` prints the version you have installed.

The Linux archives are fully static (musl), so they run on any distribution,
but they are **headless**: they render to PNG, SVG and PDF and have no live
window, because a static binary cannot load the system's X11/Wayland
libraries. For the window on Linux, build from source.

`lib/` resolves automatically as long as it stays next to the `pscat` binary.
That is the point of the bundle layout. The libraries (`lib/artkit.ps` and the
rest) are also compiled into the binary, so a bare `pscat` binary, or one from
`cargo install`, runs them with nothing on disk. (`cargo install pscat
--no-default-features` builds a headless pscat without the live window; see
[pscat as a Rust library](library.md#supported-builds).) If you move the binary
away from the bundle, set `PSCAT_HOME` to wherever the bundle ended up. The
font catalog is not in the archive: `pscat fonts install` downloads it as font
packs ([Fonts](fonts.md#font-packs-the-catalog-outside-a-checkout)).

### Where pscat finds its files

This is the one place the search order is written down (`src/paths.rs`
implements it). For a relative name a program loads with `run` or `file`,
such as `(lib/artkit.ps) run`, pscat uses the first of these that exists:

1. `$PSCAT_LIB`, for `lib/...` names only: a directory that stands in for
   `lib/`, so `$PSCAT_LIB/artkit.ps` serves `lib/artkit.ps`. Use it to try
   edited libraries without rebuilding.
2. `$PSCAT_HOME`, a pscat tree holding `lib/` and `fonts/catalog/`. Its old
   name, `$PSCAT_ROOT`, still works.
3. The name relative to the current directory. Inside a checkout this is
   the checkout's own `lib/`, so edits there take effect immediately.
4. The per-user data directory: `$XDG_DATA_HOME/pscat` (by default
   `~/.local/share/pscat`) on Linux, `~/Library/Application Support/pscat`
   on macOS, `%APPDATA%\pscat` on Windows.
5. The directory holding the `pscat` binary (the release bundle).
6. The copy of `lib/` compiled into the binary.
7. Development builds only (a debug build, or `--profile ci`): the
   checkout the binary was built in. A release build never looks there,
   so `target/release/pscat` run from outside the checkout uses its
   compiled-in `lib/` and finds no font catalog unless `$PSCAT_HOME`
   points at the checkout or the font packs are installed. The repo's own
   scripts that read the font catalog (`build_site.sh`,
   `build_font_gallery.sh`) set `$PSCAT_HOME` to their checkout themselves;
   `handwrite`, `typewrite` and `photo-etch` need neither (they are `pscat`
   subcommands now, and use only the compiled-in `lib/`).

An absolute name is read as written. A plain font catalog directory
(`fonts/catalog/`, not compiled in) is looked for in steps 2, 4, 5 and
7, then the current directory, and every one found is used. After those
come the installed font packs (`fonts/packs/`, written by `pscat fonts
install`) in `$PSCAT_HOME`, the per-user data directory and the binary's
directory. See [Fonts](fonts.md#font-packs-the-catalog-outside-a-checkout).

`PSCAT_TRACE_RESOURCES=1 pscat ...` prints one line per file to stderr,
naming the step that served it (`pscat: resource lib/artkit.ps <- embedded`).

## Where next

- [Handwriting](handwriting.md): turn a string into a handwritten PNG.
- [Fonts](fonts.md): the font library and the 67-face catalog.
- [artkit](artkit.md): the generative-art toolkit.
- [All guides](README.md).
