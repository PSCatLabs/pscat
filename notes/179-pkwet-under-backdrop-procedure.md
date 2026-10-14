## pkwet /Under as a backdrop procedure (issue #179, 2026-10-14)

`pkwet /Under` took one `[r g b]`: the colour declared to be under the mark.
Over a backdrop that changes where the mark sits (a cloud over a graded sky)
the outer passes are right at one end and leave a pale or dark rim at the
other. `/Under` now also takes a procedure `x y -> r g b`, with a required
`/At [x y]`.

Design calls, and why:

- **Per pass, not per pixel.** pkwet re-runs a caller's procedure; it cannot
  see where that procedure paints, and a pass is one colour. So each
  softening pass asks the backdrop procedure once, at `/At` plus that pass's
  displacement (from the plan, so it is the true shift). The core asks
  nothing (no backdrop mixes into it). What this fixes is the part of the
  rim the displacement causes. A mark that is tall against the gradient keeps
  the rest: split it into several calls, each with its own `/At`.
- **/At is required** with a procedure, and rejected with an `[r g b]`.
  Defaulting to [0 0] would silently evaluate the backdrop at the page
  origin; relaxing a requirement later is backward compatible, adding one is
  not.
- **Evaluation order.** After the plan loop and before the first pass, so the
  procedure sees the final displacements, nothing of the wrapped procedure
  has run, and none of pkwet's draws move (the plan loop is untouched: for a
  procedure it runs against a white placeholder, then `pqwetunder`
  overwrites the colours). A literal `[r g b]` takes the code that was there.
- **Unwinding.** The depth counter is already raised when the procedure
  runs, so each exit (an error inside it, a result of the wrong arity, a
  non-number, an out-of-range number) rolls the depth back first, and the
  arity and type rejections also put the operand stack back to its entry
  count. Tests: `wet_procedural_under_rejects_malformed_answers_by_name`,
  `wet_procedural_under_that_raises_leaves_the_depth_alone`, and the
  `pkwet-rejects-malformed-procedural-under` `%%SelfTest`.
- **Loaded colours.** Frames grew a slot 7 (per-pass backdrops, null for a
  literal); `puwet` reads it first. The literal path is unchanged.
- **The procedure is called with nothing of pkwet's on the operand stack**
  (it gets `x y`), but it must still be pure: no painting, no `pkwet`.
- Evaluation order within the plan is core-out (index 1 is the innermost
  softening pass), and the passes are drawn the other way.

Measured (examples/paintkit_wet_under_demo.ps, the sky gradient over 150pt,
a 34pt /Offset): the fringe's residual against "cloud blended into the local
sky" is 264.6 with the hand-matched constant and 121.2 with the procedure.
A 40pt thinner cloud measures 316 against 123. `wet_procedural_under_removes_the_rim_over_a_gradient`
asks for 1.6x.

Deferred: per-pixel (or per-band) backdrops would need either a hook into
every preset's colour set or clipping each pass into bands (which breaks the
brush's random stream across bands), so neither was attempted.
