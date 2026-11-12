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

## Check syntax without running a program

`pscat::syntax::check_syntax` tokenizes a program with the interpreter's own
scanner, balances `{` and `}`, and executes nothing, so an editor, a service or
an agent can reject a program that cannot scan before it creates an `Interp`:

```rust
use pscat::syntax::check_syntax;

match check_syntax(source.as_bytes()) {
    Ok(summary) => println!("{} tokens", summary.tokens),
    Err(e) => eprintln!("{e}"), // "syntaxerror: unterminated string (line 3, byte 41)"
}
```

The error carries `kind` (`Lexical`, `UnmatchedCloseBrace`,
`UnterminatedProcedure`), `message`, the byte `offset` and 1-based `line` where
the offending token starts (for a missing `}`, the outermost `{` never closed),
and the `tokens` read before it. It finds what a scanner can see, not a
misspelt operator or a type error, and it cannot know that a program reads its
own text as data (`currentfile eexec`, inline `image` samples), so those bytes
can be reported as errors.

## Pages, halftone, review sheets, chalkboards and handwriting

These are the pieces behind `pscat --halftone`, `pscat sheet`, `pscat chalk`
and `pscat --capabilities`, for a program (an MCP server, say) that runs each
PostScript program itself and wants the same output. They all take and return
`Page`, pscat's own raster type, so your build never names a rasterizer type
and does not have to match pscat's version of one.

- **Pages one at a time.** `interp.page_count()` and `interp.page(i)` read the
  pages that `pages()` returns, copying one at a time.
  `interp.into_last_page()` takes the last page out of the interpreter without
  a copy, the page `pscat sheet` uses for a cell. `Page::blank(w, h)` is a
  white page.
- **Output setup.** `interp.set_transparent_background()` before running
  gives PNG pages an alpha channel and SVG pages no white backdrop
  (`pscat --transparent`). After `enable_pdf()`,
  `interp.set_pdf_info_from_dsc(source)` gives the PDF the title and author in
  the program's `%%Title:` and `%%For:` comments (`pscat --pdf` does this).
- **Halftone.** `page.halftone()` screens a page to pure black and white,
  45 degree clustered dots, as `pscat --halftone` does:
  `page.halftone().to_png()` is the same file `pscat --halftone --png` writes.
- **Review sheets.** `pscat::sheet` lays out rows of cells from their sizes
  alone (`SheetLayout::new`, before anything renders, so an oversized sheet
  costs nothing), sets labels and captions with pscat's own fonts
  (`text_strip`), enlarges a small cell pixel by pixel (`zoom`), and draws
  the sheet (`Sheet`). Each cell is a `Page` you rendered:

  ```rust
  use pscat::sheet::{CellSize, RowSize, Sheet, SheetLayout, text_strip};

  let label = text_strip("logo", 16, true)?.expect("non-empty");
  let cell = my_render("logo.ps", 96.0)?; // your own sandboxed render: a Page
  let row = RowSize::new(
      (label.width(), label.height()),
      vec![CellSize::new((cell.width(), cell.height()), (0, 0))],
  );
  let mut sheet = Sheet::new(&SheetLayout::new(&[row])?)?;
  sheet.put_header(0, &label)?;
  sheet.put_image(0, 0, &cell)?;
  sheet.into_page().save_png("sheet.png")?;
  ```

  A sheet is at most `sheet::MAX_SIDE_PX` (8000) pixels on a side. An image
  must be the size its slot was planned for; a label or caption may be
  smaller than its slot.
- **Chalkboards.** `pscat::chalkboard::BoardConfig` reads the
  `pscat chalk --json` config (see
  [From LaTeX to a board](dry-media.md#from-latex-to-a-board-pscat-chalk)),
  and `config.driver(latex)` writes the PostScript program that puts the
  formula on the board. Run it on an `Interp` of `driver.page_size()` at
  `driver.dpi()`, after `driver.prepare(&mut interp)` (which makes the page
  transparent when there is no board, and sets the PDF title; call it after
  `enable_pdf`). When the run fails, turn the
  error back into a TeX error with `driver.explain(interp.captured_stdout(),
  &err)` (call `interp.capture_stdout()` before the run): it carries every
  unsupported command or character in the formula, each with its place.
  `chalkboard::explain(&err)` alone names only the first, with no place. A
  `ChalkError` has a stable `code()`, the `token()` a TeX error named, and
  `message()`, which names a bad config key by its path
  (`layout.width must be ...`).

  `error.problems()` is the list `pscat chalk` prints and pscat's MCP tool
  returns as `problems`, in the same order and at the same places: a
  `&[chalkboard::Problem]` with `code()`, `message()`, `token`, `line`,
  `column` (both counted from 1) and `offset` (bytes into the LaTeX). It is
  empty for an error that is not a list of TeX problems: a config error, no
  formula, a render failure, or the position-less error from `explain` alone.

  ```rust
  use pscat::chalkboard::BoardConfig;
  use pscat::Interp;

  let board = BoardConfig::default();
  let driver = board.driver("x + \\aleph\n+ ☃")?;
  let (w, h) = driver.page_size();
  let mut interp = Interp::with_page_scaled(w, h, driver.dpi() / 72.0).expect("valid page");
  interp.capture_stdout();
  let err = interp.run_str(driver.source()).expect_err("outside the subset");
  let e = driver.explain(interp.captured_stdout(), &err).expect("a TeX error");

  // two problems from one input, with their places
  let found: Vec<_> = e.problems().iter().map(|p| (p.token.as_str(), p.line, p.column)).collect();
  assert_eq!(found, [(r"\aleph", 1, 5), ("☃", 2, 3)]);
  assert_eq!(e.problems()[0].code(), "unsupported_command");
  assert_eq!(e.problems()[0].message(), r"unsupported LaTeX command \aleph");
  # Ok::<(), pscat::chalkboard::ChalkError>(())
  ```

  (The same example runs as the doctest on `ChalkError::problems`.)
- **Handwriting.** `pscat::handwriting` writes the programs behind
  `pscat handwrite`, as source for you to run in your own sandbox, so you can
  check the page between the two passes and run each under its own limits.
  `HandOptions` is the text and the flags (`Default` is the subcommand's
  defaults, `validate()` checks them against the `MIN_`/`MAX_` consts and
  returns a `HandError` with a stable `code()` and the `field()`):

  ```rust
  use pscat::handwriting::{HandOptions, measure_source, parse_line_count, render_source};

  let mut opts = HandOptions::new("meet me at nine");
  opts.seed = 7;
  opts.validate()?;
  let stdout = my_run(&measure_source(&opts))?; // pass 1: prints the line count
  let lines = parse_line_count(&stdout).expect("a count");
  let height = opts.height_pt(lines) as u32; // check it against your own cap here
  let page = my_render(&render_source(&opts, height), opts.width, height)?; // pass 2
  ```

  `page_height_pt(lines, size)` is `height_pt` at the default margin
  (`DEFAULT_MARGIN`) and leading (`DEFAULT_LEADING`). A note made this way is
  the same PNG `pscat handwrite` writes for the same options. The module's
  own documentation has a complete example that runs as a doctest.

  `has_glyph(c)` and `drawable(text)` say which characters the font draws
  (letters in either case, digits and `. , - ' ! ?` today; a space and
  anything non-ASCII draw nothing), so a server can refuse blank text before
  it spends a worker. They read the font's own table in `lib/handscript.ps`
  (the library text `render_source` embeds, so an override is followed), in a
  bounded run that reads no files and gives no glyphs if the library misbehaves,
  rather than a copy that could drift.
- **The catalog.** `pscat::catalog::capabilities()` is the JSON
  `pscat --capabilities` prints: fonts, Type 3 faces, palettes, templates,
  library procedures and native operators.

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

The supported API is what docs.rs shows: `Interp`, `Page`, `PsError`,
`Limits`, `syntax::check_syntax`, and the `sheet`, `chalkboard`, `handwriting`
and `catalog` modules. The crate has other public modules that the binaries, the browser
build and the
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
