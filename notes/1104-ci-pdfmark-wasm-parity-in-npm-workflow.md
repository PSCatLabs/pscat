# 1104: run the CLI-vs-wasm pdfmark byte check in the npm workflow

`wasm_pdfmark_document_matches_the_cli` (tests/pdf_pdfmark.rs, #1032) checks
that `pscat --pdf` and the wasm build write the same bytes for a `pdfmark`
document. Only the npm workflow builds the wasm target, and it ran only
`tests/wasm.rs`, so the check never ran in CI.

The npm workflow now runs it as its own step, with `--exact --nocapture` so
that a skip (no wasm target) shows up in the log instead of a silent pass.
`tests/pdf_pdfmark.rs` is in both `paths:` filters beside `tests/wasm.rs`.
