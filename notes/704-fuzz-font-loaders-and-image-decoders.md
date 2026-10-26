## Fuzz targets for the font loaders, filters and image operators (issue #704, 2026-10-25)

**What shipped**
- Five new cargo-fuzz targets (`fuzz/fuzz_targets/`, bodies in
  `src/fuzzing.rs`), seeds in `fuzz/seeds/<target>/`, all replayed by
  `tests/fuzz_corpus.rs` and added to the weekly `fuzz` workflow's matrix:
  - `truetype`: a font file (glyf, CFF/OpenType, `.ttc`, variable) through
    `font::prepare_face`, the pure half of the loader that `findfont` and
    `addFont` share (it leaks and registers nothing, so a long run does not
    grow the process), then about 24 glyph ids spread over the face (plus a few names and code
    points) through `paint_resolved_glyph` in all three show modes.
  - `type1_charstring`: decrypted charstring bytes, a subroutine list and a
    name table for `seac`, straight into `type1::run`.
  - `type1_font`: a whole PFA as PostScript, then text in every font it
    defined, so the `eexec`, `RD`, `definefont` and charstring decrypt paths
    all run. The eexec cipher is self-synchronising, so mutating the
    encrypted region stays local to a few bytes of plaintext.
  - `filters`: ASCIIHex, ASCII85, RunLength, eexec, Flate, LZW and DCT, one
    or two chained, with the output capped at 1 MiB.
  - `image`: a `%!` program runs as PostScript (the five
    `selftest/drivers/image_*.ps` drivers are seeds); anything else is a
    recipe (form, bits 1..16, colour space, size, Decode, matrix, how the
    data arrives, payload) that builds the call, so a mutation stays a
    valid-looking `image` call instead of dying in the scanner.
- Not fuzzed: `readreals` (a file-path API; its parser is `from_le_bytes`
  on exact chunks; the read path is `interp`'s).
- `tests/fuzz_corpus.rs` replays every input on a thread under a 30 s
  deadline (a hang fails, naming the file), re-runs truncations and flipped
  bytes of every new seed, and checks that `(/dev/zero) (r) file` (a seed,
  `fuzz/seeds/interp/read_dev_zero.seed`) and a FIFO nobody writes to end on
  a limit error, not on luck. The limits themselves were already tested in
  `tests/limits.rs` (#702); this pins them under the fuzz harness's limits.
  `fuzzing::interpreter_result` returns how a run ended.

**What fuzzing found (all fixed, each kept in `fuzz/regressions/<target>/`
and with a unit test where one fits)**. Found by the fuzzer:
1. `ttf-parser` `debug_assert!` on a `.ttc` header with a huge font count
   (`parse_face` checks the count against the file size first).
2. `ttf-parser` `debug_assert!` in `GSUB` FeatureVariations (a count too big
   for the file).
3. `ttf-parser` `u16` overflow in `var_store::evaluate_region` (`HVAR` region
   index times axis count), only with overflow checks on. Hand-built 1 KB
   regression.
4. `zune-jpeg` `assert!` in the aarch64 NEON upsampler on a crafted
   progressive JPEG: a real release-mode panic on Apple silicon and linux
   arm64. The regression input only fails on aarch64.
For 1 to 3 release builds compile the check out; debug builds and the `ci`
test profile panic. `parse_face` (the parse) and the advance-width lookup run
under `guard::catch` (`src/guard.rs`); the DCT filter decodes under it and
reports `ioerror`. Outline calls are deliberately not guarded: the sink calls
back into pscat, and a guard there would hide pscat bugs.
Found by reading the code while writing the targets, then reproduced:
5. Type 1 charstring call tree: 30 subroutines each calling the next twice is
   2^30 operators inside `CALL_DEPTH_LIMIT` (42 s for one glyph in a
   ci-profile build, measured). A glyph now has an operator budget
   (`OP_BUDGET`, 100 000; real glyphs use hundreds), shared with `seac`
   components; the OtherSubr result stack keeps its newest 48 values.
6. LZW: the code table grew for every code after it was full, each entry up
   to ~4 KB for a 12-bit code. Capped at 4096 codes.

**Design notes**
- Fuzzed code is production code: `prepare_face`, `parse_face`,
  `type1::run`, `Decoder`, `image`/`colorimage` all run as shipped; the
  harness only builds inputs.
- cargo-fuzz builds with debug assertions and overflow checks on, and
  libFuzzer's panic hook aborts before unwinding. The four targets that
  reach `ttf-parser` and `zune-jpeg` call
  `fuzzing::tolerate_dependency_panics()` in `init:`: it lets a panic pass
  only when it starts in `~/.cargo/registry` code *and* pscat is inside a
  `guard::catch`, i.e. exactly where production code catches it too.
  Everything else still aborts. In `cargo test` there is no such hook, so a
  guarded panic is swallowed as in production and an unguarded one fails the
  replay (checked: removing the advance-width guard fails the HVAR input).
- Real fonts are not committed as seeds (size, and the OFL reserved-name
  rule for modified Liberation/TeX Gyre files); the seeds are small
  fontTools-built faces. The local runs also used subsets of
  `fonts/` (`pyftsubset`) in the working corpus: Liberation Sans, TeX Gyre
  Pagella (CFF), Caveat, Noto Sans KR, Oswald, Noto Sans Thai, EB Garamond.

**Local runs** (7 minutes each, release build with ASan and debug
assertions, `-timeout=10 -rss_limit_mb=2048 -detect_leaks=0`): see the PR
body for the final table.

**Not done / follow-ups**: the `ttf-parser` and `zune-jpeg` panics above
are worth reporting upstream; an `.pfb` reader (no Type 1 binary container
is read); PDF reader target when #558 lands; `ImageType 3`
InterleaveType 1/2 (#765) when they exist.
