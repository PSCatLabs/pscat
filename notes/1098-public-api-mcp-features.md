## Supported library API for halftone, review sheets, chalkboards and the catalog (issue #1098, 2026-11-11)

pscat-mcp (PSCatLabs/pscat-mcp) embeds pscat as a library and built its
`halftone` option, `review_sheet`, `chalkboard_math` and
`describe_art_capabilities` on `#[doc(hidden)]` modules (all wrapped in its
`src/pscat_extras.rs`). Any release could have broken it silently. This adds
a documented, semver-covered API for the four, in pscat-mcp's priority order.

**What was added** (all in the supported surface, so `missing_docs` checks it):

- `Page::halftone(&self) -> Page` and `Page::blank(w, h) -> Option<Page>`.
- `Interp::page_count()`, `Interp::page(i) -> Option<Page>` (one copy at a
  time) and `Interp::into_last_page(self) -> Page` (no copy), plus
  `Interp::set_transparent_background()` and
  `Interp::set_pdf_info_from_dsc(source)` (what `pscat --transparent` and
  `pscat --pdf` do). These sit in an `impl Interp` block in `src/page.rs`,
  so `interp.rs` is untouched.
- `pscat::sheet`: `SheetLayout`, `RowSize`, `CellSize`, `Sheet`,
  `text_strip`, `zoom`, `SheetError`/`SheetErrorKind`, `MAX_SIDE_PX`,
  `MAX_TEXT_CHARS`, `MAX_TEXT_PX`. A facade over `contact_sheet`.
- `pscat::chalkboard`: `BoardConfig`, `Driver` (with `prepare`),
  `ChalkError`, `explain`, `CHALK_COLORS`. A facade over `chalk`.
- `pscat::catalog::capabilities() -> serde_json::Value`, which is
  `capabilities::payload_json`.

**tiny-skia stays private.** pscat-mcp named `tiny_skia::Pixmap` itself, so it
had to track pscat's exact tiny-skia version. There were two ways to fix that:
`pub use tiny_skia`, or a raster type pscat owns. Under Rust API convention a
re-exported dependency is part of the public API, so every tiny-skia release
that breaks semver (it is 0.x, so any minor release can) would become a
breaking pscat release. An owned type keeps pscat's semver independent of
tiny-skia. pscat already had one, `Page`, so every new item takes and returns
`Page`. serde_json is different: it is 1.x and stable, so `catalog` returning
a `serde_json::Value` and `BoardConfig` reading one is an acceptable public
dependency.

**Facades, not un-hiding.** The hidden types could not just be made visible.
`chalk::ChalkError` is an exhaustive enum that pscat-mcp matches on, so
marking it `#[non_exhaustive]` would break pscat-mcp's build. `chalk::Config`
and `contact_sheet::Layout` have public fields, which would freeze their
representation. `contact_sheet` takes `Pixmap` everywhere. So each facade
wraps the hidden type opaquely. The hidden items are unchanged, because
pscat-mcp's pinned build still uses them.

**Design points:**

- `Page::halftone` composites a transparent page onto white before screening.
  `halftone::screen` reads premultiplied bytes as opaque RGB, so a
  transparent pixel (a chalkboard with `"board": false`) would screen as
  solid ink. Opaque pages come out byte-identical to `pscat --halftone`. The
  CLI refuses `--transparent` with `--halftone` instead. A library caller
  can hold a transparent `Page` (from `Driver::prepare`), and screening it
  as print on paper is the useful reading. One change pscat-mcp will see
  when it ports: its `halftone` option on a `"board": false` chalkboard
  screens onto white, where the hidden path inked the whole background black.
- `Sheet` keeps the `RowSize`s its layout was made from. `put_image` refuses
  an image that is not the planned size, and `put_header`/`put_caption` refuse
  a strip larger than its slot. A strip may be smaller than its slot, so a
  server can plan with an upper bound before any text is set. A bad index is
  an error, not a panic. `SheetLayout::new` drops the CLI advice
  (`-- use fewer cells, a lower --dpi ...`) from its too-large message.
- `text_strip` runs PostScript with no limits (the text goes in as an
  escaped string literal, never as code), so it caps the input: 1000
  characters, type at most 256 px tall.
- `ChalkError::message()` returns the unprefixed config message
  (`chalk.color must be one of ...`), which pscat-mcp rewrites into its own
  flat argument names. `Display` adds `config: ` in front.
- `Driver::prepare` must run after `enable_pdf`, because `Gfx::set_pdf_info`
  does nothing without a PDF recorder. This is documented and tested. The two
  `Interp` methods it uses are public too: pscat-mcp's worker applies them to
  every generated program (chalkboards and handwriting), not only through a
  chalk driver.
- `Sheet::new` borrows the layout and keeps a copy (so `contact_sheet::Layout`
  gained `Clone`), so the caller can still read positions while drawing.
- `sheet::zoom` records `k` times the page's resolution, so a zoomed page
  prints at the same physical size.

**How pscat-mcp moves over** (`tests/library_api.rs` has one test per row and
checks the before and after give the same bytes):

| Hidden today | Supported |
|---|---|
| `halftone::screen` + `png::encode` over `gfx().pages()`, `gfx().pixmap`, `has_trailing_art`, with `dpi / 72` passed in | `interp.page(i)?.halftone().to_png()` for `i in 0..interp.page_count()` |
| `mem::replace(&mut gfx_mut().pixmap, ..)` then the canvas or the last shown page | `interp.into_last_page()` |
| `tiny_skia::Pixmap::new` + white fill | `Page::blank(w, h)` |
| `contact_sheet::{Layout, RowSize, CellSize}` | `sheet::{SheetLayout, RowSize, CellSize}` |
| `contact_sheet::render_text` | `sheet::text_strip` |
| `contact_sheet::zoom_nearest` | `sheet::zoom` |
| `Layout::new_sheet` + `frame` + `blit_at`, `png::encode(&sheet, 1.0)` | `Sheet::new`, `put_header`/`put_image`/`put_caption`, `into_page().to_png()` |
| `chalk::Config::{default, from_value, apply, check}`, `.width/.height/.dpi/.board` | `chalkboard::BoardConfig::{default, from_value, apply, check, page_size, dpi, has_board}` |
| `chalk::driver`, `chalk::Driver` fields | `BoardConfig::driver`, `Driver::{source, page_size, dpi, transparent}` |
| `chalk::explain`, matching `ChalkError` variants | `chalkboard::explain`, `ChalkError::{code, token, message, is_input_error}` |
| `Gfx::set_transparent_background` | `Interp::set_transparent_background` (or `Driver::prepare`) |
| `pdf::scan_document_info` + `Gfx::set_pdf_info` | `Interp::set_pdf_info_from_dsc` (or `Driver::prepare`) |
| `chalk::CHALK_COLORS` | `chalkboard::CHALK_COLORS` |
| `capabilities::payload_json` | `catalog::capabilities` |

The syntax scan (`lexer`, #1096) and the default page size
(`PageSizeSource`, #1097) are covered by those issues' PRs.

**Checked against pscat-mcp itself.** pscat-mcp `a9a019d` was cloned into a
scratch directory with its `pscat` dependency pointed at this branch, and
`src/pscat_extras.rs` was ported to the API above. The port keeps the
module's own interface, except that `SheetCanvas` takes row and cell indices
instead of positions, which changes three call sites in its `src/sheet.rs`.
It compiles with its `tiny-skia` dependency removed, and every hidden item
it uses is gone apart from `lexer` and `PageSizeSource`. On the port, its
unit tests (63, including its chalk config error tests), `tests/backend.rs`,
the end-to-end `tests/chalkboard.rs` and `tests/eval_replay.rs` pass.
`tests/eval_server.rs` fails only because it reads pscat's git `source` from
`Cargo.lock`, which a path dependency has none of. The rest of its
integration tests were not run.

**Deferred** (the PR says `Refs #1098`, not `Closes`, because handwriting is
in the issue's ask and not here):

- Handwriting. The issue lists it, and pscat-mcp does not use it today.
  Its argument handling lives in the binary (`src/subcmd/handwrite.rs`), so a
  library function means moving that code first.
- Packed or wrapped sheet layouts (`Layout::packed`: stacked columns,
  `--cols`), mats (`composite_on_mat`), and the sweep grid
  (`contact_sheet::compose`). pscat-mcp's `review_sheet` uses none of them.
- `chalk::render` (a one-call LaTeX to PNG/SVG/PDF) and a halftone screen
  with a custom cell size or angle.
