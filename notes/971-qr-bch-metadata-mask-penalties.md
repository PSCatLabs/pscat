## QR: BCH metadata, mask penalties and first decoded URL (issue #971, 2026-11-02)

Part of epic #966 (QR-4), on `feature/qr-code-library`. `lib/qr.ps` now
builds complete symbols: `qr--format-bits` / `qr--version-bits` (BCH words),
`qr--draw-metadata` (both format copies, both version blocks, `/Level` added),
`qr--penalty` (N1–N4), `qr--mask-penalties`, `qr--best-mask`, `qr--finish`,
`qr--codeword-symbol` and `qr--symbol` (payload + level -> symbol). All
internal; the public `qrencode`/`qrdraw` is #972.

**Penalty definition.** The scorer follows the reference encoder (Nayuki)
exactly rather than a looser reading of the standard. That covers its N3 "light
border": each line counts as padded with `Size` light modules, so a finder-like
run at an edge qualifies, and a run with at least four units of light on both
sides counts twice. Matching it means the chosen mask is the oracle's, so the
fixtures can compare whole final grids. Ties go to the lowest mask (strict `<`).
The scan reads 0/1 row and transposed column planes with `getinterval`/`forall`,
so a v40 automatic symbol (eight trials) takes about 0.5 s in PSCat.

**Metadata cells** are written as `6 or bit`, so they keep the metadata flag
(finished symbols hold cell values 0, 1, 2, 3, 6, 7). Version bits are not
written by `qr--function-matrix`: its fixture test pins its cells as {0,2,3,6}.

**Fixtures.** `tests/data/qr_mask_vectors.json`, from the pinned oracle:
format/version words read from the order its own drawing routines write
modules; real automatic-mask constructions with each `_get_penalty_score`
result recorded (versions 1, 2, 7, 10, 27, 40 × L/M/Q/H; GitHub URL × 4;
`pscat-525` at M, a real tie between masks 1 and 5); 32 hand-built and seeded
grids scored by the oracle's `_get_penalty_score`. The four earlier fixture
files regenerate byte-identically.

**Decode proof.** `rqrr` 0.11.0 (`default-features = false`) is a new
dev-dependency in `Cargo.toml`. The test renders through `pscat --png`
(white page, 4-module quiet zone, 4 px per module), decodes, and asserts the
exact URL plus version, mask and format level at L/M/Q/H. Because the decoder
is compiled in, the test cannot skip. The Cargo change makes this PR
owner-merged (CODEOWNERS).

**Checks.** Seven new Rust tests (29 QR tests), eight new selftest blocks (21).
Nine hand mutations each fail 2–5 tests: L/M swapped, N3 counted on one side
only, `le` tie-break, second format copy off by one, wrong XOR mask, no light
border, N1 increment, N2 light-only, version transpose dropped.

**Not done here.** Drawing API, SVG/PDF decode, multi-scale and
multi-decoder corpus (#972, #973); `docs/dev/HANDOFF.md` is untouched (outside
this ticket's lane).
