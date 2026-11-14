## The rest of the one-operator limit gaps (issue #1188, 2026-11-14)

#1176 (`notes/1176-single-operator-limits.md`) audited the built-ins whose
one call can do far more work than its operands suggest and left eight open.
Each was measured on `main` first (release build, `--max-time`/`--max-memory`
as given), then fixed by polling the limits inside the loop or by bounding the
work. Output does not change: the identity check below.

| Item | Before | After |
|---|---|---|
| `fill` of 100,000 page-sized curves | 1.8 s under `--max-time 0.2` at 72 dpi, 5.8 s at 300 dpi; the fill alone 1.0 s / 4.2 s with no limit | stops inside `fill` at 0.21 s / 0.25 s; the fill alone 0.77 s / 2.9 s |
| `rectfill` of 100,000 page-tall rectangles, 300 dpi | 2.3–3.2 s, no look at the limits | 1.9–2.1 s, under the limits like `fill` |
| `flattenpath` of 2000 curves at 1e20 | 1.7 s under `--max-time 0.1`, 0.95 GB RSS under `--max-memory 50` | stops inside `flattenpath` at 0.10 s; 37 MB |
| `shfill`, 4095-leg stitch, 300 dpi | 5.2 s under `--max-time 0.5` | 0.6 s with or without a limit |
| Type 1, 1000 Subrs of 64 KiB, 200 glyphs | 25 s under `--max-time 1` | 0.7 s (mostly scanning the 131 MB file) |
| Type 1, one 1 MB subr called 30,000 times | 0.45 s user (scales with the subr: 30,000 copies) | 0.01 s |
| Type 1, 500 glyphs each calling a 16 MB subr | 12.7 s under `--max-time 1` (one slice is 512 glyphs) | stops inside `show` at 1.3 s (mostly scanning the 32 MB file) |
| `kernelimage`, 8000 x 8000, every pixel retired | 4.5 s under `--max-time 1` | 1.27 s, inside `kernelimage` |
| SVG / PDF recorder, 8000 x 8000 image, no data | 0.65 / 0.46 s under `--max-time 0.2`, blamed on the next operator | 0.41 / 0.20 s, inside `image` |
| `bind`, one 8 MB name under 2000 dicts | 4.6 s under `--max-time 0.1` | 0.01 s user |
| Stencil, 64 x 64 image under a 4096 x 4096 mask | passed `--max-memory 1`: 208 MB RSS (SVG), 144 MB (PDF) | `limitexceeded (memory)` before it starts |

### What changed

- **`fill`/`eofill`** (`coverage::rasterize_limited`, `Gfx::fill_limited`).
  The rasteriser takes a `LimitProbe`. It counts work as one unit per edge
  made and one per edge laid into a band, and looks at the probe (time, heap,
  a step per 1024 units) every 64K units. The heap check matters as much as
  the clock: 100,000 curves are 10^8 edges at 16 bytes, and they were freed
  before the next look between slices, so a 100 MiB cap never saw them.
  Every edge used to visit every band. A `BandWalk` now gives each band only
  the edges whose y range reaches it, in the edges' own order. Each band
  therefore adds the same f32 areas in the same order as before, so coverage
  is bit for bit the same. The range is a superset of what `accumulate`
  keeps, and a unit test checks that on edges at, just off and between band
  edges, including NaN ends. A path that fits one band (every glyph) is not
  walked this way, so text costs what it did. The operator charges a step per
  1024 units. `fill_coverage`, which glyphs use, stays a wrapper with
  `LimitProbe::NONE`. `rectfill` goes through `fill_limited` too.

  The first version was an index of every edge for every band it crosses.
  For page-tall edges at 300 dpi that is 33 entries, 130 bytes an edge, and
  a million-rectangle `rectfill` ran slower than main. The walk keeps an
  active list instead: the edges that start in a band are merged, in edge
  order, into those still reaching it. That is a few words an edge, and a
  band costs the edges it gets. Measured on a busy machine, that `rectfill`
  took 16 s, against 20–29 s on main.

  `clip` was not changed: it goes through tiny-skia's `Mask::fill_path`,
  not this rasteriser. It is still unpolled (below).
- **`flattenpath`** (`PsPath::flattened_limited`). The probe is looked at
  every 4096 chords and a step is charged per 1024 chords. The current path
  is replaced only once the whole path is flat, so a limit leaves it as it
  was. Before, the gigabyte flattened path became the current path and stayed.
- **`shfill`** (`Shading::raster_stops`). A ramp of more than 1024 stops
  reaches tiny-skia resampled at 1024 even positions, read as tiny-skia reads
  stops: below the first, past the last, and the last of several at one
  position. The gallery, examples and selftest drivers have at most 10 stops,
  measured with a probe build over all 150 files. A smooth function is
  sampled at about 64 positions plus two per leg, so only a ramp of about 480
  legs or more is resampled. SVG and PDF keep every stop. This is a recorded
  deviation in HANDOFF.
- **Type 1** (`type1::FontData::subr`). The charstring interpreter asks for a
  subroutine by number when a glyph calls it. `type1_glyph` decrypts it then,
  once per glyph (a `RefCell<HashMap>` memo; the borrow is dropped before the
  recursive call), and hands back an `Rc<[u8]>`. Before, every glyph copied the
  whole Subrs array and decrypted every entry, and `callsubr` cloned the
  subroutine on each call. `src/file.rs` is untouched: it still uses
  `decrypt_charstring`.

  That alone left one shape, found in review. `show` paints one glyph a step
  and a slice is 512 steps, so 500 glyphs that each decrypt a 16 MB
  subroutine still ran 12 s inside one slice. The `Frame::Show` arm now
  builds a probe, which `ShowCtx::step` passes to `type1_glyph`. The probe
  is looked at before the glyph and before each subroutine it decrypts. A
  limit found inside the charstring stops it, as a missing subroutine would,
  and is then raised in its place.
- **`kernelimage`/`kernelarray`** (`kernel::run`). Each worker puts a band's
  retired pixels in raster order with a stable sort by row. Tiles run left to
  right, so within a row they already are. The bands are then joined in
  order, replacing one sort of every pixel, about 2 s for 64 million. Joining
  and building the drivers' `Pix` list (`pixel_list`) poll the limits. Under
  a memory cap, the run stops as soon as the list and the interpreter's copy
  of it (16 bytes plus two `Object`s a pixel) would not fit in what the cap
  leaves (`Interp::memory_left`). The interpreter then does the whole image,
  through the existing `Stop::Cancelled` route. Before, the list grew
  unchecked and the copy was then refused with `limitexceeded (memory)`. Now
  the image is interpreted under the cap.
- **SVG/PDF image recorders** (`ImageCtx::rasterize`, `record_svg`,
  `record_pdf`, `composite`). They look at the probe once a row, for time and
  heap with no step charge, so `--max-steps` behaves the same in every output
  mode. The image frame builds the probe from the split interpreter fields
  (`interp::limit_probe`, shared with `Interp::limit_probe`), only in the
  `Frame::Image` arm.
- **Stencil estimate** (`ImageCtx::recorder_bytes`, `composite_size`). The
  composite is counted at the expanded size `composite` builds. A new
  `composite_size` helper is shared by the estimate and `composite`, so the
  two cannot drift. Every buffer alive with the composite is counted: for SVG,
  the pixmap and the copy `encode_png` demultiplies (12 bytes a pixel); for
  PDF, the RGB and alpha split (8 bytes a pixel). Only the stenciled branch
  changed.
- **`bind`** looks names up with `load_id(n.id())`, which is what name
  execution does and gives the same result as `load(&str)`.

### Tests

- `tests/limits.rs` (`one_operator`). `fill`, `flattenpath`, `show` of
  Type 1 glyphs that each decrypt a 16 MB subroutine, and the SVG and PDF
  image and stencil recorders must stop with `limitexceeded (time)` blamed
  on the operator itself. `bind` of a long name, the 4095-leg `shfill`
  at 300 dpi, and both Type 1 shapes must finish inside a CPU budget. The
  Type 1 test encrypts its own subr 0 and glyph; the other Subrs are zero
  strings, since nothing calls them. All of these fail under
  `PSCAT_TEST_BUDGET_SCALE=0.001`. Run against main's binary, the time cases
  run past their limits and blame the next operator.
- `tests/limits_memory.rs` checks four cases:
  - the stencil estimate (refused at 4096, runs at 64);
  - `fill` under 100 MiB: `limitexceeded (memory)` inside `fill`, where main
    filled;
  - `flattenpath` under 100 MiB: refused, and the path is not kept;
  - an all-retired 2000 x 2000 `kernelimage` under 50 MiB plus a 0.3 s clock:
    `limitexceeded (time)` in the interpreter, where main gave
    `limitexceeded (memory)` after building a 64 MB list.
- Unit tests:
  - `coverage::the_band_walk_covers_every_band_an_edge_reaches`;
  - `shading::a_ramp_of_very_many_stops_is_resampled_for_the_raster`.

### Identity

`scripts/identity_check.sh` was run with `--base-bin`/`--head-bin` over
every tracked gallery piece, example and selftest driver: 168 files, gallery
at 144 dpi. It ran under `--pscat-args "--max-time 900 --max-memory 8192"`,
so the probes are live, and against main at d01f9e7 after the last rebase.
207 of 207 page renders were identical. Four files were skipped as failing
identically in both trees: the `examples/hybrid/compare_*` sheets, which
need `pscat sheet --sweep`. The control is the 4095-leg ramp, which the
resampling changes, and it differed.

PDF was checked separately, since the script compares PNG only. The 25
examples and gallery pieces that use `image`, `imagemask`, `colorimage`,
ImageType, `shfill`/`gradfill` or `flattenpath` were rendered to PDF. Main
was run twice to show its output is deterministic, then compared with this
branch: all 25 are byte-identical. SVG varies from run to run (#258) and
was not compared.

### Not done here

- The FlateDecode drain paths (image drain to EOD, `ensure_output`, the
  scanner reading a filter) are #1187's.
- `clip` of a path with very many curves is tiny-skia's `Mask::fill_path`,
  unpolled. 100,000 curves took 1.4 s at 72 dpi and 5.6 s at 300 dpi under
  `--max-time 0.2`, the same before and after. Moving `clip` to the coverage
  rasteriser would change clip edges. A pre-count of the flattened edges
  against the limits would not.
- `shfill` does not look at the clock while tiny-skia paints. A single call
  is bounded at pixels x 1024 stops instead.
- A Type 1 glyph still decrypts each subroutine it calls in full, once. A
  lazily decrypted prefix would bound that by the bytes actually executed,
  which the operator budget caps.
- Without a memory limit, an all-retired `kernelimage`'s pixel list is
  bounded only by the clock it now looks at.
