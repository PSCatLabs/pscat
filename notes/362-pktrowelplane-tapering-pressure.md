## pktrowelplane row spacing follows a tapering /Pressure (issue #362, 2026-10-14)

`pktrowelplane` (#180) spaced its rows for the nominal band `F = Width *
cos(Angle) * (0.35 + 0.65 * Cover)`. A `/Pressure` that tapers each pull
narrows the band below F, so the rows left bare gaps between them and the
caller had to raise `/Overlap` by hand.

What changed (`lib/paintkit.ps`, `pktrowelplane`): when `/Pressure` is in the
options, the plane reads it at 17 points of the stretch of the row that lies
inside the area, takes the smallest `|value|` P, and spaces the rows for
`Fe = F * P`. The default `/RowJitter` and `/AngleJitter` use `Fe` as well (they
exist to keep neighbouring rows from opening a seam, which depends on the
real band). No new key.

Decisions, and why:

- **Which part of the row.** `pktrowel` evaluates `/Pressure` at the row's own
  progress t over the whole overrun row. The overrun at both ends lies outside
  the area, and a `pktaper` is 0 at t=0, so reading `[0,1]` would let the
  overrun set the spacing. The window is `[m/(2hl), 1 - m/(2hl)]` (`tplm`,
  `tplhl` are the overrun and the row's half length).
- **Minimum, not mean (the issue allowed either).** Rows spaced for the mean
  still leave gaps wherever the profile is below the mean. Three variants of
  `pktaper`, `pkbell` and a mild `0.6..1` profile were rendered side by side
  (the old spacing; minimum floored at 0.25; mean floored at 0.25; minimum
  floored at 0.5): the minimum with floor 0.25 is the one that closes the
  plane on the two deep profiles; mean and the 0.5 floor still show spikes at
  the thin end.
- **Floor 0.25, cap 1.** `pktaper` and `pkbell` reach about 0 inside the
  window, so without a floor the spacing would collapse to 0 and every such
  call would hit the 4000-row guard. At the floor a deep taper costs at most
  four times the rows (measured: a 90x90 `pkbell` plane 0.04 s to 0.08 s CPU; the 300x300 budget plane with `/Pressure { pkbell }` 0.22 s to 0.70 s CPU, 2 s budget)
  and still keeps a thin end, as a real knife does; the deepest taper is not
  fully closed by design. The cap at 1: a pressure above 1 only overlaps
  more, it never spreads the rows (and a flat `{ pkflat }` or no `/Pressure`
  is byte-identical to before: same code path, same operations).
- **Randomness.** The caller's proc may draw (`{ frnd }` is a legal profile),
  and `pktrowelplane`'s stream is "two draws per row". The sampling saves
  `rrand`, and calls `srand` to put it back only if the proc moved it, so a
  deterministic profile never calls `srand`. Under a seed-override sweep
  (`src/ops/arith.rs`: `srand` sets the forced seed, ignoring its operand) the
  restore for a proc that draws rewinds the stream to the forced seed rather
  than to where it was, the same as every other lib that uses this
  `rrand`/`srand` pattern. An error inside the proc skips the restore (no
  `finally` in PostScript; `scatter` documents the same).
- **Where it runs.** After the empty-path `pktrowel` validation (a bad
  `/Pressure` still raises from `pktrowel`'s own guard) and before the row
  `gsave`, so an error or a `stop` inside the sampled proc cannot leave a clip
  or rotation behind.
- Determinism doc: a tapering `/Pressure` now changes the row count, so it
  belongs in the list of things that re-roll the plane.

Ghostscript 10.07 runs the new branch (`rrand`, `abs`, `max` in the `for`) and draws a solid square for a tapered plane.

Limits: a dip narrower than 1/16 of the row can be missed; the proc is read
in the caller's CTM, not the rows' rotated one (a pure function of t does not
care); the proc is called 17 extra times.

Tests (`tests/pktrowelplane.rs`): a tapering pressure leaves less than 1% of
a solid-blade plane bare (it was 4.3% before); a flat `/Pressure` renders
byte-identically to none; a proc that draws on its first 17 calls leaves the
plane and the stream exactly as a proc that does not.

Before/after render (two shadow facets, a gentle and a harder taper; left is main, right is this change): `docs/pktrowelplane_taper_before_after.png`.
