# AGENTS.md

Operating conventions for any AI coding agent working in this repo. Project
vision, priorities, and staged milestones live in `INIT.md` — read that
first. This file covers *how* to work, not *what* to build.

## Before you start

- Read `INIT.md` in full.
- If this is a fresh session and `INIT.md`'s Stage 1 hasn't been completed
  yet (check for an architecture writeup / commit history), start there.
- If Stage 1 is done, check for a stage-summary (in commit messages, a
  `NOTES.md`, `notes/`, or similar — use your judgment on where past summaries were
  left) before continuing, so you're not re-deciding settled questions.
- Stages 5 and beyond are planned in `ROADMAP.md`, including which model
  tier each task is suited to and an escalation rule for tasks that turn
  out to touch the object model or execution machine. Follow it unless
  you have a good reason not to — and record that reason in a `notes/` fragment.

## Toolchain & commands

- Language: Rust, stable toolchain unless a specific need arises to pin
  otherwise (explain why if you do).
- `cargo build` and `cargo test` should pass before considering any stage
  or sub-step complete.
- Run the suite as `cargo test --profile ci`, the same command CI runs:
  optimized (the tests run PostScript through the interpreter, ~14x
  slower in a debug build) but with `debug-assertions` and
  `overflow-checks` on (`[profile.ci]` in `Cargo.toml`; output in
  `target/ci/`). Plain `cargo test` still works, just slowly.
- `cargo clippy` should be clean (or warnings explicitly justified in a
  comment) before considering a stage complete.
- `cargo fmt` before committing.

## Workflow

- Follow the staged deliverables in `INIT.md` in order, unless you have a
  good reason to resequence — if so, say so explicitly rather than
  silently reordering.
- Pause after Stage 1 for the architecture writeup described in `INIT.md`
  before starting Stage 2.
- At the end of each stage: summarize what was built, what tradeoffs were
  made, and what was explicitly deferred. Commit with a message that
  reflects the stage, not just the last edit made.
- Commit at reasonable checkpoints within a stage too — don't hold an
  entire stage's worth of work in one giant commit.
- Make judgment calls on genuine coin-flip decisions and note them; don't
  stall waiting for input on things you're equipped to decide. Do flag
  decisions that are hard to reverse later.

## Software development lifecycle

Nontrivial work (a new stage, a feature, a fix worth more than a
one-line diff) goes through issue → feature branch → PR → review →
merge → cleanup, not straight onto `main`. The full lifecycle, and the
current merge-authority policy (who/what may merge once CI is green
and review is clean), is `SDLC.md` — read that, not this section, for
the authoritative process; `.claude/skills/work-issue/` automates it
end to end. This applies going forward; it doesn't retroactively
obligate rewriting the stage-by-stage direct-to-`main` history that
predates it.

## Code quality bar

- No `unwrap()`/`expect()` on anything derived from program input (parsed
  PostScript source, user-supplied files). Reserve those for states that
  are genuinely impossible to reach, and prefer proper error types
  (`thiserror` or similar) otherwise.
- Comments explain *why*, not *what* — skip comments that just restate the
  code; do explain non-obvious design choices (e.g. why the dict stack is
  structured a particular way).
- Tests live alongside the code they test. Add integration or golden-image
  tests where `INIT.md`'s stages call for demonstrable rendering output.
- **Timing assertions in `tests/` measure CPU time, never wall time**
  (`notes/329-cpu-time-render-budget-tests.md`). Wall time includes every
  slice the scheduler gave to other processes, so with several agents or a
  shared CI runner a 0.6 s render reads 2 s and the test flakes. The
  helpers are in `tests/common/mod.rs` (`mod common;`), on the test
  thread's CPU clock (`CLOCK_THREAD_CPUTIME_ID`; a process-wide clock would
  count the neighbouring tests). Two shapes, both best-of-N so one slow
  sample cannot fail the suite:
  - An absolute budget (DRYMEDIA.md 6.2: a CI test render takes 2 s or
    less): `within_cpu_budget(what, budget, 3, || ...)`. The closure
    rebuilds whatever it mutates; it returns the fastest try's value. The
    2 s cap is DRYMEDIA's, not a tuned bound: the four render scenes
    measure 0.25-0.7 s of CPU, so this catches a 3-8x regression, not 2x.
  - A "the guard must not walk the path first" test: a bound relative to a
    same-run baseline (`pitfalls.md`, "Timing assertions in tests"), with
    both sides timed by `cpu_time_of` / `thread_cpu_time`
    (`every_walkpath_preset_refuses_a_huge_segment_count_promptly` is the
    model).
  Do not use `Instant` in a test. Negative check for a new budget test:
  `PSCAT_TEST_BUDGET_SCALE=0.001 cargo test --profile ci --test <file>
  <name>` scales every budget in the run and must make it fail (apply
  `scale_budget` to a hand-rolled bound).
- A change to `lib/*.ps` that adds or tightens a *validation guard*
  belongs in a `%%SelfTest` block in that file, next to the guard —
  `./scripts/selftest.sh` runs them and `docs/SELFTEST.md` has the
  convention. Rust stays the right home for anything needing pixel
  readback or a real `gs`.
- A procedure added to `lib/artkit.ps`, `lib/paintkit.ps`,
  `lib/drykit.ps`, `lib/drychalk.ps` or `lib/drycharcoal.ps` is defined `} bind def`, not
  `} def`: unbound, gs looks up every operator in it at run time and runs
  1.5 to 3 times slower (`notes/307-bind-library-procedures.md`,
  `notes/318-bind-drychalk.md`, `notes/326-bind-drycharcoal.md`). `tests/libbind.rs` enforces it.
- Keep the docs accurate to *current* state, not aspirational state.
  Update them as capabilities land, not just at the end. User-facing
  how-to goes in the matching page of `docs/guides/`; `README.md` is a
  short front door and gets a line only for a new headline capability.

## Where things live

- `ARCHITECTURE.md` — the design writeup (object model, execution model,
  rendering-crate leanings, deliberate deferrals). Read before changing
  anything structural.
- `docs/guides/` — user guides, one per library or topic (the old README's
  long tour); `docs/archive/` — history kept as written, including the
  stage log. Check links with `python3 scripts/check_md_links.py` (CI runs it
  as the `links` workflow); it skips `docs/archive/` and `notes/`.
- `NOTES.md` — per-stage summaries, newest first, frozen as of 2026-10-13.
- `notes/` — newer entries, one file per issue (`notes/<issue>-<slug>.md`,
  format in `notes/README.md`). Write new notes here, not in `NOTES.md`, so
  concurrent PRs never conflict.
- `src/lexer.rs` — tokenizer; unit tests in-module.
- `src/object.rs` — `Object`/`Value`/`Dict`/`Num`.
- `src/interp.rs` — the execution machine (operand/dict/exec stacks),
  including the `begin_source`/`step_n` API front ends drive.
- `src/gfx.rs` — graphics state, paths, tiny-skia rasterization.
- `src/window.rs` — live winit window stepping the interpreter.
- `src/ops/` — operator implementations, grouped like the PLRM's operator
  summary (`stack.rs`, `arith.rs`, `graphics.rs`, `misc.rs`; new groups
  get new modules).
- `src/main.rs` — CLI: window / `--headless` / `--png` / `-e` eval / REPL
  modes, error reporting.
- `tests/eval.rs` — end-to-end tests (PostScript source in, operand-stack
  contents out, compared via `==`-style reprs).
- `tests/render.rs` — headless pixel tests (source in, canvas pixels out).
- `examples/*.ps` — sample programs used for manual testing (the Stage 2/3
  demo targets).
- `gallery/` — generative-art PostScript programs with rendered stills in
  `gallery/renders/` and a slideshow script (`show.sh`); see
  `gallery/README.md`. Art files stay within the interpreter's current
  operator set — they double as its most demanding integration tests.

## Using pscat as a tool (not working on it)

If you're an agent that just needs to *run* PostScript — render a
file, debug one, generate a handwritten note — read
`.claude/skills/pscat/SKILL.md`: it has the command recipes and
pitfalls in one page, regardless of which agent runtime you are.
MCP-wired agents can use `pscat-mcp` instead (`docs/guides/agents.md`).

<!-- sdlcify:managed:start -->
## SDLC

`SDLC.md` is the authoritative record of this repo's development
lifecycle and the branch-protection/merge policy currently enforced on
GitHub — generated and kept in sync by the `sdlcify` skill
(`.claude/skills/sdlcify/`). Day-to-day issue execution is handled by
`.claude/skills/work-issue/`.
<!-- sdlcify:managed:end -->
