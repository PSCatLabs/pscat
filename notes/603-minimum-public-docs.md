## Minimum public docs: SECURITY, stability policy, CONTRIBUTING, CODEOWNERS (issue #603, 2026-10-16)

Part of release epic #562 (split from #570). The files a public repo needs before
anything is published.

**Built.**
- `SECURITY.md`: how to report, and a threat model checked against the code and by
  running a `ci` build. PostScript has read-only file access and no write, delete,
  process, network or environment operators; `file` takes only `r`. The gaps are
  stated: no step, time, total-memory, operand-stack, gsave or path limit. The MCP
  server's `out_path` is an unconfined write, a runaway program blocks it (one
  request at a time, no child timeout, unbounded output buffer), and `handwrite`'s
  `ink` is spliced into PostScript. Fixes are planned in #568 and #604, not made here.
- `docs/STABILITY.md`: guarantees start at 0.6.0; stable, experimental and internal
  surfaces; pre-1.0 semver (a minor release may break); a deprecation is announced
  at least one minor release before removal.
- `CONTRIBUTING.md` (inbound = outbound: code MIT OR Apache-2.0, original creative
  resources CC0-1.0, fonts keep upstream licences), `CODE_OF_CONDUCT.md`
  (Contributor Covenant 2.1), three issue templates and a PR template.
- `.github/CODEOWNERS`, and the SDLC rule that agents don't self-merge PRs touching
  it. It is recorded as the `merge_policy.owner_merge_file` field in SDLC.md's
  frontmatter, the "Who merges" section in the sdlcify template and SDLC.md, and a
  check in work-issue step 8.10 backed by `scripts/codeowners_check.py` (tested by
  `scripts/test_codeowners_check.py`, run in `links.yml`).
- `.gitignore`: root-level scratch (`/1`, `/output/`, `/*.png`, `/*.ps`, `/branding*/`,
  `/.playwright-mcp/`), anchored so `examples/` and `gallery/` are unaffected.

**Decisions.**
- The rule is enforced by the skill, not GitHub. "Require review from Code Owners"
  needs a required approval, and agents open PRs as the owner, who cannot approve
  their own PR.
- The checker reads CODEOWNERS from both `origin/main` and the PR, so deleting a
  line cannot exempt a PR. CODEOWNERS lists itself.
- Some entries are forward-declared (`deny.toml` #601, `scripts/release.sh` #602,
  `LICENSE*` and `THIRD_PARTY_NOTICES.md` #563, `.claude-plugin/` #569): the rule is
  in place before the file lands. They carry `# forward:` comments, and
  `--check-paths` fails on any other entry that matches no file.

**Deferred.**
- MCP confinement and timeouts (#568), interpreter resource limits and fuzzing (#604).
- The outsider README rewrite (#602 checklist).
- The doc comment in `src/bin/pscat_mcp.rs` still says "three tools" (there are four).
- Owner decisions listed in the PR: enable private vulnerability reporting, a conduct
  contact, whether `SDLC.md` and `.claude/skills/` belong in CODEOWNERS.
