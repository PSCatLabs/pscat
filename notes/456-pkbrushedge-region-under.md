## pkbrushedge: /Under and /Mass from another path's fill (issue #456, 2026-10-14)

Evening Stream's `evunderbank` (#382) guesses where the stream runs behind a
near bank with a straight-line test. #426 deferred the second ask: let the
backdrop follow another path's fill exactly.

**What shipped** (lib/paintkit.ps, `pkbrushedge` only):

- A fourth form for `/Under` and `/Mass`, the region list
  `[ [ region colour ] [ region colour ] ... default ]`. At each stroke's
  start the first region holding the point gives the colour; outside them
  all, the default (required: the last entry). A region is artkit's
  (`screct`, `scpath`), tested with `siinside` (nonzero, as `fill` paints,
  the test pkfeather already uses), or a procedure that builds a path, turned
  into a region once per call in the call's user space, inside its own
  gsave so the caller's path survives. Each colour is one of the other three
  forms (array, pigment name, procedure `x y -> r g b`).
- It is parsed in `pecolkey` as mode 3 (`peregions`; each entry's colour goes
  back through `pecolkey`, so its guard names keep the key's prefix) and
  painted stroke by stroke through the same `pestrokes` / `pecolat` as a
  procedure, with `pereglook` as the procedure. Modes 0 to 2 are untouched.
- Told apart from `[ r g b ]` by its first element not being a number.
- New guards (prefix `pkbrushedge-under` / `pkbrushedge-mass`):
  `-regions-need-a-region-and-a-default-colour`,
  `-regions-must-end-with-a-default-colour`,
  `-region-entries-must-be-region-colour-pairs`,
  `-region-must-be-a-region-or-a-path-procedure`,
  `-region-colours-must-not-be-region-lists`.

**It is a convenience, not a new capability.** Measured first: a procedural
`/Under` can already ask a region built beforehand,
`{ 2 copy reg scin { ... } { ... } ifelse }`, with no path operator inside the
procedure. The region-list render of the same scene is byte-identical to that
hand-rolled one. What the form adds is the shape: a `{path}` straight from the
scene, first match wins, checked colours, no scratch names to manage.

**Not used:** `infill` (Level 2). It would test against the current path, so
the region path would have to be rebuilt per stroke inside a gsave; the artkit
region is built once and is the same nonzero test as pkfeather's. Not pkwet's
`/Under` either: the region list is pkbrushedge's own (said in the @param).

**Cost.** Like a procedure, a region list costs one `/Brush` call per stroke.

**Checks.** Self-tests: each guard; a failing `{path}` unwinds its gsave; the
caller's path and operand stack survive; and a closed box (so pkfeather builds
its own `scpath` region too) with both keys set to a list of a `{path}` region,
an overlapping `screct` region and a procedural default: every stroke is
painted in the colour of the first region its start lies in, and all three
colours are seen. Making the lookup skip the first region fails that test.
`scripts/identity_check.sh` (default pick and `--loader lib/paintkit.ps`) is
byte-identical; a control driver using a region list differs. Ghostscript
renders the scratch scene without errors.

**Follow-up.** Evening Stream adopting it in `evunderbank` is noted on #455
(a visible change; not done here).
