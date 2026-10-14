# 413: raise the remaining 2 s CPU budgets to 5 s

The drykit/drychalk/drycharcoal/drycrayon/pktrowelplane CI budget smoke
tests used a 2 s thread-CPU bar. drychalk failed at 2.46 s under load
(thread CPU time, best of 3). The bar is now 5 s, matching the other
`within_cpu_budget` callers (mathkit, paintkit's 5 s budgets).

Changed: the five `Duration::from_secs(2)` budgets, their doc comments,
and DRYMEDIA.md §6.2 (per-file row) and §8 item 8. Historical
measurements (for example "measured 2.4 s") are left as recorded.
No render changed.

`tests/paintkit.rs` has a 2 s budget too; it is outside #413's list and
was not touched.
