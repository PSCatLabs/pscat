## Diagnostics polish: lint after an error, preset operand guards, sheet output names (issue #947, 2026-10-29)

Four follow-ups from the 0.7.0-rc.1 QA fixes (#943, #944, #946). No render changes.

- **CLI `--lint` after an error.** `print_lint` now prints the `[run-error]` line and nothing else
  when the run ended in an error, as `pscat-mcp`'s `split_lint_findings` has since #942: `blank-page`,
  `gsave-imbalance`, `dict-leak` and `stack-leak` read end-of-run state, so after an abort they describe
  the abort. A banded run's `note:` line is dropped with them (it only explains findings), matching the
  MCP rule. The run still exits non-zero; `--lint-strict` prints no footer for an errored run, since there
  is no finding to count. Tests that leaned on the old output were changed on purpose:
  `lint_strict_footer_counts_real_findings_not_the_run_error` (now pins "no blank-page, no footer after an
  error", and the footer on an unerrored `showpage`), and the selftest test
  `rows_9_11_12_a_scenario_that_renders_blank_fails_strict_lint`, whose "neutered" driver replaced the dict
  together with the call, so its `pop` underflowed and the test passed on a consequence `blank-page`. It
  now keeps the dict, so the run is clean and the blank page is a real finding.
- **Paintkit operand guards.** Every public preset and motion (`pkribbon`, `pknib`, `pkdry`, `pkspray`,
  `pkoil`, `pktrowel`, `pktrowelplane`, `pkfan`, `pkbroad`, `pkliner`, `pkwash`, `pkdab`, `pkscumble`,
  `pktap`, `pkpull`, `pkzigzag`, `pkfeather`, `pkbrushedge`, `pkcanvas`; two operands for `pkwet`,
  `pkcrisscross`, `pkscumblein`) and the pigment helpers that take operands (`pigmix`, `pigcolors`,
  `pigtint`, `pigshade`) start with `count N lt { name-needs-... } if`, the shape `pkpaper` got in #944.
  With too few operands the first `exch def` used to underflow and leave its own name on the stack;
  now the error names what is missing and nothing is consumed. One `%%SelfTest` block
  (`presets-reject-too-few-operands`) holds a `mustguard` for each. `pigment` and `pigcolor` start with
  `dup`, which underflows without leaving a name, so they need no guard. Identity check against the base:
  all renders identical.
- **`--contact-sheet PATH` and `pscat sheet -o`** are always PNG, so they use `subcmd::check_output_name`
  like `--png`: another known image or document type is refused with the extension named, nothing written.
- **MCP `out_path`**: no code change. `confine` already refuses any name that does not end in the output
  format's own extension (the overwrite rule), which is stricter than the CLI's `check_output_name` (it also
  refuses a missing or unknown extension, where the CLI keeps those working for existing scripts). New test
  names (`.jpg`, `.JPEG`, `.gif`, `.webp`, `.ps`, `.eps`) pin the agreement.

Skill and guide text about `[run-error]` (pscat skill, getting-started) says only that an errored run
prints that line, which is still true; nothing there needed a change.
