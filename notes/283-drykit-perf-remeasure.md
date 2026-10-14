## drykit board proxy: re-measured after bind, inside budget (issue #283, 2026-10-13)

#283 said the board core plus the ghost-text proxy took 31–33s in pscat,
against DRYMEDIA 6.2's 30s for a full chalkboard. Then #307 (PR #319)
bound every top-level procedure in artkit, paintkit and drykit. Measured
again on that commit, pscat is at **25.2s, inside the budget**. No code
changed for this issue. The levers and two correctness gaps that #283
listed move to #324, and the pscat interpreter cost to #323.

### Method

- Proxy: `docs/drymedia/prototypes/surface_cost.ps`, with the board
  prelude `/MODE 6 def /TOUCH 0.82 def /PEN 36 def /GRAIN 0.4 def`, plus
  `/LEVELS 2 def` for the core or `/LEVELS 1 def` for the ghost-text
  proxy.
- Settings: 144 dpi (`--dpi 144 --page 720x480`, gs `-r144 -g1440x960
  -dNOSAFER`), release pscat, gs 10.07.1.
- Time: CPU `user` seconds. The main rows are the best of 3, with a load
  average of 3 to 5.
- Leave out the prelude and the file's defaults (`/GRAIN 0.7`, `/PEN 70`)
  draw a lighter board: about 507k and 269k cells, and 17s in pscat. That
  board is not the one 6.2 budgets. The cell counts below match #283's.

| Commit | Case | Cells (pscat / gs) | pscat | gs |
|---|---|---|---|---|
| c22b262 (D2 merged, pre-bind) | core | 827978 / 827105 | 20.53–20.63s | 9.27s |
| c22b262 | ghost | 427464 / 427960 | 11.88–11.91s | 5.44s |
| c22b262 | **core + ghost** | | **32.4s** | **14.7s** |
| 5214389 (#307 bind) | core | 827978 / 827105 | 15.92–16.10s | 4.35–4.38s |
| 5214389 | ghost | 427464 / 427960 | 9.26–9.35s | 2.54s |
| 5214389 | **core + ghost** | | **25.2s** | **6.9s** |

The c22b262 runs used the same lib and docs (`git archive`) with the
current pscat binary. They reproduce #283's 31.2–32.8s.

### What it means

- **Bind alone moved it.** pscat got 1.29× faster and gs 2.1×. The
  pscat/gs ratio widened from about 2.2× to 3.6×.
- **The margin is 4.8s (16%).** Run-to-run spread is under 1%, so the
  margin is real. But 30s has to hold board dressing too, and D1 left
  about 8s for it. #238 should re-measure the real board (about 2.2M cells
  at `/Weave 0.4`, past `pjmemocap`) before it adds dressing. #324 has the
  levers.
- **pscat's cost is now the interpreter, not drykit.** A `sample` of the
  core run (9270 main-thread samples) shows where it goes:
  - `step_n` (inlined dispatch): 21%
  - `PsArray::get`: 18%. 1363 of these samples are the proc-frame element
    fetch, which clones under a `RefCell` borrow.
  - `Value` drop: 9%
  - `load_id`: 7%
  - `free`: 5%
  - all rasterisation: about 1.5%

  bind took name lookup out, so what remains is per-element fetch,
  clone and drop. That is #323.
- **pscat and gs build slightly different cell sets.** Their unique-cell
  counts differ by about 0.1% (827978 against 827105), which is
  float-rounding at band edges. They were already apart at c22b262.

### Not done

- The reach band per orientation, revisits, the single walk pass and the
  memo cap, and the correctness gaps (the chunk estimate made before
  thinning, and the `gsave` a mid-render budget error leaves open): see
  #324.
- The portrait proxy and the specimen were not re-measured. Both were
  already inside budget before bind.
