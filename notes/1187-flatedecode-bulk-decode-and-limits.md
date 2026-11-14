## FlateDecode decodes in full, and every decode filter looks at the limits (issue #1187, 2026-11-12)

### The bug

`Decoder::Flate` read one byte from its source, gave it to the inflater with
a 512-byte output buffer, and went on to the next byte. miniz_oxide's
streaming `inflate` keeps the output that did not fit in its 32 KiB window
(`dict_avail`). The next call only hands that output over and returns
**without taking any input** (`bytes_consumed = 0`, `stream.rs`). So whenever
one compressed byte expanded past 512 bytes, the byte after it was read off
the source and thrown away. The stream then came out short, ended early or
ended in `ioerror`: 1 MiB of spaces gave 354,232 bytes (gs: 1,048,576), and a
FlateDecode image with large flat areas was cut short or failed.

### What changed

- **FlateDecode** (`FlateState::pull`, `src/file.rs`) lends the inflater the
  source's whole run of available bytes through the new `PsFile::with_input`
  (a raw file's rest, or a filter's decoded run). It consumes only `total_in`'s
  delta. tinfl gives back look-ahead bits at the end of the stream, so the
  source still stops at the byte after the zlib stream: the inline-data
  contract (`currentfile /FlateDecode filter`) holds, tested raw, through
  ASCIIHex, and against gs. A pull decodes up to 32 KiB into a reused `Vec`
  (`decompress_vec`, so no buffer is cleared per pull).
  - The loop stops when it gets output, the stream ends, or the source ends.
    "Input offered, nothing taken, nothing made" is an `ioerror`, never a spin.
  - A corrupt stream hands over the output from before the damage first, then
    raises `ioerror` on the next pull, the same bytes the byte-at-a-time
    decoder gave.
  - A limit error from a filter below passes through as itself, never as
    `ioerror`.
- **The other decoders** were checked for the same fault and don't have it:
  LZW pushes one dictionary entry (≤ 4 KB) per pull, and RunLength, ASCII85,
  ASCIIHex and eexec push every byte they make. DCT is not streamed: it
  buffers one JPEG and decodes it whole. It now puts the header's
  `w × h × components` to the memory cap before decoding, since a few hundred
  bytes can claim 65535 × 65535.
- **Limits (`limits::DecodeBudget`).** The fix removed an accidental cap on
  decompression bombs, so a few KB can now expand without bound and one call
  can drain it. #1176 listed these drain paths as open: the image drain to
  EOD, `ensure_output`'s skip loops, the scanner reading a filter, and
  `readhexstring`.
  - The interpreter now holds one `Rc<DecodeBudget>`, and every filter it
    makes (`filter`, `eexec`: `PsFile::filtered_limited`) holds a handle.
    `ensure_output` charges each pull's output, in any layer, at a step per
    KiB. The step cap is checked as each KiB is counted. Time and heap are
    checked every 64 KiB (`LimitProbe::check`).
  - `read_into` asks `check_alloc` before it grows the caller's buffer (an
    image's samples).
  - The interpreter takes the decoded KiB as steps and refreshes the probe
    wherever it reads its limits (`sync_decode_budget`: `limit_budget`,
    `after_slice`, `poll_slow`, `poll_read`, `poll_limits_slow`). It resets
    the budget in `set_limits` and `begin_source`, so bytes decoded before the
    limits, or in an earlier program, are never charged.
  - Because the handle is shared, a filter made before `set_limits` obeys the
    limits set after it.
  - With no limits, a pull reads one bool.
  - `flushfile` still polls every 64 KiB and once at the end. It charges a
    raw file's bytes as steps itself, since a raw file has no budget; a
    filter's bytes are only looked at there, because the filter already
    charged them.
  - `PsFile::filtered` (no budget) keeps its signature for code outside an
    interpreter (the fuzz harness).
- **Errors** are blamed on the operator that was draining: `flushfile`,
  `image`, `readhexstring`, `read` (an ASCIIHex skip loop), `token`, and
  `exec` (the scanner frame). They are uncatchable, `stopped` included.

### Tests

- `tests/flate.rs`, against gs 10.07:
  - the issue's probe (1048576);
  - a 4 MiB mixed stream (text, 30 KB flat runs, noise): length and an
    Adler-style pair of sums printed by the same PostScript in both;
  - an inline stream in the program, followed by more program text;
  - a 240 × 160 FlateDecode `colorimage` with flat areas, a gradient and a
    checkerboard, pixel-compared with gs's png16m (0 differing pixels) and
    with its own samples.

  On main the image is an `ioerror`.
- `src/file.rs` unit tests:
  - Flate over raw bytes and over an ASCIIHex filter (one byte per pull)
    returns every byte, and leaves the source at the trailer;
  - a corrupt stream gives its prefix, then a sticky `ioerror`;
  - a truncated stream is a premature EOD.
- `tests/limits.rs` (`one_operator`). The bomb is 4 GiB of spaces from 150 KB
  (RunLength pairs under Flate), opened before the limits are set. Under a
  100 ms clock each drain must stop with `limitexceeded (time)` blamed on its
  own operator: `image`, `readhexstring`, `read` on ASCIIHex over it,
  `cvx exec`, `token`, and `{ flushfile } stopped`. `steps(200000)` must stop
  `flushfile` with `StepsExceeded`. With `DecodeBudget::charge` disabled,
  these and the #1176 flushfile test fail at the deadline.
- `tests/limits_memory.rs`: the bomb as a hex string, read as a 15000 × 15000
  image's samples and as one string token, under a 32 MiB cap. Both must stop
  with `MemoryExceeded`, with a sampling thread's peak heap growth under
  128 MiB. With the checks disabled the image test fails.

### Codex round 1 (PR #1208)

Two P2s, both fixed, each with a regression test in `tests/limits.rs`:

1. **Raw `flushfile` had no limits.** The first version dropped
   `flushfile`'s #1176 poll, and a raw file has no budget, so
   `currentfile flushfile` over 1 MiB of spaces passed a 20-step cap. The
   poll is back, as described above.
2. **A short drain overran and blamed the next operator.** The step cap was
   checked only every 64 KiB, so under a 20-step cap a drain of 32,000
   decoded spaces (31 steps) let `(survived) =` run and take the blame, and a
   following `1 0 div` hid the overrun behind `undefinedresult`. The step cap
   is now checked at every KiB inside the draining operator, whether that is
   `flushfile`, `readstring`, `readhexstring` or `image`.

   The steps left are the figure at the start of the slice, as for
   `poll_limits`. A drain that crosses the cap only together with the steps
   run earlier in the same slice (at most a slice, 512 steps under a time
   limit) is still caught when the slice ends.

With the round's `src` changes reverted, both new tests fail.

### Identity

`scripts/identity_check.sh` was run with `--base-bin` (main at 0698b20) and
`--head-bin` (this branch rebased on it). It covered every tracked example,
`hybrid/` included (they hold the FlateDecode, DCTDecode and ASCII85 users:
postcard, etching_demo, the hybrid pieces), plus `gallery/mochi_in_oils.ps`
(DCT) and the 12 smallest gallery pieces, at `--gallery-dpi 72`. Every text
example also loads Type 1 fonts through `eexec`. Result: 143 of 143 page
renders were byte-identical across 130 files. The four `hybrid/compare_*.ps`
were skipped: they fail the same way in both trees (they need an input file
that isn't tracked). The negative control, a 200 × 200 flat FlateDecode
`colorimage`, differs between the trees: main gives `ioerror`.

### Performance (release, user time, best of 3, base eb44371 → head)

| Workload | Base | Head |
|---|---|---|
| 48 MiB of text through FlateDecode, `readstring` 64 KiB at a time | 1.07 s | 0.39 s |
| The same under `--max-time 100` | 1.05 s | 0.38 s |
| 3000 × 3000 RGB `colorimage` from FlateDecode (no flat runs, so base decodes it right), PNG out | 0.68 s | 0.09 s |
| 9 MB through ASCIIHexDecode, `readstring` (one pull per byte) | 0.13 s | 0.145 s |
| 9 MB through ASCIIHexDecode, `colorimage` | 0.12 s | 0.14 s |

The real image pieces, rebased (base 0698b20 → head, best of 3):
`gallery/mochi_in_oils.ps` (DCTDecode) 11.64 → 11.46 s,
`examples/etching_demo.ps` 0.11 → 0.11 s, `examples/postcard.ps` (FlateDecode)
0.00 → 0.00 s. No gallery piece reads image data through ASCIIHexDecode or
`readhexstring`.

The two ASCIIHex rows are slower by a nanosecond or two a byte. Moving the
budget check out of the pull loop, dropping the budget argument from `pull`,
and making filters with no budget at all did not bring it back. So it is not
the budget; the cause was not isolated. Later runs on a loaded machine read 0.17-0.18 s for
base. Recorded as a guard, not tuned further.

### Not done (proposed follow-ups)

- The other open items in `notes/1176-single-operator-limits.md` (`fill`'s
  bands, `flattenpath`, Type 1 subrs, kernel retirements, the SVG/PDF image
  rows, many-stop gradients) are unchanged.
- The browser build has no clock, and the playground sets no step cap, so a
  bomb drained by one operator there runs to its end in a single step. The
  decoded KiB now count as steps, so setting a step cap in `web/pscat.js`
  would stop it.
- The lexer's own token buffer grows without asking `check_alloc`. The
  decoder's 64 KiB heap check bounds it, at up to about three times the cap
  while a doubling copies.
