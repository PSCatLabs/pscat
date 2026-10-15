## pkwet /FillLength: a long loaded run cut into per-stop fills (issue #492, 2026-10-14)

#403's `/PerFill true` asks the backdrop once per inked run, at the run's middle
stop, so one long unbroken run (a single vertical `pkbroad` stroke down a
vertical gradient) was asked once, no better than per pass. `/FillLength` (points,
with `/PerFill true`) cuts such a run into fills about that far apart, each
coloured and so asked at its own middle.

Design calls, and why:

- **Opt-in key, no change of meaning.** `/PerFill true` alone renders as it did
  (it is in no gallery piece yet, #491). The new key is `/FillLength [points]`,
  an error without `/PerFill true`, not a number, a procedure, or not positive
  (four new `pkwet-filllength-*` names). A length is the one knob a painter has
  for "how closely": a stop count would depend on `/Pitch`.
- **Frame slot 9 grows a third element**, the length or 0, copied at entry like
  the other two. `pufilllen` reads it: the shortest positive length among the
  nested levels that are running a softening pass now (nonzero pass factor). The
  core pass is asked nothing, so cutting it would only add fills.
- **`pusplit`** takes each stop's arc length and the length and returns cut
  indices (a cut at the first stop where the length since the last cut reaches
  it, never at the last stop). Piece k runs from the stop before cut k to cut
  k+1, so neighbours overlap by one segment: the abutting-polygon seam gotcha.
  One piece (`[0 n]`) means "do not cut", and then the old code runs, not the
  split code with one piece. So a length past the run's end is the `/PerFill`
  render to the byte (tested).
- **`pktrowel`** (`ptemit`, which `ptridge`'s edge ridges also go through, so the
  full-length ridges are cut too): the strip's edges are already in `ptua`/`ptub`,
  so `ptsplitfill` fills the same polygon over a sub-range of stops, with the
  half-pitch and /Drag extension only at the real ends.
- **`pkbroad`** (`pmemit`): each boundary point draws an edge jitter, so
  `pmsplitfill` computes the whole boundary first, in `pmemit`'s own order
  (na side forward, nb side back) with the same expression, then fills sub-polygons
  from the stored points. The run-end taper stays the whole run's. The stream
  after the call is identical with and without the cut (tested, with
  `/Edge`, `/Depletion` and `/ColorJitter` on for pkbroad and `/Jitter`,
  `/EdgeBuildup` on for pktrowel). `puat`, `shade` and `puwetat` draw nothing.
- **Not cut: `pkfan`, `pkdab`.** A `pfrun` run goes to `pkribbon` as one pointed
  ribbon, and cutting it would point every piece; a dab is one fill already.
  Filed as a follow-up. `/TipColor` (#229) can reuse `pusplit`.
- **Units.** The length is in the user space the mark is drawn in (walkpath's arc
  lengths), not pkwet's as `/At` is; they differ only if the mark scales inside
  the procedure.
- **The procedure must not draw random numbers**: with more fills it would draw
  more and move the stream.

Measured (`tall_stroke`: one 110pt vertical stroke, 10pt wide, over the 150pt
gradient, /Offset 34, 6 layers, rows 15..135 of `rim_residual`): pkbroad
/PerFill alone 239.6, `/FillLength 10` 71.6; pktrowel 206.8 and 70.2. Fills (asks of
the procedure, 4 layers, /Offset 20): pkbroad 45 whole, 345 cut at 12pt; pktrowel
141 and 357. Over a constant backdrop the cut render is the uncut one to the
byte for pkbroad, and 275 of 180000 channel values differ for pktrowel (the
overlap re-blends the lane edges' antialiasing).

Byte identity: `scripts/identity_check.sh` on the default pick and with `--loader
lib/paintkit.ps`, and a control that opts in (`/FillLength 10`) differs as it must.
No committed render changed.

Deferred: adopting `/PerFill` (and `/FillLength`) in a gallery piece (#491), and
cutting `pkfan` runs.
