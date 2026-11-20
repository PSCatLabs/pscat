## CI installs poppler and requires it (issue #1206, 2026-11-19)

`tests/pdf_text.rs` reads pscat's text PDFs back with `pdftotext` and
rasterizes them with `pdftoppm` (poppler). CI had no poppler, so both tests
skipped there, and the skip note went through `eprintln!`, which libtest hides
for a passing test. They had only ever run locally (for #1193 / #1200).

- `common::poppler_available(tool)` (tests/common/mod.rs) is the poppler twin of
  `gs_available()`. It writes `ran (<tool> found): <test>` or `SKIPPED (<tool>
  not installed): <test>` straight to stderr, so a CI log shows each outcome,
  and panics when `PSCAT_REQUIRE_POPPLER` is set (any value but empty or `0`)
  and the tool is missing. `poppler_decision` is the pure rule, covered in
  tests/gs_helper.rs. The old `tool_available` in pdf_text.rs is gone.
- The Linux `cross-platform` job installs `poppler-utils`, probes both tools
  (so a broken install fails its own step), writes the versions to the job
  summary and sets `PSCAT_REQUIRE_POPPLER=1` on the test step, as it does for gs.

Deferred: the macOS `test` job (the required check) does not install poppler
(a brew install adds runner time to the required job), and Windows has no
poppler on the runner; both still skip. The `crate` job's Linux leg runs the
tests without the variable.
