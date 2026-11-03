## QR: full version/ECC boundary and independent decode corpus (issue #973, 2026-11-02)

Part of epic #966 (QR-6), on `feature/qr-code-library`. Tests and docs only:
the corpus found no encoder bug, so `lib/qr.ps` is unchanged.

**Oracle corpus.** `scripts/qr_reference_vectors.py` adds
`tests/data/qr_corpus_vectors.json`, and the five earlier fixture files
regenerate byte-identically. For each of the 160 version/level cells it holds
a binary payload of exactly the byte capacity, encoded by the oracle's real
`encode_segments` at a forced mask. The fixture stores codeword and
finished-grid hashes, not grids, to keep the file at 160 KB. It also holds
the payload one byte longer with an automatic mask (next version, eight
penalties, mask, grid). `tests/qr_corpus.rs` checks all of it in the default
suite, one test per level, with the internal codeword stage compared first
so a mismatch names its stage. Fixed-version overflow, v40 overflow, invalid
options and URL/UTF-8 texts are covered too.

**Second decoder.** rxing 0.9.3 (ZXing port, Apache-2.0, MSRV 1.85), its QR
reader only. Its `no_character_set_support` feature compiles but panics on
decode, so `encoding_rs` is on. Comparisons use raw bytes: rqrr's
`decode_to` writer and rxing's byte-segment metadata, never their text APIs.
rxing's raw bytes are the corrected data codewords and are compared with the
encoder's padded data. The new features reaching pscat's normal dependencies
are inert (num-traits `i128`, simdutf8 `public_imp`/NEON). pscat binaries
built with and without rxing rendered 120 artkit/paintkit pages identically
(`identity_check.sh --base-bin/--head-bin`). `cargo +1.88 check --tests` and
`cargo deny` pass. The raster helpers moved from `tests/qr.rs` to
`tests/qr_support/` so both test files share them.

**Pixel check.** Before any decode, every pixel wholly inside one area (page,
light, dark) must have that area's exact grey. This replaces "decodes" as
the proof of geometry at every pitch. Two conventions mattered. pscat's PNG
rounds the page height in `f32` (214.5 px rounds to 214) and puts y = 0 on
the bottom pixel edge. resvg/hayro map the page exactly, so their last
column or row can be partly off the page.

**Finding: decoders, not the encoder, limit fractional pitches.** The
ignored sweep produced 2,080 pixel-exact rasters (22.7 s, 8 threads). Both
decoders read all 160 symbols at 2 px/module on the pixel grid. At 2.6,
3.77 and 4.5 px/module from a fractional origin they miss many dense
symbols (rqrr reads 0 of 160 at 2.6). Thresholding the rasters to pure black
and white reshuffles which ones they miss, without fixing them. The default
subset pins its 24 fractional misses, so the documented table cannot drift.
Guidance went into the user guide: use whole device pixels per module for
screen PNGs. #974's card should do the same, or rely on print resolution.

**Physical.** The card page's PDF MediaBox and SVG size, the 300 dpi
1050 × 600 PNG recipe, an exact 240 px (0.8 in) light square, and quiet zones
of 4/5/6 at seven scales from 1 to 6.8 px/module are covered. Recorded
limitation: the SVG's `width`/`height` carry no unit (a browser shows points
as CSS pixels) and the PNG has no `pHYs` chunk. Both are possible follow-ups
outside this lane.

**Timing.** Default QR corpus tests take about 12 s wall (`ci` profile; the
level tests are about 9–10 s each in parallel). The sweep takes about 23 s.
