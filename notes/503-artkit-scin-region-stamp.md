## artkit: scin refuses a hand-built region, without a per-call walk (issue #503, 2026-10-14)

The residual #295 left: `scin` runs `siinside`, so a hand-built path region
whose `/Edges` (or an element of it) is a procedure still ran it through
`siscan`, and a procedure as `/BBox` or `/Kind` could run the same way.
`scatter` and `scarea` were already covered (notes/295). `scin` is in callers'
inner loops (paintkit's `mx`/`mc` hatching at `lib/paintkit.ps` 9465 and 9563,
`dySR` in drycrayon, `sfregion` in surfacekit), so an O(edges) check on every
call was the thing to avoid.

**What shipped** (lib/artkit.ps only, plus one name in `ARTKIT_INTERNAL`):

- `screct` and `scpath` stamp the region they build: `/Checked` holding the
  region dict itself. Nothing else writes it.
- `scin`, after its operand checks, runs `dup /Checked known { dup /Checked
  get 1 index eq } { false } ifelse not { scregionvet } if`. A stamped region
  skips the check: `known`, `get`, `eq` and a branch, a constant handful of
  operators against `siinside`'s bbox test (rect) or edge walk (path).
- `scregionvet` (region -> region, internal) is the check for a region that
  fails that test: `/Kind` known and `/rect` or `/path`, `scbboxok`, and for a
  path region `scedgesok` and a `/Rule`. Stack-only, nothing bound. Names:
  `scin-region-kind-must-be-rect-or-path`, `scin-region-bbox-must-be-four-
  numbers`, `scin-region-edges-must-be-numbers`, `scin-region-needs-a-rule`.

**Why the dict itself and not a flag.** `eq` on dicts compares the object. A
bare `/Checked true` is a thing a hand-built dict can hold by accident, and
`dup length dict copy` of a built region would carry it into a dict whose
`/Edges` is then replaced. With the region itself as the stamp a copy's
`/Checked` still names the original, so the copy falls to the full check; so
does a hand-written key. Measured first: pscat's `eq` is identity for dicts
(`<< >> << >> eq` false, a dict against itself true) and `==` on a
self-referencing dict prints `-dict-`.

**Not stamped on first use.** A region that passes `scregionvet` is not marked:
writing into the caller's dict is a side effect and fails on a readonly one. So
a hand-built region (a literal `<< /Kind /rect /BBox [...] >>`) is re-checked on
each `scin` call: O(1) for a rect, O(edges) for a path. Every caller that takes a
region from `screct` or `scpath` (all the repo's) pays only the stamp test.

**Limits.**

- A built region whose slot is replaced in place afterwards
  (`region /Edges {...} put`, or a planted element in the live array) keeps the
  stamp and is not re-checked. `region /Checked undef` re-arms it. The threat
  model is the #123 / #295 one, accidental misuse, not a caller who sets the
  stamp on purpose.
- `siinside` is unchanged: `scatter` (after its own check) and paintkit call it
  directly. paintkit's `pereglook` and `mfinmass` do, with a region whose only
  paintkit-side check is `mhisregion` (`/BBox` and `/Kind` exist), so a
  hand-built region in a `pkbrushedge` `/Under` list can still run a planted
  `/Edges`. That is lib/paintkit.ps; filed separately.

**Identity.** No valid region changes. `scripts/identity_check.sh`, base at the
merge-base: results recorded on the PR. The control is a driver whose
hand-built path region has a procedure element in `/Edges` that draws a mark when
`scin` runs it: base draws the mark, head raises the guard.

**Cost.** No timing work (shared machine); by construction the hot path gained
the stamp test only, and `siinside` (used directly by scatter) is untouched.
