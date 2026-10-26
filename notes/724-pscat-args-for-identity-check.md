# 724: `identity_check.sh --pscat-args`

A change to the limit paths (`--max-time`, `--max-memory`, `--max-steps`,
#604) acts only when those flags are set, so the identity check could not
give it a negative control. `--pscat-args STR` adds flags to every render
(both trees and the control).

## What changed

- `scripts/identity_check.sh`: `--pscat-args STR`. STR is split with
  `xargs` (quotes group words, nothing is executed, so `$(...)` stays
  literal). An unbalanced quote exits 64 before any scratch directory
  exists. `--png`, `--svg`, `--pdf`, `--page`, `--dpi` are refused (exit 64)
  because the script owns output and page. The words go NUL-separated into
  `$OUT/pscat-args.nul`; `--render-one` reads them into an array and puts
  them before `--page`/`--dpi`/`--png`. Bash 3.2 safe: the empty array is
  expanded as `${EXTRA[@]+"${EXTRA[@]}"}`.
- `tests/identity_check.rs`: a logging wrapper that halves the control's dpi
  only when argv has `--max-steps`. Without the flag the control is identical
  and the run exits 2; with `--pscat-args "--max-steps 10000000"` the control
  differs, the examples stay identical, and every render in both trees has
  the flag. Also covers quoting (`$(touch ...)` not run, a quoted word with a
  space stays one word), refused flags and unbalanced quotes, and whitespace
  as no flags.
- `docs/dev/HANDOFF.md`: one sentence in the paintkit identity paragraph.
- `--max-steps 1000` is too low for the examples (sierpinski hits
  `limitexceeded (steps)`), so the documented value is 10000000.

The `references/` doc line (perceptual-review.md) is left for #709, since
`.claude/skills/work-issue/references/` is a CODEOWNERS path.
