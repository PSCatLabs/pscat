# QR data/correction API reference

This documents the implemented checkpoint at `deba699`, not a released public
QR API. All procedure names below are internal and may change before merge.
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
implementation yet. No matrix, finder patterns, format/version bits, masks,
quiet zone, rendering or scan result should be inferred from these bytes.

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

`qr--byte-data` writes mode `0100`, an 8-bit count for versions 1–9 or 16-bit
count for versions 10–40, raw bytes, four zero terminator bits and alternating
`EC`/`11` hexadecimal pad bytes. A single byte segment is nibble-aligned after
the header/payload; four terminator bits complete byte alignment. The explicit
capacity seam is intentionally separate from standards capacity selection.

`qr--interleave` returns no remainder bits. Matrix construction must append
those bits in the module-placement stage. Short blocks precede long blocks;
only long blocks contribute to the extra data column. Correction columns always
contain one byte per block, with no synthetic short-block pad byte emitted.

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
and `qr--block-count` store the standards block-layout facts. These names are
implementation details, not rendering or extension hooks. The tables use
standard PostScript `readonly`; this is not a new security boundary in PSCat.
