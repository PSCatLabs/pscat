## One operator that runs far past the limits (issue #1176, 2026-11-12)

The step, time and memory limits are read between interpreter slices, so a
built-in whose one call does far more work than its operands or its allocation
suggest ran past `--max-time` (and past `pscat-mcp`'s limits) untouched. `search` was
the reported case: a naive `windows().position()`, O(n·m), so a 16 MiB string
searched for an 8 MiB pattern took about 20 minutes in one call.

### What changed

- **`search`** uses `memchr::memmem::find` on the borrowed bytes (memchr was
  already in every build through serde_json; it is now a direct dependency).
  It is linear and no longer copies either string: the 16 MiB / 8 MiB case
  takes 0.04 s. A unit test compares it with the old scan on every haystack up
  to eight bytes long over a two-letter alphabet. A microbenchmark of 3 million
  small `search` calls got faster, not slower (user time, `ci` profile, base
  then head): a 4-byte hit in a 43-byte string 0.61 → 0.38 s, `(abcdef) (cd)`
  0.40 → 0.31 s, a miss 0.33 → 0.21 s.
- **`bind`** keeps the procedure views it has bound in this call, with the
  shallowest depth each was reached at, and skips one already bound at that
  depth or shallower. Before, a procedure holding itself twice walked 2^100
  views and sixty levels of `[p p]` walked 2^60. Binding does not depend on the
  order or the count of visits while the walk runs (the dictionary stack does
  not change), so the result is the same. Distinct views that overlap (each
  suffix of one array) are still n²/2 elements, and every name is looked up
  through the whole dictionary stack (60,000 unresolved names under 2,000
  dictionaries is 1.2e8 probes). So `bind` calls `Interp::poll_limits` inside
  its element walk: an element is a unit of work and a name lookup one more
  per dictionary on the stack, polled every 64K units, a step per 1024 units.
- **`arc` / `arcn`**. ang2 was turned past ang1 a full turn at a time
  (`while a2 < a1 { a2 += TAU }`): `1e15 0 arc` took 1.6e14 trips, and past
  about 1.6e18 degrees adding a turn no longer changes the value, so
  `1e20 0 arc` never ended, with or without limits. `gfx::turn_past` now gives
  that loop's result bit for bit, in a few steps per binade. Inside one binade
  (sign and exponent) every trip adds the same rounded amount. The exception
  is a tie, which makes the value even, after which every step is even and
  equal. So after three plain trips in one binade it skips to just short of
  the binade's edge or ang1, and crosses edges trip by trip. A unit test
  checks it bit for bit against the loop on whole and near-whole turns in both
  directions and on gaps of up to a million turns.

  Codex round 1 found that the first version, which jumped `floor(gap/TAU) - 2`
  turns, drew a full circle for `2160 -2520 arc` (gs and main: 2 elements).
  The jump landed a hair below ang1, and the loop then added one more turn.
  That case is now in `tests/limits.rs` with 146 gs-pinned element counts.
  On 8 of those, pscat makes one more segment than gs, the same on main: the
  sweep is a whole number of quarter turns, and pscat's double-precision
  value lands a hair over it where gs's single precision lands on it.

  Angles are refused as gs refuses them:
  - gs 10 gives `limitcheck` for a positive angle past about 2^31 degrees
    (`0 0 10 2.15e9 0 arc`, `0 0 10 0 1e10 arc` and `arcn`), and draws
    `2.14e9 0` (4 path elements) and `0 -1e10` (2). pscat now matches all of
    these. gs also refuses the integer `2147483520`, just below 2^31, which
    pscat draws.
  - An angle that is not finite, or -2^53 degrees or below, is `limitcheck`
    too. gs draws something for `0 -1e17 arc`, but such an angle has no unit
    precision left (recorded in HANDOFF's deviations).
  - A huge sweep is still drawn segment by segment, as gs does: gs ran
    `0 0 10 -1e9 0 arc` until it was killed. The segment loop checks a
    `LimitProbe` every 4096 segments, and the operator charges a step per 1024.
- **`flushfile`** on an input filter looks at the limits every 64 KiB it
  discards, a step per KiB as for a whole-file read. A stacked chain
  (FlateDecode over RunLengthDecode) makes about 3000 bytes per source byte.
- **`shfill`**: the function parse counts every dict it parses, a shared one
  each time it is reached, and refuses past 4096 (`limitcheck`). Thirty levels
  of a stitching function naming the level below twice was 2^30 parses (and
  that many nodes in memory) in one call. This also bounds `build_stops`'
  O(K²) evaluation. It does not bound painting: tiny-skia's gradient costs
  pixels × stops. At 300 dpi letter, 64 legs took 0.15 s, 256 took 0.35 s,
  1024 took 1.2 s and 4095 took 4.4 s, the same on base and head. That
  remains open.
- **Tests** (`tests/limits.rs`, `one_operator`). Each worst case runs on a
  thread under the suite's 60 s deadline and inside a CPU budget. The polled
  ones (`bind` over overlapping views, a huge `arc` sweep, `flushfile` on a
  FlateDecode + RunLengthDecode bomb built in a scratch file) must stop with
  `limitexceeded (time)` blamed on the operator itself, not on the `1 pop`
  after it. With the polls in `bind` and `flushfile` commented out, both of
  those tests fail (`bind` at the deadline).
  Added in Codex round 1:
  - `bind` under 2,000 dictionaries must stop inside `bind`;
  - a huge arc sweep under `steps(10)` must stop with `StepsExceeded` inside
    `arc`;
  - the gs element-count table;
  - `gfx::turn_past` against the plain loop.

  `limits_memory.rs` covers the SVG/PDF image check: a huge image is refused,
  and a 3000 × 3000 PDF imagemask passes under a 20 MiB cap that refuses a
  colour image of the same size.
- **`image` / `imagemask` / `colorimage` with SVG or PDF output**:
  `begin_image` checks the recorder's decode buffer (w × h × 4 for SVG, × 3 for
  PDF) against a memory cap before the image starts. The probe
  `30000 30000 8 [30000 0 0 30000 0 0] {()} image --svg` was 3.6 GB and 9e8
  pixel iterations in one step, and ran 10 s past `--max-time 1`. Each kind
  is charged its real buffer (`ImageCtx::recorder_bytes`):
  - SVG: RGBA for every kind;
  - PDF: a mask's packed bits, RGB for a colour image, and RGBA plus RGB and
    alpha for a stenciled image.

  Codex round 1 found the first version charging a PDF mask 3 bytes a pixel.
- New plumbing in `src/interp.rs` / `src/limits.rs`:
  - `Interp::poll_limits(steps)`: the slice loop's checks (step cap, time,
    memory), raising the same uncatchable `limitexceeded`. It is an inlined
    no-op when no limit is set.
  - `Interp::limit_probe()` → `LimitProbe` (steps left, deadline and heap cap
    as a value; the loop passes the steps its work comes to so far, so a step
    cap stops a huge arc sweep inside the loop),
    for loops in `Gfx` that cannot borrow the interpreter. `Gfx::arc` keeps its
    signature and `Gfx::arc_limited` takes the probe.

### Identity

`scripts/identity_check.sh` was run with `--base-bin` (main at 3be5e46) and
`--head-bin` (this branch before its rebase), over every example and the 12
smallest gallery pieces that use `arc`, `arcn` or `shfill`, plus the default
paintkit pick, at `--gallery-dpi 72`. Result: 56 of 56 page renders were
byte-identical across 41 files. `bind` runs in every library those files load.
After Codex round 1 it was run again against main at 6b34d79 (merged in), with
the same result. Two arc stress files were also compared, base against head:
- a grid of 474 arc/arcn pairs (whole and near-whole turns, random angles up
  to 1e6 degrees) rendered at 144 dpi: byte-identical PNGs;
- the device coordinates of 948 arcs, from `pathforall`: identical.
Path element counts for 1212 arc/arcn cases matched main exactly.

### The audit

Verdicts were measured with `--max-time 1` probes on a `ci` build of main, or
read from the code (marked).

| Operator / path | Verdict |
|---|---|
| `search` | **Fixed** (O(n·m) → linear). Measured. |
| `anchorsearch` | O(m) prefix compare, linear. Read. |
| `token` (string) | Linear per call. It copies the rest of the string every call, so a `token` loop over 16 MiB is O(n²) overall, but the limits apply between calls. Read. |
| `cvi` `cvr` `cvn` `cvs` `cvrs`, string `eq`/`gt` | Linear. Read. |
| `getinterval` `putinterval` `copy` `aload` `astore` `packedarray` | Linear in the 16M cap. `aload` checks memory first. Read. |
| `roll` `index` `copy` (stack) `counttomark` `cleartomark` | Linear; `roll` uses `j mod n` (`1 2 3 3 1e15 roll` is instant). Measured. |
| `dict` `copy` (dict) `forall` `undef` | Linear. `dict` ignores its capacity argument. Read. |
| `save` / `restore` | Linear in the journal. The first `put` after a `save` copies the whole array (O(16M) once per save level per object). Read. |
| `bind` | **Fixed** (exponential on a DAG or cycle → each view once, plus polling). Measured. |
| `==` `pstack` `stack` | Bounded (#745). Read. |
| `show` family, `charpath`, `stringwidth` | One glyph per interpreter step. Read. |
| `filenameforall` | Polled per entry (#702); the glob match is single-backtrack. Read. `resourceforall` is not implemented. |
| `readreals` `writereals` `file` `run` | Polled (#702). Read. |
| `printobject` `writeobject` / binary object sequences | Capped: reading by a slot budget; writing by the 64 MiB `MAX_BYTES` and `poll_read`. A DAG is expanded copy by copy, but within the cap. Read. |
| `flushfile` on a filter | **Fixed** (polls). Measured. |
| image drain to EOD (`ImageCtx::drain_filters`), `readhexstring` / ASCIIHex skip loops (`ensure_output`), the scanner reading a filter | Open: one call can drain a whole decode chain. The file layer has no `Interp`. Read. |
| FlateDecode | Bounded by accident: a bug truncates any input byte that expands past 512 bytes (below), so a uniform bomb errors out early. Measured. |
| `arc` `arcn` | **Fixed** (normalisation O(1), gs-pinned limitcheck, polled segments). Measured. |
| `arcto` `arct` | One line and one curve. Read. |
| dashed `stroke` / `strokepath` | tiny-skia gives up past its dash cap. `[0.00001] 0 setdash` on a 600-point line: 0.03 s. Measured. |
| `flattenpath` | Open: up to 65,536 segments per curve (depth 16), with no memory check. 1000 curves at 1e20 took 0.7 s and about 1.8 GB. Measured. |
| `fill` / `eofill` / `clip` (pscat's coverage rasteriser) | Open: lines × bands. 100,000 curves: 2.5 s in one `fill`, 1M: well past 8 s. Measured. |
| `pathforall` `pathbbox` `rectfill` `rectstroke` | Linear; `pathforall` is frame-driven. Read. `reversepath`, `ufill` and the other user-path operators are not implemented. |
| `shfill` (function parse) | **Fixed** (4096-dict cap). Measured. |
| `shfill` (painting a many-leg stitch) | Open: pixels × stops, 4.4 s for 4095 legs at 300 dpi. Measured. |
| `image` `imagemask` `colorimage` (raster) | Bounded by the device box. A file source is one bulk read of `needed()` bytes. Read. |
| SVG / PDF image recorders | **Memory checked up front**; the w × h loop is still unpolled under a time limit alone. Measured. |
| Type 1 charstrings | Op budget of 100,000 and call depth of 30 per glyph. Open: `callsubr` clones the subroutine, and every glyph decrypts all Subrs again. Read. |
| `kernelimage` / `kernelarray` (compiled) | Polled (`RunCtl`, every lane batch, at loops and calls). An 8000 × 8000 kernel of 200-trip loops stops at 1.02 s under `--max-time 1`. Measured. |
| `kernelimage` with most pixels retired | Open: the retired-pixel list (16 B a pixel) is built and sorted with no poll and no memory check. 8000 × 8000 all-retired: 5.2 s in the compiled phase under `--max-time 1`. Measured. |
| kernel compile | Capped (`.kernelcaps`) and polled. Read. |
| GPU dispatch | Documented in SECURITY.md (#1065). |
| noise operators | Octaves capped at 32. Read. |
| patterns, halftone screens, Type 4 / mesh shadings, SubFileDecode | Not implemented. |

### Not done here (proposed follow-ups)

- **FlateDecode truncates highly compressible streams** (a correctness bug).
  `Decoder::Flate` feeds the decompressor one byte into a 512-byte output
  buffer and moves to the next byte even when that buffer filled, so the output
  still pending is lost. 1 MiB of spaces, compressed, decodes to 354,232 bytes
  (pscat) against 1,048,576 (gs 10), or ends in `ioerror`. An image with large
  flat areas under FlateDecode would be cut short. Fixing it removes the
  accidental cap on decompression bombs, so poll the image drain at the same
  time.
- Poll the image drain to EOD, `ensure_output`'s skip loops and the scanner
  reading a filter. This needs a `LimitProbe` held by the file layer.
- `fill`'s band loop and `flattenpath`: a `LimitProbe` in `coverage::rasterize`
  and `PsPath::flattened`, or a cap on total segments.
- Type 1: borrow the subr instead of cloning it, and decrypt Subrs lazily.
- Kernel retirements: cap the retired-pixel list (fall back to the whole
  interpreter past it), or poll and memory-check while it grows.
- SVG / PDF image recorders: a `LimitProbe` per row.
- Many-stop gradients: resample a stitching function with more than a few
  hundred stops into a fixed table before tiny-skia sees it, or lower the cap.
