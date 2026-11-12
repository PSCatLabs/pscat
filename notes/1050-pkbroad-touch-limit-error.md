# #1050: the /Touch size guard is a clear error

`pkdry`, `pktrowel`, `pkfan`, `pkbroad` and `pkliner` refuse a `/Touch` stroke
whose catch box is over `pjbudget` (60000 weave cells). They used to do it by
running an undefined name (`pkbroad-touch-area-exceeds-safety-limit`), which
reads as a missing definition.

Now `pjtoucherr` (lib/paintkit.ps) raises the PLRM's `limitcheck` with a
message as the offending object:

    pkbroad: /Touch is limited to 60000 weave cells per stroke (about 612 x 612 pt
    at /Weave 2.5) and this stroke reaches 80896 cells (about 640 x 790 pt). Drop
    /Touch from it, split it into separate strokes that each reach less, or set a
    coarser /Weave in pkcanvas

Decisions:

- `limitcheck`, not `undefined`, because this is a resource limit, not malformed
  input. It is the one guard in paintkit that is not a self-documenting undefined
  name: a name cannot carry this stroke's size. The self-test checks the error
  name (`mustfail /limitcheck`) and that the message names the preset, the limit
  and the fix.
- The unit is weave cells, not pt^2: the budget counts cells of the call's reach
  box (grown by the knuckle margin), so it moves with `/Weave` and the CTM. The
  message gives the cells and the equivalent points at the current `/Weave`.
- The reach is the bounding box of the whole call, so a long diagonal stroke is
  the worst case. The fix list says "separate strokes", and the docs say why.
- A box past 1e9 cells (`pjhuge`) has no honest count, so it says "far beyond".
- The text has no `;` or `]`: pscat's error report and `pscat-mcp`'s parser split
  on them.
- The five `@param: /Touch` lines carry the limit, so `pscat --capabilities`
  (the `pkbroad` entry and the others) states it. `src/capabilities.rs` reads
  those lines from the catalog; it holds no per-brush text, so it is unchanged.
- `pjcatch`'s own `pjcatch-area-exceeds-safety-limit` backstop is untouched (the
  presets reject first, so it is unreachable from them).

Not done: tiling a large stroke internally so `/Touch` works at any size. That is
painting polish, parked with #530.
