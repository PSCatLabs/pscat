## `bind` makes nested procedures read-only (issue #1189, 2026-11-12)

The issue said gs makes the procedures `bind` binds read-only and pscat left
them writable: `/p { 1 add } bind def /p load wcheck` was supposed to be
`false` in gs. It is not. Ghostscript 10.07.1:

```
$ gs -q -dNODISPLAY -dNOSAFER -dBATCH -c '/p { 1 add } bind def /p load wcheck ='
true
$ ... '/p { { 1 add } } bind def /p load wcheck = /p load 0 get wcheck ='
true false
$ ... '/p { { 1 add } } bind def /p load 0 get 0 1 put'
Error: /invalidaccess in --put--
$ ... '/p { 1 add } bind def /p load 0 1 put (ok) ='
ok
```

The procedure given to `bind` keeps the access it had. Access belongs to the
reference, not the storage; `bind` walks the slots of that procedure and makes
each slot that holds an executable array read-only, then walks into it. The `bind` entry (a mirror of the Level 2 reference,
<https://hepunx.rl.ac.uk/~adye/psdocs/ref/PSL2b.html>, found by search; no copy
is checked in, so the wording is as that page gives it) says "For each
procedure object in proc, bind applies itself recursively to that procedure,
makes the procedure read-only, and stores it back into proc", and that bind
ignores a read-only array, "that is, it will neither bind elements of the
array nor examine nested procedures", while packed arrays are processed
whatever their access. gs 10.07.1 agrees on all three.

### The model

pscat did not model access at all: `readonly`/`noaccess`/`executeonly` were
identities and `wcheck` was always `true`. Now `Object` carries a `readonly`
flag, set only on arrays, beside `executable` (it lives in padding:
`size_of::<Object>()` is unchanged, and a test pins that).

- Set by `readonly`, `executeonly`, `noaccess`, `packedarray`, and by `bind`
  on every nested executable array slot it enters.
- Seen by `wcheck`; enforced by `put`, `putinterval` (destination), `astore`,
  `copy` (destination array) and the matrix writers (`identmatrix`,
  `currentmatrix`, `defaultmatrix`, `concatmatrix`, `invertmatrix`, and
  `translate`/`scale`/`rotate` given a matrix): `invalidaccess` (new `PsError::InvalidAccess`
  and an `errordict` handler).
- Carried by `dup`, `getinterval`, `cvx`, `cvlit` and `load`. A fresh array
  (`array`, `copy`'s result view) is writable. `eq` ignores it.
- A second flag, `packed`, rides beside `readonly` (`packedarray` sets both):
  `bind` still enters a packed procedure and the packed procedures nested in
  it, as gs and the reference do, though they are read-only.
- `bind` on a read-only (not packed) procedure is a no-op: names stay names, nested
  procedures stay writable (gs: `{ add } readonly bind 0 get type =` is
  `nametype`).
- A nested slot that is already read-only is not entered unless this same
  walk restricted that slot (`BindWalk::restricted`, by storage address and
  index). The `seen` map (#1176) keeps bind linear; a re-walk of a view at a
  shallower depth must still go into slots the first walk flagged, or the
  depth-100 cut-off would leave their contents unbound (found by Codex: a
  leaf reached at depth 101 first and at depth 2 second stayed unbound; gs
  prints 7 for the case in `tests/access.rs`).
- A procedure literal in a running procedure is pushed with the slot's
  attributes (`proc_literal`), so `p wcheck` after `/p { { } } bind def` is
  `false`, as in gs.
- `restore` brings the flag back with the array contents (`bind` journals the
  array before it writes).

### Checks

- `tests/access.rs`: 43 cases pinned from gs 10.07.1 and run through gs too
  (top level writable, nested read-only, two deep, inside `if`, a slot shared
  twice, the dictionary's own reference still writable, `getinterval`/`cvlit`/
  `aload`, already-read-only procedures, `packedarray`, the four writers,
  `stopped` + `$error`), plus restore and the size.
- `selftest.sh` clean; `scripts/identity_check.sh` with a base and a head binary
  over every tracked gallery, example and selftest-driver file: 207 page renders
  identical across 168 files (4 files fail identically in both trees; the control
  differs as it must). Nothing in `lib/` writes into a bound procedure.

### Not done (deviations, kept in HANDOFF)

- Reads are never refused: `rcheck` is always true, `get` on a `noaccess` or
  `executeonly` array works.
- Strings, dictionaries and files have no access attribute.
- `wcheck` on a name or operator answers `true` (gs: `typecheck`).
- Procedures scanned under `true setpacking` are writable (gs: read-only
  packed arrays). `packedarray` itself is read-only.
