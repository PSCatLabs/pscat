# #868: the band preamble no longer shifts the program's name ids

Found during #844 (`notes/844-bands-work-queue.md`, "Left"). `--bands` runs
`/BandIndex .. def /BandCount .. def /BandBBox [..] def` before the program.
Name ids are handed out in interning order, and `forall` over a dict's name
keys follows the ids' hash order (`Dict::pairs`). So in a banded run the
three band names took three ids that a plain run gives the program's own
names, and a name-keyed dict could come out of `forall` in another order:
`/early pop /BandIndex pop`, then a 40-key dict painted in `forall` order,
differed between a plain render and `--bands 1` (now
`tests/bands.rs` `band_names_do_not_shift_the_programs_name_ids`, which fails
without the fix). A program that never mentions the band names was shifted
by 3 too; that case happened to keep its order (a shift of 3 rotates the
table by one bucket) but is in the test as a guard.

## Fix: detached interning (`src/name.rs`)

`name::detached(f)` runs `f` with new names taking ids from a separate range
(from `1 << 31`, capped at 65,536 names, well below the `u32::MAX` sentinels in
`interp.rs` and `profile.rs`) and no sequential id. Such a name is
*pending* until the program itself first interns it (scanner, `cvn`, any
`intern`/`intern_rc`): then the interner pushes a placeholder into the
sequential table, at the slot a plain run would have used for that name
there, and keeps returning the detached id. Every later name then gets the
same id as in a plain run. `lookup` and `resolve` never take the slot (they
are not the program interning a name). The mode flag is restored by a drop
guard, so an error or panic inside the preamble can't leave it on.
`bands::run_band` wraps the preamble's `run_source` in it. That's the only
change in `bands.rs`.

Plain renders never enter the mode, so their ids are unchanged. The cost on
the hot path is one `id < DETACHED_BASE` compare on an interner hit.

### Rejected

- **Intern the band names at a fixed early point in every run.** Simplest,
  but every plain render's program names would move by 3 ids, which changes
  `forall` order for name-keyed dicts in plain output. That breaks the issue's
  hard constraint, even if the corpus happens to render the same.
- **Make dict order independent of ids** (sort, or insertion order). Changes
  plain `forall` order everywhere, and costs on the hot path.

## What is still not identical

The band names' own ids differ from a plain run (detached vs sequential), so
a program that puts a band name into its own dict as a key and `forall`s it,
or `forall`s userdict, can still see another order. The userdict case
differs anyway (a banded userdict holds the band entries; see the HANDOFF
note on hashing userdict).

## Other per-run preambles

- `--sweep-seed` has no preamble (`Interp::set_seed_override`): unaffected.
- `--sweep NAME=...` runs `/NAME value def` before each frame (`main.rs`
  `run_sweep`), the same class of bug against a standalone `-D`-less run.
  Wrapping it in `name::detached` would fix it. Left as a follow-up, out of
  this lane.
- Sweep frames also run one after another on one thread, so names a frame
  creates (`cvn`) are already interned for the next frame, which can shift
  its ids against a standalone run of that seed or value. Bands avoid this
  with a fresh thread per band (#844). Also a follow-up.

## Verification

`scripts/identity_check.sh` with two binaries (base = main at 457cbfc, head =
this branch), `--loader lib/artkit.ps` plus every `examples/*.ps`: 150 of 150
page renders across 118 files identical, plain, `--bands 2` and `--bands 4`.
The plain run has no negative control (plain behaviour is unchanged by
design, so nothing can differ); the banded runs use the repro as the
control, and it differs between the binaries, as it must. Banded-equals-plain
is checked by `tests/bands.rs`.
