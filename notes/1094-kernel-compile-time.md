## Compile time of large kernels (issue #1094, 2026-11-08)

fractal_clouds' scenes compiled in 5–9 s at 160×100 against a kernel run of
about 0.4 s, so at preview sizes the compile was most of the gain lost.

### Where the time went

A `sample` profile of golden_coast's and evening_lake's compiles put 85–90%
of it in two join helpers:

- `slots_of` built the list of slots to compare with `Vec::contains`;
- `slot_val` found each one by scanning the scope's names.

The pixel dictionary holds a few thousand names, and every `if` join and
every loop fixpoint round compared all of them, so each join was quadratic.
That cost fell on both `/auto` passes in proportion to their size, so the
inlined first pass was about 84% of the compile. The separable costs the
issue listed (sharing keys, `prune_subs` and DCE, register allocation) were
about 1% each; rolled-back branches and repeated fixpoint rounds cost
mostly through their joins.

### What changed

Nothing about what compiles: the IR dumps of the five scenes are identical
before and after, and every render is `cmp`-identical to `/interpret`.

- **Indexed scopes.** A scope's names keep an id index beside their
  definition order, so a lookup or `def` is O(1). A join builds its slot list
  in the same order as before (that order numbers the merged registers):
  the names of the first state, then the new ones of each later state,
  found past the prefix the states share. It compares most bindings in
  place, and copies a value out only when a sharing frame needs it.
- **Shared scopes.** The names are an `Rc`, copied on the first write after
  a clone. Every branch, checkpoint and shared body clones the state, and
  most change few names.
- **One register allocation.** Under `/auto`, the first pass's kernel is
  usually replaced by the shared retry, so allocation now runs only on the
  kernel returned.

`joins_over_many_names_compile_in_linear_time` (2,000 names, 2,000 joins,
under `/auto` and `/large`) takes 7.7 s on the old compiler and about 70 ms
now, within a 400 ms CPU budget.

### Numbers

`kernelimage` compile time at 160×100 on an M4 under load, release build,
median of 3 runs interleaved with the base (`feature/compute-kernels` at
b3ab0af). Every render is `cmp`-identical to `/interpret`.

| scene | before | after |
|---|---:|---:|
| golden_coast | 8,605 ms | 622 ms |
| evening_lake | 5,503 ms | 645 ms |
| pnw_coast | 5,289 ms | 418 ms |
| tropical_island | 7,542 ms | 545 ms |
| mountains | 91 ms | 29 ms |

The kernel gallery pieces (`--dpi 144`, still byte-identical to
`gallery/renders`) compile in a few milliseconds either way: Mirror Garden
9.0 → 3.6 ms, Mandelbulb 4.6 → 2.8 ms, Dreaming Network 0.29 → 0.27 ms, Mochi
in Oils 4.8 → 4.7 ms.

### Left as they are

- The inlined first pass under `/auto` still runs in full (about two thirds
  of what is left). It decides whether the shared retry runs, its kernel can
  win (#1091), and its work is charged as steps, so skipping it on an
  estimate would change results.
- What remains is spread thin: state clones on write, the per-slot compare
  at joins, DCE.
