# Pitfalls

Read by `SKILL.md` before step 1 (skim) and whenever a step says so.

Step numbers ("step 1" ... "step 9") refer to the numbered steps in `../SKILL.md`, which keep their original numbering. "Pitfalls" / "the ... pitfall below" means `pitfalls.md` in this directory; "Claim the issue" means `claim-protocol.md`.

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
- **A PR opened a while ago (or after another PR merged first) will
  likely show `mergeStateStatus: BEHIND`** at step 8, because branch
  protection's `strict` status check requires the tested commit to
  reflect current `main` — the CI run from when the PR was opened no
  longer counts. This is normal, not a failure: `gh pr update-branch`,
  wait for CI to re-run on the updated branch, then merge. Confirmed
  hitting this on the very first agent-merged PR in this repo (#22).
- **Remove the worktree before deleting the local branch, not after**
  (step 9's order matters) — `git branch -d` fails with "branch used
  by worktree" if the worktree still references it. `git worktree
  remove` first, then the branch delete, then `fetch --prune` to clear
  the now-dead remote-tracking ref.
- **A Codex review at step 8 legitimately takes several minutes** — it
  reads around the diff (NOTES.md, HANDOFF.md, prior commits) rather
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
  (task notification said `completed`, but `/tmp/codex-review-<N>.json`
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
  Same reasoning applies to the reclaim path's post-`mv` `mkdir` +
  heartbeat write.
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
- **The full quality gate (`cargo build && cargo test && ...`) takes
  longer than the Bash tool's 120s default foreground timeout.** Run it
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
