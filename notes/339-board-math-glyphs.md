# #339: more board-math glyphs in strokeglyphs and mathkit

Found while working #237 and #238. mathkit refuses, by name, any TeX
whose glyph strokeglyphs lacks, and since #350 sgpath does the same
(`sg-missing-glyph:<char>`). This fills the commonest gaps. Only glyphs
and mappings were added: no existing glyph, advance or mathkit layout
changed (see Evidence).

## What was added

lib/strokeglyphs.ps (glyph names are Adobe-style where Adobe has one):

- Relations: `/equiv` ≡, `/sim` ∼, `/simeq` ≃, `/reflexsubset` ⊆,
  `/propersuperset` ⊃, `/reflexsuperset` ⊇, `/mapsto` ↦, `/perpendicular`
  ⊥ (also U+27C2), `/proportional` ∝, `/therefore` ∴, `/because` ∵
  (#238's agent asked for ∴; ∵ is its mirror and costs nothing).
- Binary operators: `/ringoperator` ∘, `/asteriskmath` ∗, `/star` ⋆,
  `/setminus` ∖.
- Delimiters: `/angleleft` ⟨ and `/angleright` ⟩ (U+27E8/9 and the
  older U+2329/A), `/dblbar` ‖ and ∥ (one glyph, two code points). All
  three are `sggrow` symbols as well as text glyphs.
- Letters: `/hbar` ℏ, `/ell` ℓ, `/iota`, `/upsilon`, `/Upsilon`; omicron
  (U+03BF) maps to the existing `/o`.
- Integrals: `/iint` ∬, `/iiint` ∭, `/oint` ∮, all growing. ∬ and ∭ are
  the integral builder repeated with each copy shifted by 0.85 of the
  inked width; ∮ is the integral with a ring on the axis of its extent
  and over its stem. The ring stays text size (radius 120 + 3 percent of
  the extent, at most 200 units), as the jitter does.
- Superscript digits ⁰ ¹ ² ³ ⁴ ... ⁹ and ⁺ ⁻ ⁿ (`/zerosuperior`, ...).
  These are *derived* in `sginit`: the base glyph's skeleton at 0.6 scale,
  15 units in from the left, raised 320 units (the signs 60 more, to sit
  level with the digits' middle). Nothing about a digit is authored
  twice: when #351 reshapes the 4, `⁴` follows. The superscripts reach
  sgpath only; in mathkit `x²` would put a glyph after x, and `x^2` is
  still the way to write a superscript.

lib/mathkit.ps: `\equiv \sim \simeq \subseteq \supset \supseteq \mapsto
\perp \parallel \propto \therefore \because \circ \ast \star \setminus
\hbar \ell \iota \upsilon \Upsilon \langle \rangle \Vert \lVert \rVert
\iint \iiint \oint`, `\|` (as a symbol and as a delimiter), the delimiter
words for `\left\langle \right\rangle` and the `Vmatrix` environment. The
three integral signs share `\int`'s limits-beside placement through
`mkisintname`. `\ell` used to be a plain `l`; it is now the script ℓ (no
tracked file or open gallery piece uses it).

## Where the shapes came from

The repo's reference board (`docs/drymedia/refs/chalk_full_board.jpg`)
shows a path-lifting derivation with ≃ drawn as a wave over a straight
line, ∘ as a small round ring at mid height, ∃, and superscripts at
about 0.6 of the line height, which is what ≃, ∘ and the superscripts
follow. The rest copy the existing skeletons' own constructions: ≡ is
`/equal` with a third bar, ⊆ is the C of `/propersubset` with a bar
under it the way `/lessequal` has one, ⊃ and ⊇ are those mirrored, ∼ and
≃ reuse `/approxequal`'s wave, ↦ is `/arrowright` with a tail bar, ∴ is
three of `/periodcentered`'s dots. Every glyph was checked on a
`/Jitter 0` render.

## Not done

- Variant Greek (ϕ ϑ ϵ) and the other Greek capitals that look Latin.
- ⟨ ⟩ ‖ typed as UTF-8 *delimiters* after `\left` (the TeX `\langle` form
  works; `mkdelim` only reads ASCII).
- Subscript digits, and ASCII `*` in mathkit (`\ast` works). The mathkit
  selftest pins `*` as unsupported.
- ≠-style negations of the new relations.

## Tests that changed

Tests that asserted a gap now point at symbols still outside the set:
the strokeglyphs missing-glyph selftest uses ⊕ and ℵ where it used ² and
∮; mathkit's unsupported list uses `\oplus`, `\aleph` and `\lceil` where
it used `\iint`, `\star`, ∮ and `\langle`.

## Evidence

See the PR description: `scripts/identity_check.sh --loader
lib/strokeglyphs.ps` and `--loader lib/mathkit.ps` against the merge
base, plus `gallery/chalkboard_lecture.ps` (PR #353's branch) and
`gallery/crayon_drawing.ps` (PR #336's branch) rendered with main's
libraries and with these, byte for byte.
