## work-issue: a move-free generation lock closes the claim.sh double claim (issue #297, 2026-10-14)

`claim.sh` could report `claimed` to two runs under load. Its stale steal moved the lock dir away
(`mv`), checked what it had moved, and moved it back on an ABA mismatch. While the dir was away the
lock *name* was free, so a late racer's fresh `mkdir` succeeded. Then either the first claimant's
by-path heartbeat write or the restore `mv` (BSD `mv` onto an existing dir moves *into* it) landed
in the newcomer's dir, and both runs reported `claimed`. #281 narrowed that window but could not
close it.

- **Design.** Ownership is now `mkdir`-only. A fresh claim is `mkdir LOCKDIR` plus a heartbeat, as
  before (generation 0, byte-for-byte the old format). A stale reclaim is `mkdir
  LOCKDIR/gen.<K+1>`, where K is the newest generation it judged stale. Nothing except the release
  `rm -rf` ever moves or deletes the container or a generation dir, so each `gen.K` can be created
  once and the lock name is never free while a lock exists. Read order: container inode, then one
  generation listing, then that generation's age, then the heartbeat. A newest generation under
  10 s old reads as live (the #35 grace window, per generation). Post-checks around the heartbeat
  write: same container inode, our gen still there, no `gen.<K+2>`.
- **Simpler than the issue's sketch.** The heartbeat stays the one shared `LOCKDIR/heartbeat`, not
  one file per generation. `heartbeat.sh` runs in a fresh shell with only `<N>` and has no way to
  know its generation, and old copies only write that file anyway. So `heartbeat.sh` is unchanged
  apart from a per-process temp name. The #281 inode pre-check, ABA restore and settle/retry are
  all gone: with no moves, a stale race has exactly one `mkdir` winner. `claim.sh` went from 138
  to 111 lines and returns at once.
- **Compatibility.** The matrix is in `claim-protocol.md` "Rollout". Old `heartbeat.sh` keeps a
  generation lock live. Old `claim.sh` reads a live generation lock as live, and it reclaims a
  stale one by its old move (leaving an old-format lock the new script reads correctly). `rm -rf`
  removes any layout. Old-format stale locks (`work-issue-lock-113/-120/-169`) are reclaimed as
  `gen.1` when someone claims that number. No lock sweeper exists; `sweep-max.sh` is the unrelated
  #303 gallery tool. The residual: an old `claim.sh` copy still moves locks, so a stale race that
  includes one keeps the transit window until the pre-#297 worktrees are gone.
- **Harness.** `test-claim.sh` gained a forced **transit-window** test (A decides and is delayed,
  B reclaims, newcomer C arrives mid-window) that runs on any copy. It also has generation-only
  tests: in-flight gen, replaced container, reclaimed past us, numeric newest gen with junk
  ignored, and the old/new compatibility cases against `OLD_REF=6a4cd71`, plus informational
  mixed races. The move-specific #281 tests went with the move.
- **Numbers** (4 racers, 10-core Mac, `scripts/load-run.sh -n 20`, load average 27-30):

  | Script | Stale trials | Exactly 1 winner | Zero | Double | Forced transit window |
  |---|---|---|---|---|---|
  | pre-#281 (`6a4cd71~1`) | 300 | 162 | 137 | **1** | **5/5 double** |
  | `main` (#281) | 300 | 300 | 0 | 0 | **5/5 double** |
  | generation lock | 600 | 600 | 0 | **0** | **0/5 double** |
  | mixed, 2 old (#281) + 2 new | 100 | 91 | 9 | 0 | n/a |

  The random stale race barely shows the bug on `main` (0/300 here, 0/1000 in #281), so the
  forced schedule is the before/after evidence. The mixed run's zero-winner trials are an old
  mover and a new winner knocking each other out. The lock then holds a fresh heartbeat and reads
  live for 45 minutes. That is a liveness cost of the rollout only, never a double claim, in these
  trials.
- **Deferred.** Generation-aware ownership in `heartbeat.sh` (an ownership token) belongs to #72.
  So does release by a superseded owner, which is unchanged.
