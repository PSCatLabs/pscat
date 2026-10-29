## chalk/mathkit: logic, set and geometry symbols, three alphabets, the subset in --help (issues #934, #935, 2026-10-29)

A QA pass of `pscat chalk` on 0.7.0-rc.1 could not write a proof, a truth
table, a vector in bold, a number set or a Lagrangian: each command failed
with `unsupported LaTeX command`.

- **Symbols** (strokeglyphs skeletons, mathkit commands, Unicode code points):
  `\wedge`/`\land` ∧ and `\vee`/`\lor` ∨ (binary operators), `\vdash` ⊢,
  `\nvdash` ⊬, `\models` ⊨, `\leftrightarrow` ↔, `\cong` ≅ (relations),
  `\oplus` ⊕, `\otimes` ⊗, `\dagger` †, `\ddagger` ‡ (binary, so `A^\dagger`
  is an ordinary superscript, as in TeX), `\top` ⊤, `\bot` (the existing ⊥ as
  an ordinary symbol), `\nexists` ∄, `\emptyset`/`\varnothing` ∅, `\angle` ∠,
  `\triangle` △, `\Re` ℜ, `\Im` ℑ. `\cup`, `\cap` and `\subseteq` already
  existed. ∧ and ∨ are a little under the x-height and wider than tall, with
  straight arms and a sharp vertex: at x-height and letter width the first
  render of `u \vee v` read as "u v v", and a taller one as a capital V and a
  Lambda.
- **`\mathbf{...}`**: strokeglyphs derives `/x.bf` for A-Z, a-z and 0-9 at
  load. Each stroke is written twice, the second 32 units right and 16 down,
  closer than a board's chalk is wide, so the pair merges into one heavier
  line. A first try at a pen-width apart read as hollow outlines next to
  `\mathbb`. Greek and operators are unchanged, as in TeX.
- **`\mathbb{...}`**: `/A.bb` ... `/Z.bb`. A hand doubles a capital's stem (or
  stands a short upright in a round letter's bowl: C G O Q) about a sixth of
  an em inside it. At a pen-width and a half (the first try) ℕ ℤ ℚ ℂ read
  as plain letters at board size. ℤ is drawn whole (a second diagonal). ℂ ℍ ℕ
  ℙ ℚ ℝ ℤ typed directly reach the same glyphs.
- **`\mathcal{...}`**: 26 authored script capitals, one flowing stroke where a
  hand makes one, most with a curl leading in at the top. The script I has a
  short crossbar, so it doesn't read as the script J, which descends.
- `\mathbb` and `\mathcal` take capitals only, as in TeX (amsfonts). Anything
  else fails `mk-mathbb-takes-capital-letters:x`, naming the character.
  Inside an alphabet a letter run is letters (`\mathbf{max}` is not refused
  as a bare `\max`). The alphabet is mathkit's `mkFont`, reset by
  `mkparsetop`, so a formula that stops inside `\mathbb{` never leaves the
  next one in blackboard bold (tests/mathkit.rs pins it; the test fails with
  the reset removed).
- `top` and `bot` are new command names, so mathkit's bare-word check would
  have refused `x_{top}`. They are exempt: `(\top)` and `(\bot)` in a
  PostScript string lose a tab or a backspace, which the control-character
  check already names.
- **The subset in `--help`**: an installed pscat has no `lib/mathkit.ps`, so
  `pscat chalk --help` prints the supported LaTeX (a separate block, not in
  the usage text a usage error prints), and the unsupported-command error
  points there. `tests/chalk.rs::help_lists_only_supported_commands` checks
  that every control word it lists is in mathkit's table. psart's
  `board-math.md` and `docs/guides/dry-media.md` list the new commands.
- The stock "unsupported" example was `\oplus` / ⊕ in selftests, tests and
  docs. It is now `\aleph` / ⊛.
- Not done: the pscat skill's `references/handwriting-chalk.md` still quotes
  the old `\oplus` error (another lane), and the remaining blackboard-bold
  digits (𝟙) and lowercase are out, as they are in amsfonts.
