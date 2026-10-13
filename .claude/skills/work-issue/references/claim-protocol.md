# Claim protocol and heartbeat: full reference

Read when editing the claim. `SKILL.md` step 1 runs **`../scripts/claim.sh <N>`** as one command
and step 3 onward runs **`../scripts/heartbeat.sh <N>`**; neither is pasted into the skill any more
(#276, round 6: two agents in a row ran only the fresh-claim `mkdir` branch of the pasted block,
which is the abbreviation the script exists to prevent). The scripts are this file's commands moved
verbatim, `<N>` becoming `$N`; the only added lines are the argument check. Do not edit, shorten,
reorder or split them without re-running **`../scripts/test-claim.sh`** (below). Everything else
in this file is the reasoning behind the scripts, kept so a future editor does not "simplify" a
guard away.

Step numbers ("step 1" ... "step 9") refer to the numbered steps in `../SKILL.md`, which keep their original numbering. "Pitfalls" / "the ... pitfall below" means `pitfalls.md` in this directory; "Claim the issue" means `claim-protocol.md`.

### Claim the issue (atomic lock)

An `in-progress` issue with no open PR is ambiguous on its own — it's
both the normal state of a session still mid-run *and* what a crashed
session leaves behind (#29). A never-before-claimed issue has the same
ambiguity from the other direction: two invocations can both decide
"nobody's on this" at once. A timestamp comparison alone can't
arbitrate either case — two invocations that both read the same stale
(or absent) heartbeat before either writes can both conclude "safe to
proceed" (#35). The lock below is an atomic claim, not just a
staleness read: `mkdir` is atomic for a fresh claim (exactly one
caller can create a given directory name), and `mv` (rename) onto a
not-yet-existing name is atomic for stealing a stale one (exactly one
caller can rename a given source name away — every other racer's `mv`
on that same source fails with "no such file", a clean signal it lost
rather than a false belief it won — verified empirically).

The lock lives in the *main repo's* shared git dir, keyed by issue
number — not the worktree's own git dir, which for a fresh pick
doesn't exist yet at this point in the flow. `--path-format=absolute
--git-common-dir` resolves to the same canonical path from the main
checkout or from inside any worktree (verified) — unlike most paths in
this doc, this one is immune to the cwd-reset pitfall below, so no
`-C` or remembered variable is needed:

The commands are in `../scripts/claim.sh` (read it there; it keeps the original inline comments).
It prints exactly one `RESULT=` line.


`RESULT=claimed*`: this invocation owns issue #<N>. Continue (to step
2 for a genuinely fresh pick — `gh issue edit <N> --add-label
in-progress` is harmless to re-run on an issue that already carries
it, for the resume case; straight to step 3 to create or reuse the
worktree).

`RESULT=live*` or `RESULT=lost*`: **stop, don't proceed.** Report to
the user that issue #<N> looks actively worked on (or was just claimed
by a concurrent invocation) instead of silently racing another
session's edits.

**The `ORIG_HB_CONTENT`/`MOVED_HB_CONTENT` comparison after the steal
closes an ABA race the `mv`-is-atomic argument alone doesn't cover**
(found by Codex's review of this PR, then reproduced and fixed —
confirmed empirically): `mv` arbitrates *names*, not *object identity*.
If racer A reads the same stale lock as racer B, but B wins the steal
and recreates a fresh `$LOCKDIR` before A's own (possibly delayed —
scheduler jitter, not just an artificial sleep) `mv` fires, A's `mv`
still succeeds — it moves *B's brand-new claim* out from under it,
because `mv $LOCKDIR $STEAL` only cares that something currently
occupies that name, not which generation of object it is. Without the
content check, A would then believe it legitimately reclaimed a stale
lock and report `claimed`, while B's real claim sits orphaned in `$STEAL`
about to be deleted — two invocations both believing they own the
issue, the exact failure #35 exists to close. The fix: snapshot the
heartbeat's content *before* acting on it, and after the `mv`, confirm
the moved object still has that exact content. A staleness read is
always 45+ minutes old, so it can never coincidentally match a
freshly-written "now" timestamp — a mismatch is unambiguous proof
we grabbed the wrong object. On mismatch, put it back (`mv "$STEAL"
"$LOCKDIR"`) and back off rather than proceeding; the loser's `mkdir`
in the ABA-free case is what makes the restore safe to attempt (the
name is free again, and the low-concurrency threat model here doesn't
need to handle a third racer landing in that exact instant). Verified
by artificially delaying one racer's `mv` past the other's full
reclaim-and-recreate cycle: reproduced the double-claim without this
check, zero double-claims across repeated runs with it.

`stat -f %m` is BSD/macOS syntax (this repo's environment) — if it
ever fails for a reason other than a genuinely missing directory, the
`|| date +%s` fallback makes `DIR_MTIME` "now," i.e. `DIR_AGE` near
zero, i.e. `AGE=0` (live). That's deliberate: when the age can't be
determined, fail toward "don't steal" rather than toward "assume
stale" — the former just delays a legitimate reclaim slightly, the
latter silently reopens the race this whole mechanism exists to close.

The 10s `DIR_AGE` grace window (not to be confused with the 45-minute
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
can still steal it and both ends up believing they own it — a known
residual gap tracked in #72, not fully closed here (closing it needs
real compare-and-swap semantics over the whole span, which the
`mkdir`/`mv` primitives alone don't give once content, not just the
name, is what needs arbitrating).

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
an existing target), which is why the `DIR_AGE` fallback above exists
as the real close: even if the window is hit, a lock dir under 10s old
reads as live (`AGE=0`), not stale, so a racer can't steal a claim
that's still
mid-creation. Verified empirically — racing two claims against the
same fresh lock reproduced a double-claim (both racers reporting
`RESULT=claimed`) with only the `&&`-chain protection in place; adding
the `DIR_AGE` fallback and re-running the same race 50 times (30 fresh
-claim races + 20 stale-reclaim races) produced zero double-claims.
Don't remove the `DIR_AGE` fallback thinking the `&&` chain alone is
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

Run `bash .claude/skills/work-issue/scripts/test-claim.sh` after any edit to `claim.sh`. It builds a
throwaway git repo (no real lock is touched) and races `RACERS` (4) copies of the script behind a
start barrier so they genuinely overlap:

- 30 fresh-claim races (exactly one `claimed (fresh)`, every other racer `live` or `lost`);
- 20 stale-heartbeat reclaim races (never two winners; losers report `lost` or `live`). Zero winners
  is a safe outcome, but a real one: roughly 2% of stale trials, up to 20% on a loaded machine
  (liveness gap tracked in #281): a racer's ABA restore lands on another's
  in-flight reclaim, everyone reports `lost`, and the lock stays reclaimable;
- an empty lock dir under 10 s old reads `live`, one backdated past the grace window is reclaimed by
  exactly one racer, and a garbage heartbeat on an old dir is reclaimed;
- a fresh heartbeat makes every racer `live`;
- the ABA case: one racer delayed (a `sleep 1` injected into a *copy* of the script) past another's
  full reclaim-and-recreate, which must report `ABA detected, restored`;
- with `CLAIM_ORIG=<file>` (the original block, from `git show <pre-#276 ref>:...claim-protocol.md`)
  it diffs the script body against that block with `<N>` replaced by `$N`.

`FRESH_TRIALS`, `STALE_TRIALS` and `RACERS` override the counts.
