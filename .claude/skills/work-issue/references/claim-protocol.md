# Claim protocol and heartbeat: full reference

Read when editing the claim. `SKILL.md` step 1 runs **`../scripts/claim.sh <N>`** as one command
and step 3 onward runs **`../scripts/heartbeat.sh <N>`**; neither is pasted into the skill any more
(#276, round 6: two agents in a row ran only the fresh-claim `mkdir` branch of the pasted block,
which is the abbreviation the script exists to prevent). #281 added an inode identity check and a
bounded retry to the move-based steal; #297 replaced the move-based steal with a generation lock
that never moves the lock dir (below), which made both unnecessary. Do not edit, shorten, reorder
or split the scripts without re-running **`../scripts/test-claim.sh`** (below). Everything else in
this file is the reasoning behind the scripts, kept so a future editor does not "simplify" a guard
away.

Step numbers ("step 1" ... "step 9") refer to the numbered steps in `../SKILL.md`, which keep their original numbering. "Pitfalls" / "the ... pitfall below" means `pitfalls.md` in this directory; "Claim the issue" means `claim-protocol.md`.

### Claim the issue (atomic lock)

An `in-progress` issue with no open PR is ambiguous on its own — it's
both the normal state of a session still mid-run *and* what a crashed
session leaves behind (#29). A never-before-claimed issue has the same
ambiguity from the other direction: two invocations can both decide
"nobody's on this" at once. A timestamp comparison alone can't
arbitrate either case — two invocations that both read the same stale
(or absent) heartbeat before either writes can both conclude "safe to
proceed" (#35). The lock is an atomic claim, not just a staleness
read: `mkdir` is atomic (exactly one caller can create a given
directory name). A fresh claim is `mkdir LOCKDIR`; since #297 a stale
reclaim is `mkdir LOCKDIR/gen.<K+1>` inside the existing lock (see
"Generation lock" below).

The lock lives in the *main repo's* shared git dir, keyed by issue
number — not the worktree's own git dir, which for a fresh pick
doesn't exist yet at this point in the flow. `--path-format=absolute
--git-common-dir` resolves to the same canonical path from the main
checkout or from inside any worktree (verified) — unlike most paths in
this doc, this one is immune to the cwd-reset pitfall below, so no
`-C` or remembered variable is needed.

The commands are in `../scripts/claim.sh` (read it there; its comments say why each line is
there). It prints exactly one `RESULT=` line and returns in well under a second (no waits, no
retry since #297).

`RESULT=claimed*`: this invocation owns issue #<N>. Continue (to step
2 for a genuinely fresh pick — `gh issue edit <N> --add-label
in-progress` is harmless to re-run on an issue that already carries
it, for the resume case; straight to step 3 to create or reuse the
worktree).

`RESULT=live*` or `RESULT=lost*`: **stop, don't proceed.** Report to
the user that issue #<N> looks actively worked on (or was just claimed
by a concurrent invocation) instead of silently racing another
session's edits.

### Generation lock (#297)

On disk, `work-issue-lock-<N>/` (the *container*) holds:

- `heartbeat`: a Unix timestamp, refreshed by `heartbeat.sh`. One file shared by every
  generation, exactly where every older script reads and writes it.
- `gen.1`, `gen.2`, ...: empty dirs, one per stale reclaim. The owner is whoever created the
  newest one; with none, the owner is whoever created the container (generation 0). A lock that
  was never reclaimed is byte-for-byte the pre-#297 format.
- possibly leftovers (`heartbeat.tmp*`, an older script's nested `work-issue-lock-<N>.stale.*`):
  ignored; the release `rm -rf` removes them.

**The invariant: nothing but the release (`rm -rf` of the whole container, step 9 or a step 8
stop) ever moves or deletes the container or a generation dir.** So within one container's
lifetime `mkdir gen.<K>` can succeed once, for one caller: it is a compare-and-swap on "the
newest generation is still K-1". And because the container is never moved, the lock name is never
free while a lock exists, which closes the #297 transit window by construction rather than by
narrowing it.

A stale reclaim (`claim.sh` after its fresh `mkdir` fails):

1. **Read order.** Stat the container's inode (`I0`); list `gen.*` once and take the numerically
   largest K (0 if none; `gen.10` beats `gen.9`; non-numeric names are skipped); stat the age of
   that generation (the container itself for K=0); read the heartbeat. Then
   `mkdir gen.<K+1>` with the K from that one listing, never a re-list. However long the process
   stalls between these steps, anything another racer created after the listing makes the
   `mkdir` fail with EEXIST. Reading the heartbeat first and listing later would let a racer
   that stalled over 10 s combine its stale heartbeat read with someone else's no-longer-young
   `gen.K+1` and win `gen.K+2`: the double claim again.
2. **In flight.** If the newest generation is under 10 s old, report `live`, whatever the
   heartbeat says: its creator may not have written the heartbeat yet, so the file can still
   hold the previous owner's stale timestamp. This is the #35 empty-dir grace window, applied
   per generation. Nothing writes inside a `gen.K` dir, so its mtime is its creation time.
   (The container's mtime moves with every heartbeat rename, so for K=0 a young container also
   just means a recent heartbeat.)
3. **Stale** = past the grace window and a heartbeat that is missing, garbage, or 45+ minutes
   old. Then `mkdir gen.<K+1>`; failure is `lost`.
4. **Post-checks**, around the heartbeat write: the container's inode is still `I0` (before and
   after the write), our `gen.<K+1>` still exists, and no `gen.<K+2>` does. The inode catches a
   container that was released and re-created by a fresh claim between our decision and our
   `mkdir` (our gen dir then sits in someone else's lock), and a container an older `claim.sh`
   moved away (see Rollout). `gen.<K+2>` can only exist if we stalled over 10 s after our
   `mkdir` and someone reclaimed past us; reporting `lost` there narrows #72 a little. A failed
   post-check reports `lost`, never a retry and never a fresh `mkdir LOCKDIR` from inside the
   reclaim path. A lost gen dir is left in place (deleting it would let its number be created
   twice).

Why the heartbeat is not per generation: `heartbeat.sh` runs in a fresh shell with only `<N>`,
so it cannot know which generation its run owns without a token the skill would have to carry
between tool calls, and the older copies of `heartbeat.sh` that resuming agents run write only
`LOCKDIR/heartbeat` anyway, so liveness has to read that file whatever else exists. Keeping it the
single liveness source means `heartbeat.sh` and its older copies need no change (the only edit is
a per-process temp name, so two writers never rename each other's half-written file).
Generation-aware ownership checks in `heartbeat.sh` would need that token; they belong to #72.

Why there is no retry any more: the #281 retry existed for the one racer that moved the stale dir
away and then lost its re-create, which could leave the lock with no owner. With `mkdir gen` as
the only steal, a stale race has exactly one `mkdir` winner, who then writes the heartbeat; a
racer whose view is out of date fails its `mkdir` and reports `lost`. The only zero-winner case
left is the winner itself failing a post-check (a release or an older script's move at that
instant), and then the lock reads `live` for at most 10 s and is reclaimable after.

Residuals (new scripts only):

- **#72 stalls.** A winner suspended over 10 s between `mkdir gen` and its heartbeat write can be
  reclaimed past (it then reports `lost` if it gets to the post-check, but a stall *after* the
  post-check is invisible). A dead-looking owner (45+ min) that wakes up after a reclaim keeps
  writing the shared heartbeat. Both predate #297 and need an ownership token.
- **Release by a superseded owner.** `rm -rf` does not check ownership: an owner whose lock was
  reclaimed can still delete the new owner's lock at step 9. Unchanged by #297.

### Rollout: older scripts on the same disk (#297)

Agents with open PRs resume later and run step 9 (heartbeat, release, labels) with whatever
copies they have: their worktree's `claim.sh`/`heartbeat.sh`, or main's. "Old" below means any
pre-#297 copy (the #281 one, or older). Rows are what runs; columns, the layout it meets.

| Script that runs | Old-format lock (no `gen.*`) | Generation lock (`gen.K` present) |
|---|---|---|
| new `claim.sh`, no lock yet | creates one in the old format (`mkdir` + `heartbeat`, generation 0), which every old script reads correctly | n/a |
| new `claim.sh`, live lock | `live` (heartbeat fresh, or container under 10 s old) | `live` (heartbeat fresh, or `gen.K` under 10 s old) |
| new `claim.sh`, stale lock | `mkdir gen.1` inside it (113/120/169 shape: tested) | `mkdir gen.<K+1>` |
| new `heartbeat.sh` | refreshes `heartbeat` | refreshes `heartbeat` (the liveness source) |
| old `heartbeat.sh` | refreshes `heartbeat` | refreshes `heartbeat`: new and old `claim.sh` both read it as live (tested) |
| old `claim.sh`, live lock | `live` | `live`: it reads only `heartbeat`, which new scripts keep fresh (tested) |
| old `claim.sh`, stale lock | old move-based steal (unchanged) | moves the whole container (gens included) away and re-creates the old format; new scripts then read that as generation 0, live (tested) |
| release `rm -rf`, any version | removed | removed, gens and leftovers included (tested) |
| old `claim.sh` during a new reclaim in flight (`gen.K` young, heartbeat not yet rewritten) | n/a | reads only the stale heartbeat, so it may move the container: the rollout residual below |
| new `claim.sh` during an old copy's move-based steal | the name may be briefly free (fresh `mkdir`: the rollout residual) or the dir young and empty (`live`) | same |

Nothing leaves a lock that cannot be released (`rm -rf` takes any layout) or reclaimed (the new
script judges any container by its newest generation and the shared heartbeat; an old script
judges it by the heartbeat alone, as before).

**The rollout residual.** An old `claim.sh` still steals by moving the container, so a stale race
in which an *old* copy is one of the racers keeps the pre-#297 transit window, against new racers
as well as old ones (a new racer's `mkdir LOCKDIR` can land while an old mover holds the
container away). The new script's inode post-check turns some of those into `lost` instead of a
double claim, not all. In practice: old copies only exist in worktrees cut before this change;
the agents holding them run `heartbeat.sh` and the release on their *own* issue, and a resume
re-claim of their own `<N>`, whose lock they released at step 8 (so that claim is a fresh `mkdir`,
which never moves anything). A stale race on the same `<N>` with an old copy needs a second agent
picking that same issue at the same moment. The mixed-version test (2 old + 2 new racers) reports
its numbers as informational for this reason: 100 trials under 20 CPU hogs gave 91 with one
winner, 9 with none (an old mover and a new winner knocking each other out; the lock then holds a
fresh heartbeat and reads live for 45 min) and 0 doubles. It goes away as those worktrees are
removed.

The three stale old-format locks in the common git dir (`work-issue-lock-113`, `-120`, `-169`,
heartbeats from September, issues closed) need nothing: no lock sweeper exists, a lock is only
judged when someone claims that number, and then the new script reclaims it as `gen.1` (the test
copies that exact shape). Release them by hand only if you mean to, never as part of a run.

### History: the move-based steal (pre-#297)

Kept so nobody reintroduces it. From #35 to #281, a stale lock was stolen by
`mv LOCKDIR LOCKDIR.stale.*` (rename is atomic: exactly one racer can move a given name away),
then re-created with `mkdir`. That arbitrates *names*, not objects, which cost three rounds:

- **ABA (#35 era).** A delayed racer's `mv` could carry away a *fresh* lock another racer had
  just re-created. The fix compared the moved dir's heartbeat with the snapshot and moved it back
  on a mismatch.
- **Empty-dir ABA (#281).** Two empty dirs have equal (empty) heartbeats; #281 added an inode
  check before and after the `mv`, and a bounded retry for the racer whose re-create was knocked
  away (91/300 loaded stale trials had ended with no winner).
- **The transit window (#297).** Every ABA move, away and back, frees the lock name for a moment.
  A late racer's fresh `mkdir` lands in it, and then either the original claimant's by-path
  heartbeat write lands in the newcomer's dir or the restore `mv` nests into it (BSD `mv` onto
  an existing dir moves *into* it): two claims. The pre-#281 script showed 2 double claims in 300
  loaded stale trials; the #281 pre-check made ABA moves rarer (0/1000 observed) but could not
  remove them, since POSIX has no "rename only if absent" for a directory and no way to write
  into a dir by identity rather than by path. `test-claim.sh`'s "transit window" test forces the
  schedule with injected delays: the #281 script double-claims in every trial, the generation
  lock in none.

### Ages, thresholds and the grace window

`stat -f %i` / `stat -f %m` are BSD/macOS syntax (this repo's environment). If
the age stat of the newest generation fails, its mtime is taken as "now", so
it reads as in flight (live). If the container's inode stat fails, the script
reports `lost` and touches nothing. That's deliberate: when the age can't be
determined, fail toward "don't steal" rather than toward "assume
stale" — the former just delays a legitimate reclaim slightly, the
latter silently reopens the race this whole mechanism exists to close.

The 10 s grace window on the newest generation's age (not to be confused with the 45-minute
staleness threshold above) is chosen to be *short*, not long: it only
needs to outlast the few milliseconds between a legitimate claim's
`mkdir` and its heartbeat write landing — it is not a safety margin
against a slow or interrupted claim. If a claim's `&&` chain is ever
genuinely cut mid-way (the tool call itself killed, not just disk
slowness — see the auto-backgrounding pitfall below), the lock dir
sits empty past 10s and the next invocation correctly reads it as
abandoned and reclaims it. That's the intended behavior for a dead
claim, not a bug — 10s is picked to reclaim a genuinely dead claim
promptly, not to give a live one room to breathe (the 45-minute
threshold already does that job). If the pause is *not* the tool call
being killed but the claimant genuinely still running, just suspended
past 10s between `mkdir` and its heartbeat write, a second invocation
can still reclaim past it — a known residual gap tracked in #72 (since
#297 the stalled claimant reports `lost` if the reclaim happens before
its post-check; closing it fully needs an ownership token).

45 minutes is deliberately generous, not tight: PR #30 needed six
Codex review rounds in one legitimate run (~62 min total, each round
itself several minutes) — a false "still live" fully blocks resume,
which is worse than a slower crash-detection window.

**The `mkdir` and its immediately-following heartbeat write must stay
one `&&` chain in a single command, never split across separate tool
calls or steps** — this minimizes how long the dir-exists-but-empty
window is open. That window can't be closed to zero in portable POSIX
shell (there's no atomic "create a populated directory" primitive —
`mv`'s own directory-destination handling nests rather than replacing
an existing target), which is why the grace window above exists
as the real close: even if the window is hit, a lock dir under 10s old
reads as live, not stale, so a racer can't steal a claim
that's still
mid-creation. Verified empirically — racing two claims against the
same fresh lock reproduced a double-claim (both racers reporting
`RESULT=claimed`) with only the `&&`-chain protection in place; adding
the grace window and re-running the same race 50 times (30 fresh
-claim races + 20 stale-reclaim races) produced zero double-claims.
Don't remove the grace window thinking the `&&` chain alone is
sufficient — it isn't, and a future edit that "simplifies" it away
reopens exactly the race #35 exists to close.

## Heartbeat writes after the claim (moved from step 3)

The step 3 command block itself stays in `SKILL.md`. The rationale:

The lock/heartbeat lives in the *main repo's* shared git dir, keyed by
issue number (step 1's **Claim the issue** already created it before
this step ran) — not inside this worktree's own git dir, so it
survives independent of whether the worktree itself is later removed.
It never needs a `.gitignore` entry and never shows up in `git status`
or a diff. Refresh it at the end of steps 4, 5, 6, and around each
Codex review round in step 8 — and, since "commit checkpoints" or
"step boundaries" can themselves be 45+ minutes apart during a long
compile or a lengthy single edit, also refresh it before starting any
single operation you expect to take a while (a big `cargo build`/
`cargo test`, an `advisor` call, the Codex review) rather than only
after it finishes — so step 1's claim check can tell a session still
working from one that crashed. Each Bash tool call is a fresh shell —
a `$LOCKDIR` variable set here won't survive to a later step's tool
call — so those refreshes re-derive the path inline rather than
trusting a remembered variable. Write it atomically (temp file + `mv`,
which is atomic on the same filesystem) rather than a bare `>`
redirect: a process that dies between the redirect's truncation and
`date` finishing its write would otherwise leave a heartbeat file that
exists but is empty — step 1's claim check above already treats that
as stale, but only *because* it validates content, not just presence.
Unlike `$WORKTREE_DIR`-derived paths, `--path-format=absolute
--git-common-dir` doesn't need `-C` at all — it resolves to the same
canonical main-repo path regardless of where cwd happens to be,
so it's immune to the cwd-reset pitfall below on its own.

Every refresh site from here through step 8 writes into `$LOCKDIR`
*without* re-`mkdir`ing it — deliberately: the lock dir should already
exist for the entire span between this claim and step 8's terminal
non-merge stops (the only places that release it, and only *after* the
fix-and-re-review loop in step 8 has already concluded — see step 8),
so a refresh finding it missing is a genuine anomaly, not routine
housekeeping. An earlier draft of this fix made refreshes self-heal
with `mkdir -p`, reasoning a refresh "isn't an ownership decision." A
Codex review of this change caught why that's wrong: if the lock is
gone because a *different* invocation legitimately claimed the issue
in the meantime (crash recovery, a stale reclaim, a human re-running
`/work-issue <N>` after this session's own run ended) and this
session's refresh code is still executing for some reason — a stray
retry, a follow-up in the same conversation — `mkdir -p` would silently
recreate the name and overwrite that invocation's heartbeat with this
session's own, with neither side any the wiser that two invocations
are now both editing the same worktree. That's the exact failure #35
exists to close, reintroduced through the back door of "helpful"
self-healing. Let the write fail loudly (the redirect errors with no
such directory) instead — that's the correct signal to stop and
re-verify ownership via **Claim the issue**, not silently patch over it.

**A missing lock dir is the anomaly bare writes catch — a lock dir
that exists but was legitimately reclaimed by someone else in between
is a related, more dangerous case bare writes do *not* catch**: the
directory is present, so the write succeeds and silently overwrites
the new owner's heartbeat, without a mismatch to signal the takeover.
Closing that fully needs an ownership token verified with real
compare-and-swap semantics, which POSIX shell can't express atomically
over the whole read-decide-write span (a token turns the problem into
"don't let the compare-then-write itself get raced," which is the same
shape of gap, just narrower) — tracked as a known residual gap in #72
rather than fixed here:

(the two commands are `../scripts/heartbeat.sh <N>`)

(the same "re-derive, don't trust a remembered variable" reasoning
already applies to `$WORKTREE_DIR`/`$BRANCH` reused across steps 7-9
below — re-emit the assignment in each new Bash block rather than
assuming it's still set; those two *do* still need `-C`/absoluteness
care, since they resolve to worktree-relative paths, not the
cwd-independent main-repo one above.)

Everything from here on — reading, editing, building, testing,
committing — happens *inside `$WORKTREE_DIR`*, not the original
directory. If a worktree for this issue already exists (a prior
attempt — step 1's claim already confirmed it's stale and reclaimed
it), reuse it rather than erroring or clobbering it: set `WORKTREE_DIR`
to the existing path, and read `BRANCH` from what's actually checked
out there —

```sh
BRANCH="$(git -C "$WORKTREE_DIR" branch --show-current)"
bash "$WORKTREE_DIR/.claude/skills/work-issue/scripts/heartbeat.sh" <N>
```

— instead of recomputing a fresh `SLUG`/`BRANCH` from the issue title.
A freshly regenerated slug can differ from a prior run's (wording
tweak, truncation difference) even though the worktree itself is being
correctly reused, and every later branch assertion/push/cleanup in
this skill trusts `$BRANCH` — pointing it at a name that doesn't match
what's actually checked out silently targets the wrong branch instead
of erroring (#29).

## Race tests (`../scripts/test-claim.sh`)

Run `bash .claude/skills/work-issue/scripts/test-claim.sh` after any edit to `claim.sh` or
`heartbeat.sh` (about 45 s). It builds a throwaway git repo (no real lock is touched) and races
`RACERS` (4) copies of the script behind a start barrier so they genuinely overlap. Delays are
injected with `sed` into *copies* of the script.

On any copy (`CLAIM=<path>`):

- 30 fresh-claim races (exactly one `claimed (fresh)`, every other racer `live` or `lost`);
- 20 stale-heartbeat reclaim races, each with **exactly one** winner, the lock ending with a
  recent heartbeat (and, for the generation lock, exactly one `gen.1`). A failure is labelled
  `zero` (nobody claimed) or `double` (two claims). For a real measurement run 300+
  `STALE_TRIALS` under load: `bash scripts/load-run.sh -n 20 -- bash scripts/test-claim.sh`;
- an empty lock dir under 10 s old reads `live`, one backdated past the grace window is taken by
  exactly one racer, a garbage heartbeat on an old dir is reclaimed, a fresh heartbeat makes
  every racer `live`;
- **the transit window** (#297): racer A decides "stale" and is delayed 1 s, B reclaims at
  +0.1 s, a newcomer C arrives at +1.5 s. On a move-based copy the delays go before A's `mv` and
  before its restore, so C's fresh `mkdir` lands while A holds B's lock away: the #281 and
  pre-#281 scripts double-claim in every trial. On the generation lock A's delay goes before its
  `mkdir gen.1`, which then fails, and C reads B's young generation as live;
- `heartbeat.sh` refreshes an existing lock and fails, without creating it, on a missing one.

On the generation lock only:

- a reclaim in flight (B stalled 2 s between `mkdir gen.1` and its heartbeat): the newcomer reads
  `live` although the heartbeat is still stale;
- a container released and re-created by a fresh claim under a delayed racer: the delayed racer
  sees the new inode and reports `lost`;
- reclaimed past us (a `gen.2` appears while B is stalled): B reports `lost`;
- `gen.10` beats `gen.9`, non-generation entries are ignored, a young newest generation is live;
- compatibility, against `OLD_REF` (default `6a4cd71`, the #281 scripts): an old-format stale lock
  shaped like `work-issue-lock-113` is reclaimed once, as `gen.1`; an old `heartbeat.sh` keeps a
  generation lock live for old and new `claim.sh`; the new `heartbeat.sh` keeps an old-format lock
  live for the old `claim.sh`; an old `claim.sh` reclaiming a stale generation lock leaves an
  old-format lock the new one reads as live; `rm -rf` removes a generation lock; and
  `MIXED_TRIALS` (10) stale races of 2 old + 2 new racers, reported but not failed (the rollout
  residual above).

The move-specific tests of #281 (ABA restore, inode pre-check, empty-dir ABA, 3-way retry) went
with the move-based steal; they are in this file's git history at `6a4cd71`. `TRACE=1` prints
every racer's output for each stale trial whose first attempt had no winner (only a retrying
copy can have one and still pass). `FRESH_TRIALS`, `STALE_TRIALS`, `MIXED_TRIALS` and `RACERS`
override the counts.
