## pkbrushedge /Mass: a pull-out colour that follows the mass (issue #426, 2026-10-14)

`pkbrushedge` (#347) pulled out the *current* colour: one colour for the whole
edge. A long edge on a graded mass (a meadow lit toward the glow at one end)
had to settle for one mix, which is what Evening Stream did in #382.

**What shipped** (lib/paintkit.ps, `pkbrushedge` only):

- `/Mass`, the colour of the outward strokes, in `/Under`'s three forms: an
  `[r g b]` array, a pigment name, or a procedure `x y -> r g b` asked once
  per outward stroke, at the stroke's start (where it leaves the mass, so the
  colour is the mass's own there). Unset is the current colour, through the
  same line of code as before.
- Name: the issue suggested `/Over` or `/Colour`. `/Over` already means the
  opposite in drycrayon (the colour a stroke is laid *over*), and keys keep
  one meaning (#141). `/Mass` pairs with `/Under`: the mass and what is under
  it.
- The `/Under` parsing and the per-stroke loop became shared helpers
  (`pecolkey`, `pestrokes`, `pecolat`, the last replacing `peunderat`). Guard
  names are built from the key's prefix with `pugrd`, so the `/Under` guards
  are named exactly as before and `/Mass` gets `pkbrushedge-mass-*`.
- pkwet's backdrop machinery (#179, #359) was not reused: it samples a
  procedure per pass or per band at `/At`, while this needs one colour per
  stroke, which pkbrushedge's own procedural `/Under` already did. Sharing
  that loop was the reuse that fits.

**Cost.** A procedural `/Mass` costs one `/Brush` call per outward stroke, as
a procedural `/Under` does per inward one; so even a constant procedure paints
differently from the array form (the brush's random draws fall per stroke).

**Checks.** Self-tests: a guard per failure (array, components, pigment,
procedure answers), each form passes, an array `/Mass` is the colour the
brush sees, a procedural one is called once per brush call and each stroke is
painted in the colour of its own first point (a gradient along x), and a bad
answer after a good one unwinds every gsave. Replacing `pestrokes` with one
brush call fails the block. `scripts/identity_check.sh` (default pick, which
includes Evening Stream's procedural `/Under` and the brushedge specimen, and
`--loader lib/paintkit.ps`) is byte-identical; the control (a driver with
`/Mass` set) differs. A scratch render of a graded meadow with and without
`/Mass` is right in pscat and Ghostscript; no specimen or gallery render
changes here.

**Follow-ups.** #455 (Evening Stream adopts `/Mass` on its far shore and
stream banks; a visible change). #456 (the issue's second ask: a `/Under`
that is another path's fill, so `evunderbank` need not guess with a
straight-line test).
