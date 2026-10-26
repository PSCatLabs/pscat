## `--bands`: a work queue of many small bands (issue #844, 2026-10-26)

Follow-up to #736 (`notes/736-bands-parallel-render.md`), from fractal_clouds'
golden_coast benchmark (#736, 2026-10-26 write-up). On a 10-core M4 (4
performance + 6 efficiency cores), `--bands 10` with native noise took 74 s
(cache primed) against 44 s for their cost-balanced process splitter, which
cuts 10 bands of equal *cost* from a low-dpi probe. Equal-height bands leave
the threads that got the sky idle while one grinds through the cliffs, and
the slowest band, often on an efficiency core, sets the wall time.

Two changes came out of it.

### 1. The heap counter was the bigger cost (`src/memtrack.rs`)

`TrackingAllocator` (#604, for `--max-memory`) did one relaxed `fetch_add`
on a single shared `AtomicUsize` per allocation and free. With several
threads allocating, they fight over that cache line. Measured on an
allocation-heavy program (arrays, strings and dicts in a loop, plus `fib`):

| run | wall | user CPU |
|---|---|---|
| one plain run | 0.16 s | 0.15 s |
| 4 separate processes, at once | 0.22 s | 0.80 s |
| `--bands 4`, shared counter | 0.65-0.88 s | 2.56-2.68 s |
| `--bands 4`, no tracking allocator | 0.23-0.29 s | 0.89-1.11 s |
| `--bands 4`, batched counter (this PR) | 0.23-0.24 s | 0.86-0.93 s |

So four band threads used about 2.8x the CPU of four processes doing the same
work, and the batched counter brings threads level with processes. That
contention is a likely part of why golden_coast's `--bands 10` used 531 s of
CPU against 141 s for a plain run, and why 20 and 40 bands got slower; their
splitter ran separate processes, each with its own counter. (Not measured on
golden_coast itself; the band-aware colorimage scenes here allocate little and
show no difference, so how much of the 74 s -> 44 s gap this closes depends on
how much the program allocates.)

The fix: each thread keeps a `thread_local!` running total (a `const` `Cell`,
no destructor, safe inside an allocator) and folds it into the shared count
once it passes 64 KiB either way. `live_bytes()` folds the calling thread first,
so single-threaded readers (every plain render, the memory tests) still see an
exact number. Other threads' counts can lag by up to 64 KiB each. The shared
count is now an `AtomicIsize` and `live_bytes()` clamps at 0: memory that one
thread allocated (not yet folded) and another freed shows up as a free first,
which would have wrapped a `usize` to about 2^64 (advisor). `fold_thread()` is
called at the end of each band worker so nothing it did is left out.
Single-threaded cost: no change measured (a PostScript-only loop, `fib` plus
`sin`/`cos`/arrays, 1.15-1.21 s against main's 1.19-1.31 s).

### 2. Bands are queued work, not threads (`src/bands.rs`)

`--bands N` is now N worker threads. `bands::plan` cuts the page into K
equal-height bands, and the workers take them from an `AtomicU32`, top band
first, until none are left. Each band is still a complete, separate run of
the program with `/BandIndex`, `/BandCount` and `/BandBBox` defined, now
describing the queued band (`/BandCount` = K, not N), so output stays
byte-identical.

- **Default:** K = 2 per thread, but no band under 8 device rows (and never
  fewer than N); `--bands 1` is one band (nothing to balance).
- **`--band-rows R`** cuts ceil(H/R) equal bands instead (bands about R rows
  tall; K may exceed 64, the thread cap). It works with `--bands 1` too, which
  is how the per-band overhead below was measured.
- A band hands back only its own rows of each page and drops its canvas, so
  memory is N canvases (one per running thread) plus the finished rows (one
  page's worth per page), not K canvases.
- **Errors:** a PostScript error in a band doesn't stop the queue. Every band
  still runs, so the partial page is the one a plain render leaves, and the
  error is reported once, from the lowest band that failed. A *limit* error
  (`--max-steps`, `--max-time`, `--max-memory`) does stop it (advisor): with
  per-band limits and K bands queued on N threads, a runaway program would
  otherwise take about K/N times the limit to stop (2x by default, 70x with
  `--band-rows 1` on 140 rows), where #736's N parallel bands took 1x. Bands
  already running finish; rows of bands never run stay the page background.
  Internal failures (a thread that can't start, a panic) stop it too.
- **A fresh thread per band.** `forall` over name keys follows the names'
  interned ids, and the interner is thread-local. A worker that ran a second
  band on its own thread would start it with the first band's names already
  interned, so a name only the earlier band made shifts the later band's ids
  and can reorder its `forall` (`tests/bands.rs`
  `a_band_does_not_inherit_an_earlier_bands_names` fails with thread reuse).
  So each worker spawns one scoped thread per band, which also catches a
  band's panic. A thread costs microseconds next to an interpreter run.
- **Dictionaries:** a worker's last band leaves its dictionaries to the
  process exit, as a plain render does; earlier bands free theirs, so a long
  queue doesn't pile up dead interpreters (advisor). Those last-band
  interpreters stay in the heap, so near the end of a render up to N-1 of
  them count against a shared `--max-memory` budget.
- **`--max-memory`:** one budget for the whole render. The base is the heap
  before the workers start, plus one fresh interpreter (measured from the
  canvas-size probe) per worker, and every band counts from it
  (`Interp::set_memory_base`, new, called after `set_limits`). A first
  version rebased each band's budget from a second heap reading instead,
  which another band's allocation between the two readings could slip past
  (Codex round 1). This replaces #736's
  two start gates: they existed only so that per-band bases taken at
  different times counted each other's canvases (gate 1) and allocations
  (gate 2, Codex round 2). With bands starting at any time on a queue, a
  shared base is the only sound choice, and it needs no gates.
- **Steps and time limits** apply per band, as before.

### Per-band overhead, and why 2 per thread

Every band re-runs the whole program, so a band costs its rows plus a fixed
overhead: the program's setup, the margin rows a band-aware program computes
outside its band, and interpreter build and teardown. Measured on one thread
(`--bands 1 --band-rows R`, `examples/bands_uneven.ps`, plain run 2.55 s user):

| R (rows) | bands | user CPU |
|---|---|---|
| 200 | 1 | 2.55 s |
| 50 | 4 | 2.80 s |
| 20 | 10 | 3.19 s |
| 10 | 20 | 3.97 s |
| 5 | 40 | 5.55 s |
| 2 | 100 | 12.78 s |
| 1 | 200 | 18.09 s |

So about 75-100 ms per band, roughly 5-6 average rows: the scene's 4 margin
rows plus a small setup. fractal_clouds measured about 1.4 s of setup per band
plus 3 margin rows at about 0.7 s per row, also about 5 average rows, so the
scene is a fair stand-in for golden_coast. Every banded PNG above was
byte-identical to the plain one.

### Results on the scene (10 threads)

`examples/bands_uneven.ps`, 320x200, `--bands 10`, five interleaved rounds.
The machine was shared and loaded (load average 20-23 throughout), so the
spread is wide; the minimum is the cleanest number:

| run | bands | wall min | wall median | user median |
|---|---|---|---|---|
| main: equal bands, one per thread | 10 | 1.34 s | 1.67 s | 4.88 s |
| this PR, `--band-rows 20` (equal bands, batched counter) | 10 | 1.25 s | 1.63 s | 4.96 s |
| **this PR, default (2 per thread)** | 20 | **1.13 s** | **1.44 s** | 6.37 s |
| this PR, `--band-rows 5` (4 per thread) | 40 | 1.18 s | 1.45 s | 8.96 s |
| 10 processes, cuts of equal cost (the 44 s analog) | 10 | 0.92 s | 1.12 s | 6.52 s |

The cost-balanced reference is fractal_clouds' method applied by hand: the
row-cost profile came from single-thread runs of growing top slices
(`/BandBBox` defined by a wrapper), and the 10 cuts at rows
81/86/91/97/107/116/134/150/167 split it into equal-cost parts, one process
each. All banded PNGs were byte-identical to the plain render. The profile
was taken under the same load (the full-frame slice took 4.21 s of user time
against 2.55 s on a quieter machine), so the cuts are a little off true equal
cost, and the real cost-balanced time may be somewhat lower than 0.92 s.

Against main's equal bands, the queue's default is 1.19x faster (minimum
times; 1.16x by median), and the cost-balanced split 1.46x (1.49x by
median). So the fixed-height queue closes about half the gap to a
cost-balanced split on this scene, not all of it. The floor is the hottest
band: rows 80-100 cost about 50 ms each, so a 10-row band there is about
0.5 s on a performance core and more on an efficiency core, plus its share
of setup and margin rows. Finer bands don't help: 4 per thread is no faster
than 2 (1.18 against 1.13 s) and uses 40% more CPU, because each band pays
the setup and margin rows again (the overhead table above). Cuts of equal
cost avoid this by making only the hot bands thin. 2 per thread with an
8-row minimum is the default because it got the best wall time here for the
least extra CPU. At N=4 (an earlier, noisier sweep at load 9-11), 2 per
thread (K=8) took 1.29-1.31 s against main's 1.93-2.03 s, 1.5x, with 3 and 4
per thread no better.


**The unaware penalty.** A program that doesn't read the band names runs in
full in every band, so K bands cost about K times a plain render. With the
scene's band check disabled, at N=4: 3.69 s wall at K=4 against 6.78 s at
K=8 (plain 3.15 s, all under heavy load). `--bands` was never a speedup for
such a program (#736 measured 0.61x at N=8), and the docs now say it costs
about K times the work.

### The test scene: `examples/bands_uneven.ps`

A cheap sky over ray-marched terrain at 320x200 (golden_coast's size). Every
pixel comes from one `colorimage` data procedure. A ground pixel marches a
ray through an `fbm2` height field, so rows just under the horizon march
farthest and cost the most. Row cost, from single-thread cumulative runs: the
top 70 rows (sky) cost about 0.06 s, rows 80-100 about 1.5 s, and the
foreground tapers off. Shading reads the row above, so a band computes two
spare rows each side, as `gallery/basin_and_range.ps` does. An 8192-entry
table built first stands in for real scenes' setup.

### Also checked: 0.6.2 against main on one thread

The issue's 230 s (0.6.2) against 237 s (main, unchanged) on golden_coast: a
PostScript-only workload (the loop above, about 1.2 s) took 1.18-1.22 s on the
installed 0.6.2, 1.19-1.31 s on main and 1.15-1.21 s with this PR, three
interleaved runs each. That's noise; there is no single-thread regression.
fractal_clouds' own runs were single runs with load drifting from 3 to 12.

### Follow-ups (not done)

- **Pre-existing (#736): the band preamble can shift name ids.** The
  preamble interns `BandIndex`, `BandCount` and `BandBBox` before the
  program, so a program that first mentions one of them partway through (or
  never) gets different ids for its later names than a plain run, and a
  `forall` over a name-keyed dict can come out in another order. Repro: a
  file starting `/BandIndex pop`, then building a 40-name dict with `cvn` and
  painting in `forall` order, differs between a plain render and `--bands 1`
  on main too. The identity corpus never hits it. A fix must not change plain
  renders' ids (e.g. pre-intern the three names in every interpreter only
  after checking the corpus), so it is left for its own issue.
- Cost-aware cuts, to close the rest of the gap to a cost-balanced split:
  a caller-given `--band-cuts`, or a pscat-side cost estimate. A low-dpi
  probe like fractal_clouds' only works when the program's sample grid
  follows the dpi, which it doesn't for most of the in-repo pieces.
- Backup tasks for the tail: once the queue is empty, idle threads re-run the
  slowest bands still running and keep whichever copy finishes first (safe,
  since the bytes are identical). This helps when a hot band lands on an
  efficiency core.
- `--bands` in pscat-mcp (#843); `--lint` on band 0; a write-enabled band 0 for
  caches (fractal_clouds' "still slow" items 2 and the friction list).
