# AGENTS.md

Operating conventions for any AI coding agent working in this repo. Start
with `docs/dev/HANDOFF.md` for where the project stands and what to work on;
the original vision and staged milestones are history, kept in
`docs/archive/INIT.md`. This file covers *how* to work, not *what* to build.

## Before you start

- Read `docs/dev/HANDOFF.md`, then `docs/dev/ARCHITECTURE.md`; the other
  contributor docs are indexed in `docs/dev/README.md`.
- Read `docs/archive/INIT.md` for the project's original vision and
  priorities. Every stage in it is done, so use it for intent, not as a to-do
  list.
- Per-issue summaries of past work are in `notes/` (one file per issue); the
  summaries from before `notes/` existed are in the frozen
  `docs/archive/NOTES.md`. Check them before re-deciding a settled question.
- The staged roadmap (with the model tier suited to each task and an
  escalation rule for tasks that touch the object model or execution machine)
  is `docs/archive/ROADMAP.md`. It is complete; the guidance still applies to
  how to scope new work, and a reason for departing from it belongs in a
  `notes/` fragment.

## Toolchain & commands

- Language: Rust, stable toolchain unless a specific need arises to pin
  otherwise (explain why if you do).
- `cargo build` and `cargo test` should pass before considering any stage
  or sub-step complete.
- Run the suite as `cargo test --profile ci`, the same command CI runs:
  optimized (the tests run PostScript through the interpreter, ~14x
  slower in a debug build) but with `debug-assertions` and
  `overflow-checks` on (`[profile.ci]` in `Cargo.toml`; output in
  `target/ci/`). Plain `cargo test` still works, just slowly: the absolute
  CPU budgets and the default child deadline stretch in a dev-profile build
  (see "Timing assertions in `tests/`" below), so it passes too.
- `cargo clippy` should be clean (or warnings explicitly justified in a
  comment) before considering a stage complete.
- `cargo fmt` before committing.

## Workflow

- Work from issues (see `SDLC.md`). If you resequence work the issues or
  `docs/dev/HANDOFF.md` lay out, say so explicitly rather than silently
  reordering.
- At the end of each stage: summarize what was built, what tradeoffs were
  made, and what was explicitly deferred. Commit with a message that
  reflects the stage, not just the last edit made.
- Commit at reasonable checkpoints within a stage too — don't hold an
  entire stage's worth of work in one giant commit.
- Make judgment calls on genuine coin-flip decisions and note them; don't
  stall waiting for input on things you're equipped to decide. Do flag
  decisions that are hard to reverse later.
- Goals, in order: correctness and output quality first, then finishing the
  ticket. pscat performance is a guard, not a goal; Ghostscript is a
  correctness reference only, never timed. A render is judged against the
  issue's perceptual acceptance check and `docs/dev/TASTE.md`, never traded
  for a metric.
- Don't file new issues while working one. List proposed follow-ups, each
  with its user-visible impact, in your final report; the dispatcher files
  the ones that matter and puts polish on the parked parent epic
  (`docs/dev/DISPATCH.md`). Never file anything that assigns physical work
  to the owner.

## Software development lifecycle

Nontrivial work (a new stage, a feature, a fix worth more than a
one-line diff) goes through issue → feature branch → PR → review →
merge → cleanup, not straight onto `main`. The full lifecycle, and the
current merge-authority policy (who/what may merge once CI is green
and review is clean), is `SDLC.md` — read that, not this section, for
the authoritative process; `.claude/skills/work-issue/` automates it
end to end. A PR that touches a path in `.github/CODEOWNERS` is merged by the
owner, never by an agent (`SDLC.md`, "Who merges"; check with
`python3 scripts/codeowners_check.py --base origin/main`). The one exception is
Dependabot PRs, which agents may merge once CI is green (same section). A
visible render change is merged by the agent after an independent perceptual
review, and logged for the owner's read before the release (same section,
tier (b)). This applies going forward; it doesn't retroactively
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
  tests where `docs/archive/INIT.md`'s stages call for demonstrable rendering output.
- **Test scratch files come only from `common::scratch_dir(tag)`**
  (`tests/common/mod.rs`, `mod common;`), never from a bare temp-dir call or
  a fixed `/tmp` name. Agents run `cargo test` in several worktrees at once
  and a shared name gets clobbered by the other run. The helper returns a new
  directory unique per call and per process; put a file under it with
  `.join(name)`. `tests/scratch_paths.rs` fails on any other route
  (`notes/487-unique-test-scratch-paths.md`).
- **A test that runs `pscat` or `pscat-mcp` never waits on it without a
  deadline** (`notes/841-test-child-deadline.md`). A child that does not exit
  (a limit that regressed, a test written before its feature) would block the
  suite forever, and an interrupted runner leaves the orphan spinning (one ran
  for 2.8 days). In `tests/common/mod.rs`: `cmd.output_deadline()` /
  `cmd.status_deadline()` replace `output()` / `status()` (`use
  common::CommandDeadline as _;`), `cmd.output_with_input(bytes)` feeds stdin,
  `common::wait_output(child)` replaces `wait_with_output()`, and
  `common::Watchdog::new(&child, ..)` guards an interactive child (the MCP
  `Server`). Each has a `*_within(duration)` form for a per-call deadline. The
  default is 60 s (`PSCAT_TEST_DEADLINE_SECS` overrides it); a hang kills the
  child and its descendants and fails the test with the command in the message.
  `tests/child_deadline.rs` proves it and scans for raw calls.
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
  - **Build profile** (#886). The absolute budgets are written for the
    optimised `ci` profile. A plain dev-profile `cargo test` runs the same
    PostScript 11-18x slower (35x for one test under load), so `within_cpu_budget` multiplies its budget
    by `UNOPTIMISED_BUDGET_FACTOR` (20) there, and
    `default_deadline()` (the child-process deadline, 60 s) by 3, unless
    `PSCAT_TEST_DEADLINE_SECS` sets it. The render still runs and the
    assertion still catches a catastrophic regression; it just does not
    assert an optimised-build figure on an unoptimised one. The build is
    "unoptimised" only when the test executable sits in `target/debug/`
    (`common::unoptimised_build()`; `cfg!(debug_assertions)` cannot tell,
    since `[profile.ci]` keeps debug assertions on). `--profile ci`,
    `--release`, a custom profile, or an unreadable path keep the budgets
    exactly as written. A bound relative to a same-run baseline is
    profile-neutral and uses `scale_budget` only. Use
    `within_cpu_budget` for any new absolute budget, never a hand-rolled
    `Duration` compare.
  Do not use `Instant` in a test. Negative check for a new budget test:
  `PSCAT_TEST_BUDGET_SCALE=0.001 cargo test --profile ci --test <file>
  <name>` scales every budget in the run and must make it fail (apply
  `scale_budget` to a hand-rolled bound).
- A change to `lib/*.ps` that adds or tightens a *validation guard*
  belongs in a `%%SelfTest` block in that file, next to the guard —
  `./scripts/selftest.sh` runs them and `docs/dev/SELFTEST.md` has the
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

- `docs/dev/` — contributor docs, indexed in `docs/dev/README.md`:
  `HANDOFF.md` (read first), `VM.md`, `FONTS.md`, `CAPABILITIES.md`, the
  selftest and library-coupling notes, and `design/` (the per-library design
  records and their evidence images).
- `docs/dev/ARCHITECTURE.md` — the design writeup (object model, execution model,
  rendering-crate leanings, deliberate deferrals). Read before changing
  anything structural.
- `docs/guides/` — user guides, one per library or topic (the old README's
  long tour); `docs/archive/` — history kept as written, including the
  stage log. Check links with `python3 scripts/check_md_links.py` (CI runs it
  as the `links` workflow); it skips `docs/archive/` and `notes/`.
- `docs/archive/NOTES.md` — per-stage summaries, newest first, frozen as of 2026-10-13.
- `notes/` — newer entries, one file per issue (`notes/<issue>-<slug>.md`,
  format in `notes/README.md`). Write new notes here, not in `docs/archive/NOTES.md`, so
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
  **`gallery/manifest.json` is the one list of what the gallery shows.** A new
  piece adds an entry there (title, category, libraries, blurb, alt text, page
  size, speed) plus its `.ps` and `renders/*.png`, then runs
  `python3 scripts/gallery.py sync` and `check`. The site's `gallery.html` and
  per-piece pages, the README highlights, the `gallery/README.md` table and
  `show.sh` are generated from it: **nobody edits `gallery.html` by hand**.
  `cargo test` runs the check, so a `gallery/*.ps` with no entry fails CI.

## Using pscat as a tool (not working on it)

If you're an agent that just needs to *run* PostScript — render a
file, debug one, generate a handwriting-style note — read
`plugin/skills/pscat/SKILL.md`: it has the common-path command recipes and
pitfalls (advanced sections in its `references/`), regardless of which agent runtime you are.
MCP-wired agents can use `pscat-mcp` instead (`docs/guides/agents.md`).

For reusable higher-order artwork or explicit procedure instrumentation, read
[`plugin/skills/functional-postscript/SKILL.md`](plugin/skills/functional-postscript/SKILL.md).
The experimental library's contracts, rendered demo and maintenance reference
are indexed in [`docs/functional-postscript/`](docs/functional-postscript/README.md).

<!-- sdlcify:managed:start -->
## SDLC

`SDLC.md` is the authoritative record of this repo's development
lifecycle and the branch-protection/merge policy currently enforced on
GitHub — generated and kept in sync by the `sdlcify` skill
(`.claude/skills/sdlcify/`). Day-to-day issue execution is handled by
`.claude/skills/work-issue/`.
<!-- sdlcify:managed:end -->
