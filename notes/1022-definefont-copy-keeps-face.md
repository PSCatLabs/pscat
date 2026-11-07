## A definefont copy keeps its face; glyphshow (issues #1022 and #1023, 2026-11-06)

**#1022.** The PLRM re-encoding idiom copies a font dict entry by entry,
skipping `FID`, swaps in `ISOLatin1Encoding` and calls `definefont`. In
pscat that copy drew and measured as Helvetica, whatever the source face.

Root cause: pscat's built-in dicts are Type 42 in name only. The outlines
live in the Rust registry, and the dict's only link to them is `FID`. A real
Type 42 or Type 1 copy keeps its face through `sfnts` or `CharStrings`. Ours
had nothing left, so `definefont`, finding no `FID`, gave it
`font::SUBSTITUTE` (Helvetica).

Fix: `build_builtin_dict` also writes `/PSCatFace <registry index>`, which the
copy carries along. `definefont` uses it for a missing `FID` when it is an
integer naming a loaded face (`font::face_entry_fid`), and substitutes
otherwise, so a program writing junk there gets Helvetica, never an error.
Type 3 and Type 1 dicts are checked first, as before. `FontName` was not used
as the key: the idiom often renames it.

The issue's second repro line, `/Copy /Times-Roman findfont definefont`,
already measured as Times on main (that dict keeps `FID`). Only the `forall`
copy was broken. `dup length dict copy`, which the in-tree examples use, was
never affected, so no tracked render changes (the two-binary identity check
over every `definefont`/`glyphshow` file and the Type 3 font libraries was
identical).

**#1023.** `name glyphshow` (Level 2) shows one glyph by name, bypassing the
Encoding. It is a one-glyph `ShowCtx` (`ShowCtx::glyph`), so it shares
`show`'s pipeline, frame and errors. Behaviour was checked against
Ghostscript 10.07 (correctness only):

- name operand only; a string or an integer is a `typecheck`; `invalidfont`
  with no font; `nocurrentpoint` with no current point;
- outline faces resolve the name as an encoded name would be (`post` table,
  then AGL to `cmap`), Unicode-mode faces too;
- Type 1 looks it up in `CharStrings`, with `.notdef` as the fallback;
- Type 3 calls `BuildGlyph` with the name, even one the Encoding lacks. A
  `BuildChar`-only font gets the first Encoding slot holding the name, else
  0, as gs does (the PLRM only defines glyphshow through `BuildGlyph`);
- an unknown name advances by `.notdef`'s width.

Deferred: a UTF-8 mode for Latin faces (needs a design decision, per the
triage). Liberation's `.notdef` is 778 units wide where URW's is 250, so an
unknown glyph advances further than in gs. That affects `show` as well and
predates this change.
