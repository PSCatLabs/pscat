## mathkit: TeX-subset math layout to stroke skeletons (issue #237, 2026-10-14)

`lib/mathkit.ps` is a new catalog-tagged library with the prefix `mk`. It
requires handscript and strokeglyphs. It lays out a subset of TeX math with
TeX's box model, as strokeglyphs skeletons, and roughens the layout like a
hand on a board. It paints nothing, so a pen `stroke` or drychalk's
`dcchalk` inks the path. #238 (the chalkboard lecture) builds on it.

**Public procedures**

- `(tex) x y opts mkpath` appends the formula's path with the baseline's
  left end at x y.
- `(tex) opts mkmetrics` returns `/Width /Height /Depth /Align` without
  drawing. It always matches mkpath.
- `[lines] x y opts mkalign` writes a derivation aligned on `&` or before
  each line's first relation. It returns one record per line
  (`/Origin /Angle /Width /Height /Depth /Align /BBox`). An `/Ink` hook
  inks each line as it is written.
- `mkframe` (a box or `/Shape /oval`), `mkunderline` (`/Double`) and
  `mkarrow` (`/Bend`) are the annotations. The first two also accept an
  mkalign record and follow its slope.

**Supported TeX:**
- scripts, both on one base, and primes;
- `\frac`, `\dfrac`, `\tfrac`, `\binom`, `\sqrt[n]{}`;
- `\sum`, `\prod` and `\int` with limits, `\limits` and `\nolimits`;
- `\lim`, `\max` and the other operator names, `\operatorname`;
- `\left`/`\right` and `\big`..`\Bigg`;
- `matrix`, `pmatrix`, `bmatrix`, `Bmatrix`, `vmatrix` and `cases`;
- `\vec`, `\hat`, `\bar`, `\dot`, `\ddot`, `\tilde`, `\overline`,
  `\underline` and `\boxed`;
- `\text` and `\mathrm`, and the spacing commands;
- Greek, the strokeglyphs operators, and Unicode symbols typed directly.

The psart skill has the table.

**Design calls**

- **Backslashes are doubled.** A PostScript string drops an unknown escape
  (`(\sqrt)` is "sqrt") and turns `\f \n \t \b \r` into control
  characters. Both pscat and gs do this. The convention is
  `(\\frac{a}{b})`, and two guards catch a single backslash:
  - a control character raises `mk-control-character:...`;
  - a bare word that is a supported command raises
    `mk-bare-word-needs-a-backslash:sqrt`. This counts words of 3 or more
    letters, plus pi, mu and xi.

  The second guard has a cost: `x_{max}` must be written
  `x_{\\text{max}}`. Two-letter words (in, ln) are not checked.
- **Errors name the offender.** An unsupported construct executes an
  undefined name built from it, for example
  `mk-unsupported-command:\iint`, `mk-unsupported-character:*`,
  `mk-unsupported-delimiter:<` or `mk-missing-argument:\frac`. pscat and
  gs both print that name.
- **The parser keeps atoms on the operand stack between marks.** Nesting
  needs no stack of its own. Atoms are dicts with `/T`, `/C` (the TeX
  class) and `/K` (the index in the source). The layout recursion keeps
  its locals in one small dict per call.
- **Errors unwind the stacks.** Every public procedure runs through
  `mkentry`. It begins `mkscratch` (drychalk's private-dict pattern) and
  records the operand and dict stack depths. On error it pops back to
  both depths and re-raises, so `$error` still names the original guard.
  A self-test covers an error three groups deep and an error inside
  `/Ink`.
- **The box format** is `[w h d items]`. The items are glyph, grower, rule,
  accent and frame entries, plus `[/b dx dy items]` for a placed box.
  The renderer walks the tree once. The slope is a CTM rotate that is
  saved and restored, never `gsave`, because grestore would drop the path.
- **Metrics come only from public strokeglyphs procedures.** Glyph boxes,
  advances, italic corrections and classes come from `sgmetrics`, cached
  per formula at size 1000. A Unicode character is named by running
  `sgmetrics` on its UTF-8 bytes. A grower's advance, and the radical's
  vinculum start, come from a dry run of `sggrow` at `/Jitter 0` inside
  gsave, reading `currentpoint`. mathkit reads no strokeglyphs internals
  except in a self-test that checks every table name exists in `sgtab`.
- **TeX's rules, re-fitted.** Scripts follow rules 18a-f, fractions rule
  15, radicals rule 11 and limits rule 13, with font parameters per em
  fitted to the skeletons (x-height 500, axis 250). Clearances run a
  little larger than Computer Modern's because the strokes are pen lines.
  Display operators are taller: ∑ and ∏ span 1.3 em and ∫ 1.92 em. An
  integral's limits sit beside its ends, the lower one tucked under the
  hook.
- **Alignment.** `&` stands for TeX align's `{}` at the start of the right
  column, an empty ordinary atom. So `&=` keeps its space before `=`,
  and a line that starts with `=` (no `&`) does too. The = signs then
  line up (`tests/mathkit.rs::mkalign_lines_up_the_relations`).
- **Irregularity is drawn per atom.** Each atom reseeds a private Schrage
  generator from the formula's seed and its `/K`, and draws its 8 values
  before laying out its contents. So nested atoms cannot shift it, and
  with a fixed `/Seed`, editing the end of a formula never re-rolls the
  start (a self-test).
  - Per atom: size ±3.5% and a baseline bob of 0.025 em.
  - Gaps: ±0.05 em, never below -0.03 em.
  - Superscripts: ±0.08 em, subscripts ±0.04 em.
  - Fraction bars: each end overshoots 0.03 to 0.17 em and tilts.
  - Per line: a slope of up to 0.8° and a slow sine drift of up to
    0.045 em.
  - mkalign lines: the = column wanders up to 0.07 em, the leading up to
    0.1 em.

  All of it scales with `/Irregular`. At 0 the layout is TeX's, and only
  the glyph `/Jitter` remains.
- **Seeds.** With no `/Seed`, the layout seed is a hash of the TeX string,
  so `mkmetrics` agrees with `mkpath` without one. The glyph jitter adds
  the device-space position, as `sgpath` does, so one formula written
  twice still differs.

**Tests**

- Seven `%%SelfTest` blocks in the library:
  - every operand guard;
  - 28 refusals of unsupported TeX, including both single-backslash
    traps;
  - the stacks after deep and `/Ink` errors;
  - determinism, the caller's `rand` stream, and prefix stability;
  - the TeX rules (scripts rise, a unary minus has no space, `&`
    alignment, `/Irregular 0` is seed-independent);
  - every table name has a glyph;
  - the annotation guards.
- A strict-lint driver, `selftest/drivers/mathkit.ps`.
- `tests/mathkit.rs`:
  - the fixed test set of 14 formulas, measured in a headless pass, with
    its ink inside the measured box;
  - determinism;
  - the error text a user sees;
  - every `@example`;
  - the specimen;
  - `mkalign` alignment;
  - Ghostscript: it renders the specimen and builds the same metrics and
    path from the same seed, within 0.02 pt. Local only, since CI has
    no gs.
- `tests/libbind.rs` now checks mathkit, with handscript and strokeglyphs
  loaded before the snapshot. `tests/capabilities.rs`' reserved-name
  check covers it too.

**Cost** (release build, a shared machine):

| | pscat | gs |
|---|---|---|
| `examples/mathkit_specimen.ps`: 14 formulas, an `/Irregular` row and a chalk board | 0.60 s at 1x, 0.70 s at 2x | 0.29 s |
| one display formula (quadratic, Taylor) | about 10 ms | |

**Deferred** (filed):

- #339: glyphs strokeglyphs lacks: ≡ ∼ ⟨⟩ ℏ ∘ ⊆ ∬ ∮, ι υ.
- #340: more structures: `\displaystyle`, `\overbrace`/`\underbrace`,
  `\overset`, multi-column align, `\not`.

**For the maintainer's eye**

- The default `/Irregular 1` is a steady lecturer. #238 may want more on
  a real board; 2 reads hurried but legible.
- `\cdot` sits on the math axis as in TeX, so it looks low beside capitals
  (∇ · E).
