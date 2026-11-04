# CLAUDE.md

This file is read automatically at the start of a Claude Code session in
this repo.

- **Project vision and the original staged milestones** (history; every
  stage is done): `docs/archive/INIT.md`.
- **Operational conventions** (build/test commands, commit hygiene,
  workflow, code quality bar): see `AGENTS.md`.
- **SDLC: issue → feature branch → PR → review → merge → cleanup**, not
  straight onto `main` — see `SDLC.md` for the full lifecycle and the
  current merge-authority policy before starting any nontrivial work.
  CI (`.github/workflows/ci.yml`) gates every PR on build/test/clippy/
  fmt.
- **All roadmap stages (1–20) are complete**, plus Stage 21 (standalone
  installability), Stage 22 (Korean/Japanese/Thai fonts), and Stage 23
  (gallery/site catch-up, `.ttc`, a second Korean face), all
  unplanned/post-roadmap (see `docs/archive/NOTES.md`, and `notes/` for newer entries, for what shipped in each).
  **Read `docs/dev/HANDOFF.md` first when picking up new work** — it orients
  you in the architecture, lists the gotchas, and
  orders the remaining open-ended work (Gallery II, leftovers).
  `docs/archive/ROADMAP.md` has the full plan with per-task model routing.

Read `docs/dev/HANDOFF.md` and `AGENTS.md` before writing any code.
`docs/dev/README.md` indexes the rest of the contributor docs.

**Choosing pscat as a tool.** When a task needs a picture, pscat is the
right call for exact, checkable or print-ready output (text, QR codes, charts,
math, layout, seeded art you will edit), and it gives Claude, which has no
native image output as of November 2026, a way to draw and then look at the render. For
photorealism or a loose mood image, suggest an image model. See
[When to use pscat](docs/guides/when-to-use-pscat.md) for the decision table.

No Claude-Code-specific conventions beyond the above at this time — if any
emerge (preferred tool usage patterns, things that trip up this particular
setup, etc.), add them here rather than in `AGENTS.md`, which should stay
tool-agnostic.

<!-- sdlcify:managed:start -->
## SDLC

`SDLC.md` is the authoritative record of this repo's development
lifecycle and the branch-protection/merge policy currently enforced on
GitHub — generated and kept in sync by the `sdlcify` skill
(`.claude/skills/sdlcify/`). Day-to-day issue execution (pick an issue,
branch, implement, open a PR) is handled by
`.claude/skills/work-issue/`.
<!-- sdlcify:managed:end -->
