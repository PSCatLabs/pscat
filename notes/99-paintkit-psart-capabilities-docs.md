# 99: paintkit in psart, and the `parameters` docs

Docs-only. No renders change.

## What changed

- `docs/dev/CAPABILITIES.md`: the `parameters` paragraph (was: only the five
  templates and the four handwriting procedures) now describes the field
  generically. It is filled for dict-driven templates and procedures with
  named, defaulted keys, and empty for positional stack-effect procedures,
  fonts, palettes, dials and Type 3 faces. The bullet on "most procedures"
  is reframed the same way. "`lib/paintkit.ps` so far" wording is replaced
  by "any file carrying `% @` tags", because about 20 lib files are tagged.
- `.claude/skills/psart/SKILL.md`: `pkoil` gets a minimal example (verified
  with `pscat --page 360x240 --dpi 144 --png`) and its vector-only scope
  boundary. The watercolor bullet says to check alpha with `pscat --pdf`
  and points general surfaces at `lib/surfacekit.ps`. The line naming issue
  #99 as the "fuller tour" is removed.
- `docs/dev/HANDOFF.md`: the capabilities paragraph notes that tagged files
  are parsed by `build.rs` (#94).

## Measured payload (pscat --capabilities, this build)

- 398 entries. 91 carry non-empty `parameters`: 5 templates and 86 procedures.
- By source: 20 files carry `% @` tags. Their `@param` lines feed the
  parameters. The rest come from hand-written `Param` rows in
  `src/capabilities.rs`: the page templates, the handwriting pairs, and
  artkit's `scatter`.
- Procedures with positional arguments (e.g. `lattice`) have none.

## Not changed

- `src/capabilities.rs` module docs (about lines 26 and 56-64) still say only
  templates, the handwriting pair and `scatter` use `parameters`, and that
  paintkit is "so far". That is code-adjacent, so it is filed as a follow-up (#544).
- README: no catalog wording, so it is left alone.
