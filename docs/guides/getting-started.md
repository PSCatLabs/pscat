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
  puts the saved stream back instead.) It also seeds the native noise
  operators (`N noiseseed`, #753), so noise art varies per frame; a program
  that calls `noiseseed` itself keeps its own table. `--sweep NAME=...` predefines `/NAME`
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
  flag. `filenameforall` lists files where `run` would look for them
  (`/` between directories on every platform, Windows too: `C:/dir/*.ps`;
  `\` is the escape character). `pscat-mcp` never lets a program write.
  Writes are buffered and flushed by `closefile`, `flushfile`, `flush`,
  `showpage`, `quit` and at exit, even when a program never closes a file;
  `fileposition`, `setfileposition` and `resetfile` work on files. At most 64
  files can be open for writing, and `--max-write-bytes N` (`k`, `m`, `g`
  suffixes; default 1g, `0` for none) stops a program that writes more than N
  bytes: the write is a `limitcheck`.
- **Confining reads.** Reading is open by default. `--read-root DIR`
  (repeatable) confines `(f) (r) file`, `run`, `readreals` and
  `filenameforall` to files inside the named directories: a path outside, a
  `..`, a symlink leading out or `/dev/zero` is `invalidfileaccess`, and
  `filenameforall` lists nothing outside. The embedded `lib/` still works.
  `--confine-reads` alone allows only that. `pscat-mcp` always confines its
  renders this way (see the [agents guide](agents.md)).
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

  To read everything, give `/all` for the count: `(path) /all readreals` (a
  `-1` is a `rangecheck`, so a count that went negative by mistake is not a
  silent whole-file read). To read part of a file, put an offset, counted in
  values and not bytes, before the count, as `getinterval` does:
  `(path) 100 50 readreals` is values 100 to 149, and `(path) 100 /all
  readreals` is value 100 to the end. An offset past the last value is a
  `rangecheck`; a result over the 16,777,216-element array cap is a
  `limitcheck`.

  Both operators also take an open file. `array file writereals` writes at the
  file's position, so it works with `(%stdout) (w) file`, an `(a)` append file
  (two calls give two arrays back to back) and `setfileposition`, and it
  counts a disk file's bytes against `--max-write-bytes`, as `writestring` does; it does not close the file, and a
  closed or input file is an `ioerror`. `file n readreals` (or `file /all
  readreals`) reads from an input file's position and leaves it just after
  the values, so a file can be read in pieces, `setfileposition` (in bytes)
  jumps, and `currentfile 4 readreals` reads binary data inline in a program.
  A file that has fewer than `n` values left is a `rangecheck` and keeps its
  position (a decode filter cannot say how much is left, so it has already
  given up what it read). There is no offset form for a file: `setfileposition` is the
  offset.
- **Limits on every run.** `--max-steps N`, `--max-time SECS` and
  `--max-memory MIB` (0 = no limit, the default) apply to one program run, and
  `--spool` and the tool subcommands take them too (`pscat handwrite`,
  `typewrite`, `photo-etch`, `chalk`, `sheet`). A stopped run exits non-zero
  with `limitexceeded (steps|time|memory)`. The unit is the run, not the page:
  a multi-page program is one run; each `--spool` job, `--sweep` frame
  and `pscat sheet` cell is its own run with the whole budget (so is each
  `--bands` band, for steps and time; memory is shared by the bands);
  `handwrite`, `typewrite` and `photo-etch` run a measuring pass and then the
  render and give each the full limits. Reading a whole file (`file`, `run`)
  charges one step per KiB.
- **Parallel bands.** `--bands N` (1 to 64) renders a PNG on N threads and
  stitches it into one page; the PNG is byte for byte the plain `--png`
  render. The page is cut into more horizontal bands than threads (by
  default two per thread, none under 8 device rows; one band for
  `--bands 1`; `--band-rows R` picks bands about R rows tall instead), and
  each thread takes the next band from a queue when it finishes one. So a
  scene whose cost is uneven down the page, a cheap sky over expensive
  terrain, doesn't wait on the one thread that drew the expensive strip.
  Each band is a full run of the program with
  `/BandIndex` (0 is the top band), `/BandCount` and `/BandBBox
  [llx lly urx ury]` (the band's area in default user space, points) defined
  in `userdict`, all describing the queued band: `/BandCount` is the number
  of bands, not threads. They are ordinary `userdict` entries, visible to the
  program, and their values differ from band to band. A program that hashes or
  serialises `userdict` (a cache key, a state snapshot) or walks it with
  `forall` sees different contents in each band, so leave the three names out
  of such keys. One project's bake cache missed in every band but one for this
  reason ([#844](https://github.com/PSCatLabs/pscat/issues/844#issuecomment-6429744992)).
  A related issue is that defining the names can change the `forall` order of
  other name-keyed dicts against a plain run
  ([#868](https://github.com/PSCatLabs/pscat/issues/868)). A program that reads them can skip the work outside its band:
  an expensive `colorimage` procedure computes only the image rows that reach
  `BandBBox`, plus a row of margin. That is where the speed comes from. A
  program that doesn't read them still renders correctly, but every band runs
  all of it, so with K bands it does about K times the work of a plain render
  and is slower than one; only image rasterization skips the other bands'
  rows. Each band also repeats the program's setup and any margin rows, which
  is why the default doesn't cut finer. Band `i` of `K` keeps device rows
  `floor(i*H/K)` to `floor((i+1)*H/K)`. `clippath` and the rest of the
  graphics state still see the whole page. Every page of a multi-page program
  is banded, and every band must emit the same number of pages. PNG only: it
  refuses `--svg`, `--pdf`, sweeps, `--progress`, `--profile` and
  `--allow-write`, since every band would write the same file (or, for
  `--profile`, report N copies of the same calls: profile a plain run).
  `--lint` and `--lint-strict` do work, and lint once, after the run, with a
  `pscat: lint: note:` line saying so: band 0 (the top band) is checked for
  the declared page count, unbalanced `gsave` and leftover operands, which are
  the same in every band unless the program branches on `/BandIndex` (a leak
  in another band only goes unseen), and the blank-page check runs on the
  stitched page, since a band-aware program leaves its own canvas empty
  outside its band. An error in any band makes the verdict "not clean".
  `--max-steps`
  and `--max-time` apply to each band on its own, and the first band to hit
  any limit stops the queue: bands already running finish, no new one starts,
  and the rows of bands that never ran are left blank in the partial page. `--max-memory` is one
  budget for the whole render, counted from before the threads start; each
  running thread holds its own canvas (N times a plain render's), which the
  budget leaves out as a plain render's leaves out its one canvas. Only the top band's
  stdout and stderr (`(%stderr)` writes, `handleerror`) are printed; an uncaught error is
  reported once, from the first band that hit it (`band i/N failed`), and the stitched
  partial page is still written. A font stand-in note that only a lower band raised is
  printed once at the end; an error a program catches and reports itself in a lower band
  only is not heard. A piece seeded from
  `realtime` or `usertime` can tear at the seams, since the bands read
  different clocks.
- **Profile.** `pscat --profile prog.ps` runs the program headlessly, then
  prints to stderr one line per procedure bound to a name: the call count,
  the exclusive time (its own, without the named procedures it called) and
  the inclusive time (call to return), sorted by exclusive time, 20 rows
  (`--profile-top N`; 0 is all; `--profile-json` prints the same as one line
  of JSON, and `--profile-json out.json --allow-write DIR` writes that line
  to a file inside DIR instead, printing nothing; a path is taken only when
  it ends in `.json`, so `--profile-json prog.ps` still profiles `prog.ps`).
  It works with `-e`, `-`, `--png`, `--svg`, `--pdf` and `--lint`,
  and prints after a failed run too. Rules to read it by:
  - A procedure counts under the name it was *called through*. A body run
    by `exec`, an `if` or `ifelse` branch, a `forall` body or a Type 3 glyph
    has no name of its own and counts toward the procedure that ran it.
    Rebinding a name merges both procedures into one row.
  - Recursion counts every call, but inclusive time once per outermost call.
  - Time inside a procedure's last element is its own, and a procedure
    called from the last position is part of its caller's inclusive time (a
    plain run drops the caller's frame first; the profile does not).
  - `stop`/`stopped`, errors, `exit` and `quit` close the calls they unwind,
    so a run that dies still reports where its time went.
  - With mutual tail recursion (`a` ends by calling `b`, `b` by calling `a`)
    the chain stays one call deep, as it does without `--profile`; the calls
    are all counted, and the time goes to the last one to open a call (with
    `ping`/`pong`, to `pong`; `ping` keeps the time before its first call).
  - Each open call holds one more exec-stack slot than a plain call, so
    non-tail recursion that runs about 10,000 levels deep in a plain run
    stops with `execstackoverflow` at about 5,000 under `--profile`. The
    report still prints.
  - It costs time: about 55 ns per named procedure call (the clock is read
    twice), so `fib 30` runs about a third slower and a typical piece 5%.
    Times include that cost, so small procedures read high. Without the flag
    the cost is one flag test per procedure call (under 3% on `fib 27`).
  - `--profile-graph` adds the call graph: one line per caller -> callee pair
    (`<top level>` for a call made outside every procedure) with the calls
    along it and the time the callee ran when called from there, children
    included, longest first. Calls into a callee, summed over its edges, are
    its call count. A recursive edge (`fib -> fib`) counts its time once per
    outermost frame, and a tail call back into a procedure already open adds
    to the calls and adds no time (it is part of the call it continues).
    `--profile-top` limits the edges as well, and `--profile-json` gains
    `edges` (`caller` is `null` at the top level) and `total_edges`. About
    10% more than `--profile` on `fib`.
  - `--profile-ops` adds time per built-in operator, so `fill` can be weighed
    against `add`: calls, total time and time per call, longest first
    (`operators` and `total_operators` in the JSON). It reads the clock around
    every operator, so a run takes about three times as long and the
    procedure times grow too: the "per call" of a cheap operator (`add`) is
    mostly that clock cost, about 20 ns. It times an operator's own code. `for`,
    `if`, `repeat` and `forall` only push a frame (their bodies are timed
    as the operators they run), and `show` and `image` do most of their work
    afterwards, in steps the clock does not see, so they read cheap here: use
    the procedure table for those. An operator is counted under its systemdict
    name (`a/b` if two names share one function). Without the flag the cost
    is one check per slice of 4,096 steps, and no slowdown was measured on
    `fib` (`notes/799-profile-followups.md`).
  - Not with `--bands` (each band is a full run, so a merged table would be N
    copies of the same calls), sweeps, `--spool`, `--interactive` or a
    windowed run: it refuses them.
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

- [Handwriting](handwriting.md): turn a string into a handwriting-style PNG.
- [Fonts](fonts.md): the font library and the 67-face catalog.
- [artkit](artkit.md): the generative-art toolkit.
- [All guides](README.md).
