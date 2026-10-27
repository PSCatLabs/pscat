## Bulk read of file image sources in `ImageCtx::fill` (issue #896, 2026-10-26)

Follows #835. Profiled first (release build, macOS `sample` over ten 4096²
CMYK images read from a file): `PsFile::read_byte` plus `ImageCtx::fill`
were 766 of 1505 samples, about 51%. The rest was `rasterize` and `rgb_at`.

What changed:

- **`ImageCtx::fill`** (src/image.rs) reads a file source with one
  `PsFile::read_into(&mut data, want)` instead of a `read_byte` per sample
  byte. A short count is end of data, as a `None` was. A closed file is
  still end of data rather than `ioerror`. `read_byte` returns `None` for
  a closed file, `read_into` raises `ioerror`, so `fill` checks `is_open`
  first. An output file is `ioerror` either way.
- **`read_into` on a filter** (src/file.rs) copies whole runs of the
  decoder's output buffer instead of looping over `read_byte`. A single
  byte is pushed directly, because ASCIIHex decodes one byte per pull and
  the slice copy cost more than it saved there. The byte, line and
  position bookkeeping is one helper (`count_read`) shared by both arms.
  If the decoder fails partway, the bytes read before the error stay in
  the buffer, counted, and then the error is returned: the state a
  `read_byte` loop leaves. `readstring` and `readhexstring` reach this
  path through realsio's `read_into` calls (#773).
- **The decoders are unchanged.** Each one still pulls its source a byte at
  a time, so it never reads past its EOD marker and the program resumes on
  the right byte. That is why filtered sources gain little (table below).
  Reading the decoder input in bulk is the next step: Flate over a `Bytes`
  source could hand `decompress` the remaining slice and advance by the
  `total_in` delta, which never over-reads.
- SubFileDecode is not implemented (`filter` raises `undefined` for it),
  so there was nothing to keep for it.

Checks:

- The `filters` fuzz target (`fuzzing::filters`) now builds each chain
  twice. It reads one copy byte by byte and the other with `read_into`,
  at a chunk size picked by the top two bits of byte 0. It panics on any
  difference in the bytes, the error, any layer's byte and line counts,
  or the bottom file's position. The unit test
  `bulk_reads_match_byte_reads_through_every_filter` runs it over every
  committed filter seed (DCT included) at all four chunk sizes, over
  CR/LF-heavy decoded output, over errors that come after some output,
  and over pseudo-random input for all 64 selectors. The line and byte
  counts show in no render, so this test is what covers them: with
  `count_read` skipped it fails ("layer counts differ").
- tests/images.rs: a raw `currentfile` source takes exactly its bytes
  (checked with `fileposition`), a filter ending mid-row renders what
  arrived and the program resumes, a closed-file source renders zeros
  without an error, and bad hex data is `ioerror`.
- `selftest/drivers/image_sources.ps` page 5: raw `currentfile` sources
  with no filter. The data is followed on the same line by the next call.
  Before it, no tracked file read image samples from an unfiltered file:
  two broken heads (`Bytes` arm one byte short; `fill` over-reading 8
  bytes) rendered the whole corpus identically.
- Two-binary identity check over the image drivers, the tracked files that
  call `image`, `colorimage`, `imagemask` or `readstring`/`readhexstring`,
  and the artkit, drychalk, etching, fillkit, halftonekit, hatchkit,
  lettering and released loaders: 130 of 130 page renders identical
  across 97 files. Broken heads against base, image file set:
  `Bytes` arm one short and 8-byte over-read each fail
  `image_sources.ps`; the filter arm dropping a byte per chunk makes
  `image_sources.ps` pages 1 and 2 and `postcard.ps` differ. SVG and PDF
  byte comparison over the image drivers, fillkit, postcard and
  bands_uneven: identical. The six bench renders as PNG: identical.

Measured with pscat only, release, `--page 983x983 --dpi 300 --headless`.
The bench files were regenerated to #835's description, because its
files were scratch. CPU seconds are best of 5 from
`scripts/bench_compare.sh`, with the identical-binary control within
0.02 s of the base. Instructions retired come from `/usr/bin/time -l`:

BENCH_TABLE
