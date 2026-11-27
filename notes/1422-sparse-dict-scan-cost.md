## A sparse dictionary's retained slots are charged (#1422, 2026-11-26)

Built on `feature/0.8.0`. Codex round 2 on #1410 noted that the backtrace's name
scan (`Dict::name_entries`, `snapshot_error_trace`) iterates a `HashMap`, whose
cost is its bucket count, not its length, so the 65,536-entry scan cap did not
bound it: a program that defines millions of names and removes them leaves a
dictionary whose scan passes over millions of empty slots before every uncaught
error is reported (about 0.28 ms after 4,194,304 entries). The same walk is in
`Dict::pairs`, which the host value copy (`pop_value`, `lookup`) uses.

### What changed

- `Dict::name_overhead()` / `slot_overhead()`: the empty slots a walk passes over
  beyond what a table that only grew has (more than twice its length: such a table
  is at least 7/16 full, so it never has more empty slots than entries). Not
  `capacity() - len()`: removals leave tombstones, and `capacity()` counts only
  free slots, so an emptied table of 458,752 slots read 363,378 in std and
  158,758 with the interner's id hash. `Dict::undef` therefore records the
  largest `capacity()` it saw (a table is tombstone-free just after it grows),
  in `peak_slots`; one compare per `undef`, nothing on `def`.
- The name scan keeps its `NAME_SCAN` entry budget exactly as it was and charges
  the overhead against a second budget of its own (`NAME_SCAN * 4` slots), and,
  when that does not fit, stops there: the frames not yet named stay unnamed. It
  does not skip to an outer dictionary, which would let an outer definition win
  over the innermost. A dictionary that never lost many entries has no overhead,
  so every existing backtrace is named as before. (The first version charged all
  slack from the entry budget; a 60,000-entry dictionary, table 114,688 slots,
  then ate enough of it that a procedure below it went unnamed. Found by Codex
  round 1; `a_dictionary_that_only_grew_leaves_the_entry_budget_as_it_was`.)
- The host copy (`Walk::scan`) charges `slack` in steps (4 slots to a node, 1,024
  nodes to a step), not in nodes, so `MAX_VALUE_NODES` and the `limitcheck` of
  a small dictionary are unchanged; the step, time, memory and interrupt looks
  see it.

- A copy may pass over at most `64 * MAX_VALUE_NODES` (67 million) empty slots,
  then it is `limitcheck`. Slots are steps, which bound a run but not a copy
  between runs, and a sparse dictionary that holds itself twice is copied once
  per path to it (up to the node cap, about 500,000 visits), each visit walking
  the whole table. No real dictionary keeps that many slots.
- What a reader of an error report sees: the scan stops at the first dictionary
  (innermost first) whose retained slots do not fit the budget, so every
  procedure that lives in that dictionary or in one below it on the stack is
  reported as `in a procedure`, not only the stale dictionary's own. The
  alternative, skipping that dictionary and scanning the rest, would let an
  outer definition win over the innermost one.

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
thousand names). All four fail on the base commit; the last (the cap) fails on
CPU time, 12.1 s there against 0.009 s here, since the refusal is `limitcheck` on
both.

Identity: two-binary `scripts/identity_check.sh --loader lib/artkit.ps`, 127 of
127 pages identical. No render changes, so `--control` has nothing to differ on;
the control was a tracked crafted error file in the corpus, which reports `in p`
on the base binary and `in a procedure` on the head binary.
