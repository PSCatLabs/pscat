# 595 -- a copy sends the typed ribbon's colour through the copier's curve

**Bug.** `rel-page` draws its paper, stamps and notes through the copier's grey
curve (`rlSet` / `rlCol`), but the typewriter set its ribbon with a plain
`setrgbcolor`, so `/Ink [0.66 0.13 0.11]` typed inside `<< /Generation 2 >> rel-page`
stayed red beside a grey stamp (459 pixels with red - blue > 60 in the issue's
repro; 0 now). The same held for `/BleedInk`.

**Fix.** `lib/typewriter.ps` has a new option `/InkMap [proc]` (`tInkMap`, default
`{ }`): a procedure `r g b -> r g b` that the ribbon colour and the bleed colour go
through just before `setrgbcolor` (`tStrikeGlyph`, `tZoneDo`). `rel-page` sets it
next to `/Copies` in `rlOriginal` and `rlRunPaged`, for a copy only, to
`{ RelState begin rlCol end }` and puts the caller's own map back after the
content. Two details:

- The content runs with RelState ended, so the hook has to `begin` it; `rlCol`
  also `def`s scratch names (`rmD`, `rhM`), which would otherwise land in TWState.
- The old map is restored with `/rlOldM load`, not by name: a procedure fetched by
  name inside `<< >>` would run.

As with `/Copies`, an error in the content leaves the map set (documented on
`rel-page`).

**What did not change.** Originals (`/Generation 0`) and typewriter-only pages are
byte-identical: `gallery/time_traveler_memo.ps` and `examples/typewriter_specimen.ps`
by `scripts/identity_check.sh --loader lib/typewriter.ps --loader lib/released.ps`,
with a red-ink copy as the control (which differs, as it must). The redaction bar
(`0 setgray`) and `rel-redact` have their own colours and are untouched.

**What did change.** Every copied typed page: the curve sends near-black
(`[0.13 0.13 0.15]`, the default ribbon) to pure black at a first copy already
(darkness 0.87 is past the curve's clamp), so the type on a copy is a little blacker.
Regenerated: `gallery/renders/released_memo.png`,
`docs/dev/design/released_pages_1.png` / `_2.png`,
`docs/dev/design/released_specimen.png` and `released_specimen_gs.png`.
`tests/released.rs` `a_copy_whitens_the_paper_and_the_type_gets_fatter` now types its
by-hand comparison in `/Ink [0 0 0]` for the same reason.
