## gcheck and globaldict (issue #1159, 2026-11-18)

The two Level 2 VM operators next to `setglobal`/`currentglobal` (#1146).
Both were `undefined`, so a prolog probing `/gcheck where` or running
`globaldict begin ... end` stopped.

**Model** (`src/ops/vm.rs`, `Interp::globaldict`). Still one VM, so no
object records where it was allocated. `globaldict` is a real dictionary,
reachable by name from systemdict (like `userdict`), empty at start, and on
the dictionary stack between systemdict and userdict as in gs: `globaldict
/x 1 put x` and `globaldict begin /P {..} def end P` work, and
`countdictstack` starts at 3 (was 2; `PERMANENT_DICTS` in `src/interp.rs`
replaces the literal 2s, tests that pinned 2 now say 3). It is emptied by
`Drop for Interp` with the other permanent dicts. Name lookup walks one
more (empty) dictionary for a systemdict name. `gcheck`
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
- A local array stored in `globaldict` stays rolled back by `restore`
  (only globaldict's own slots are exempt); gs refuses the store.
