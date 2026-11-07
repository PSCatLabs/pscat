# #1052: `[shadowed-operator]` lint

`--lint` now warns when `userdict` holds a plain value under the name of a
built-in operator, e.g. `/sub (subtitle) def`: every later `sub` pushes the
string, and the failure shows far from the cause. One finding per name, sorted,
at most 10, then a count of the rest.

Shape (src/lint.rs, `check_shadowed_operators`):

- A scan of the finished run's `userdict` against `systemdict`, like the other
  checks (they all read end state). Hooking `def` would put a lookup on the hot
  path and live outside lint.
- Only non-procedure values. `/showpage { ... showpage } def` and other
  procedure wrappers are the deliberate way to override an operator. An operator
  bound to its own name (`/sub /sub load def`) is a no-op.
- Not gated by `render_checks`, and `check_band` runs it too (userdict is the
  same in every band).
- The opt-out is scoping, not a comment: a name defined between `begin` and
  `end` is gone from `userdict` by the end, so it is not seen.

Limits, on purpose:

- No line number: lint gets the finished interpreter, which does not record
  where a `def` ran.
- A shadow in a local dictionary that has already ended is invisible. That is
  the opt-out, and also a miss for a bug there.
- `store` of an operator name overwrites it in `systemdict`; not seen.
- The triage asked for a `%lint: allow shadowed-operator` comment. Lint never
  sees the source, so a comment needs the source threaded to five call sites
  (main.rs x3, subcmd, bands.rs), which overlap the open CLI PR #1069. Left as a
  follow-up; the scoping opt-out covers the case meanwhile.

Corpus check: every lib file and every `gallery/` and `examples/` program was
linted. One hit, `gallery/hundred_lines.ps` (`/ln` loop index shadows `ln`):
a true positive, harmless there, and `--lint-strict` is not run on the gallery.
