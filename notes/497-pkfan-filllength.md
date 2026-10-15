## pkfan runs cut for pkwet /FillLength (issue #497, 2026-10-14)

#492 cut `pkbroad` and `pktrowel` runs into overlapping per-stop fills for
`pkwet /FillLength` (with `/PerFill true`) and left `pkfan`: `pfrun` hands each
bristle's run to `pkribbon` as one pointed ribbon (`/StartCap /pointed /EndCap
/pointed /EndTaper`), so cutting at a stop would have pointed every piece. This
cuts it, opt-in only; see notes/492-pkwet-filllength.md for the rest.

Design calls, and why:

- **`pfrun` is a dispatcher now.** It asks `pffillcuts` (the same shape as
  `pmfillcuts`: `pufilllen` and `pusplit` over the run's stops' arc lengths,
  only with `/Colors` loaded since only a loaded run is asked per fill) and takes
  `pfsplitrun` for more than one piece, else `pfwholerun`, the old body renamed
  and untouched. No length, a short run, an unloaded fan and the core pass all
  take `pfwholerun`, so every render without the key is the same code.
- **The random stream is unchanged.** The draws a run makes are one `pkjit` per
  stop in building the displaced centreline (and `pkribbon` draws only with
  `/Jitter`, which pkfan never passes). `pfsplitrun` takes those draws first, in
  the same order, into arrays, then builds each piece's path from them. Tested
  with `/Jitter`, `/WidthJitter`, `/Ragged`, `/Dropout`, `/Load`, `/ColorJitter`
  on: the next draw after the call is the uncut call's.
- **Pieces are pkribbons with flat interior ends.** Piece k runs from the stop
  before cut k to cut k+1 (neighbours share a segment), `/StartCap /pointed` only
  on the first and `/EndCap /pointed` only on the last, `/flat` between. All pieces
  use the whole run's pitch (floor included), so the 8-samples-per-stop cost
  bound is the run's.
- **The flick stays the run's.** `/EndTaper` is a fraction of the *piece's*
  progress, so it cannot be passed through. Each piece gets `/Pressure`
  `pfpress`, which maps the piece's `t` (arc-length fraction of the piece, per
  walkpath) to the run's by the displaced cumulative lengths and applies the
  same `1 - (1-t)/Flick` ramp `pktaperf` does, only for a run that reaches the
  bristle's end (`pflast`). The pieces' pressures at their ends are tested to
  be continuous and to reach 0 only at the run's end.
- **Cost.** Each piece is a nested `pkribbon` call, and the deposit budget
  (`Bristles x stops <= 150000`) counts stops, not pieces. A piece is at least
  one stop long, so a tiny `/FillLength` is at most one call per inked stop per
  bristle per softening pass. Not charged to the budget (that would refuse
  ordinary fans: charging a call like a ray's 32 stops caps a fan at about 4600
  bristle-stops); the `@param` says to keep the length well above the pitch.
  The fill count is what is reported (37 asks whole, 161 cut); no timing.
- **Not cut:** a pressed fan's rays (`pfray`, one short ribbon each) and
  `pkdab` (one fill per dab).

Measured (`tall_stroke` with a 10pt fan of 20 one-point bristles, `/Flick 0`,
rows 15..135 of `rim_residual`, /Offset 34, 6 layers): `/PerFill` alone 252.7,
`/FillLength 10` 73.5. Asks of the procedure (a fan of 8 bristles with every
random key on, 4 layers, /Offset 20, `/FillLength 12`): 37 whole, 161 cut. Over
a constant backdrop the cut render differs from the uncut in 78 of 180000
channel values (93 with the flick on), all on antialiased edges where the
overlap re-blends them.

A pixel test cannot check "pointed only at the run's ends": the core pass,
painted uncut underneath, fills the notch a pointed interior end leaves (a
mutant with every cap pointed passed the pixel test). The test records the
ribbons handed to `pkribbon` instead (caps and `/Pressure` at both ends).

Byte identity: `scripts/identity_check.sh` on the default pick and with
`--loader lib/paintkit.ps`, plus a control that opts in (`/FillLength 10`) and
differs as it must. No committed render changed. Gallery adoption stays #491.
