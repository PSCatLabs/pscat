# Pitfalls

Read by `SKILL.md` before step 1 (skim) and whenever a step says so.

Step numbers ("step 1" ... "step 9") refer to the numbered steps in `../SKILL.md`, which keep their original numbering. "Pitfalls" / "the ... pitfall below" means `pitfalls.md` in this directory; "Claim the issue" means `claim-protocol.md`.

<!-- TOC -->
This file is long enough that one `cat` truncates. Find a section with `grep -n '^#' .claude/skills/work-issue/references/pitfalls.md` and read it
with the Read tool (`offset`/`limit`). The list below is by section name, not line number, so it does not
drift when the file is edited; keep every heading's text unchanged, since `SKILL.md` and the other
references cite sections by their quoted name. The first bullets, before the first heading, are the flat list
of general pitfalls.

- [Added in #194 (friction from the #189 / PR #193 run, and #151)](#added-in-194-friction-from-the-189--pr-193-run-and-151)
- [Waiting and timeouts](#waiting-and-timeouts)
- [Step 5 checklist](#step-5-checklist)
- [CI wait (#199, from the #178 / PR #198 run; timings and pass/fail rule updated in #208)](#ci-wait-199-from-the-178--pr-198-run-timings-and-passfail-rule-updated-in-208)
- [Main CI after merge (#257, #273)](#main-ci-after-merge-257-273)
- [Pending-CI hand-back (only if you truly cannot wait)](#pending-ci-hand-back-only-if-you-truly-cannot-wait)
- [Resume paths (#208)](#resume-paths-208)
- [Stop without merging when the dispatcher says so (#208)](#stop-without-merging-when-the-dispatcher-says-so-208)
- [Rebase and merge rules (#208)](#rebase-and-merge-rules-208)
- [Step 4 is a hard gate](#step-4-is-a-hard-gate)
- [One scratch location](#one-scratch-location)
- [Evidence runs and scratch hygiene (#273)](#evidence-runs-and-scratch-hygiene-273)
- [Added in #303 (round 7: friction from the #110, #271, #279, #281, #285, #293 runs)](#added-in-303-round-7-friction-from-the-110-271-279-281-285-293-runs)
- [Prototypes and timing (#331, from the #316 and #329 runs)](#prototypes-and-timing-331-from-the-316-and-329-runs)
- [Papercuts (#674, from the dispatcher retrospective)](#papercuts-674-from-the-dispatcher-retrospective)
- [Render DPIs](#render-dpis)
<!-- /TOC -->

- **Don't work in the main checkout.** The whole point of the worktree
  is that the branch's changes are physically separate from wherever
  this skill was invoked from. If you catch yourself editing files
  outside `$WORKTREE_DIR`, stop.
- **Shell cwd can reset to the invoking directory between tool calls**
  (confirmed in the first real run of this skill — it happened twice
  in one session). Don't rely on an earlier `cd` into `$WORKTREE_DIR`
  still holding; either prefix every shell command with `cd
  "$WORKTREE_DIR" &&` or use absolute paths throughout, for cargo
  invocations *and* for any PostScript that does `(lib/artkit.ps) run`
  or similar relative-path loads. A cwd reset there fails silently —
  it just quietly loads `main`'s copy of the file instead of the
  worktree's — which is a much worse failure mode than an error.
- **An issue with no clear implementer scope is normal, not a bug.**
  This repo's issue-writing convention deliberately omits
  implementation detail (see step 4) — that's not a sign the issue is
  underspecified.
- **Both advisor calls (steps 4 and 6) need something written down
  first.** `advisor` reviews the session transcript, not your
  intentions — if the plan or the summary of changes never got put
  into words, there's nothing substantive for it to react to.
- **Label creation is idempotent by construction** (`|| true`) — don't
  let "label already exists" abort the run.
- **If `cargo fmt --all -- --check` or clippy fails**, fix it before
  opening the PR — don't open a PR you know CI will reject.
- **`BEHIND` is not a blocker and not a reason to `update-branch`** (#257).
  `main` no longer requires branches to be up to date (`require_up_to_date:
  false` in SDLC.md, `strict: false` on GitHub), so a PR behind `main`
  merges on its last green CI run. The old update-and-rerun loop cost one
  full CI run per PR when several agents merged in parallel. The safety
  net is "Main CI after merge" below. Only `DIRTY` (real conflicts) needs a
  branch update, done by merging locally ("Rebase and merge rules").
- **Never `git stash` in a worktree.** `refs/stash` is shared by every
  worktree of the repo, so a `git stash pop` can apply (and drop) another
  session's stash. To compare with `main` use `git show origin/main:path`;
  to keep a copy use `cp`.
- **Remove the worktree before deleting the local branch, not after**
  (step 9's order matters) — `git branch -d` fails with "branch used
  by worktree" if the worktree still references it. `git worktree
  remove` first, then the branch delete, then `fetch --prune` to clear
  the now-dead remote-tracking ref.
- **A Codex review at step 8 legitimately takes several minutes** — it
  reads around the diff (docs/archive/NOTES.md, docs/dev/HANDOFF.md, prior commits) rather
  than just pattern-matching the patch. Don't mistake a long-running
  `--wait` call for a hang. If the harness auto-backgrounds the Bash
  call itself (it may, past its own timeout), don't treat that as
  automatically fine. #57 confirmed the failure only for explicit
  `run_in_background: true` paired with a same-turn `ScheduleWakeup` —
  that pairing produced one round with a 0-byte review file behind a
  "completed" notification, and a separate round that terminated
  outright (root cause not confirmed, only the correlation).
  Harness-imposed backgrounding paired with a same-turn
  `ScheduleWakeup` was never actually exercised in #57, so treat
  avoiding that combination as precautionary, not confirmed-dangerous
  — but avoid it anyway, since the plausible failure mechanism (a
  background job racing a same-turn poll trigger) doesn't obviously
  care which side initiated the backgrounding. The job is tracked
  independently by `codex-companion.mjs`, so recovering from it
  doesn't require re-running the review — poll `status --all --json
  --cwd "$WORKTREE_DIR"` or `result <job-id> --json --cwd
  "$WORKTREE_DIR"` in a *later* turn (triggered by the task's own
  completion notification or a `Monitor`, never by a `ScheduleWakeup`
  called in the same turn that launched it — see the `ScheduleWakeup`
  pitfall below for why that pairing specifically is the danger, not
  backgrounding alone). **The explicit `--cwd "$WORKTREE_DIR"` is
  required, not optional** — jobs are stored keyed by workspace root
  (`git rev-parse`-derived), and cwd can reset to the original
  checkout between tool calls (see the cwd-reset pitfall below); a
  poll that lands back in the main checkout without `--cwd` looks up
  the *wrong* workspace's jobs, can falsely report a still-running
  review as gone, and trigger exactly the duplicate-review scenario
  this hazard-avoidance guidance exists to prevent. `CODEX_SCRIPT` and
  `WORKTREE_DIR` are themselves per-call shell locals (this skill's
  fresh-shell-per-call rule, above) — re-derive both inline in
  whatever command actually does the polling, the same way `$LOCKDIR`
  and `$BRANCH` are re-derived elsewhere in this file, rather than
  assuming either is still set. If the poll comes back and the
  job is simply gone or the file never materializes, treat it as a
  failed round and re-run the review rather than trusting a partial
  result. **Refresh the heartbeat at each poll**, not just once before
  kicking the review off — this is the
  one operation in the whole flow that can genuinely run past the
  45-minute staleness threshold in a single blocking stretch, and a
  pre-refresh alone doesn't cover that. Other single blocking calls in
  this skill (a `cargo build`/`test` cycle, an `advisor` call) aren't
  separately covered this way — they're not backgrounded/polled, so
  there's no natural point to refresh mid-call; in practice they run
  well under the threshold for this repo's size, but that's an
  observation, not a guarantee, if the flow is reused somewhere the
  build is much slower.
- **`status --all --json` can report a job as `"running"` long after
  the underlying `codex` process has actually died.** Confirmed while
  building this step: the process exited cleanly (macOS's unified log
  showed a normal exit-handler sequence, not a crash) mid-review, but
  the job tracker kept reporting `running` with a stale `updatedAt`
  for 10+ minutes until manually `cancel`led — nothing timed it out on
  its own. If a `--wait` call seems stuck, don't just trust `status`;
  cross-check the reported `pid` is actually alive (`kill -0 <pid>`)
  before waiting longer, and `cancel` + fall back if it isn't. Also
  note `status --all --json`'s own output is occasionally truncated/
  malformed mid-write (a transient parse failure, not evidence of
  anything) — retry the status call once before treating a parse error
  as a signal.
- **Test the diff scope on something small before trusting it on a
  real PR.** `--scope working-tree` against a directory full of
  unrelated large/binary files (confirmed while building this step —
  a pile of untracked art assets) made a review stall for 6+ minutes
  reading files that had nothing to do with the change; `--scope
  branch --base origin/main` against just the PR's actual commits is
  the intended shape and stayed on-task.
- **`ScheduleWakeup` is for `/loop`, not for polling a backgrounded
  Codex review job.** Observed misuse: once self-corrected mid-run
  (wasted a cycle), once a stale fallback wakeup fired after the PR
  was already merged and the worktree cleaned up. To wait on a
  `codex-companion.mjs` review job, poll it directly (`status --all
  --json` / `result <job-id> --json`, per the pitfall above) or via
  `Monitor`/background-task notifications — not `ScheduleWakeup`.
  **Specifically, never call `ScheduleWakeup` in the same turn that
  launches the review with `run_in_background: true`** (#57): on two
  separate occasions this exact pairing — never a plain foreground
  `--wait` call — was followed by the review process either vanishing
  (task notification said `completed`, but `<SCRATCH>/codex-review-<N>.json`
  was a 0-byte file and `codex-companion.mjs status --all --json`
  showed no record of the job at all) or being actively terminated
  (task notification said `killed`). Not root-caused against the
  harness — flagging the correlation, not a mechanism — but the fix in
  practice is simple: don't background this call on purpose, and don't
  pair it with a same-turn `ScheduleWakeup` if the harness backgrounds
  it on its own.
- **The lock's `mkdir` and its first heartbeat write must be one `&&`
  chain, never split** (step 1's **Claim the issue**) — splitting them
  reopens the exact TOCTOU race #35 exists to close: a second
  invocation's `mkdir` fails, it reads the not-yet-written heartbeat
  as empty/stale, and steals from a run that already believes it won.
  Same reasoning applies to the reclaim path's `mkdir gen.<K+1>` +
  heartbeat write (#297: the newest generation reads live for 10 s).
- **Claim and heartbeat are scripts since #276 (round 6).** Two agents in a row ran only
  the `mkdir` branch of the pasted claim block. Run `scripts/claim.sh <N>` as one command and
  never retype its lines; `scripts/heartbeat.sh <N>` is the refresh. After editing
  `claim.sh`, run `bash .claude/skills/work-issue/scripts/test-claim.sh` (race tests in a
  throwaway repo). A worktree cut before #276 has no `scripts/`: use the main checkout's
  copy of the script, or merge `origin/main` into the worktree.
- **The lock lives in the main repo's shared git dir
  (`work-issue-lock-<N>`), not the worktree's** — this is deliberate
  (a fresh pick has no worktree yet to hang a lock off of), but it
  means `git worktree remove` at step 9 no longer cleans it up as a
  side effect the way the old worktree-gitdir heartbeat file did.
  Step 9's explicit `rm -rf` is load-bearing, not decorative.
- **A Codex failure is not automatically "Codex unavailable" — check for
  a stale shared broker first** (#160, seen on #147). Round 1 of a review
  worked; minutes later `review` and `task` both failed instantly with
  `failed to load configuration: No such file or directory (os error 2)`
  and `node "$CODEX_SCRIPT" setup --json` reported `ready: false`,
  `auth.detail: "failed to load workspace requirements"` with
  `sessionRuntime.mode: "shared"` pointing at one `cxc-*/broker.sock`.
  The `codex login` was fine and `codex --version` worked; the failing
  runs were launched from a linked worktree. From the main checkout the
  same `setup --json` said `ready: true` and a `task "reply pong"`
  succeeded on a *new* broker. Observed cause: the session's broker
  process (pid in its `broker.pid`) was still alive but had no
  `codex app-server` child; the exact trigger is unproven. Before
  falling back to the same-family `Agent` review: (1) run `setup --json`
  and read `auth.detail` / `sessionRuntime.endpoint`; (2) smoke-test
  with `node "$CODEX_SCRIPT" task --fresh "Reply with exactly: pong. Make no edits."`
  from the main checkout (`git rev-parse --show-toplevel` of the main
  worktree). That smoke test only *diagnoses* the runtime — **never run
  the real review from the main checkout**: `--scope branch` reviews
  whatever is checked out there, and it would return a confident clean
  verdict for the wrong diff. The real review always runs from
  `$WORKTREE_DIR` with step 8's branch assertion. If the smoke test
  passes but the worktree run still fails, kill only *this session's*
  stale broker (`ps -p "$(cat <endpoint dir>/broker.pid)"`; other
  issues' brokers belong to other sessions), then re-run the step-8
  command from `$WORKTREE_DIR` and confirm the review's stated target is
  `$BRANCH`; (3) only if that still fails, fall back and put the error
  text in the review comment.
  Also note `task` is where an *opinion* question ("which of these two
  designs reads better?") goes — it works as well as `review` once the
  runtime is healthy.

## Added in #194 (friction from the #189 / PR #193 run, and #151)

- **`git worktree add -b <branch> <path> origin/main` makes the new
  branch track `origin/main`**, so a bare `git push` (no refspec) targets
  `main` — branch protection rejects it, but it is a confusing failure
  and, with admin bypass on, not guaranteed to be rejected. Step 3 now
  passes `--no-track`, and step 7 always pushes with the explicit
  refspec `git push -u origin "$BRANCH"`, which sets the correct
  upstream. Never run a bare `git push` before step 7 has set it.
- **After a squash merge, `git branch -d "$BRANCH"` always fails** —
  the squash commit on `main` is a new commit, so the branch's own
  commits are never ancestors of `main` and git refuses the "safe"
  delete. Step 9 uses `-D`, but only after `gh pr view <PR#> --json
  state --jq .state` returned `MERGED` (step 9's first line aborts the block otherwise); that check is what makes `-D`
  safe, so never run `-D` without it.
- **The full quality gate (`PSCAT_PROFILE=ci ./scripts/selftest.sh &&
  cargo test --profile ci && clippy -D warnings && fmt`, the #204 shape,
  identical to what CI runs) can exceed the Bash tool's 120s default
  foreground timeout.** Run it
  with `run_in_background: true`, redirecting output to a log file, and
  wait for the background task's completion notification (or a
  `Monitor` on the log file — `Monitor` is a deferred tool: load it with
  `ToolSearch` query `select:Monitor` first). Do not use foreground
  `sleep` (blocked) and do not use `ScheduleWakeup`. This applies to the
  cargo gate only: the step-8 Codex review is the opposite case and must
  stay a foreground blocking call (see the `ScheduleWakeup` pitfalls
  above).
- **The Edit tool refuses to edit a file you have not Read in this
  session.** Read the file (at its absolute path inside
  `$WORKTREE_DIR`) before the first Edit to it. Write also needs a prior
  Read when overwriting an existing file.
- **`in-review` used to stay on closed issues** (#151). Step 9 now
  removes it (and `in-progress`, in case it lingered) once the merge is
  verified. `scripts/issue_summary.sh` only reads labels from *open*
  issues, so this changes nothing in that dashboard.
- **The main checkout does not pick up a merged change to this skill
  until it is pulled.** Skills load from the checkout the session runs
  in; merging updates `origin/main` only.
- **Quote globs in shell examples** (`grep -r --include='*.md' ...`).
  zsh, this repo's shell, aborts an unquoted `*.md` that matches nothing
  with "no matches found" (#208).
- **`gh pr merge --delete-branch` always printed a harmless local-branch
  error** while the branch was checked out in a worktree (it tries to
  switch the main checkout and delete the local branch). The repo's
  `delete_branch_on_merge` setting already deletes the remote branch, so
  step 8.13 no longer passes the flag and step 9 deletes the local
  branch after removing the worktree (#208).

## Waiting and timeouts

- macOS has no `timeout`. Use `perl -e 'alarm N; exec @ARGV' cmd args`.
- **Long foreground commands (#303).** A foreground Bash call is capped at 600 s (`timeout`
  600000) and is moved to the background past ~120 s by default. A gallery-piece timing loop,
  an `identity_check.sh` over a big corpus, a sweep or a stress loop must run with
  `run_in_background: true`, writing to a `<SCRATCH>` log, so the cap never kills it halfway
  and leaves a half-written log.
- Foreground `sleep` may be blocked. Wait with a background Bash task
  (`run_in_background: true`, you are notified when it exits) or a Python
  poll, `python3 -c 'import time; time.sleep(60)'`, kept under 115 s per call.
- A bounded single-call wait on a log file is fine: a Python poll such as
  `until grep -q GATE_EXIT <log>; do python3 -c 'import time; time.sleep(5)'; done`, kept
  under 115 s per call (the call is moved to the background past that, and you are notified).
- `Monitor` is a deferred tool: load its schema first with `ToolSearch` (`select:Monitor`),
  or calling it fails.
- Keep scratch output under `<SCRATCH>` as `<kind>-<N>.*`, never a relative
  path that escapes the worktree (`../x`) and never `/tmp` when the system
  prompt names a scratchpad.

## Step 5 checklist

Cheapest first, before the long gate:

1. `cargo fmt --all -- --check` (seconds) before everything else.
2. A new library's first `%%SelfTest` block needs a line in
   `selftest/libraries.txt`. A new top-level artkit name needs an entry in
   `ARTKIT_INTERNAL` in `src/capabilities.rs`. Check with
   `PSCAT_PROFILE=ci ./scripts/selftest.sh` and
   `cargo test --profile ci --test capabilities` (about 30 s together).
3. In a `lib/` file that opts into the `% @kind` catalog, `build.rs` (`find_top_level_defs`)
   only understands `/key value def` at depth 0 where `value` is ONE object (a number, a
   name, a string, or a balanced `{}`/`[]`/`<< >>` group). It panics on `/name N dict def`
   ("`def` with no `/name` two positions back on the depth-0 stack"), which fails every build,
   not just your test (untagged `LEGACY_FILES` such as `lib/etching.ps` still use that
   shape). Use a `<< >>` literal (`/name << /k v >> def`), or a probe-first def whose `def`
   sits at depth 1, as `lib/lettering.ps` and `lib/drykit.ps` do with `where`:
   `/name where { pop } { /name N dict def } ifelse`. Run `cargo build` before the long gate to
   see a panic in seconds.
4. Any `lib/` change that must render byte-identically (a paintkit slice, but equally an artkit,
   hatchkit or other library change, or a refactor): `scripts/identity_check.sh BASE_WORKTREE
   --control FILE` (see its header: one shared binary, one negative control). Default files: those
   that load `lib/paintkit.ps`. For any other lib add `--loader lib/<name>.ps` (repeatable; it also
   picks files that reach the lib through another lib, so `lib/artkit.ps` covers about 63 files in
   ~70 s) or `--files LIST` (tracked paths, one per line). Multi-page output (`page-001.png`, ...)
   is compared page by page. Order matters: rebase LAST (step 5.5), then create the base worktree
   from the rebased branch's base, `git worktree add --detach <path> "$(git -C "$WORKTREE_DIR"
   merge-base HEAD origin/main)"`, then run the check. Pick `<path>` outside the repo and NOT ending
   `-issue-<N>` (step 3 resumes on that suffix); write the exact path down for step 9. A rebase after
   the check leaves a stale base: re-create the base and re-run (matrix in "Rebase and merge
   rules"). The control driver may live in `<SCRATCH>`: pass it by absolute path. A pure
   refactor has no new key to use as the control: append a marker (`printf '\n/probe true def\n' >>
   BASE/lib/<name>.ps`, uncommitted, in the base) and let the control draw by `/probe where`. That
   proves each tree loads its own lib; keep the marker out of the branch. The corpus can take
   minutes at gallery resolution: run the check with `run_in_background: true` and a log
   (pitfalls "Long foreground commands"). A `SKIPPED` line (and the closing WARNING) is a file that
   fails identically in both trees (#504: `tw_proto.ps` needs an untracked `twglyphs.ps`): not a
   difference, so do not chase it; an `ERROR` still means a failure that differs between the trees.
   **A change under `src/` (interpreter, operators, rasteriser) is the standard case for the two-binary
   form (#335):** `scripts/identity_check.sh BASE_WORKTREE --base-bin BASE_BIN --head-bin HEAD_BIN
   --control FILE --loader lib/artkit.ps` (or `--files LIST`). The base tree renders with the base
   binary, the head tree with the head binary, so no wrapper that picks a binary by cwd is needed.
   Build the base binary FROM the base worktree, in the background (a cold release build takes
   minutes): `scripts/build_base_bin.sh BASE_WORKTREE "<SCRATCH>/base-pscat-<N>"`; it copies the
   binary out because a shared target dir would let the head build overwrite it. The head binary is
   `cargo build --release --bin pscat` in `$WORKTREE_DIR`. Give both flags or neither, and not with
   a positional BIN. The control must use behaviour only the head binary has (for an operator fix,
   the call that used to raise the error): it must render differently, which proves each tree ran
   its own binary; exit 2 otherwise. Use a lib corpus wide enough to reach the operator
   (`--loader lib/artkit.ps`, about 63 files), since the default paintkit pick is narrow. If the
   change touches both `src/` and `lib/`, the base tree is still the merge-base worktree. List the
   base worktree for step 9's `EXTRA_WORKTREES` (the binary in `<SCRATCH>` dies with the session).
   **An intended visible change (#674):** `--expect-diff LIST` (tracked paths, one per line) lets the listed
   files differ (`EXPECTED` lines) and fails if one of them comes out identical (`UNCHANGED`: the change did
   not take effect) or is not in the compared set (exit 64). Every other file must still be identical. A run
   that needed it is a tier (b) visible change (perceptual-review.md); without it, identical means tier (a).
   Timing guard (not a goal): `scripts/bench_compare.sh BASE_BIN HEAD_BIN FILE [N]` prints CPU
   min/median for base, head and an identical-binary control. It is fine to stop a running bench
   and report a partial sample when the coordinator narrows the scope, or just use a small N.
5. Every pscat probe you run by hand needs `--headless`, or `-e`/`--png`. Without one of
   them pscat opens a window and the call blocks until it is closed:
   `pscat --headless -e '3 4 add ='`, `pscat --png <SCRATCH>/probe-<N>.png file.ps`.
6. Then the full gate in step 5.5.
7. **Light path for a docs- or asset-only diff (#674).** If every changed path is in SDLC.md's
   `light_gate.paths` and none in `light_gate.exclude` (check: `git -C "$WORKTREE_DIR" diff --name-only origin/main...HEAD | grep -Ev '\.(md|png|jpg)$|^notes/' ; git -C "$WORKTREE_DIR" diff --name-only origin/main...HEAD | grep '^docs/guides/'`
   both print nothing), run `light_gate.command` from `SDLC.md` instead of the full gate, and write `quality gate light`
   in the report. CI still runs everything. `docs/guides/` is excluded because tests run its samples
   (`tests/landscape_cookbook.rs`, `tests/headline.rs`). A skill script, a workflow or any code file is not on the list.

## CI wait (#199, from the #178 / PR #198 run; timings and pass/fail rule updated in #208)

Step 8.12's CI wait is the one place a run is most tempted to hand back
early. The first Haiku run of the slimmed skill did: it stopped with
`test` pending, and CI went green six seconds later. Since #204 the
`test` job takes **about 2-3 minutes** (`perf-regression` about 1; with
runner queueing the whole wait is 2-6 minutes; it was 11-17 minutes
before #204). That is still longer than a foreground Bash timeout, so the
wait that works is a background Bash task whose completion wakes you:

```sh
gh pr checks <PR#> --watch --fail-fast > <SCRATCH>/ci-<N>.log 2>&1; echo "exit=$?" >> <SCRATCH>/ci-<N>.log
```

Run it with `run_in_background: true`, heartbeat first and after (and
again after any push, which restarts CI).

- **Do not sleep-poll** (foreground `sleep` is blocked and a poll loop
  burns turns) and **do not pipe through `tail`**: the pipeline's exit
  status becomes `tail`'s, which hides a failed check (#194's run hit this).
  The redirect plus the trailing `exit=` line keeps the watch's own status.
- **Merge state settles late (#303).** Right after a watch ends, and again just before `gh pr
  merge`, the eligibility query can briefly print `false` (or `mergeStateStatus` read `UNKNOWN`)
  although every check is green. Re-run `git merge-tree --write-tree origin/main HEAD` and the
  query a few seconds after the watch (a 10 s Python sleep) and again immediately before the
  merge; re-query up to 3 times, ~10 s apart, before treating `false` as a real failure. Look
  at the check list (`gh pr checks`) to tell a settling `false` from a red check.
- **2-6 minutes is normal.** A task that has run 10 minutes is worth a
  look (`gh pr checks <PR#>`), but it is not necessarily hung.
- **The `exit=` line proves nothing about pass/fail** (#208). It was `0`
  on a run where both jobs were cancelled ("job was not acquired by
  Runner of type hosted"). Decide from GitHub's own check state, and merge
  only when SKILL.md 8.12's query prints `true` (the one copy; it also
  requires `mergeable == "MERGEABLE"` and `mergeStateStatus != "DIRTY"`).
  Its `length > 0` guard matters: jq's `all` is `true` on an empty
  array, and the rollup is empty for a few seconds after a push. A
  `CANCELLED`, `SKIPPED`, `NEUTRAL` or still-`IN_PROGRESS` check is not
  `SUCCESS`. (Branch protection requires only `test`, but a red
  `perf-regression` is a signal to read, not to merge past.)
- **"no checks reported"**: seconds after a push
  no check runs are registered yet, so `--watch` can exit non-zero with
  that text (or the rollup is empty). It is not a CI failure. SKILL.md 8.12 retries a
  `gh pr checks` probe (up to ~1 min, keyed on the output text, never the exit code, since pending
  checks also exit non-zero) before it starts the watch; if the watch still prints it,
  re-launch the same command. After any push (a local merge for `DIRTY`
  included), start a fresh watch: the run before it covers an older head.
- **Rerun after an infrastructure failure only.** `gh run rerun <run-id>
  --failed` is allowed when the failed job never ran a step: it was
  `CANCELLED` or "not acquired by Runner" and has no `success`/`failure`
  step. Check, using the run id from the check's `detailsUrl`
  (`.../actions/runs/<run-id>/job/...`):
  `gh run view <run-id> --json headSha,jobs --jq '{headSha, jobs: [.jobs[] | {name, conclusion, ran: ([.steps[]? | select(.conclusion=="success" or .conclusion=="failure")] | length)}]}'`
  Rerun only when `headSha` is the PR's current head (an older run
  cancelled by a newer push to the same PR is superseded, not broken: `ci.yml`'s
  `concurrency` cancels PR runs on purpose; push runs on `main` are never cancelled) and every failed job shows `ran: 0`. At most two
  reruns; then start a fresh watch. Any job with a `failure` step is a real
  failure (a test, clippy or fmt): stop and report, never rerun it.
- After a clean `true`, merge. `BEHIND` meanwhile is fine (see above).
- The `true` query also requires `mergeable == "MERGEABLE"` and
  `mergeStateStatus != "DIRTY"`: a conflicted PR still shows green checks
  from its last run, and right after a push GitHub reports `UNKNOWN` for both
  fields while it computes them, which `!= "DIRTY"` alone lets through (#273;
  the likeliest cause of a `true` printed for a conflicted PR in the #263
  run). `UNKNOWN` is not a failure: re-query up to 5 times, ~10 s apart
  (`python3 -c 'import time; time.sleep(10)'` between), and if it never
  resolves treat the PR as not mergeable and report it. Before the CI wait,
  `git fetch origin main && git merge-tree --write-tree origin/main HEAD
  >/dev/null` (git 2.38+; non-zero means conflicts) catches a PR that will go
  `DIRTY`, so the conflict is fixed before a CI cycle is spent on it. Its `jq` is parenthesised
  on purpose (`|` binds loosest, so `A and B | C` would parse as
  `(A and B) | C`).

### Main CI after merge (#257, #273)

With `strict` off, two PRs can each pass alone and break `main` together.
The push-to-main `ci` run for your merge commit is the net. Since #273
`ci.yml` never cancels a push run (each push has its own concurrency group
keyed by the commit SHA), so your merge commit gets its own run to
completion, even when another agent merges seconds later. After step 9's
cleanup, in a fresh Bash call, with `SHA` from `gh pr view <PR#> --json
mergeCommit --jq .mergeCommit.oid`, wait for that run (background, ~3 min
for `test`, cap about 20 min; `perf-regression` is `pull_request` only):

```sh
SHA=<SHA>; for i in $(seq 1 60); do
  R=$(gh run list --workflow ci --branch main --commit "$SHA" --json databaseId,status,conclusion --jq '.[0] // empty | "\(.databaseId) \(.status) \(.conclusion)"')
  case "$R" in *" completed "*) break;; esac
  python3 -c 'import time; time.sleep(20)'
done; echo "run=$R"
```

`R` is `<run-id> completed <conclusion>`; empty for the first few seconds after
the merge is normal. Decide from the conclusion, not any exit code: `success`
is green.

- Never rerun a main run to "get a result" (a rerun of someone else's run
  races them); a `failure` is a real signal.
- `cancelled` should not happen any more. If it does: if a newer push to
  `main` exists, look at the newer run, provided its head contains your commit
  (`git fetch origin main`, then `git merge-base --is-ancestor <SHA>
  <newer-sha>`), and repeat if that one is cancelled too (this is also the
  path for a merge made before #273's `ci.yml` change was on `main`).
- A `cancelled` run with zero steps run and no newer push is an infrastructure
  failure, not red. Check it as in "Rerun after an infrastructure failure
  only" above (`ran: 0` for every job, `headSha` is `SHA`) and rerun with
  `gh run rerun <run-id> --failed`, at most twice. This is the one case where
  a main run may be rerun; the push group is keyed by SHA, so the rerun cannot
  be cancelled by or race another merge's run.
- Red (`failure`): stop at once. Report it as the first line of the final
  report (`Merge: merged ... but main CI RED`) with the run URL and the failing
  step, and start no other work. Do not revert or push a fix unprompted.

### Pending-CI hand-back (only if you truly cannot wait)

If you must stop with CI still pending (the harness refuses a background
task, or the user cut the run short), the PR is open but not merged. Do
**not** run step 9 and do **not** release the lock. Instead:

1. Keep the `in-review` label and leave `work-issue-lock-<N>` in place.
   Its heartbeat will go stale after 45 minutes; that is intended, it is
   what lets a later invocation reclaim the issue.
2. Make the final report's `Merge:` line `pending CI`, and list the exact
   remaining steps: the PR number and branch; the worktree path; whether
   step 8.11's `## Summary of work` comment is already posted (do not post
   it twice); then `gh pr checks <PR#> --watch --fail-fast` (recipe
   above), SKILL.md 8.12's pass/fail query (not `DIRTY`, non-empty, every check
   `SUCCESS`), `gh pr merge <PR#> --squash`, step 9, and "Main CI after merge".
3. Resuming: the same session just continues from step 8.12 with no
   re-claim. A fresh `/work-issue <N>` after the lock is stale takes the
   "Resume paths" route below (issue is `in-review` with an open PR, so it
   claims, reuses the worktree and lands at step 8). A re-run inside 45
   minutes reports `live` and stops.

### Resume paths (#208)

Entered from step 1 when an explicit `<N>` is `in-review` and an open PR
claims it: a pending-CI hand-back, a no-merge hand-back (below), or a
run that died after step 7. Claim as usual, skip step 2 (re-adding
`in-progress` would make the next invocation read "someone is mid-run" and
refuse). Then restore the worktree: if `git worktree list` shows one ending
`-issue-<N>`, take step 3's reuse branch. If it is gone, **do not** take
step 3's "Otherwise" route (it cuts a new branch from `origin/main` and
would lose the PR's work, or fail on the existing branch); make one on
the PR branch: `git fetch origin "$BRANCH" && git worktree add "$WORKTREE_DIR" "$BRANCH"`
(find `$BRANCH` with `gh pr list --state open --json number,headRefName --jq '.[] | select(.headRefName | test("/<N>-"))'`).
Find the PR with `gh pr list --state open --head "$BRANCH" --json number,url`.
**Then sync the worktree with the PR head before choosing a row** (Codex
round 3 on #208: step 8 reviews local git state, so a stale local branch
would be reviewed while the newer remote head gets merged):
`git -C "$WORKTREE_DIR" fetch origin "$BRANCH"`, then compare
`git -C "$WORKTREE_DIR" rev-parse HEAD` with the PR's `headRefOid`.
Behind: `git -C "$WORKTREE_DIR" merge --ff-only "origin/$BRANCH"`. Ahead
(unpushed local commits): run the gate, `git push`, continue. Diverged:
stop and report, do not guess.
**Decide the re-entry point from GitHub state, never from scratch files**
(`<SCRATCH>` is per session; the old gate log, review JSON and summary
file are gone). First match wins:

| What GitHub shows | Re-enter at |
|---|---|
| PR `CLOSED` unmerged | Stop and report. |
| PR `MERGED` | Step 9 only, then "Main CI after merge". |
| The PR head equals the `reviewed head` in the last `## Codex review, round` / `## Fallback review, round` issue comment, every row of its table has a response, and a `## Summary of work (PR #<PR#>)` comment exists | 8.10 (re-read `merge_policy` and whether the dispatcher still says no merge), then 8.12. |
| Same, but no summary comment yet | 8.10, 8.11. |
| Anything else or ambiguous: no review comment, no recorded `reviewed head`, a finding without a response, or non-merge commits since the reviewed head | 8.3 with R+1: re-review. |

A head that only differs by clean merges of `origin/main` (no conflict
resolutions: `git show --remerge-diff --format= --stat <merge>` empty, and
`git merge-base --is-ancestor <merge>^2 origin/main` succeeds, since a clean
merge of another feature branch is unreviewed code) still counts as equal to
the reviewed head. When unsure, re-review.

The final report on a resume fills `Plan review` with `done (earlier run)`
and counts the review rounds from the issue's `## Codex review, round`
and `## Fallback review, round` comments, not from this session.

### Stop without merging when the dispatcher says so (#208)

Some dispatches say not to merge (a human who wants the last look). A render no
longer waits on the maintainer's eye: since #674 a visible change goes through the
perceptual review and merges as tier (b) (perceptual-review.md). The same stop is
tier (c)'s (an owner path): green CI, then stop, with a Decision block atop the body. Run steps 1-8.9 as
usual, then at 8.10 do step 11 (the summary is posted so the issue carries
the wrap-up), then step 12 (the `merge-tree` check, the "no checks reported" retry and the CI
watch) until 8.12's query prints `true` (#331). Hand over a PR whose CI is green: a maintainer
who finds it red or still running has to babysit it. Stop **before** 8.13, the merge:

0. Write the PR body for the maintainer from the start (step 7): what changed and why, what
   you verified and how, what you did not or could not verify, what to look at first
   (renders, a risky function), and that merging is theirs to do (`/work-issue <N>` later
   finishes it). A body drafted for a self-merge ("CI is green, merging") misleads them.
   A specimen or render PR commits its render and embeds it in the body, pinned to the head SHA
   and rendered after the final rebase (demo-surfaces.md); a body with no picture sends the
   maintainer off to run it themselves (#286).
1. Release the lock: `rm -rf "$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>"`.
2. Keep the `in-review` label, the worktree and the branch. Do not run
   step 9. If CI is red (a failed step, not infrastructure: "CI wait"), stop and report that
   instead; never hand over silently.
3. Report `Merge: awaiting human merge (dispatcher said no merge; CI green)`, or for an owner
   path `Merge: awaiting owner merge (tier c: <paths>; CI green)`, with the PR number, branch and
   worktree path.

To merge later, `/work-issue <N>` again: the issue is `in-review` with an
open PR, the lock is free (fresh claim), and the Resume table above lands
on its "review current, summary posted" row (or on re-review if commits were
added since). Step 9
then removes the worktree.

### Rebase and merge rules (#208)

Rebase onto `origin/main` exactly once, as late as possible (#303): at step 5.5,
right before the identity check and the full gate, and before the **first
push** (commit first: a rebase refuses a dirty tree), so a break already on `main` (as #189's clippy failure was) does
not fail an unrelated issue, and no check goes stale behind a later rebase. Finish the
implementation, notes and renders first; rebase only when the branch is otherwise done. After the PR exists, history is published:
never rebase it. Being `BEHIND` needs nothing (#257). Only real conflicts
(`DIRTY`) bring `main` in, by merging locally: `git -C "$WORKTREE_DIR" fetch
origin main && git -C "$WORKTREE_DIR" merge origin/main`, resolve, commit,
plain `git push` (no force). `gh pr update-branch` is only a shortcut for a
conflict-free merge; if you do use it, `git -C "$WORKTREE_DIR" pull
--ff-only` before any later local commit.

**Gate after a clean rebase or merge (#276).** If a gate already passed and the
rebase or merge then brought in only documentation, skill or CI files, that gate result
stands; do not re-run it. Decide with an allowlist, not a denylist, before the rebase:
`git -C "$WORKTREE_DIR" fetch origin main && git -C "$WORKTREE_DIR" diff --name-only HEAD...origin/main`
lists what `main` gained. If every path is `*.md`, under `docs/`, `notes/`, `.claude/` or
`.github/`, the gate stands. Anything else (`lib/`, `src/`, `tests/`, `selftest/`,
`examples/`, `gallery/`, `scripts/`, `build.rs`, `Cargo.*`, `site/`) means re-run it.
Either way, re-read the worktree's `SKILL.md` after a rebase that moved `HEAD`.

**A merge of `main` after the gate has passed (#331)** (the local merge for a `DIRTY` PR in 8.12;
8.9's re-push of review fixes always re-runs the gate) follows the same allowlist. If it brought in `src/`, `lib/` or tests, either
re-run the gate, or state in the PR comment that CI covers it: CI runs the identical four commands on
the merge result, so for a code-only merge with no conflicts the second is legitimate and saves a
local gate. Re-run locally when you resolved a conflict in code, or changed the identity-checked output.

What to redo after a rebase that pulled in changes (#303):

| the rebase pulled in | redo |
|---|---|
| only `*.md`, `docs/`, `notes/`, `.claude/`, `.github/` | nothing (re-read SKILL.md) |
| anything in `lib/`, `src/`, tests, `selftest/`, `examples/`, `gallery/`, `scripts/`, `build.rs`, `Cargo.*`, `site/` | (1) re-create the identity base from the new merge-base and remove the old one, (2) re-run `identity_check.sh`, (3) re-run the full gate, (4) re-render any committed specimen or still |

**docs/archive/NOTES.md conflicts** should be rare since #273: new notes go in
`notes/<issue>-<slug>.md`, one file per issue, so two PRs never touch the
same file. If you still conflict in `docs/archive/NOTES.md` (someone edited it, or an old
PR predates #273), keep both sides; never drop either. Move your own new
section out of `docs/archive/NOTES.md` into your `notes/` fragment rather than resolving
it in place.

**Doc-only conflicts skip the re-review.** If every conflicted file in
such a rebase or merge was plain documentation (`docs/archive/NOTES.md`, `docs/dev/HANDOFF.md`,
`README.md`, other `*.md`) and your resolution changed no other file
(check, while still conflicted: `git diff --name-only --diff-filter=U`
lists only `*.md` files), re-run the quality gate (5.5) but not
the Codex review, and say so in the PR/issue comment. Markdown that steers
agents is not documentation for this rule: anything under `.claude/` (skills
and their references, which define executable workflow), `AGENTS.md`,
`CLAUDE.md`, `SDLC.md` (Codex review, #208 round 1). Any conflict in those,
code, `lib/`, tests or scripts means the review is stale: re-review (8.3, R+1).
A merge of `main` with no conflicts at all never needs a re-review.

### Step 4 is a hard gate

The same run skipped the plan-review checkpoint without noticing. Step
4.4 now says no code before the `advisor` call, and the final report has
a `Plan review (step 4)` line that must read `done` or `skipped (small)`.
Skipping is allowed only for a **small** change (step 4.2 is the one
definition: 20 changed lines or fewer and no API change; count tests and
notes in the 20, as #293 skipped its plan review by counting only the fix),
and the report must say so; for a full-size change the call is required.

### One scratch location

All scratch files live in `<SCRATCH>` as `<kind>-<N>.*` (`gate-<N>.log`,
`ci-<N>.log`, `codex-review-<N>.json`, `issue-<N>-summary.md`).
`<SCRATCH>` is the session scratchpad directory when the system prompt
names one (the Claude Code harness tells agents to prefer it, and it needs
no permission prompts); otherwise `${TMPDIR:-/tmp}`. #205 had pinned `/tmp`
over the harness default; #208 reversed that. Write the literal path into
each command (variables do not survive between Bash calls). The `<N>` in
every name keeps two concurrent issues apart even in a shared directory.
The cost: a scratchpad is per session, so a resumed run never finds the
earlier run's files; resume decisions are made from GitHub state
("Resume paths" above), and a review JSON or summary is regenerated.

### Evidence runs and scratch hygiene (#273)

- **Run long evidence sweeps in the background, in parallel.** Old-binary
  vs new-binary comparisons across many inputs (a gallery sweep, every
  example, a perf A/B) belong in `run_in_background: true` Bash tasks,
  one per binary or per slice, writing to `<SCRATCH>` files, with a heartbeat
  before launching. Wait for the notifications; do not run them back to back
  in the foreground.
- **When a fix corrects previously wrong output, compare against ground
  truth, not the old output.** "New equals old" proves nothing, and "new
  differs from old" is expected. Compare with the raster (or gs, or the
  specification) instead; a diff against the old output only shows what
  moved.
- **Use fresh scratch subdirectories, not `rm` globs.** Make
  `<SCRATCH>/sweep-<N>-<tag>/` per run with `mkdir -p` and write there. Do
  not clear a shared scratch directory with `rm <dir>/*` or `rm -rf <dir>/*`:
  it deletes other runs' files, and an unset or wrong variable turns the glob
  into something far worse. Remove only a directory you created (`rm -rf
  "<exact dir>"`) once the run is finished.
- **Step 5.5's `rebase origin/main` is a no-op when the branch is already up
  to date.** Do not treat "Current branch is up to date" as a problem.
- **Run the gate in the foreground when the last one took under ~60 s**
  (a doc-only or tiny change); the background-and-wait routine costs more than
  it saves there. Anything longer, or unknown, stays in the background.

### Added in #303 (round 7: friction from the #110, #271, #279, #281, #285, #293 runs)

- **Never kill by a pattern.** `pkill -f release/pscat`, `killall pscat` and
  `kill $(pgrep ...)` match every agent's processes on this machine, not yours: two
  agents' pattern kills hit other agents' renders on 2026-10-13 (#259's run killed #110's).
  Start the process, record `$!` (`cmd & PID=$!`, write the number in your reply since
  shell state does not persist), and `kill "$PID"` only that. A process you did not start
  is not yours to stop, even when it is wedged. A task started with `run_in_background` has no
  `$!` in your shell: stop it with `TaskStop <task id>`, which is scoped to your own tasks.
- **`gh --jq` runs gojq, not system `jq`.** Their regex flavours and some builtins differ
  (see #293, the `scan` expression). Test every jq expression that
  will run inside `gh ... --jq` through `gh` itself (`gh pr list --json body --jq '...'`
  against real or fixture data), not through `echo ... | jq`. `.claude/skills/work-issue/scripts/test-scan.sh` is the model.
  A plain `| jq` after `gh` is system jq; know which one a line uses.
- **PostScript probe harnesses (executed names, `$error`).** In a hand-written probe, hold each
  callable you want to run later in a 1-element array (`[ { ... } ]`, run it with `0 get exec`),
  so storing and fetching it never executes it by accident; and stringify `$error /command` the
  moment you read it (`$error /command get 64 string cvs`): it is an executable name, and left
  as an object it executes (or errors) the next time it passes through the program flow. This
  is the auto-execution trap.
- **A new `xcheck`-array guard must accept `packedarraytype`** (from #271 / PR #300). Ghostscript's
  `true setpacking` turns `{ ... }` procedures into packed arrays, and `type` then says
  `packedarraytype`, not `arraytype`; a guard that tests `type /arraytype eq` rejects a valid
  procedure there. Accept both (`dup type /arraytype eq 1 index type /packedarraytype eq or`),
  and add a test that runs under a real Ghostscript with `true setpacking`, not only pscat's own
  suite (a guard that has never seen gs's packed arrays passes every pscat test).
- **zsh does not word-split `$FILES`.** A string holding several paths stays one word, so
  `for f in $FILES` and `cmd $FILES` pass one broken argument (it works in bash, which is why
  scripts pass). In an interactive Bash call use an array (`FILES=(a b c); for f in "${FILES[@]}"`),
  `while read -r` over a file or heredoc, or `bash -c '...'`. The repo's scripts have a bash
  shebang; the Bash tool's own shell is zsh.
- **"Max value fed to X across all pieces" sweeps.** Questions such as "what is the
  largest segment count `walkpath` sees across every gallery piece" go through
  `bash .claude/skills/work-issue/scripts/sweep-max.sh --glob 'gallery/*.ps' -- <SCRATCH>/probe.sh`: it runs your per-file hook (any
  command whose last stdout line is a number) over the tracked files in parallel and prints
  `MAX <value> <file>`. Write the hook once (a pscat run with a counting probe), keep it in
  `<SCRATCH>`, and run the sweep with `run_in_background: true`. `bash .claude/skills/work-issue/scripts/test-sweep.sh` tests it.
- **Extra worktrees you create are yours to remove, by exact path.** An identity base or a
  before/after tree from step 5 is cleaned in step 9 (finish.md) from the list of paths you
  wrote down, never by matching `git worktree list`. Name them so the path does not end
  `-issue-<N>` (step 3 resumes on that suffix) and keep them outside the repo.
- **A "won't change" outcome needs its own path (#287/PR #304).** When measurement shows the issue's
  change should not be made and the PR ships only comments and notes: the PR body says `Refs #<N>`
  (never `Closes`, which closes the issue as COMPLETED on merge); the plan gate counts it small; step 9
  closes the issue with `--reason "not planned"` and the `wontfix` label (finish.md). Comment the
  measurement on the issue first (step 5.4).
- **`gh pr checks --watch` logs a refresh every 10 s**, so `<SCRATCH>/ci-<N>.log` grows long. Read it
  with `tail -n 25`, not whole (the `exit=` line is last).
- **Never edit a shell script while it runs.** bash reads a script as it executes; an edit mid-run
  gave a syntax error at a later line (seen while verifying #303). Wait for the run, or copy the script.
- **Codex cannot run harnesses**; for concurrency or shell changes the advisor plus real load
  runs are the review that counts (codex-review.md).

### Prototypes and timing (#331, from the #316 and #329 runs)

- **A throwaway measurement prototype is allowed after the step 4 plan review** even though no
  production code is written before that gate. Use it to answer "is the change worth making" (a
  timing, a count, a size), never to build the change. Make it in a scratch worktree outside the
  repo whose path does not end `-issue-<N>` (step 3 resumes on that suffix), never commit or push it,
  and write the path down: it goes in step 9's `EXTRA_WORKTREES` or in finish.md's "Close without a
  PR" block. The measurement itself belongs on the issue (step 5.4); the prototype does not.
- **Timing a change** is advisory: performance is a guard, not a goal. Use it to show a `src/` change
  did not slow a render, not to tune. `scripts/bench_compare.sh BASE_BIN HEAD_BIN FILE [N]` (checklist
  item 4) already does it right: CPU time (user+sys) of the child, interleaved arms, best of N, and an
  identical-binary control whose gap to the base is the noise floor. By hand, the same recipe:
  `/usr/bin/time -l` and read user+sys, never real. Suite wall time is unreliable when other agents
  push the load average above 30: do not quote it, and do not chase a wall-time "regression" without
  re-measuring in CPU time. To see whether a change survives a busy machine, run it under
  `bash .claude/skills/work-issue/scripts/load-run.sh -n 8 -- <command>`: it starts the CPU hogs,
  records their PIDs, and kills only those on exit (test: `scripts/test-load-run.sh`).
- **Timing assertions in tests** measure CPU time, never wall time. The convention and helpers
  are in AGENTS.md ("Code quality bar") and `tests/common/mod.rs`; do not restate the numbers here.
  Pick the shape by the question:
  - An **absolute budget** (a fixed cost with a documented cap, DRYMEDIA.md 6.2: a CI test render takes
    2 s or less): `within_cpu_budget(what, budget, 3, || ...)`, best of 3. It catches a large regression
    (3-8x), not 2x, so use it only where the cap is a requirement, not a tuned guess.
  - A **self-relative bound** (a guard must not walk the path first; no fixed cap exists): time both
    sides with `cpu_time_of` / `thread_cpu_time` in the same run and assert `guarded <= k * baseline`
    (k a few). Best of N for both, N >= 5, so one slow sample cannot fail the suite.
  - Never `Instant` in a test. Add a negative check so the test can fail: the identical assertion with
    the guard disabled (a flag, an env var, the old build) must fail, and for a budget test
    `PSCAT_TEST_BUDGET_SCALE=0.001 cargo test --profile ci --test <file> <name>` must fail (apply
    `scale_budget` to a hand-rolled bound). If the unguarded run passes, the test does not measure the guard.

### Papercuts (#674, from the dispatcher retrospective)

- **macOS `sed -i` needs an empty suffix: `sed -i '' 's/a/b/' file`.** GNU's `sed -i 's/a/b/' file`
  on macOS takes the script as the backup suffix and the file name as the script, and fails. Inside
  an `&&` chain that failure silently skips every later step (a commit, a push), and the call's
  output may not make it obvious. Prefer the Edit tool, or `python3` for a scripted replacement;
  after any chain, check the last step's effect (`git log -1`, `git status`), not just "no error shown".
- **The `@param` unit vocabulary is closed.** A `lib/` `@param` description starts with a unit from
  the list in `tests/capabilities.rs` (`unit_is_known`: `points`, `degrees`, `count`, `0..1`, `ratio`,
  ... and the `fraction of`/`per`/`array of`/`U or V` forms). Check a new parameter's unit against it
  before the gate: `cargo test --profile ci --test capabilities` takes seconds, and the full gate
  fails on it minutes later (`notes/195-units-on-every-param.md`).
- Already covered above, and still the most common slips: zsh does not word-split `$FILES`
  ("Added in #303"), and a foreground call past about 115 s is moved to the background ("Waiting and
  timeouts"): write poll loops to finish under that, or run them with `run_in_background: true`.

### Render DPIs

Render at the resolution the output is judged at, or a before/after comparison and an identity check
disagree with what the owner sees. Where each number comes from:

| Output | DPI and page | Source |
|---|---|---|
| `gallery/*.ps` stills (`gallery/renders/*.png`) | 2x supersampled: 144 effective; page from the file's `%%BoundingBox: 0 0 W H` (the manifest's `page` matches) | `gallery/README.md` "Re-rendering the stills"; `identity_check.sh --gallery-dpi` (default 144) |
| `examples/*.ps` | 72; page from the file's `%%BoundingBox: 0 0 W H` | `identity_check.sh --dpi` (default 72) |
| `selftest/drivers/*.ps` | 72; page from `%%SelfTestPage: WxH` | `docs/dev/SELFTEST.md`; `identity_check.sh` |
| Site "Learn" examples | 144 | `scripts/build_site.sh` |
| A test's own render | whatever the test passes (`--dpi` in `tests/*.rs`) | the test |

A file with none of these headers renders on pscat's default page; `identity_check.sh` warns about it.
