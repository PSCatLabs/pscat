# Board math: TeX formulas (`lib/mathkit.ps`, after handscript and strokeglyphs)

Paths such as `examples/...`, `gallery/...` and `docs/...` name files in the
[pscat repository](https://github.com/PSCatLabs/pscat); an
installed pscat does not include them. Every `(lib/...) run` works anywhere:
the library is compiled into `pscat`.

For formulas on a board, use `lib/mathkit.ps`, not hand-placed `sgpath`
calls. It lays out a subset of TeX with TeX's box model and appends the
strokeglyphs skeletons as a path. It paints nothing, so a plain `stroke`
or drychalk's `dcchalk` inks it. **Write every TeX backslash twice** in a
PostScript string: `(\\frac{a}{b})`. A single backslash is eaten by the
string, and mathkit refuses what is left with a named error
(`mk-bare-word-needs-a-backslash:sqrt`, `mk-control-character:...`).

```postscript
(lib/handscript.ps) run (lib/strokeglyphs.ps) run (lib/mathkit.ps) run
newpath (x = \\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}) 60 500 << /Size 36 /Seed 1 >> mkpath
1.6 setlinewidth 1 setlinecap 1 setlinejoin stroke
```

- **`(tex) x y opts mkpath`** puts the baseline's left end at x y.
- **`(tex) opts mkmetrics`** returns `/Width /Height /Depth /Align` in
  points without drawing. It always agrees with `mkpath`, so measure,
  then place.
- **`[ (line) ... ] x y opts mkalign`** writes a derivation, aligned at x
  on each line's `&` or before its first `=`. It returns one record per
  line: `/Origin /Angle /Width /Height /Depth /Align /BBox`.
  `/Ink { pop ... dcchalk }` inks each line as it is written, so pressure
  or colour can change down the board. Several `&` on a line give TeX
  align's columns (right, left, right, left ...), the pairs `/ColumnGap`
  apart (multiples of `/Size`, default 2): `(f(x) &= x^2 & f'(x) &= 2x)`.
  `/ColumnGap 0` is `alignat`; `/ColumnWander 1` lets the later columns
  drift down the lines as a hand's do (default 0).
- **`mkframe`**, **`mkunderline`**, **`mkarrow`** and **`mkbrace`** draw the
  annotations. `x0 y0 x1 y1 << >> mkbrace` is a curly brace whose arms
  end at the two points, pointing right of the direction of travel (left
  to right: down); it returns its point's `x y` for the label.
  `mkframe` makes a box with overshooting corners, or `/Shape /oval`.
  `mkunderline` takes `/Double`, and `mkarrow` takes `/Bend`. `mkframe`
  and `mkunderline` take a line record (`recs 2 get << >> mkframe`) and
  follow its slope. Inside a formula, `\\boxed{...}` and
  `\\underline{...}` do the same.
- A side note is plain `sgpath` text, or `\\text{...}` inside a formula.

Supported TeX:

| Group | Commands |
|---|---|
| Scripts | `x^2`, `x_i`, `x_i^2`, `f'` |
| Fractions and roots | `\frac`, `\dfrac`, `\tfrac`, `\binom`, `\sqrt{x}`, `\sqrt[n]{x}` |
| Large operators and names | `\sum`, `\prod`, `\int` (limits under/over in display style, `\limits` / `\nolimits`); `\lim`, `\max`, `\min`, `\sup`, `\inf`, `\det` (limits under); `\sin`, `\cos`, `\log`, `\ln`, `\exp` and others; `\operatorname{...}` |
| Delimiters | `\left( ... \right)` with `( ) [ ] \{ \} \| \langle \rangle` (the characters `⟨ ⟩ ‖ ∥` typed directly work too) and `.`; `\big` `\Big` `\bigg` `\Bigg` and their `l`/`r` forms |
| Environments | `matrix`, `pmatrix`, `bmatrix`, `Bmatrix`, `vmatrix`, `cases` (rows `\\`, cells `&`) |
| Accents and frames | `\vec`, `\hat`, `\bar`, `\dot`, `\ddot`, `\tilde`, `\overline`, `\underline`, `\boxed` |
| Text | `\text{...}`, `\mathrm{...}` |
| Spacing | `\,` `\:` `\;` `\!` `\ ` `\quad` `\qquad` `~` (plain spaces are ignored, as in TeX) |
| Braces and stacks | `\overbrace{...}^{label}`, `\underbrace{...}_{label}` (`\nolimits` after it: the label beside), `\overset{a}{b}`, `\underset{a}{b}`, `\stackrel{a}{b}` (a relation), `\sum_{\substack{i<m \\ j<n}}` |
| Styles | `\displaystyle`, `\textstyle`, `\scriptstyle`, `\scriptscriptstyle`: the rest of the group in that style |
| Negation and congruence | `\not=` (≠), `\not\in` (∉), `\not<` and any other symbol (a slash through it); `a \bmod b`, `a \equiv b \pmod{n}`, `\mod{n}`, `\pod{n}` |
| Symbols | Greek `\alpha` ... `\Omega`, `\infty`, `\partial`, `\nabla`, `\pm`, `\times`, `\cdot`, `\le`, `\ge`, `\ne`, `\approx`, `\to`, `\Rightarrow`, `\iff`, `\in`, `\subset`, `\cup`, `\cap`, `\forall`, `\exists`, `\cdots`, ...; Unicode symbols typed directly (α ∞ ≤ ∂) |

Anything else fails with a name saying what it was, for example
`mk-unsupported-command:\oplus` or `mk-unsupported-character:@`.

- **`/Irregular`** (default 1) is the board hand. 0 is clean TeX, and 2 is
  hurried. It varies sizes, gaps and script heights, makes fraction bars
  overshoot, and slopes and drifts the baseline (`/Slope` fixes the tilt).
  `/Jitter` is each glyph's own wobble.
- Other keys: `/Style /text` gives inline sizes with side limits,
  `/Spacing` multiplies TeX's spaces, and `/MathHand false` drops the
  curly x.
- **Seeds.** With a fixed `/Seed`, editing the end of a formula never
  re-rolls its start. Unseeded, the layout comes from the text (so
  `mkmetrics` agrees with `mkpath`) and the glyph jitter from the
  position.
- **Pitfalls:**
  - Parentheses in a PostScript string must balance. Write
    `\\left[ 0, 1 \\right\)` with `\)`.
  - Keep each formula on one source line, or end the line with a
    backslash to continue the string.
  - `x_{max}` is refused, because `max` is a command. Write `x_{\\max}`
    or `x_{\\text{max}}`.

`examples/mathkit_specimen.ps` draws the test set, from the quadratic
formula to Maxwell's equations, and a chalk derivation;
`examples/mathkit_structures.ps` the braces, stacks, `\not` and align
columns; `examples/mathkit_columns_braces.ps` `\mod`, `\pod`, side
labels, a column's wander, `alignat` and `mkbrace`.
