## artkit: scatter and scarea check a hand-built region (issue #295, 2026-10-14)

A residual from the #123 audit, split out of #259 (hatch). A region dict that
`screct` or `scpath` did not build could hold a procedure in a slot that
`scatter` or `scarea` binds with `def`, and the next bare reference ran it
(again for every reference). `screct` and `scpath` never produce such a region.

**What ran, measured on main first** (a `tgot` flag set by the planted
procedure): `/BBox` as a procedure or with a procedure element
(`/scbb ... def`, then `scbx`), `/Edges` as a procedure or with a procedure
element (`siscan`'s `/siedges ... def`, `siedge`'s `six0`; `sqmeasure`'s
`/sqaedges`, `sqainsert`'s `/sqav`), and a stored `/Area` that is a procedure
(`scarea` hands it back, then `scregarea 0 le` in scatter's `/Density` path
runs it). The `%%SelfTest` block was run against main's file first and failed
on those; with the change it passes.

**What shipped** (lib/artkit.ps only):

- `scbboxok` and `scedgesok` (region -> bool). The slot must be a literal
  array (`type` is arraytype, `xcheck` false), `/BBox` of length 4, `/Edges`
  of length divisible by 4, every element `scisnum`. Stack-only, nothing
  bound, so they are safe from both `sc-` (scatter) and `sq-` (scarea) code.
- `scatter`, right after its existing "needs a region" test:
  `scatter-region-kind-must-be-rect-or-path`,
  `scatter-region-bbox-must-be-four-numbers`, and for a path region
  `scatter-region-edges-must-be-numbers` and `scatter-region-needs-a-rule`
  (a missing `/Rule` was a raw `undefined` from `siscan`, same class).
- `scarea`: `scarea-needs-a-region` (was a raw `typecheck` from `known`),
  `scarea-area-must-be-a-number` (a stored `/Area` still wins and `/Edges`
  is not looked at then, so `region /Area <num> put` stays the escape hatch),
  and on the measuring path `scarea-region-edges-must-be-numbers` and
  `scarea-region-needs-a-rule`. A bad `/Area` reached through scatter's
  `/Density` raises the `scarea-` name, not a `scatter-` one.

**Not done: `scin`.** It keeps its two operand checks only. `scin` sits in
callers' inner loops (paintkit's `scin` calls, drycrayon), so an O(edges)
check per call would multiply their cost; `scatter` validates once and then
calls `siinside` directly, which is why scatter is fully covered. A
hand-built path region with a procedure edge still runs through `scin`.
Filed as #503.

**Cost.** One pass over `/Edges` per `scatter` call (twice with `/Density`),
against `Count * Tries` candidate tests that each walk the same edges.

**Identity.** No valid region changes: `scripts/identity_check.sh` default
pick and `--loader lib/artkit.ps` byte-identical (see the PR for the numbers).
