# #350: sgpath refuses a missing glyph

`sgpath` and `sgwidth` used to advance an unknown glyph as `.notdef`
(300 units) and draw nothing. In #238 `(i² = -1 !)` came out as
`i  = -1 !` with no error. Now they raise `sg-missing-glyph:<char>`
(the character itself for a string, the glyph name for a name array),
before anything is added to the path, the way mathkit raises
`mk-unsupported-character:<char>`.

## Departure from the issue

The issue proposed an opt-in `/Strict true` and keeping the lenient
default for handscript compatibility. The task brief for this run asked
for the named error as the goal with a small lenient opt-out, so the
default is strict and `/Strict false` is the opt-out. Handscript itself
is a separate Type 3 font and is untouched. Anyone relying on the old
silent `.notdef` passes `/Strict false`.

## How

- `sgnames` records the first missing glyph in `sgmiss` while decoding
  (a string: the source bytes of that character; an array: the name, if
  it is `.notdef` or not in `sgtab`).
- `sgreadopts` reads `/Strict` (a boolean; anything else raises
  `sg-strict-must-be-a-boolean`).
- `sgcheckstrict` raises, in `sgpath` and `sgwidth`, after the options
  are read and before the first moveto. Labels are cut to 40 bytes.
- `sgmetrics` is deliberately lenient: mathkit's `mkutf8` calls it and
  tests `/Name /.notdef` to raise its own `mk-unsupported-character:`.
  `sggrow` takes no text.

## Evidence

- `scripts/identity_check.sh --loader lib/strokeglyphs.ps` against a
  detached `origin/main`: 12 of 12 pages identical (drychalk and mathkit
  specimens, strokeglyphs specimen, mathkit and strokeglyphs drivers);
  the negative control (`(a²)` under a `stopped`) differs, as it must.
- The open gallery pieces render with the new error live and match
  `origin/main`'s libraries byte for byte: `gallery/chalkboard_lecture.ps`
  from `feature/238-professors-chalkboard-lecture` and
  `gallery/crayon_drawing.ps` from
  `feature/240-gallery-young-childs-crayon-drawing` (PR #336).

## Not done

A `--lint` finding for a drawn `.notdef`: that is Rust-side static
analysis and a different feature; the runtime error covers the case the
issue hit.
