## QR: reserved patterns and fixed-mask matrix construction (issue #970, 2026-11-02)

Part of epic #966 (QR-3), on `feature/qr-code-library`. `lib/qr.ps` gains the
matrix stage: `qr--alignment-centers`, `qr--function-matrix` (timing,
finders and separators, alignment, dark module, format and version
reservations), `qr--matrix` (MSB-first zigzag placement of a complete
interleaved stream, remainder cells left light) and `qr--apply-mask` (one
forced mask 0–7 on a fresh copy; refuses an already-masked matrix). The
raw-module count moved into `qr--raw-modules`, shared with `qr--block-spec`.

**Representation.** A byte-per-cell `/Cells` string, row-major from the
top-left, with flags 1 dark / 2 function / 4 metadata. Bytes rather than
booleans or packed bits: get/put on a string is the cheapest PostScript cell
access, a v40 grid is 31 KB, and #971's eight mask trials are string copies.
The metadata flag lets #971 find the format/version cells without recomputing
their positions, and lets tests compare fixed patterns separately. Named
`/Cells` so it does not collide with #972's planned public boolean `/Modules`.

**Scope moved from #971.** The eight mask predicates landed here: comparing
against a fixed-mask oracle needs them. #971 keeps BCH format/version bits,
penalty scoring and selection. `QR_CODE_ISSUES.md` says so.

**Fixtures.** `scripts/qr_reference_vectors.py` now also writes
`tests/data/qr_matrix_vectors.json` from the pinned Nayuki oracle's real
constructor at a fixed mask, wrapping its `_draw_codewords`, `_apply_mask`,
format/version routines and `_set_function_module` to record intermediate
grids. The dark module is separated from format metadata empirically (the one
format-routine cell dark for every level and mask). Placement fixtures inject
formula codewords in place of the oracle's correction output, so placement is
pinned independently of RS; end-to-end symbols cover the joined pipeline.
The three earlier fixture files regenerate byte-identically.

**Checks.** Seven new Rust tests (22 QR tests) and five new selftest blocks
(13). A mismatching full grid reports the first differing column/row. Five
hand mutations (mask 4 axes, timing-column skip, one version block, zigzag
direction, bit order) each fail 2–4 tests. Ghostscript reproduces the
selected full grids and the guards. Timing: v40 placement plus all eight
masks takes about 0.1 s in PSCat.

**Not done here.** BCH metadata, penalty, mask selection, drawing, public API;
`docs/dev/HANDOFF.md` is untouched (outside this ticket's lane; the epic
updates it when the branch lands).
