## Procedure element fetch and `def` without re-intern (issue #323, 2026-10-13)

This is the first slice of #323, scoped down when perf work was
deprioritised. #330 holds the rest at p3.

### What the profile said

A macOS `sample` of the drykit board proxy core run on the base binary
(bb49d21) took 7780 main-thread samples. The proxy is
`docs/drymedia/prototypes/surface_cost.ps` with
`/MODE 6 /LEVELS 2 /TOUCH 0.82 /PEN 36 /GRAIN 0.4`, at 144 dpi. Two causes
stood out:

- **`def` re-interned its key.** `def` popped a `PsName`, passed it as a
  `&str` to `Interp::define`, and `Dict::put(name.into())` then allocated
  an `Rc<str>` and interned it again: a SipHash probe, a memcmp, and a
  free. That chain cost about 1130 samples (15%). It was nearly all of
  the `free` and SipHash time. #323 had blamed an id-keyed map's hasher;
  the SipHash was the interner's string map, reached only through this
  detour.
- **The procedure fetch cloned every element.** `next_item`'s
  `body.get(pc)` cost about 1180 samples (15%). For an executable name,
  the clone also touched the name's `Rc<str>` refcount, and the drop
  touched it again.

### What changed

- `def` calls a new `Interp::define_name(&PsName, ..)`, which uses
  `Dict::put_name`, and `load` uses `load_id`. Neither allocates or
  interns.
- `next_item` returns an `Item`. The Proc arm inspects the element under
  one momentary borrow (`PsArray::peek`). An executable operator becomes
  `Item::Op(fn)`, an executable name becomes `Item::Name(id)`, and only
  the other elements are cloned. The borrow ends before anything
  executes, so a `put` into the running procedure is still seen by the
  same run. Recursion and `save`/`restore` are unchanged, because the
  fetch is still live, one element per step. `execute_resolved` moves the
  procedure body out of the resolved object instead of cloning it.

### Numbers

These are user CPU seconds over 7 interleaved rounds of base, an
identical-copy control, and head. The machine was loaded, so the medians
are noisy; the control's medians stay within 0 to 7% of base.

| Workload | base min / median | head min / median | median change |
|---|---|---|---|
| drykit proxy (core) | 18.58 / 23.60 | 11.86 / 15.93 | -32% |
| fib 30 | 0.71 / 0.78 | 0.45 / 0.64 | -18% |
| defloop 1M | 0.12 / 0.14 | 0.07 / 0.08 | -43% |
| fern | 0.28 / 0.37 | 0.20 / 0.27 | -27% |

On a quiet machine, one earlier run measured the proxy at 16.2 s (base),
13.4 s (`def` fix only) and 10.75 s (both changes).

### Correctness

All 107 tracked `examples/`, `gallery/` and `selftest/drivers/` files
were rendered as 128 pages, plus the `alpine_lake` still at 2x. Each page
is byte-identical between the base binary and the branch binary. The
check was `scripts/identity_check.sh --files` with a wrapper that picks
the binary by tree; its log shows both binaries were used, 107 runs each.
New unit tests in `src/interp.rs` cover a `put` into a running
procedure, self-recursion through a name and through `exec`, `def`
rolled back by `restore`, a string-keyed `def`, and an undefined name's
`OffendingCommand`.

### Left (#330)

After this change, `step_n` (with the inlined fetch) takes about 35% of
the samples, `load_id` about 10% and `execute_resolved` about 8%.
