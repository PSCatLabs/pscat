# #340: more TeX structures in mathkit

Found while working #237. mathkit refused these by name as unsupported
commands. All of #340 shipped in one PR. No existing glyph, layout or
committed render changed (see Evidence).

## What was added (lib/mathkit.ps)

- **Style switches.** `\displaystyle`, `\textstyle`, `\scriptstyle` and
  `\scriptscriptstyle` push a `/style` marker. `mklist` sets the rest of
  its list in that style: sizes, spaces and whether limits stack. Like
  spaces and `&`, the marker has no class and can't carry a script.
- **`\overbrace{...}^{label}`, `\underbrace{...}_{label}`.** As in TeX,
  each is a `\mathop{...}\limits`: atom `/brace`, class `/op`, `/Lim /yes`.
  `mkuselimits` now accepts `/brace` as well as `/op` and `/fn`, so the
  label is a limit (`mklimits`).
  - The brace is strokeglyphs' existing grown left brace (`sggrow
    /braceleft`), drawn under a CTM turned -90° (over) or +90° (under).
    Its point faces away from the content and its arms reach toward it.
  - A new box item, `[/h x y H D size seed angle]`, saves and restores the
    CTM around `sggrow`, as the line slope does (never gsave, which would
    drop the path).
  - No strokeglyphs change and no new grower. The brace's length is the
    content's width (at least 0.4 em). Its thickness, 0.08 to 0.28 em,
    comes from the existing builder.
  - The brace sits 0.08 em from the content. Its thickness comes from a
    dry run (`mkgrowink`: `pathbbox` under an identity CTM, so a caller's
    rotation can't inflate it).
  - It is drawn upright (`/Slant 0`): inside the turned frame the slant
    would shear along the wrong axis. Under `/Slant` the layout shifts it
    by its height times tan(slant), as accents are.
- **`\overset{a}{b}`, `\underset{a}{b}`, `\stackrel{a}{b}`.** a is set in
  script style, centred over (under) b with a 0.1 em gap. The class follows
  amsmath's `\binrel@`: b's class when b is one relation or binary
  operator, else ordinary. `\stackrel` is always a relation. So
  `x \overset{\text{def}}{=} y` keeps the relation spaces (a self-test).
- **`\substack{a \\ b}`.** A new parse mode, `/stack`, wired into four
  places: `}` ends it (`mkbrbrace`), `\\` gives a new row (`mkctlsym`),
  end of string raises `mk-missing-closing-brace` (`mkateof`), and `&`
  stays `mk-misplaced-ampersand`.
  - The rows are centred, a script line apart (at least 1 em, more if
    they are tall), and the block is centred on the axis, like amsmath's
    `subarray`.
  - A trailing `\\` adds no row.
- **`\not`.** It takes exactly one symbol token; anything else raises
  `mk-not-needs-one-symbol-after-it`. `\not=` is the ≠ glyph and
  `\not\in` the ∉ glyph. Any other symbol (`\not<`, `\not\subset`,
  `\not\equiv`) gets a hand-drawn slash, an `/r` rule through the glyph's
  ink centred on the axis. The symbol keeps its own width and class.
- **`\bmod` and `\pmod{n}`.** `\bmod` is an upright "mod" of class bin,
  with the medium space each side; TeX's net is 5 mu, ours 4. `\pmod{n}`
  is an 18 mu space, then `(`, "mod", 6 mu, the argument and `)`, as
  amsmath sets it in display and text style.
- **`mkalign` columns.** With more than one `&` on a line, the columns
  between them alternate right and left aligned, as in TeX's `align`:
  `(f(x) &= x^2 & f'(x) &= 2x)`.
  - `mklist` now records every top-level `&`: its x, and how many items
    come before it.
  - `mkcolumns` sets pair k's alignment point from the widest columns
    across all lines, plus `/ColumnGap`. That is a new mkalign option, in
    multiples of `/Size`, default 2.
  - It shifts each line's items after its 2nd, 4th, ... `&` and widens the
    line's box by the same amount, so `/Width`, `/BBox`, `mkframe` and
    `mkunderline` follow.
  - A line with one `&` or none is untouched, and so is everything when
    no line has two. So every existing derivation lays out as before.
  - The 1st, 3rd ... `&` still stand for TeX's `{}` (an empty ordinary
    atom). The 2nd, 4th ... start a fresh cell, as amsmath's right-aligned
    columns do: the class pass ends the list there (a trailing binary
    operator becomes ordinary) and no space carries across. So
    `x + y &= 3 & -x + 2y &= 0` sets the second minus unary (advisor
    review; a self-test, which fails on the first cut by two medium
    spaces).
  - `mkpath` and `mkmetrics` are unchanged: `/Align` is still the first
    `&`.

## Design calls

- **No new strokeglyphs grower for the horizontal brace.** The issue
  expected one, but sggrow's API is a height and a depth. A horizontal
  grower would need a second calling convention, or would misuse H and D.
  Turning the existing brace keeps one brace shape for both the vertical
  and the horizontal one.
- **The second column pair isn't jittered on its own.** The whole line
  still wanders (`/Jx`), and the gaps between atoms are irregular, which
  reads as a hand.

## Follow-up filed

- #439: `alignat`-style columns and a per-column wander, `\mod` and
  `\pod`, `\nolimits` on a brace, and an `mkbrace` annotation built the
  same way as the turned brace.

## Behaviour change to know about

These words are now command names, so a bare run of their letters in
math raises `mk-bare-word-needs-a-backslash:<word>` where it used to
draw the letters: `not` (3 letters, the one likely to bite), `bmod`,
`pmod`, `overset`, `underset`, `stackrel`, `substack`, `overbrace`,
`underbrace`, and the four style words. `\text{not}` is unaffected.
No tracked file writes any of them bare.

## Tests

- Two `%%SelfTest` blocks:
  - `tex-structures-lay-out`: every construct lays out and draws (also
    under `/Slant 10`); the style rules; braces and labels stand beyond
    the content; `\overset` and `\stackrel` take relation spacing;
    `\not=` is `\neq`; `\pmod{n}` equals a quad and (mod n); ten
    refusals.
  - `mkalign-columns-alternate-right-and-left`: the second pair lines up
    across lines, its right column right-aligned, `/ColumnGap`'s unit,
    and a single `&` left as it was.
- `tests/mathkit.rs`:
  - The new specimen renders with ink and an empty stack.
  - Ghostscript renders it, and builds the same layout and path (within
    0.02 pt) for a nested brace, `\not<`, a substack and `\pmod`. The gs
    check is now two helpers shared with #237's test.

## Demo

`examples/mathkit_structures.ps` is a new specimen, rendered at 2x as
`docs/drymedia/mathkit_structures.png`. It shows braces (nested too),
stacked labels, negation, style switches with a continued fraction, and
align columns, with a pen and on a drychalk board. `mathkit_specimen.ps`
is untouched.

## Evidence

- `scripts/identity_check.sh <merge-base worktree> --loader lib/mathkit.ps
  --loader lib/strokeglyphs.ps --control <driver drawing an overbrace,
  \not< and \pmod under stopped>`: 16 of 16 pages identical across 9 files
  (both mathkit and strokeglyphs specimens, drychalk's, the chalkboard
  lecture and crayon gallery pieces, both selftest drivers). The control
  differs, as it must.
- Every tracked mkalign line has at most one `&` (grepped), so the column
  code never runs on them.
- At `/Irregular 0` the board's three lines align exactly on both pairs.
  At 1 they differ by the per-line wander, 0.07 em at most.
