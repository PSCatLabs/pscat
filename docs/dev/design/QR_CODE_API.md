# QR data/correction/matrix API reference

This documents the implemented internal stages (data and correction through
`deba699`, matrix construction from #970, metadata and mask selection from #971), not a released public QR API. All procedure names below are internal and may change before merge.
Start with the [review handoff](QR_CODE_REVIEW.md),
[design/roadmap](QR_CODE.md), and [verification guide](QR_CODE_TESTING.md).

```postscript
(lib/qr.ps) run
/plan (https://github.com/PSCatLabs/pscat) /M qr--byte-plan def
plan /Version get =         % 3
plan /DataCodewords get =   % 44
plan /Data get plan /Version get plan /Level get qr--interleave
length =                   % 70: complete data/correction codewords
```

This program emits numbers, not a QR image. There is no `qrencode` or `qrdraw`
implementation yet. The [matrix stage](#matrix-construction) places these
codewords and applies a caller-chosen mask; the
[selection stage](#metadata-penalties-and-mask-selection) (#971) writes
format/version bits and picks the mask, giving a complete symbol. Quiet zone,
drawing and the public API are #972.

## Data conventions and effects

Strings contain raw bytes. No Unicode conversion, ECI, BOM insertion or
numeric/alphanumeric mode optimization occurs. Empty input is one explicit
byte segment, not an empty sequence of segments. Levels are case-sensitive
names `/L`, `/M`, `/Q`, `/H`; selection never boosts the requested level.

Readable executable strings and executable level names are accepted as data.
The code stores literal views with `cvlit`, preserving the input's bytes,
access and executable attributes. It does not execute payload text. Outputs
are fresh mutable strings/dictionaries; changing one result does not mutate
the input or a later result. Existing input objects are read, not changed.

Loading defines only prefixed helper/table names in the current dictionary.
It builds bounded finite-field tables and leaves no operands or open private
dictionary. No drawing, `showpage`, random draws or graphics-state changes occur.
Calls use private dictionary frames and invoke no caller callbacks.

## Entry points

| Procedure | Stack effect | Contract |
|---|---|---|
| `qr--byte-data` | `payload version capacity -> padded-data` | One byte segment in a supplied data-codeword area. Version integer 1–40; capacity integer 1–2956. Does not check the capacity against an ECC/version table. |
| `qr--block-spec` | `version level -> layout` | Fresh layout dictionary for any of the 160 Model 2 version/ECC combinations. |
| `qr--byte-plan` | `payload level -> plan` | Smallest fitting version at the exact requested ECC; returns layout plus padded `/Data`. |
| `qr--rs-divisor` | `degree -> coefficients` | Integer degree 1–30, the QR correction range. High-to-low coefficients excluding the leading monic coefficient. |
| `qr--rs-remainder` | `data divisor -> parity` | Readable data up to 2956 bytes; readable divisor of 1–30 bytes. Returns fresh parity of the divisor's length, including all-zero parity for empty data. |
| `qr--interleave` | `padded-data version level -> codewords` | Requires exactly `/DataCodewords` bytes. Splits blocks, calculates correction and interleaves data columns then parity columns. Returns `/RawCodewords` bytes. |
| `qr--alignment-centers` | `version -> centers` | Fresh ascending array of alignment row/column centers; empty for version 1. |
| `qr--function-matrix` | `version -> matrix` | Fresh unmasked matrix with function patterns and metadata reservations; every data cell light. |
| `qr--matrix` | `codewords version -> matrix` | Requires exactly the version's `/RawCodewords` bytes (level-independent). Places them and returns a fresh unmasked matrix. |
| `qr--apply-mask` | `matrix mask -> matrix` | Unmasked matrix, mask integer 0–7. Fresh copy with that mask applied to data cells and `/Mask` set; the input is unchanged. |
| `qr--format-bits` | `level mask -> integer` | 15-bit format word, bit 0 least significant. |
| `qr--version-bits` | `version -> integer` | 18-bit version word; versions 7–40 only. |
| `qr--draw-metadata` | `matrix level -> matrix` | Masked matrix. Fresh copy with format (and, v7+, version) bits written and `/Level` added. |
| `qr--penalty` | `matrix -> integer` | Mask-selection score of every cell's dark bit. |
| `qr--mask-penalties` | `matrix level -> scores` | Unmasked matrix. Fresh array of the eight scores, mask 0 first. |
| `qr--best-mask` | `scores -> mask` | Eight integers; index of the lowest, the lowest index on a tie. |
| `qr--finish` | `matrix level -> symbol` | Unmasked matrix. Best mask applied, metadata written. |
| `qr--codeword-symbol` | `padded-data version level -> symbol` | Fixed version: interleave, place, finish. |
| `qr--symbol` | `payload level -> symbol` | Smallest fitting version at that level, automatic mask. |

`qr--byte-data` writes mode `0100`, an 8-bit count for versions 1–9 or 16-bit
count for versions 10–40, raw bytes, four zero terminator bits and alternating
`EC`/`11` hexadecimal pad bytes. A single byte segment is nibble-aligned after
the header/payload; four terminator bits complete byte alignment. The explicit
capacity seam is intentionally separate from standards capacity selection.

`qr--interleave` returns no remainder bits. Matrix construction must append
those bits in the module-placement stage. Short blocks precede long blocks;
only long blocks contribute to the extra data column. Correction columns always
contain one byte per block, with no synthetic short-block pad byte emitted.

## Matrix construction

```postscript
/plan (https://github.com/PSCatLabs/pscat) /M qr--byte-plan def
plan /Data get plan /Version get plan /Level get qr--interleave
plan /Version get qr--matrix          % unmasked: function patterns + data
2 qr--apply-mask /symbol exch def     % forced mask 2
symbol /Cells get length =           % 841 = 29 * 29
```

The internal matrix is a fresh dictionary:

| Field | Meaning |
|---|---|
| `/Version` | Integer 1–40. |
| `/Size` | Side in modules, `17 + 4 * Version`; no quiet zone. |
| `/Cells` | Mutable string of `Size * Size` bytes, row-major from the top-left module: index `y * Size + x`, `x` the column (left to right), `y` the row (top to bottom). |
| `/Mask` | `null` until `qr--apply-mask`, then the applied mask 0–7. |

Each `/Cells` byte is a set of flags: `1` dark, `2` function (never placed or
masked), `4` metadata. Data cells are therefore `0`/`1`; fixed patterns are
`2`/`3`; unwritten format/version metadata cells are `6` (reserved, light).
The dark module at column 8, row `Size - 8` is a fixed pattern (`3`), not
metadata. `/Cells` is named so it cannot be confused with the public API's
planned boolean `/Modules`; #972 converts.

Function patterns: timing row and column 6; three finders with their light
separators (9x9 squares clipped at the edge); alignment patterns at every pair
of centers except the three finder corners; the dark module; two 15-cell
format copies (row 8 and column 8 beside the finders) and, for version 7 and
up, two 6x3 version blocks (columns `Size-11`..`Size-9` of rows 0–5 and the
transpose). The BCH stage (#971) writes the metadata cells: it should `or`
its bits into those bytes, keeping the `4` flag, so later stages can still
tell metadata from fixed patterns.

`qr--matrix` places bits most significant first, in two-column strips from the
right edge, alternating up and down starting upward at the bottom-right,
skipping function cells and the whole of column 6. Remainder cells (0–7 per
version) stay light data cells, and masks apply to them like any data cell.

`qr--apply-mask` uses the standard predicates, with `x` the column and `y`
the row; a data cell is inverted where the predicate is zero:

| Mask | Predicate |
|---|---|
| 0 | `(x + y) mod 2` |
| 1 | `y mod 2` |
| 2 | `x mod 3` |
| 3 | `(x + y) mod 3` |
| 4 | `(x div 3 + y div 2) mod 2` |
| 5 | `(x y) mod 2 + (x y) mod 3` |
| 6 | `((x y) mod 2 + (x y) mod 3) mod 2` |
| 7 | `((x + y) mod 2 + (x y) mod 3) mod 2` |

It refuses an already-masked matrix: masks are XORs, so a second mask would
silently corrupt the symbol. Mask selection (#971) applies each candidate to
the same unmasked matrix and scores the fresh copies.

## Metadata, penalties and mask selection

```postscript
(https://github.com/PSCatLabs/pscat) /M qr--symbol /symbol exch def
symbol /Version get =   % 3
symbol /Mask get =      % 0, the oracle's choice too
```

A symbol is a matrix with `/Level` added: `<< /Version /Level /Size /Cells
/Mask >>`. Its format and version cells hold `6` (light) or `7` (dark): the
metadata flag stays set, so later stages still tell metadata from fixed
patterns. Cell flags in a finished symbol are therefore 0, 1, 2, 3, 6 or 7.

Format word: the level code (L `01`, M `00`, Q `11`, H `10`) and three mask
bits, then a ten-bit BCH remainder over `0x537`, XOR `0x5412`. Bit `i` goes to
(8, i) for i < 6, (8, 7), (8, 8), (7, 8), then (14 − i, 8) for i ≥ 9 (column,
row); the second copy is (Size − 1 − i, 8) for i < 8 and (8, Size − 15 + i) for
i ≥ 8. Version word (v7+): the six version bits then a twelve-bit remainder
over `0x1F25`; bit `i` goes to (Size − 11 + i mod 3, i div 3) and its transpose.

`qr--penalty` follows the reference encoder (Nayuki), which the fixtures pin:

| Rule | Score |
|---|---|
| N1 | Each row/column run of 5+ equal modules: 3, plus 1 per module past 5. |
| N2 | Each 2×2 block of equal modules (overlapping blocks all count): 3. |
| N3 | Each dark 1:1:3:1:1 run with ≥ 4 units of light on one side and ≥ 1 unit on the other, once per qualifying side: 40. Lines count as padded with `Size` light modules at both ends. |
| N4 | 10 per whole 5% the dark share lies outside 45–55%. |

Each mask is scored with its own format bits drawn, as the encoder does.
Selection keeps the first strictly lower score, so ties go to the lowest mask.

## Layout fields

| Field | Meaning |
|---|---|
| `/Version` | Integer 1–40. |
| `/Level` | Literal name `/L`, `/M`, `/Q` or `/H`. |
| `/Size` | Matrix side in modules, `17 + 4 * Version`; no quiet zone. |
| `/RawCodewords` | Data plus correction bytes, after discarding remainder bits. |
| `/RemainderBits` | Remaining data-area module bits, 0–7. |
| `/DataCodewords` | Padded-data byte capacity. |
| `/ECCodewordsPerBlock` | Correction byte count for each block, not total correction. |
| `/Blocks` | Total number of correction blocks. |
| `/Data` | Present only in a byte plan: fresh string of `/DataCodewords` bytes. |

The relation is `DataCodewords = RawCodewords - Blocks * ECCodewordsPerBlock`.
For version 40, maximum byte payloads are 2953/2331/1663/1273 at L/M/Q/H.
Their capacity calculation reserves space for the byte-segment header/terminator.

## Validation and failures

Named validation failures use the repository's undefined-name guard convention.
They occur before a private frame opens. Operand rollback is not promised:
callers catching an error must manage their operand stack. This is not a
general unwinder for allocation errors, interpreter limits or changed helpers.

| Category | Guard names |
|---|---|
| Raw packing capacity | `qr-data-capacity-must-be-integer`, `qr-data-capacity-must-be-in-1-2956`, `qr-data-does-not-fit-capacity` |
| Raw packing version/count | `qr-data-version-must-be-integer`, `qr-data-version-must-be-in-1-40`, `qr-data-count-does-not-fit-version` |
| Layout version/level | `qr-version-must-be-integer`, `qr-version-must-be-in-1-40`, `qr-level-must-be-L-M-Q-or-H` |
| Payload | `qr-data-must-be-a-string`, `qr-data-must-be-readable`, `qr-data-too-long-for-level` |
| Correction degree | `qr-ecc-degree-must-be-integer`, `qr-ecc-degree-must-be-in-1-30` |
| Correction divisor | `qr-ecc-divisor-must-be-a-string`, `qr-ecc-divisor-must-be-readable`, `qr-ecc-divisor-length-must-be-in-1-30` |
| Correction input length | `qr-ecc-data-too-long`, `qr-ecc-data-length-must-match-capacity` |
| Matrix codewords | `qr-codewords-must-be-a-string`, `qr-codewords-must-be-readable`, `qr-codewords-length-must-match-version` (version guards as layout) |
| Masking | `qr-mask-must-be-integer`, `qr-mask-must-be-in-0-7`, `qr-matrix-must-be-a-matrix` (not a dictionary with integer `/Version` 1–40, matching `/Size`, readable `Size * Size` `/Cells` and `/Mask`), `qr-matrix-already-masked` |
| Metadata/selection | `qr-matrix-must-be-masked`, `qr-version-bits-need-version-7-40`, `qr-penalties-must-be-eight-integers` (mask, level and matrix guards as above) |

If multiple arguments are invalid, the first checked argument determines the
error. This is an internal contract; no cross-procedure validation ordering is
promised. Native PSCat currently treats access operators as compatibility
operations; real Ghostscript tests establish the unreadable-string guards.

## Other internal names

`qr--gf-mul` expects two trusted byte integers (0–255) and returns their product
in GF(256), primitive polynomial `0x11D`, generator 2. Its hot path deliberately
does not validate arbitrary caller objects. Zero operands are handled before
logarithm lookup. `qr--make-gf` builds exp/log tables; `qr--gf` stores them.
`qr--check-level` validates and literalizes an ECC name. `qr--ecc-per-block`
and `qr--block-count` store the standards block-layout facts.
`qr--raw-modules` (trusted version -> data-area module count, remainder bits
included) is shared by `qr--block-spec` and `qr--matrix`. `qr--check-version`
and `qr--check-matrix` are argument guards; `qr--mask-predicates` is the mask
table. `qr--level-bits` maps levels to format codes; `qr--check-masked` guards
a masked matrix. These names are
implementation details, not rendering or extension hooks. The tables use
standard PostScript `readonly`; this is not a new security boundary in PSCat.
