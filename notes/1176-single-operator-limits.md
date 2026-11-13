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
  suffix of one array) are still n²/2 elements, so `bind` also calls
  `Interp::poll_limits` every 64K elements, charging a step per 1024.
- **`arc` / `arcn`**. ang2 was turned past ang1 a full turn at a time
  (`while a2 < a1 { a2 += TAU }`): `1e15 0 arc` took 1.6e14 trips, and past
  about 1.6e18 degrees adding a turn no longer changes the value, so
  `1e20 0 arc` never ended, with or without limits. A gap of more than four
  turns now jumps most of the way in one step. Ordinary arcs never jump, so
  they are bit-identical (identity check below). Angles are refused as gs
  refuses them:
  - gs 10 gives `limitcheck` for a positive angle past 2^31 degrees
    (`0 0 10 2.15e9 0 arc`, `0 0 10 0 1e10 arc` and `arcn`), and draws
    `2.14e9 0` (4 path elements) and `0 -1e10` (2). pscat now matches all of
    these.
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
  O(K²) evaluation.
- **`image` / `imagemask` / `colorimage` with SVG or PDF output**:
  `begin_image` checks the recorder's decode buffer (w × h × 4 for SVG, × 3 for
  PDF) against a memory cap before the image starts. The probe
  `30000 30000 8 [30000 0 0 30000 0 0] {()} image --svg` was 3.6 GB and 9e8
  pixel iterations in one step, and ran 10 s past `--max-time 1`.
- New plumbing in `src/interp.rs` / `src/limits.rs`:
  - `Interp::poll_limits(steps)`: the slice loop's checks (step cap, time,
    memory), raising the same uncatchable `limitexceeded`. It is an inlined
    no-op when no limit is set.
  - `Interp::limit_probe()` → `LimitProbe` (deadline and heap cap as a value),
    for loops in `Gfx` that cannot borrow the interpreter. `Gfx::arc` keeps its
    signature and `Gfx::arc_limited` takes the probe.

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
