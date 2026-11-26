## A sparse dictionary's retained slots are charged (#1422, 2026-11-26)

Built on `feature/0.8.0`. Codex round 2 on #1410 noted that the backtrace's name
scan (`Dict::name_entries`, `snapshot_error_trace`) iterates a `HashMap`, whose
cost is its bucket count, not its length, so the 65,536-entry scan cap did not
bound it: a program that defines millions of names and removes them leaves a
dictionary whose scan passes over millions of empty slots before every uncaught
error is reported (about 0.28 ms after 4,194,304 entries). The same walk is in
`Dict::pairs`, which the host value copy (`pop_value`, `lookup`) uses.

### What changed

- `Dict::slack()` / `name_slack()`: the empty slots a walk passes over. Not
  `capacity() - len()`: removals leave tombstones, and `capacity()` counts only
  free slots, so an emptied table of 458,752 slots read 363,378 in std and
  158,758 with the interner's id hash. `Dict::undef` therefore records the
  largest `capacity()` it saw (a table is tombstone-free just after it grows),
  in `peak_slots`; one compare per `undef`, nothing on `def`.
- The name scan charges `slack / 4` against its `NAME_SCAN` budget per
  dictionary (four slots to an entry visit, an overcharge) and, when that does
  not fit, stops there: the frames not yet named stay unnamed. It does not skip
  to an outer dictionary, which would let an outer definition win over the
  innermost. A dictionary that only grew costs at most a quarter of its length
  extra, so every existing backtrace is named as before.
- The host copy (`Walk::scan`) charges `slack` in steps (4 slots to a node, 1,024
  nodes to a step), not in nodes, so `MAX_VALUE_NODES` and the `limitcheck` of
  a small dictionary are unchanged; the step, time, memory and interrupt looks
  see it.

### The `pairs()` allocation question

`Value::from_object` takes `len` nodes and `len * (size of an entry's copy)` bytes
from the `Walk` *before* it calls `pairs()`, and `pairs()` allocates one vector of
`len` pairs. So the allocation is seen first; only the walk over empty slots was
not. That is what is charged now.

### Not done

- Shrinking the table in `undef`: it would fix every walk at once, but the
  table size sets the iteration order of name keys, so `forall` and `copy` output
  would change for programs that remove most of a large dictionary.
- `forall` / `copy` of a sparse dictionary still pass over the empty slots
  uncharged (steps stand for iterations there): a follow-up.

Tests: `tests/host_sparse_dict_1422.rs` (its own binary: it interns a few hundred
thousand names). All three fail on the base commit.
