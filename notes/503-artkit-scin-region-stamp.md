## artkit: scin refuses a hand-built region, without a per-call walk (issue #503, 2026-10-14)

The residual #295 left: `scin` runs `siinside`, so a hand-built path region
whose `/Edges` (or an element of it) is a procedure still ran it through
`siscan`, and a procedure as `/BBox` or `/Kind` could run the same way.
`scatter` and `scarea` were already covered (notes/295). `scin` is in callers'
inner loops (paintkit's `mx`/`mc` hatching, `dySR` in drycrayon, `sfregion` in
surfacekit), so an O(edges) check on every call was the thing to avoid.

**What shipped** (lib/artkit.ps only, plus one name in `ARTKIT_INTERNAL`):

- `screct` and `scpath` stamp the region they build: `/Checked` holds the
  array that is in the region's `/BBox` (rect) or `/Edges` (path), the slot a
  hand-built region could plant a procedure in. Nothing else writes it.
- `scin`, after its operand checks, runs `dup /Checked known { dup dup /Edges
  known { /Edges } { /BBox } ifelse get 1 index /Checked get eq } { false }
  ifelse not { scregionvet } if`. A stamped region skips the check: `known`,
  two `get`s and an `eq`, a constant handful of operators next to `siinside`'s
  bbox test (rect) or edge walk (path). `siinside` is untouched.
- `scregionvet` (region -> region, internal) is the check for a region that
  fails that test: `/Kind` known and `/rect` or `/path`, `scbboxok`, and for a
  path region `scedgesok` and a `/Rule`. Stack-only, nothing bound. Names:
  `scin-region-kind-must-be-rect-or-path`, `scin-region-bbox-must-be-four-
  numbers`, `scin-region-edges-must-be-numbers`, `scin-region-needs-a-rule`.

**Why the array and not a flag.** `eq` on arrays compares the object. A bare
`/Checked true` is a thing a hand-built dict can hold by accident, and
`dup length dict copy` of a built region would carry it into a dict whose
`/Edges` is then replaced. With the slot's own array as the stamp, a copy given
a new `/Edges` (or, for a rect, `/BBox`) no longer matches and gets the full
check; so does a hand-written key. Measured first: pscat's `eq` is identity for
arrays and dicts.

**Rejected: the region dict itself as the stamp** (first version). It is a
stronger test (a copy never matches), but it makes every region a reference
cycle, and pscat's dicts are `Rc<RefCell<..>>` with no cycle collector: 20000
`scpath` regions measured 220 MB peak against 7.7 MB (each region also pins
its 256-slot `sqbuf`). A render that builds regions in a loop would grow
without bound. With the array stamp the same loop is 8.5 MB.

**Not stamped on first use.** A region that passes `scregionvet` is not marked:
writing into the caller's dict is a side effect and fails on a readonly one. So
a hand-built region (a literal `<< /Kind /rect /BBox [...] >>`) is re-checked on
each `scin` call: O(1) for a rect, O(edges) for a path. No region in the repo's
`lib/`, `gallery/`, `examples/` or `selftest/` is hand-built or copied
(grepped for `/Kind /path|rect`, `/Edges [`, `dict copy`): every `scin` caller
gets a `screct` or `scpath` region (surfacekit's `sfregiondef` only reads it).

**Limits.**

- A built region's array edited in place afterwards (an element replaced, such
  as `region /Edges get 1 {..} put`) keeps the stamp and is not re-checked
  (measured: it runs). A path region's `/BBox` replaced in a copy is not
  re-checked either (the path stamp names `/Edges`). `region /Checked undef`
  re-arms the check. The threat model is the #123 / #295 one, accidental
  misuse, not a caller who sets the stamp on purpose.
- `siinside` is unchanged: `scatter` (after its own check) and paintkit call it
  directly. paintkit's `pereglook` and `mfinmass` do, with a region whose only
  paintkit-side check is `mhisregion` (`/BBox` and `/Kind` exist), so a
  hand-built region in a `pkbrushedge` `/Under` list can still run a planted
  `/Edges`. That is lib/paintkit.ps; filed as #526.

**Identity.** No valid region changes. `scripts/identity_check.sh`, base at the
merge-base, default pick 50 of 50 pages across 41 files, `--loader lib/artkit.ps` 98 of 98 across 82, `--loader lib/paintkit.ps` 50 of 50 across 41, all identical. The control is a driver whose hand-built path
region has a procedure element in `/Edges` that fills a square when `scin` runs
it: base draws the square, head raises the guard (differed in all three runs).
Ghostscript (correctness only, `-dNOSAFER`): the planted procedure raises
`undefined` without running, and an `scpath` region answers `true` and `false`
for an inside and an outside point, as pscat does.

**Cost.** No timing work (shared machine). By construction the hot path gained
the stamp test only; `siinside`, used directly by scatter's candidate loop, is
untouched. The checks that run on the unstamped path are the same
`scbboxok` / `scedgesok` passes #295 added to `scatter`.
