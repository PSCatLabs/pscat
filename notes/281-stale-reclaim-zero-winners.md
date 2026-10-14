## work-issue: stale-reclaim races with zero winners (issue #281, 2026-10-13)

`claim.sh` could end a stale-lock reclaim race with nobody holding the lock, so a free issue read
"being worked on" for 10 s and `/work-issue <N>` stopped. Measuring it under load also showed the
protocol could **double-claim** (filed as #297).

- **Mechanism (zero winners).** Racer A moves the stale dir away and `mkdir`s its own. Racer B,
  delayed after reading the same stale heartbeat, moves A's new dir, sees the content mismatch
  (ABA) and puts it back. While it is away, A's heartbeat write fails, so A reports `lost a rare
  3-way reclaim race` and the dir is left with no heartbeat.
- **Retry, for the 3-way loser only.** At most one racer can be the 3-way loser: the first `mv`
  moves the stale dir and passes the checks. That racer now waits (at most 13 s) for a heartbeat
  or for the leftover dir to pass the 10 s grace window, then re-runs the protocol, with up to 3
  attempts. A missing dir does not end the wait: a `mkdir` into an ABA mover's gap is the
  double-claim path.
- **Inode identity check.** One `stat -f '%i %m'` before the heartbeat read gives the snapshot.
  A pre-check before the `mv` means a racer with an out-of-date decision moves nothing. A
  post-check after the `mv` sits next to the content check, which cannot tell two empty dirs
  apart. The empty-dir case was a deterministic double claim before (`test-claim.sh` "empty-dir
  ABA").
- **Harness.** Stale races now assert exactly one winner and a held lock, and label failures
  `zero` or `double`. New tests: inode pre-check, empty-dir ABA, and 3-way retry (orphan
  reclaimed; live if a heartbeat appears). `CLAIM=` runs the same harness on another copy, and
  `TRACE=1` dumps first-attempt-no-winner traces. The `CLAIM_ORIG` verbatim diff is gone, since
  the body now differs on purpose.
- **Numbers** (4 racers, 10-core Mac, `yes >/dev/null` CPU hogs):

  | Run | Load | Exactly 1 winner | Zero winners | Double claims |
  |---|---|---|---|---|
  | Baseline: `main`'s harness and script, 300 stale | 10 hogs, load average 7 → 24 | 207 | 91 | 2 |
  | New harness, `CLAIM=` pre-#281 script, 1000 stale | 20 hogs, load average 15 → 29 | 596 | 397 | 5 |
  | Same harness, this script, 1000 stale | 20 hogs, load average 29 → 49 | 1000 | 0 | 0 |
  | Final harness, this script, 300 stale (ALL PASS) | 10 hogs, load average 24 → 31 | 300 | 0 | 0 |

  All 397 zero-winner traces had exactly one `3-way` loser. With this script, the pre-check cut
  trials with no first-attempt winner from 397 to 1, and the retry resolved that one. 135 trials
  had a retry; the other 134 retries saw a winner's heartbeat and reported `live`.
- **Ablation:** with the pre-check disabled, 300 stale trials had 15 orphans (no first-attempt
  winner). The retry reclaimed all 15: 300/300 exactly one winner, 0 doubles. Doubles were 0
  both in total and among retry claims, so the retry's orphan reclaim is safe under contention.
- **Double claims** went from 5/1000 to 0/1000 at higher load. That is "not observed", not
  "fixed": the #297 mechanism is unchanged, and the pre-check only removes most ABA moves.
- **Changed assertion:** the backdated-dir check now accepts exactly one winner of either kind.
  Under load, a late racer's first `mkdir` can land while the stealer has the dead dir moved
  away, giving one `claimed (fresh)`. The strict version, 100 trials each under load: the
  original script had 6 such trials, this one 8. So it is load, not this change.
- **Rollout:** a mixed race (2 pre-#281 copies + 2 new, 300 stale) had 35 zero-winner trials
  and 0 doubles. When an old copy is the 3-way loser it does not retry, so zero winners persist
  until every checkout has pulled.
- **Residuals** (in `claim-protocol.md`): the transit-window double claim (#297) is narrowed, not
  closed. The retry adds one waiter to the #72 suspended-claimant gap. Old copies of `claim.sh`
  lack the inode check and the retry until every checkout pulls. `claim.sh` can now take up to
  ~30 s after a lost race (SKILL.md step 1 says to wait).
