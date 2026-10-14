## Dict forall order is deterministic (issue #277, 2026-10-13)

`Dict::exotic`, the map for non-name keys (integers, reals, booleans, array,
dict, operator, file and save keys), was a `HashMap` with std's randomly
seeded `RandomState`, so `forall`/`copy` visited those entries in a different
order every run. Pointer-keyed entries add a second source of instability
(ASLR), so a fixed-seed hasher alone would not have fixed array and dict keys.

- **Fix.** Each exotic entry now carries a sequence number (`Dict::next_seq`,
  taken on first insertion) and `Dict::pairs` sorts the exotic half by it.
  Lookups stay O(1) on the same map; nothing new is added to the dependency
  list (`indexmap` is only a transitive dependency, and it would also need
  O(n) `shift_remove` to keep order on `undef`). Redefining a key keeps its
  slot (the stored key object is replaced, as before); `undef` then `put`
  moves it to the end. `Dict` is `Clone`, so save/restore snapshots and dict
  `copy` keep the order. Fewer than two exotic entries skip the sort.
- **Order across the two key kinds.** `pairs()` lists name keys first, then
  the rest in insertion order. Names stay in the id-hashed map (a one-multiply
  fixed hasher, so that order is already identical from run to run for a given
  program). Giving names and exotics one shared insertion order would put a
  sequence number on every name entry and a sort on every `forall` over
  systemdict and the font dicts, which is the hot path, for no determinism
  gain. Not done.
- **Cost.** `put_obj` has a name fast path now, with the non-name half in an
  out-of-line `put_other`. The first version (match on `classify_key` in
  `put_obj` itself) made `defloop` measurably slower (about 15-20% on a 2M-store
  loop) by changing what the `def` path inlined; the fast path removed that.
  Release A/B against the pre-change binary, 15 interleaved runs of
  `benches/regression.rs`: fib 27 130 -> 133 ms (+2.5%), defloop 200k 27 -> 27 ms
  (+2.1%), sierpinski 5 -> 5 ms, fern 253 -> 261 ms (+2.9%), peak RSS unchanged
  within 2%. A micro-benchmark that does touch exotic keys (200k int `put`,
  200k `get`, 200 `forall`s over 1009 entries) ran 78 -> 82 ms (+5%); 2000
  `forall`s over a 1000-entry int dict ran 137 -> 143 ms (+4%, the sort).
- **Other hash containers audited.** Every other `HashMap`/`HashSet` in `src/`
  is lookup-only or sorted before use: `gfx.rs` boundary-edge map (its keys are
  sorted before iteration), `svg.rs` `clip_ids`, `font.rs` membership sets,
  `interp.rs` journal `seen` sets, `name.rs` `by_text`, `ops/level2.rs`
  resource `CategoryMap` (get-or-insert only), `spool.rs` (a `retain`, whose
  outcome does not depend on order). None reaches output order.
- **Follow-up.** `lib/drykit.ps` `dkcatch` keeps its first-seen list as a
  workaround; it can be simplified now (issue filed, `lib/` not touched here).
