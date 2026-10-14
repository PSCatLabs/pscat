## artkit, paintkit, drykit: bind every procedure (issue #307, 2026-10-13)

#307 asked for the drychalk fix (a private scratch dictionary that the
public procedures `begin`/`end`) in drykit and paintkit, because
Ghostscript slows once userdict is nearly full. Measuring first showed
the scratch dictionary does not help there, and `bind` does. The change
is `} bind def` on every top-level procedure in `lib/artkit.ps` (98),
`lib/paintkit.ps` (112) and `lib/drykit.ps` (36), plus
`tests/libbind.rs` to keep them bound. No name was added, removed or
renamed.

### Why the premise did not hold

All numbers are gs 10.07.1, `-dNOSAFER`, CPU `user`+`sys` seconds, the
minimum of 2 to 3 runs on a shared machine (load average 8 to 12, so
trust the ratios, not the digits).

| | before | private dict (prototype) | bind | both |
|---|---|---|---|---|
| paintkit loading demo, 72 dpi | 5.7–7.7 | 8.6–8.8 | 2.9–3.1 | 2.9–3.1 |
| `gallery/alpine_lake.ps`, 144 dpi | 29.8 | 40.1 | 10.2 | |
| alpine_lake, paintkit + drykit bound, artkit not | | | 15.6 | |
| drykit stroke bench (12 strokes) | 11.6 | 10.8 | | 8.1 |

The prototype wrapped every public paintkit preset (and drykit's
`dkstroke`, `dkcatch`, `dksurface`, `dkbatch`) as
`pkscratch begin { body } stopped end { stop } if`. The scratch dicts took
about 500 names out of userdict (888/1012 became 387/450 on the loading
demo) and made gs slower.

A micro-benchmark in gs (loops of 9 million lookups, with 0 to 1000
dummy names in userdict) says why:
- An operator lookup costs about 30% more with a plain dict on top of the
  dict stack than with userdict on top (240 against 170 to 180 ms), and
  most lookups in library code are operators. The private dict pays this
  on every operator and recovers it only on the scratch names.
- A *hit* in a crowded userdict is not steady. One name took 790 ms for
  9 million lookups at 691/1012 and 120 ms at 91/200 and at 1091/1518.
  What hurts is where the hot names fall in the open hash table, not the
  fill ratio. That explains the cliff #235 saw between 802 and 888 names
  (a handful of hot names happened to collide) and why drychalk's private
  dict helped there. A sweep adding 0 to 150 dummy names to the drykit
  bench showed no cliff at all (4.4 to 4.9 s unbound, 3.4 s bound).
- The userdict fill ratio does not predict the cost: the drykit specimen
  ends at 671/675 (99%) and shows none of the cliff.

`bind` removes the lookup. Every executable name in a library procedure
that is an operator when the library loads is replaced by the operator
itself, so the run-time lookup (and the miss through userdict on the way
to systemdict) never happens. Binding the three libraries and leaving
the private dict out, `userdict` fill is unchanged and the time is not.

### Before and after

`userdict` length/capacity at the end of the run, and gs CPU seconds at
144 dpi (paintkit demos at their own page). The fill column is the same
before and after: bind moves no names.

| file | userdict | before | after |
|---|---|---|---|
| `examples/drykit_specimen.ps` | 671/675 | 3.29 | 2.27 |
| `examples/paintkit_loading_demo.ps` | 882/1012 | 5.63 | 2.83 |
| `examples/paintkit_motion_demo.ps` | 1044/1518 | 0.62 | 0.34 |
| `examples/paintkit_wet_demo.ps` | 662/675 | 0.22 | 0.16 |
| `examples/paintkit_wash_demo.ps` | 456/675 | 0.15 | 0.12 |
| `examples/paintkit_oil_demo.ps` | 525/675 | 0.11 | 0.09 |
| `examples/drychalk_specimen.ps` | 514/675 | 4.08 | 2.91 |
| `examples/drycrayon_specimen.ps` | 869/1012 | 5.02 | 3.28 |
| `gallery/alpine_lake.ps` | 987/1012 | 30.01 | 9.13 |

Loading artkit, paintkit and drykit leaves 354 of 450 slots used; the
rest of the fill is the piece's own names and the libraries' runtime
scratch (about 230 `dk*` names after a drykit stroke).

### What bind can change

`bind` freezes an operator at load. An operator redefined *after* a
library loads no longer reaches that library's procedures. Audited:
- no name defined by artkit, paintkit or drykit is in `systemdict` in
  gs or in pscat, so no library procedure is bound to the wrong thing
  by a forward reference;
- nothing tracked redefines an operator that a bound library uses
  (`gallery/ring_of_type.ps` defines a variable `/stop` and
  `gallery/hundred_lines.ps` a variable `/ln`; both render identically,
  and bound libraries are now immune to them);
- `.setfilladjust2` is guarded by `where`: in gs it is bound to the
  operator, and a test prelude that defines a no-op *before* the library
  loads still wins, since `bind` leaves a name alone unless it is an
  operator at that moment (the existing gs test of that still passes);
- library names that are not operators (every `dk*`/`pk*` procedure and
  tunables such as `dkbudget`, `dkslack`) are still looked up late, so
  overriding them works as before.

Packed arrays are not a problem: pscat treats `setpacking` as a no-op,
and gs binds packed procedures.

### Identity

`scripts/identity_check.sh`, base tree at the merge-base 7f2b42e, one
binary, with a control (a driver that redefines `/lineto` after artkit
loads and fills an `ngon`: it differs between the trees, as it must,
which proves each tree loaded its own library):
- `--loader lib/drykit.ps`: 4 of 4 identical (drykit, drychalk,
  drycrayon specimens and the crayon motion demo);
- `--loader lib/paintkit.ps`: 36 of 36 identical across 27 files;
- `--loader lib/artkit.ps --gallery-still`: 82 of 82 identical across 66
  files, and `gallery/alpine_lake.ps` equals its committed still.

### Tests

- `tests/libbind.rs` loads the three libraries, then walks every
  executable array they added to userdict, nested procedures included,
  and fails for any that still holds an executable name resolving to an
  operator. A deliberately unbound control proves the walk can tell. It
  runs in pscat, and in gs when installed (local only).
- `tests/paintkit.rs` located a procedure's end with `"\n} def"`; it now
  looks for `"\n} bind def"`.

### For whoever adds a procedure to these libraries (or builds on drykit)

- Write `} bind def`. A plain `} def` is correct but is what
  `tests/libbind.rs` rejects.
- drykit's names, `dkBatch`, `dkGround`, `dkSurface` and the paintkit
  contract names (`pjCTM pjP pjW pjM pjR pjmemo`) are untouched.
  drycrayon's `userdict /dkBatch` contract still holds.
- drychalk, drycrayon, handscript and the other libraries are not bound.
  drychalk's `dcscratch` stays; binding drychalk and drycrayon is the
  obvious next step (their specimens already gain 30% from the three
  bound libraries beneath them).
- Not tried: shrinking userdict by moving the libraries' *procedures*
  into a private dict. It would change the public contract and the
  catalog, and the numbers above say lookups of operators, not of
  library names, are what the time goes to.
