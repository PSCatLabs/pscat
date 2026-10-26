## `--bands`: closing the tail gap (issue #869, 2026-10-26)

Follow-up to #844 (`notes/844-bands-work-queue.md`). On
`examples/bands_uneven.ps` at `--bands 10`, the work queue was 1.19x faster
than equal bands, while a hand-made equal-cost process split was 1.46x. The
floor is the hottest band. This tried the issue's first option, backup
tasks, dropped it, and shipped the second, a caller-given `--band-cuts`.

### Cost profile

Single-thread user time of each 10-row band of the 320x200 scene, as one
process with `/BandBBox` set by hand (base binary, the lower of 2 runs):

| rows | 0-70 | 70-80 | 80-90 | 90-100 | 100-110 | 110-120 | 120-200 |
|---|---|---|---|---|---|---|---|
| s | 0.014-0.020 each | 0.17 | 0.93 | 0.86 | 0.72 | 0.51 | 0.31 down to 0.14 |

Total about 5.2 s. An equal-cost part at N=10 is about 0.52 s, but a 10-row
band at rows 80-90 costs 0.93 s on its own. The default queue at N=10 cuts
10-row bands, so its wall time can't drop below that band's cost.

### Backup tasks: tried and dropped

The prototype: once the queue was drained, an idle worker started a second
copy of the band that had run longest, provided it had run more than 1.5x
the median of the finished bands (never band 0, at most one copy per band).
The first copy to finish won, set an atomic flag, and the loser stopped at
its next 512-step slice. The bytes were identical either way. Backups ran on
the same N workers, so the shared `--max-memory` base still covered them.
They were turned off under `--max-memory` anyway, because a copy of the
hottest band doubles that band's heap. A backup that hit a limit gave up
without reporting it. In one traced run (on the default page, not the timed
320x200 one), backups fired for bands 6, 16, 17 and 18 at N=10, and the
copies of 17 and 18 won. At N=4 only band 7 was backed up. So the backups
mostly went to the bottom bands, which started last and were cheap.

Five interleaved rounds, load average 21-23 on the shared 10-core machine,
wall times in seconds:

| N | run | wall min | wall median | user median |
|---|---|---|---|---|
| 4 | main (queue) | 2.20 | 2.41 | 4.52 |
| 4 | backups | 2.40 | 2.87 | 6.34 |
| 4 | equal-cost process split | 1.59 | 1.76 | 4.66 |
| 10 | main (queue) | 1.67 | 1.90 | 6.45 |
| 10 | backups | 1.98 | 2.30 | 9.27 |
| 10 | equal-cost process split | 1.51 | 1.65 | 6.83 |

Backups were slower at both N and used about 40% more CPU. There are two
reasons:

- A copy re-runs the whole band. At best it recovers the penalty of an
  efficiency core, but it can never get under the hot band's own cost, and
  that cost is the gap that cuts close.
- On a machine with no idle cores, the copy takes CPU from the original.

The prototype is not in the tree.

### `--band-cuts F1,F2,...`

`--band-cuts F1,F2,...` sets the band edges as fractions of the page height
from the top, each rounded to the nearest device row. It gives K = cuts + 1
bands, queued on N threads as usual. Fractions, not rows, so one list fits
any `--dpi` and the cuts can come from a low-resolution probe. Cuts must
increase strictly and lie inside (0, 1). A cut that rounds onto an edge or
onto the previous cut's row is refused. `--band-rows` and `--band-cuts`
are refused together. A `Plan` now holds the edge table, and the band's rows,
`/BandBBox`, `/BandCount` and the stitch all read it (`Plan::rows`).

The same harness and interleaved rounds, comparing the cuts above (N=4: rows
91/106/136; N=10: 83/88/94/100/106/115/127/146/170) given to `--band-cuts`,
the same cuts as separate processes, and main's default queue:

| N | run | wall min | wall median | user median |
|---|---|---|---|---|
| 4 | main (queue) | 2.26 | 2.38 | 4.56 |
| 4 | `--band-cuts`, 4 bands | 1.66 | 1.79 | 4.64 |
| 4 | process split | 1.71 | 1.79 | 4.70 |
| 10 | main (queue) | 1.79 | 2.08 | 6.44 |
| 10 | `--band-cuts`, 10 bands | 1.53 | 1.73 | 6.68 |
| 10 | process split | 1.57 | 1.77 | 6.84 |

That was seven rounds at load 24-29. An earlier five-round run at load
12-19 gave the same ordering by minimum (its medians drifted with the load): `--band-cuts` 1.07 and 0.99 s minimum at N=4
and N=10, against 1.31 and 1.13 s for the queue. In both runs, cuts in
process match the process split. Against the queue they are 1.33-1.36x at
N=4 and 1.17-1.20x at N=10 (min and median). Twice as many equal-cost cuts
(2N bands) were slower than N, because each band pays the setup and margin
rows again. All banded PNGs were byte-identical to the plain render.

### stderr, once

Before this, every band wrote its own `(%stderr)` output, `handleerror`
lines and `findfont` stand-in notes, so `--bands 4` on main printed a
`%stderr` line 8 times (K = 8). The top band now writes them alone, as it
alone writes stdout: the other bands call `Interp::quiet_stderr` (new). The
font notes still collect in `Interp::font_warnings`, so the ones that only a
lower band raised are printed once at the end. The `-e` test in
`tests/bands.rs` covers it.

### Limits and side effects (the issue's checklist)

- `--max-steps` and `--max-time` are per band, and `--max-memory` is one
  budget. Unchanged: with backups dropped, no band runs twice.
- `--allow-write` is still refused with `--bands`, so no band can write a
  file, let alone write one twice.

### Follow-ups (not done)

- Find the cuts inside pscat. One way is a cost probe: time each band of a
  first quick pass, or count steps per band at low dpi. A low-dpi probe only
  helps when the program's sample grid follows the dpi, which `bands_uneven.ps`
  (fixed 320x200 grid) and most in-repo pieces don't. Another is to learn
  from the queue as it runs, e.g. splitting a band that proves slow (option 3
  in the issue).
- `--band-cuts` in pscat-mcp, with #843 (`--bands` there).
