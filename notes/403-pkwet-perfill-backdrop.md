## pkwet /PerFill: the backdrop asked at each loaded fill (issue #403, 2026-10-14)

#179 asked a procedural `/Under` once per softening pass; #359 asked it once per
cell of a clipped grid. `/PerFill true` (beside `/At`) asks it at each
loaded-colour fill's own position instead: no clipping, no replayed random
stream, one run of the mark per pass.

Design calls, and why:

- **Opt-in flag, `/At` kept.** Existing calls that pair a loaded-colour mark
  with a procedural `/Under` already render (and are in the gallery) with the
  per-pass answer, so the new behaviour is a key, not a change of meaning.
  `/At` stays required: an inherited-colour mark has no per-fill site and is
  still asked per pass, at `/At`. Rejected: `/PerFill` with a literal `/Under`,
  with `/Box` (a banded call already gives each cell an answer; combining them
  has no meaning), a non-boolean, a procedure.
- **A fill is an inked run, asked at its middle stop.** `ptemit`, `pmemit`,
  `pmstamp`, `pfrun` and `pfray` set one colour per run, so that is the
  granularity (the middle stop, since a run's first stop would bias a long run
  toward its start). The three `*color` procs gained a second argument (the
  run's last stop) and pass the stop's x y on to `puwetat`; the first argument
  and the flip lookup are untouched. Splitting a run into several fills would be
  the next step (the design's tip-load slice needs it too) and is not here: a
  long unbroken run (a vertical stroke down a vertical gradient) is asked once.
- **`puwetat` is `puwet` plus a position.** A depth without `/PerFill`, and the
  core pass, take the old code (core: factor 0, skipped), so every existing
  render is unchanged. pkoil's ridge colours still call plain `puwet`, which
  reads the per-pass answers (slot 7 is still filled in a `/PerFill` call).
- **Frame slot 9** holds `[ proc, CTM at entry ]` for a `/PerFill` call, null
  otherwise. Both are copied into the frame, never read from `pqUnderProc`,
  which a nested `pkwet` overwrites before the outer pass's fills run. The point
  is `transform`ed to device space with the current CTM and taken back with the
  saved matrix, so it is in the user space pkwet was called in, the pass's
  displacement is included and any CTM change inside the mark is too.
- **Own asker, no depth rollback.** `pqaskunder` rolls `pqdepth` back on every
  error because it runs before the passes, outside any `stopped`. A fill is asked
  inside a pass, whose `stopped` already rolls its level back; a second rollback
  would count it twice. `puaskfill` keeps the same checks and error names and
  never touches the depth. Tested at one level and from the inner call of a nest.

Measured (`loaded_column`: twelve stacked `pkbroad` strokes with a loaded cream
colour over the 150pt gradient, /Offset 34, 6 layers, rows 15..135 of
`rim_residual`): per pass 384.4, 1x4 bands 195.6, 1x8 bands 116.9 (41 runs of
the mark), `/PerFill` 81.2 (6 runs). The issue's `graded_column` is a spray (an
inherited-colour mark), which `/PerFill` cannot reach, so its 186.7 / 82.9 are
not comparable and the new mark was used.

Byte identity: `scripts/identity_check.sh` renders every file that loads
`lib/paintkit.ps` identically on the default pick and with `--loader
lib/paintkit.ps`, and a control that opts in (`/PerFill true`) differs as it
must. No committed render changed.

Deferred: adopting `/PerFill` in a gallery piece (#491), and splitting a long
run into per-stop fills so one unbroken stroke follows the backdrop too (#492).
