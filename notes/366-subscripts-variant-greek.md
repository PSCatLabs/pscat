# #366: subscript digits and letters, variant Greek

Found while working #339. strokeglyphs refused U+2080-2089 and the variant
Greek letters, and mathkit drew `\varphi \vartheta \varpi \varrho \varsigma`
as the plain letters. No existing glyph, advance or committed render changed
(see Evidence).

## What was added (lib/strokeglyphs.ps)

- Subscripts, derived in `sginit` by the same procedure as the superscripts:
  `₀ ... ₉` (`/zeroinferior` ... `/nineinferior`), `₊ ₋ ₌`, and the letters
  `ₐ ₑ ₒ ₓ ₙ ᵢ ⱼ`. Each is the base glyph's skeleton at 0.6 scale, 15 units in
  from the left, lowered 80 units (the signs 60 higher, level with the
  digits' middle). A reshaped digit (#351's 4) reshapes its subscript.
  The superscript derivation loop became `sgderive` (table + raise); its
  arithmetic is `0.6 mul <raise> add <extra> add`, the same operations in the
  same order, so the superscripts are bit-identical (the identity check
  proves it).
- Authored variant Greek: `/theta1` ϑ (hooked script theta), `/lunateepsilon`
  ϵ, `/kappa1` ϰ, `/rho1` ϱ, `/omega1` ϖ (omega with a bar over it, the pi
  variant), `/sigma1` ς (final sigma). Adobe names where Adobe has one
  (theta1, omega1, sigma1).
- U+03D5 ϕ maps to the existing `/phi`. That glyph (closed bowl, straight
  stem) is already the stroked form that TeX calls `\phi` and Unicode calls
  U+03D5. U+03C6 keeps drawing it too. The loopy φ is `/phi.loopy`, a
  name-only variant like `/x.curly`, reached by `[/phi.loopy]` and by
  mathkit's `\varphi`.

## What was added (lib/mathkit.ps)

`\varphi` `\vartheta` `\varpi` `\varrho` `\varsigma` and the new `\varkappa`
draw the variants. As with superscripts, the subscript characters reach
sgpath only: in mathkit `x_2` stays the way to write one.

## Behaviour change to know about

`\epsilon` now draws the lunate ϵ (`/lunateepsilon`), as in TeX, where
`\epsilon` is ϵ and `\varepsilon` is the three-shaped ε. `\varepsilon` is
unchanged (it already drew the three-shaped letter, and the Maxwell
formulas in mathkit's specimen and tests use `\varepsilon_0`). Before, both
commands drew the three-shaped letter. A typed U+03B5 ε is still the
three-shaped one, U+03F5 ϵ the lunate. No tracked file, and neither open
gallery branch, uses `\epsilon` (`git grep`).

## Not done

- Superscript `⁼ ⁽ ⁾` and subscript `₍ ₎` (not asked for).
- The phonetic subscripts `ₕ ₖ ₗ ₘ ₚ ₛ ₜ` (one table row each if wanted).
- Greek capitals that look Latin; ASCII `*` in mathkit (#364).

## Evidence

All with a release binary built from this branch (no `src/` change).

- `scripts/identity_check.sh BASE --loader lib/strokeglyphs.ps --loader
  lib/mathkit.ps --control <driver: (x₂ϑ) in sgpath and (\varphi_n \epsilon)
  in mkpath, each under a stopped>` against a detached merge-base worktree:
  13 of 13 pages identical (drychalk, mathkit, strokeglyphs and
  strokeglyphs_board_math specimens, both selftest drivers); the control
  differs, as it must; the new specimen is `new`, not compared.
- The two open gallery pieces, rendered in a detached worktree of each PR
  branch with origin/main's `lib/strokeglyphs.ps` and `lib/mathkit.ps`
  copied in (base arm), then with this branch's (head arm), at 72 and 144 dpi:
  `gallery/chalkboard_lecture.ps` (#353's branch, 4c72d80) sha1 72 dpi
  1cb6eaea5adf, 144 dpi 1a873cda28ad in both arms; `gallery/crayon_drawing.ps`
  (#336's branch, a685d2d; that branch has no mathkit.ps, only
  strokeglyphs.ps was swapped) 72 dpi e7a33ce8ab6f, 144 dpi a47ca5a5d1dd in
  both arms. The control driver differs between the arms in both worktrees
  (3fa1cf5b28fc vs 2f8028f7ced2), so the swap really happened. The 72 dpi
  hashes match the ones #339 recorded.
- `PSCAT_PROFILE=ci ./scripts/selftest.sh`, `cargo test --profile ci`
  (gs installed), clippy and fmt clean.

Specimen: `examples/strokeglyphs_subscripts_greek.ps`, committed render
`docs/drymedia/strokeglyphs_subscripts_greek_specimen.png`.
