## sdlcify: the Rust quality-gate line now matches ci.yml (issue #216, 2026-10-14)

The gate line that sdlcify wrote into `SDLC.md` step 5 (and that the
Phase 0 table defaults to) was `cargo build && cargo test && cargo clippy
--all-targets -- -D warnings && cargo fmt --all -- --check`, described as
"the same command CI runs". After #204 it is not: CI runs
`scripts/selftest.sh` and `cargo test --profile ci`, drops the separate
`cargo build`, and runs fmt and clippy as separate steps.

What changed:

- `SDLC.md` frontmatter gained an optional `quality_gate:` field, set to
  CI's sequence. Step 5 now quotes it. Frontmatter, not prose, because
  a sdlcify re-run regenerates `SDLC.md` wholesale; a prose-only fix would
  have reverted on the next run.
- sdlcify's Phase 0 table is documented as only a default starting
  command. `quality_gate`, when present, wins (Phase 1 rule). Phase 2 reports
  drift between `ci.yml` and the gate in use, and asks the user to set the
  field. It does not parse the workflow, since CI steps carry env blocks,
  pipes and `continue-on-error`.
- The sdlc-doc template's step 5 takes its command from `quality_gate`
  and falls back to the language default. Its "same command CI runs"
  wording became "same checks CI runs".
- `ci.yml`'s header comment named the same stale line; it now lists CI's
  real checks. Comment only.

Not changed, on purpose: the Rust CI skeleton in
`references/language-profiles.md` (the generic default for repos with no
CI yet, which would change every future bootstrap) and AGENTS.md, which
already describes `cargo test --profile ci` correctly.

`sdlcify_version` stays 1. `quality_gate` is optional and falls back to
the old default, so existing files stay valid. Not re-run against GitHub;
branch protection is untouched.
