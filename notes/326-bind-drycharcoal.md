## Bind drycharcoal (issue #326, 2026-10-14)

#318 bound drychalk and left `lib/drycharcoal.ps` for after PR #315 merged.
All 22 top-level procedures are now `} bind def`; `tests/libbind.rs`
checks the library (and loads `lib/hatchkit.ps` before its snapshot, since
drycharcoal needs it and hatchkit is unbound). The gain is small, and
smaller than the 7% drychalk got, and by #318's own rule (skip when the gain
is under noise) it is borderline: user CPU shows no gain on the specimen
(19.08 s against 19.10 s). It is bound because the task asked for the #318
recipe and the instruction counts show a small, repeatable gain. The costs
are that a caller's later redefinition of an operator no longer reaches
drycharcoal's procedures, and that new procedures must be written bound.
No render changes. Easy to revert if that trade is not wanted.

### Measurement

gs 10.07.1, `-dNOSAFER -sDEVICE=png16m -r144`, base tree against bound
tree, one binary per renderer. The machine was shared (load average 7 to
10), so wall and `user` seconds wandered by 5% to 10% from one run to the
next, with the bound tree sometimes slower. `/usr/bin/time -l` also prints
"instructions retired", which does not wander, so that is the number to
trust.

| workload | user s, base / bound (best of 3 specimen, one Lincoln pair) | instructions retired, base / bound |
|---|---|---|
| gs, `examples/drycharcoal_specimen.ps` | 19.08 / 19.10 (3 runs each, alternated) | 471.7e9 / 470.0e9 (-0.4%), again 471.7e9 / 469.9e9 |
| gs, `gallery/lincoln_charcoal.ps` (PR #343) | 57.24 / 55.20 | 1326e9 / 1287e9 (-3.0%) |
| pscat, `examples/drycharcoal_specimen.ps` | 6.79 / 6.77 (best of 3) | not taken |
| pscat, `gallery/lincoln_charcoal.ps` | 34.81 / 34.52 | 614.9e9 / 611.2e9 (-0.6%) |
| pscat, `cargo test --test drycharcoal` (11 tests; same test binary, each tree's libs) | 10.47 / 10.54, 10.53 / 10.43 | 159.6e9 / 159.1e9, 159.5e9 / 159.1e9 (-0.3%) |

(The specimen needs 19 s in gs here against the 10 s budget in its header;
the machine was loaded, and both trees are equally slow. Not looked into.)

The specimen is dominated by drykit's strokes, which #307 bound already, so
drycharcoal's own procedures are a sliver of it. The Lincoln portrait runs
`dhhatch` over thousands of short hatch lines and gains 3% in gs, from one
pair of runs; pscat gains 0.6% on it. All numbers were taken against the
base before #312 landed (main `37ea900`/`cf83ade`); they are relative, and
#312 does not touch what is bound.

### Audit

- No name drycharcoal defines is in `systemdict` in gs or pscat, so no
  procedure binds a forward reference to the wrong thing.
- Tracked files that load drycharcoal (checked on the tree after #312):
  `examples/drycharcoal_specimen.ps`, `tests/drycharcoal.rs` and
  `docs/drymedia/prototypes/dkclip_layering.ps`. None redefines an operator
  after it loads. The self-tests call `srand`/`rand` on the caller's side to prove
  the library draws nothing; they do not redefine them, and bound
  procedures never call them (they use drykit's `dksrand`/`dkrand`).
- No procedure is meant to be overridden. `/Value`, `/Under` and the
  `/Mark` hook are arguments, not library names.
- Error names such as `dhstroke-options-must-be-a-dict`, and
  `dherr`'s `cvn cvx exec`, are not operators, so bind leaves them late-bound.

### Identity (pscat)

`scripts/identity_check.sh`, base tree at the merge-base, one binary, with a
control that redefines `lt` after drycharcoal loads and calls `dhmaxr`
(it differs between the trees, as it must): `--loader lib/drykit.ps --loader
lib/drycharcoal.ps`, 6 of 6 identical (the drychalk, drycharcoal, drycrayon,
drycrayon motion, drykit and mathkit specimens), after rebasing onto #312 (main `9f007f1`); and the one prototype that loads
drycharcoal, `docs/drymedia/prototypes/dkclip_layering.ps` (committed
`docs/drymedia/dkclip_layering.png`), 1 of 1 identical with the same control.

`gallery/lincoln_charcoal.ps` (PR #343, not on main yet) renders
byte-identical in pscat and in gs (png16m, 960x1200 at 144 dpi) with main's `lib/` and with this branch's,
given PR #343's `lib/etching.ps` in both trees (the piece needs its
`et-tonemap`; #343 changes no other lib).

### Tests

`tests/libbind.rs` loads drycharcoal after drychalk. A temporarily unbound
`dhmaxr` makes both the pscat and the gs test fail.
