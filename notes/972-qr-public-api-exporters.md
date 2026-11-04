## QR: public vector API, embedded distribution and exporters (issue #972, 2026-11-02)

Part of epic #966 (QR-5), on `feature/qr-code-library`. `lib/qr.ps` gains its
public, catalog-tagged API: `payload opts qrencode -> symbol` and
`symbol opts qrdraw -`. The user guide is `docs/guides/qr-codes.md`; the
contract is in `docs/dev/design/QR_CODE_API.md`.

**Shape.** The symbol is a fresh `<< /Version /Level /Mask /Size /Modules >>`.
`/Modules` is an array of row arrays (top row first, booleans, true = dark),
so `m y get x get` reads the standard's (column, row) directly. A flat array
would have been easier to `forall` but easier to misindex. `qrdraw` draws at
the current point, as the dispatcher's brief asked, with the size in the
options (`/ModuleSize`, or `/Side` for the whole square). `/Side` rather than
`/Size`, because `/Size` already means modules in the symbol. The colors are
`/Dark` and `/Light`, the standard's terms; `/Light null` paints nothing.
`/QuietZone` has a minimum of 4, per DENSO/ISO.

**Validation.** Every argument is checked on the operand stack before any
`begin`, `gsave` or path change, as the internal stages do. Unknown option
keys are ignored: no pscat library rejects them (halftonekit deliberately
tolerates a shared dict). Colors are a gray number or a literal 1/3/4-array;
a procedure is refused, and arrays are copied with `aload`, so no caller
value is ever executed. With a light color, the dark one must have lower
luminance. That is the one contrast rule; scan-friendly contrast beyond it
stays the caller's job.

**Geometry.** `gsave newpath`, a light square, then one path with one
rectangle per maximal horizontal dark run, filled once, then `grestore`.
Every corner is `origin + k * module` computed by the same expression, so
touching modules share bit-identical edges. The `newpath` matters: the
caller must have a current point, so they have a path, and the dark `fill`
would otherwise paint it.

**Exports.** Two test-only, owner-approved dev-dependencies: `resvg` 0.49.0
(SVG) and `hayro` 0.4.0 (PDF), both pure Rust (MIT OR Apache-2.0 and
Apache-2.0), no default features. Cargo unifies a dev-dependency's features
into the pscat binary that integration tests run. hayro-syntax 0.5 and later
turn on flate2's `zlib-rs` backend, which changed pscat's own PNG output
under test: a `tests/mcp.rs` 100-byte inline limit saw 107 bytes. hayro
0.4's hayro-syntax uses flate2's `rust_backend`, the same as pscat. 0.7 and
later also need rustc 1.92, above the 1.88 MSRV. With 0.4 the only extra
features on pscat's normal dependencies are inert ones (bytemuck
`min_const_generics`, hashbrown `default-hasher`/`inline-more`, log `std`,
strict-num `approx-eq`/`float-cmp`, syn extras). No existing package changed
version, and `cargo deny` passes. The
export test checks every module centre with row 0 at the top, the quiet
zone, the seam pixel on every dark-dark edge, and an `rqrr` decode of the
exact text, version, mask and level, for PNG, resvg and hayro rasters.

**Seam threshold.** The correct drawing gives seam pixels of exactly 0 in
all three rasterizers. A mutant that fills each run separately gives 61 in
pscat's PNG at the fractional pitch, which a `< 60` "dark" threshold missed.
So the seam check uses `<= 8`. Two more mutants fail the test as well:
flipping the row order, and shifting runs into the quiet zone.

**rqrr limit.** rqrr finds no version-10 symbol at 4 px/module in any of the
three rasters, even with padding. At 5.7 px/module it finds all three. That
case therefore uses 5.7 px/module. #973's decode corpus should add a second
decoder and probe the small-module range.

**Not done here.** Gallery card (#974), the all-version decode sweep (#973),
the site's guides page (`site/guides.html` links to `main` and is
regenerated when the branch lands), and `docs/dev/HANDOFF.md`, whose QR
banner still says "no matrix, masks or drawing". Both are outside this
ticket's lane.
