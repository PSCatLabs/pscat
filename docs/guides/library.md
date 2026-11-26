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
  Under a memory limit (`Limits::with_max_memory`), write the PDF with
  `interp.write_pdf(&mut sink)` (a file: nothing but one compressed stream
  is held besides the program) or read it with `interp.pdf_document_limited()`.
  Both refuse with `PdfWriteError::Memory`, before the allocation, once the
  document would take the heap past the limit plus 16 MiB. `pdf_document()`
  is not limited: it cannot report a refusal.
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
  `chalkboard::explain(&err)` alone names only the first, with no place. The
  check pass reports on the program's standard output, so to show what the
  program printed to a user, give `chalkboard::user_output(printed)` the same
  bytes: it returns them without the check lines (`CKPROBLEM...`), byte for
  byte otherwise. A
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

  **Pinning the font.** `lib/handscript.ps` is read in your process, outside
  any interpreter, so the interpreter's read roots do not apply. By default
  (`Library::Resolved`) it is the first match of `$PSCAT_LIB`, `$PSCAT_HOME` (or
  `$PSCAT_ROOT`), the **current directory**, the per-user data directory, the
  executable's directory, then the copy compiled in. A server that does not
  control those should pin the compiled-in font once, and pass the same value to
  the glyph checks:

  ```rust
  use pscat::handwriting::{HandOptions, Library, drawable_in, has_glyph_in};

  let mut opts = HandOptions::new("meet me at nine");
  opts.library = Library::Embedded; // measure_source and render_source carry the compiled-in font
  if !drawable_in(opts.library, &opts.text) {
      // refuse: nothing would be drawn
  }
  assert!(has_glyph_in(opts.library, 'n'));
  ```

  With `Library::Embedded` the glyph checks read a table checked in to the crate
  (a test recomputes it from the font): they run no PostScript and build no
  interpreter, so a process that never runs PostScript can use them. The
  default `has_glyph` and `drawable` answer the same way, without PostScript,
  whenever the resolver lands on the compiled-in copy.
- **The catalog.** `pscat::catalog::capabilities()` is the JSON
  `pscat --capabilities` prints: fonts, Type 3 faces, palettes, templates,
  library procedures and native operators.

## Extend the interpreter from your application

Everything above runs a program and reads its pages. The `pscat::host` module
is for an application that does more: one that gives programs operators of its
own, feeds them data, draws many pictures from one loaded program, or reports
errors as data. psanim, an animation engine that calls one procedure per
frame, is built on it.

**Operators backed by closures.** `register_operator` adds an operator to
systemdict. To a program it is an ordinary operator: `bind` resolves it, an
error it returns names it as the offending command, and the operands it popped
before failing are put back.

```rust
use std::{cell::Cell, rc::Rc};

let clock = Rc::new(Cell::new(0.0_f64));
let mut interp = pscat::Interp::with_page(200, 200).expect("valid page size");
let c = clock.clone();
interp.register_operator("now", move |i| {
    i.push_real(c.get());
    Ok(())
})?;
interp.run_str("/hand { 100 100 translate now 6 mul neg rotate 0 0 moveto 0 80 lineto stroke } def")?;
clock.set(15.0);
interp.run_str("hand")?;
```

Registering the name of a built-in replaces it, which is how a host takes over
`showpage` or `realtime`. An interpreter holds up to 256 host operators.

The interpreter looks at its limits (`--max-steps`, `--max-time`,
`--max-memory`, as `Limits`) and the interrupt flag between objects, so an
operator that does a lot of work in one call, such as a loop whose count comes
from the program, should call `check_limits(steps)` as it goes and return its
error with `?`. It charges the steps and gives the same uncatchable error the
interpreter would, or `PsError::Interrupted` for a raised flag.

```rust
interp.register_operator("spin", |i| {
    let n = i.pop_int()?;
    for k in 0..n {
        // ... one unit of work ...
        if k % 1000 == 999 {
            i.check_limits(1000)?;
        }
    }
    Ok(())
})?;
```

**Values in and out.** `host::Value` is a PostScript object as plain Rust data:
numbers, booleans, names, strings, arrays, dictionaries. `pop_value` and
`push_value` move one across the operand stack, `lookup` and `define_value`
read and write names, and `pop_f64`, `pop_int`, `pop_bool`, `pop_string`,
`pop_name`, `pop_numbers` and the matching `push_*` cover the common cases. A
procedure or a save object comes back as an opaque value that can be pushed
again, so a host can hold a program's `save` and hand it back to `restore`.
An opaque value belongs to the interpreter it came from: `push_value` and
`define_value` refuse one from another interpreter with `invalidaccess`, since
its host operators and save levels are that interpreter's. A copy is bounded:
past `host::MAX_VALUE_NODES` (about a million elements and entries, however
deep, a string or a dictionary key counting by its size) `pop_value` is
`limitcheck`, and inside an operator the copy, by `pop_value` or `lookup`,
counts against the program's limits like any other work (`lookup`'s at the
program's next look at its limits). A dictionary key that is not a name or a
string comes back as the text `==` prints for it, shortened past a few hundred
bytes.

```rust
use pscat::host::Value;

let mut interp = pscat::Interp::new();
interp.run_str("<< /Size [640 360] /Title (Dawn) >>")?;
let config = interp.pop_value()?;
assert_eq!(config.get("Size").and_then(Value::as_numbers), Some(vec![640.0, 360.0]));
```

**The current path as data.** `current_path` returns the path in user space as
`host::PathSegment`s and `set_current_path` replaces it, so a host operator can
measure a path the program built, walk along it, or cut it short.

**The canvas.** `canvas_premultiplied_rgba8` borrows the pixels of the page in
progress with no copy. `swap_canvas` exchanges the raster the program paints
on for another `Page` of the same size, leaving the graphics state alone: a
host paints part of a picture on a `Page::transparent` layer and composites it
itself (`Page::premultiplied_rgba8_mut`), or keeps a picture from one run to
the next. Inside a pattern's `PaintProc` there is no page to exchange, and the
`Page` comes back as the error.

**Files from memory.** `add_virtual_file(name, bytes)` serves a named source to
the program, so `(components/foo.ps) run`, `(components/foo.ps) (r) file`,
`readreals` and `status` work with no disk: for WebAssembly, a sandboxed
service, or an application with several library files of its own.

```rust
let mut interp = pscat::Interp::with_page(100, 100).expect("valid page size");
interp.add_virtual_file("components/dot.ps", b"/dot { 50 50 5 0 360 arc fill } def")?;
interp.run_str("(components/dot.ps) run dot")?;
```

The name is matched whole and exactly (a leading `./` is ignored), before the
search order: a registered name shadows a disk file and the embedded `lib/` copy
of the same name, which is how a host replaces a library. Every other name
resolves as it always did, so registering a file changes nothing else, and under
read roots an unregistered name outside them is still `invalidfileaccess`.
Virtual files are memory, not disk, so read roots do not gate them, **by
design**: only the host's own code can register a file, so a program can read
exactly the names the host registered and nothing it did not. A sandbox that
confines a program with read roots (`pscat-mcp`, `--allow-read`) stays closed:
registering `components/foo.ps` grants the program that one name and no disk
path. Only reading is served: `(name) (w) file`, `deletefile`, `renamefile` and
`filenameforall` do not see them. An error inside a file run this way names it,
in `ErrorDetails::file` and in a frame's `file` and `line`, as for a file on disk.

`bytes` is an owned `Vec<u8>` (moved in, never copied) or a borrowed slice
(copied once). Every check runs before the copy, so a refused registration
allocates nothing; a borrowed copy is also held to `--max-memory` and, from a
host operator, charged a step per KiB to the running program. A file is at most
`host::MAX_VIRTUAL_FILE_BYTES` (64 MiB, the bound for a `lib/` file on disk), a
name at most `MAX_VIRTUAL_NAME_BYTES` (4096), a store holds at most
`MAX_VIRTUAL_FILES` (4096) files and `MAX_VIRTUAL_TOTAL_BYTES` (512 MiB, the
bound for any one resource). The store counts what it keeps: each file's name
and its buffer's *capacity*, so `shrink_to_fit` a `Vec` that reserved more than
it holds. An empty name, a NUL, a `%` device name or a `..` component are
refused. A registration past a bound is `limitcheck` and changes nothing. Each
read copies the file into the program's file object and is charged as a read of
that size (a step per KiB, the memory cap checked before the copy), so a program
that loads a large file in a loop meets its limits; with no limits set it costs a
lookup.

**A raster under the transformation.** `draw_page(&page, Sampling)` paints a
`Page` the host holds through the current transformation, clip, `setalpha` and
`setblendmode`, as `image` does for samples a program supplied, so a layer a host
keeps (a motion layer, a rendered component) goes through pscat's own sampler
instead of the host's compositor. The page's pixel grid fills the user-space
unit square with its first row on top, the mapping of
`w h 8 [w 0 0 -h 0 h] image`; the host places it with the PostScript it
already has and scales it to size.

```rust
use pscat::{Interp, Page, host::Sampling};

let mut interp = Interp::with_page(200, 100).expect("valid page size");
let layer = Page::transparent(64, 64).expect("valid size"); // painted elsewhere
interp.run_str("gsave 100 50 translate 30 rotate 0.8 setalpha 80 80 scale -0.5 -0.5 translate")?;
interp.draw_page(&layer, Sampling::Bilinear)?;
interp.run_str("grestore")?;
```

`Sampling::Nearest` takes the pixel under each device pixel's centre (a 1:1
placement reproduces the raster exactly); `Bilinear` blends the four nearest
pixels, which suits a rotated or enlarged layer of soft art. The raster's edge is
anti-aliased either way, its own transparency composes with `setalpha` and the
blend mode, and the clip in force applies. `Page::from_premultiplied_rgba8`
takes premultiplied pixels.

It paints where the program paints: the layer while a transparency group is open
(so a host operator that places a raster inside `begingroup` ... `endgroup` is
composited at the group's alpha and blend mode, and a host reading the canvas
meanwhile still sees the page below), the tile inside a pattern's `PaintProc`
(in an uncoloured one, the raster's coverage in the pattern's colour; SVG
records that as a mask of its alpha and PDF as an image mask, one bit a pixel,
set where the alpha is at least half, since PDF allows no colour image there). A group is closed when a run ends, so draw into one from a host
operator, or between `step_n` slices. A call that has nothing to paint
(`nulldevice`, a singular or non-finite transformation, a footprint wholly off the
canvas) is `Ok` and changes nothing; a transformation so large that the corners
are 1e20 pixels away still covers the canvas it covers (that is decided in the
raster's own unit coordinates; where an edge crosses the canvas at such a scale,
the matrix's single precision decides where).

SVG and PDF output **embed the raster as an image** at its placement, with the
clip, alpha, blend mode and group, as they record `image`: a PNG in SVG, an RGB
image with a soft mask in PDF (no mask when no pixel is transparent). Recording
copies the pixels, so a raster over `host::MAX_EMBEDDED_RASTER_PIXELS` (16
million) is `limitcheck` while either recorder is on, with nothing painted or
recorded; PNG output paints the page in place and has no such limit. (Unlike
`swap_canvas`, whose raster the recorders never see.) A raster wholly off the page
is not embedded either.

Under `--bands` only the band's rows are filled (and charged).

The work is charged before anything is painted. From a host operator, a call
counts a step per 1024 device pixels its footprint reaches on the canvas (not
how far the transformation stretches it) plus a step per 1024 pixels recorded,
looks at the time and memory caps and the interrupt flag, and is refused whole
when one is passed: the canvas stays as it was. The time and memory caps and the
interrupt flag are looked at again between the heavy steps (the recorders' pixel
copies every million pixels, and strips of the fill), and the recorders are
written only after their copies are made, so a limit that fires part way leaves
the canvas and the output as they were. The PNG encode and the zlib compression
inside one SVG or PDF image are single calls, polled before and after, not within. Between runs only the memory cap
(for the recorders' copies) and the interrupt flag apply.

**Transparency groups.** While a program has a group open (`begingroup`, between
slices of `begin_source` and `step_n`, or inside a host operator) its marks go to a
layer, and what a host reads or exchanges is the page below that layer:
`canvas_premultiplied_rgba8`, `pages`, `page`, `into_last_page` and
`swap_canvas` all see the page without the layer's marks, and `swap_canvas` and
`set_transparent_background` replace the page the group will composite onto. A
run the host stops with its interrupt flag drops the open groups, as a time
limit does; a step limit composites what they painted. Either way the next run
starts on a whole page.

**Errors as data.** `error_details` gives the error's name, offending command,
line and file as fields, and a backtrace: the procedures that were running,
innermost first, each with the element that was running marked among its
neighbours.

```text
label:  10 20 moveto >>> show <<<
page:   gsave 42 >>> label <<< grestore
```

**Where a procedure is written.** Each frame also has `line` and `file`: the
line of the running element in the text it was written in, so an editor or an
agent can jump to it. `ErrorDetails::line` is where *execution* was (the line
that called the procedure); a frame's `line` is where the procedure *is*.

```rust
let mut interp = pscat::Interp::new();
interp.set_main_file_name(Some("scene.ps"));
interp.run_str("/label {\n  10 20 moveto\n  show\n} def")?;
interp.set_main_file_name(None);
let err = interp.run_str("1 label").unwrap_err();
let frame = &interp.error_details(&err).backtrace[0];
assert_eq!((frame.file.as_deref(), frame.line), (Some("scene.ps"), Some(3)));
# Ok::<(), pscat::PsError>(())
```

A `run` file's frames name the file as `run` was given it. The main program's
frames are named only if the host did so while the procedure was read, with
`set_main_file_name`, because every `run_str` is a text whose line 1 is its
own. A host that loads a prelude with one `run_str` and runs frames with
others therefore names the load, then clears the name: set it before the
load and reset it with `set_main_file_name(None)` after, since it stays set
across runs. A procedure read by an earlier *unnamed* program has `line: None`
(its line 3 is not the next program's line 3), and so does one made at run
time (`array`, `copy`, `astore`), read from an executable string, `token` or
`eexec`, or beyond the table's cap of a few million entries; the frame still
has its name and excerpt. `bind`, `cvx`, `cvlit` and `getinterval` keep the
line (they change the procedure in place or look at it); an element written
into a procedure with `put` keeps the line of the token it replaced, and one
written into with `copy`, `astore` or `putinterval` has none.

The table that holds this costs about 43 bytes per procedure plus 8 for each
further line it spans, nothing while the program runs, and under 2% of the time
to load the artkit and paintkit libraries (`notes/1405-source-positions.md` has
the figures). An entry of a freed procedure is dropped at the next sweep (when the
table has quadrupled, or a program runs out of names), and the unnamed main
program's go when the next program begins.

**Stopping a program.** `set_interrupt` takes an `Arc<AtomicBool>`; raising it
from another thread makes `run_str` return `PsError::Interrupted` at its next
look: before the program starts, every few thousand steps, inside any host
operator that calls `check_limits`, and when the program ends, so a run during
which the flag was up fails however short it is. A program cannot catch it,
nor an `Interrupted` a host operator returns.

**Many pictures from one program.** These compose with PostScript's own
`save` and `restore` into a frame loop. Run the program's setup once, take a
`save`, then for each picture restore to it, erase, and call the drawing
procedure. Every picture then starts from the same memory, so picture 300 is
the same pixels whether or not pictures 0 to 299 were drawn first, and each
costs only its own drawing: the interpreter and its libraries are not loaded
again.

```rust
use pscat::host::Value;

let mut interp = pscat::Interp::with_page(64, 64).expect("valid page size");
interp.run_str("/n 0 def /draw { /n n 1 add def 10 10 44 44 rectfill } def save")?;
let mut checkpoint: Value = interp.pop_value()?;
for _ in 0..3 {
    interp.push_value(&checkpoint)?;
    interp.run_str("restore save")?;        // a save object is spent by its restore
    checkpoint = interp.pop_value()?;
    interp.run_str("erasepage initgraphics draw")?;
    let _pixels: &[u8] = interp.canvas_premultiplied_rgba8();
}
// `draw` counted to 1 each time, never to 3: each run began from the checkpoint.
assert_eq!(interp.lookup("n").and_then(|v| v.as_i64()), Some(1));
```

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
`Limits`, `syntax::check_syntax`, and the `host`, `sheet`, `chalkboard`,
`handwriting` and `catalog` modules. The crate has other public modules that the binaries, the browser
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
