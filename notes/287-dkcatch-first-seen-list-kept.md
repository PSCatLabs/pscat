## dkcatch keeps its first-seen list (issue #287, 2026-10-13)

Question: #277 (PR #290) made pscat's dict `forall` visit non-name keys in
insertion order. Can `dkcatch` (`lib/drykit.ps`) drop the first-seen cell list
(`dkkeybuf`) that `dkemit` walks, and `forall` over `dkseen` instead?

**Answer: no. The list stays; only a comment changed.**

What the list is. `dkseen` maps a cell key to its slot; `dkdistbuf` and
`dksegbuf` are indexed by slot, `dkkeybuf` holds the key per slot in first-seen
order, and `dkemit` loops `0 1 dkn 1 sub`. `dkseen` is never `forall`'d today.
Dropping the list means `dkseen { slot-> /dkq, key-> /dkkey } forall` in
`dkemit`, i.e. the emit order becomes whatever the interpreter's `forall` does.

Why that matters: drykit must also run in Ghostscript, and Ghostscript's
`forall` over integer keys is hash order, not insertion order. Probe
(`8 keys 5 32768 100000 7 -3 65539 1 99`): gs visits
`100000 -3 1 65539 99 5 7 32768`, the same on every run. So gs does not vary
run to run (the issue's original pscat symptom does not occur there), but it
differs from pscat's order, and a PLRM-conforming `forall` is free to differ.
With the list, the emit order is the algorithm's own on every interpreter.

Measured, variant = list removed, `dkemit` iterating `dkseen`
(`examples/drykit_specimen.ps` at `--page 840x1000 --dpi 144`):

| Output | with list vs without |
|---|---|
| pscat PNG, SVG (86 MB), PDF (28 MB) | byte-identical (cmp) |
| gs `png16m` (two runs each) | identical, all four md5-equal; also equal across variants |
| gs `pdfwrite` | **differs**: 20,108,299 / 20,110,281 bytes with the list (two runs), 21,544,066 / 21,545,829 without |
| `cargo test --release --test drykit` (18 tests, incl. both gs ones) | identical: 18 pass, gs stroke block diff 0.37, gs ink 0.550 vs pscat 0.582 |

The run-to-run wobble in gs pdfwrite is about 2 KB (dates and ids); the gap
between variants is about 1.4 MB, stable. Cause: the clip's subpaths are
emitted in a different order, which pdfwrite encodes and flattens differently.
Rasters agree because a union of ellipses does not depend on order.

So removing the list is invisible in pscat (as #290 intended) and in gs
rasters, but it makes gs's vector output diverge from pscat's and from the
current gs output, and it ties the clip order to an unspecified `forall`
behaviour. The memory saved is one array of at most `dkbudget` (150000)
integers, reused between calls. The simplification is not worth that, so the
issue is closed as not planned.

Change shipped: a comment above `dkemit` in `lib/drykit.ps` giving this reason,
so the next reader does not try the same removal. No code change; the specimen
renders byte-identically (cmp against the pre-change PNG), which matters
because `lib/drychalk.ps` (#235) builds on this file.
