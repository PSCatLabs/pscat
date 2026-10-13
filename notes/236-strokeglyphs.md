## Stroke-skeleton glyphs for board math: lib/strokeglyphs.ps (issue #236, 2026-10-13)

Slice G1 of the dry-media design (`docs/DRYMEDIA.md` section 4). A new
catalog-tagged library, prefix `sg`, that returns glyph centerlines as a
path instead of painting them.

**What shipped**

- `sgpath`, `sgwidth`, `sggrow` as the design names them, plus
  `sgmetrics` and the `sgaxis` dial (250 units).
- Glyphs: 26 capitals; handscript's lowercase, digits and punctuation,
  copied from `HandScriptDict /CharDefs` at load; a math hand
  (`/x.curly /y.math /z.crossed /t.math /n.math`); 21 lowercase and 10
  uppercase Greek letters; about 35 operators and relations. Text-size
  ( ) [ ] { } | ∑ ∏ ∫ √ come from the same builders `sggrow` uses, so
  each shape has one source.
- Text is UTF-8 or an array of glyph names. The math hand is reached by
  name or by the math-italic code points (U+1D465 𝑥 and friends).
- The specimen (`examples/strokeglyphs.ps`, on the site gallery, and
  `docs/drymedia/strokeglyphs_specimen.png`) shows every glyph three
  times, the growing symbols at three sizes, and two lines of board math.
- Verified: six `%%SelfTest` blocks, a strict-lint driver,
  `tests/strokeglyphs.rs`. That covers hs-write rendering identically
  with the file loaded, every glyph path through `pkribbon`, and
  Ghostscript building the same path coordinates (to 0.002 pt) from the
  same seed. Also a unit-vocabulary test in `tests/capabilities.rs`.

**Where this departs from the approved design (for the maintainer)**

- **`sgmetrics` is a fourth public name.** The design says each glyph
  carries metrics for #237 but names no reader. It returns
  `/Advance /BBox /Italic /Class /Name` in points.
- **Classes are TeX's seven**, `/ord /op /bin /rel /open /close /punct`.
  The design listed six with one "operator". TeX spacing treats a binary
  `+` and a large `∑` differently, so the class splits into `/op` and
  `/bin`. The Greek letters Σ and Π are `/ord`. The n-ary ∑ and ∏ are
  separate glyphs (`/summation`, `/product`), class `/op`.
- **The italic correction is derived from `/Slant`**, as
  max(0, rightmost slanted x − advance), not stored per glyph. Upright
  skeletons give 0, which matches the design's "0 for upright".
- **`sggrow` draws at the current point**, because the design's
  signature `height depth opts sggrow -> advance` has no x y. With no
  current point it raises the guard `sggrow-needs-a-current-point`.
- **The radical's vinculum length is `/Span [points]`**, not `/Width`,
  because #141 keeps `/Width` for contact width.
- **The bbox is computed at load** from each glyph's points. Authors
  write `[adv stroke ...]`, and the run-time table is the design's
  `[adv [llx lly urx ury] stroke ...]`. It bounds the unjittered
  centerline. Ink reaches past it by half the pen width plus the jitter.
- **The default `/Seed` comes from position.** With no `/Seed`, the base
  seed is the origin's device-space position, quantized to 0.01 (for
  `sggrow`, the current point's). This is drykit's rule from DRYMEDIA 3.2.
  A fixed default would make every unseeded call identical, and #237
  will likely call `sgpath` once per atom. An explicit `/Seed` still
  wins, and pscat-versus-gs comparisons pin it.
- **No chalk row in the specimen.** #234 (drykit) and #235 (chalk) have
  not landed. The acceptance asks for chalk only "if #235 has landed".

**Design choices worth knowing**

- **Doubled points make corners.** Under handscript's midpoint rule an
  interior point is a control point, so a long straight stem between two
  corners becomes a big curve. A point written twice gives straight
  segments on both sides, which is a sharp corner. `sgpoly` builds the
  growing symbols this way, and also splits long runs into pieces of at
  most 250 units, so a tall bracket wobbles along its length instead of
  only at its ends.
- **Seeding.** The generator is the paintkit `pjrand` Schrage LCG, but
  its state is `seed + i` passed through three xorshift rounds. A raw
  `seed + i` gives neighbouring glyphs almost the same first draw, so
  every glyph in a line would share its shear.
- **Jitter is per em, not per symbol.** A grown parenthesis 4 em tall
  wobbles as much as a text-size one, as a hand does.
- **Load-time work runs inside a private dict** (`60 dict begin sginit
  end`), so the userdict name diff in
  `every_migrated_file_names_match_the_catalog_exactly` sees only the
  tagged names. `build.rs` reads `/x N dict def` as a three-token `def`
  and rejects it, so the tables are `<< >>` literals.

- **UTF-8 decoding takes only real continuation bytes**, so a stray
  Latin-1 byte becomes one `.notdef` and doesn't swallow the ASCII after it.
- **Scratch lives in the current dict**, under `sg` names, as in
  paintkit. The run time never calls `begin`, so a guard can't leave the
  dict stack unbalanced. The cost is that `sgpath` can't be called while a
  read-only dict is current, as inside a gs Type 3 `BuildChar`.

**Deferred**

- The math layout itself is #237. Chalk, crayon and charcoal rendering
  of these paths is #235, #239 and #241.
- More Greek (ι, ο, υ, the variant forms ϕ ϑ ϵ) and more operators
  (⊆ ⊃ ∖ ∘ ↦) are easy to add as data when #237 needs them.
