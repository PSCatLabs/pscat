## gcheck and globaldict (issue #1159, 2026-11-18)

The two Level 2 VM operators next to `setglobal`/`currentglobal` (#1146).
Both were `undefined`, so a prolog probing `/gcheck where` or running
`globaldict begin ... end` stopped.

**Model** (`src/ops/vm.rs`, `Interp::globaldict`). Still one VM, so no
object records where it was allocated. `globaldict` is a real dictionary,
reachable by name from systemdict (like `userdict`), 20 slots in gs, empty at
start, emptied by `Drop for Interp` with the other permanent dicts. `gcheck`
is `true` for simple objects (numbers, booleans, null, mark, names,
operators, files, `save` objects) and for `systemdict` and `globaldict`;
`false` for every other string, array, dictionary and gstate. Measured with
gs 10.07.1 (`tests/gcheck_globaldict.rs` runs the same probes in both).

`restore` never rolls `globaldict` back (`journal_dict` skips it): gs keeps
what was put into it across a `restore`, whichever mode the `save` was
made in (measured both ways).

**Where we differ from gs**
- A composite made while `currentglobal` is true is `true` in gs (`true
  setglobal (abc) false setglobal gcheck`); here it is `false`, because
  nothing is allocated differently. Tracking it would mean a flag on every
  allocation site for a probe whose only common use (`gcheck` before storing
  into a global dict) never matters when there is no `invalidaccess`.
  Pinned in `deviation_a_composite_made_in_global_mode_is_reported_local`.
- gs raises `invalidaccess` when a local composite is stored into
  `globaldict` (`globaldict /a [1] put`); here the store succeeds.
- `globaldict` is not on the dictionary stack, so a name `put` into it is
  not found by a bare lookup (gs: `globaldict /x 1 put x` gives 1) and
  `countdictstack` stays 2 (gs: 3). `globaldict begin ... end` works.
