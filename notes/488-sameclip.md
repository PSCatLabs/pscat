# #488: `.sameclip`, a clip identity query, so drykit sees a bound grestore (2026-10-14)

## Decision

pscat gains a non-standard operator, `gstate .sameclip bool`: true when the
clip in force is the very clip the gstate holds (both none, or the same clip
chain). drykit uses it, when it exists, to notice the `grestore` of a bound
wrapper inside a `dkbatch` (#459), so in pscat that case is now exact. Under
Ghostscript, which has no such operator, drykit behaves as before and the
#459 limit and its three workarounds stand.

## The operator

- `src/ops/graphics.rs` `sameclip`, `src/gfx.rs` `GraphicsState::same_clip`:
  `Rc::ptr_eq` on the clip chain's head node (the mask and the node are
  always made together, so the node alone decides). Both `None` is true; one
  `None` is false.
- Identity, not geometry. `gsave`/`grestore`, `setgstate` and
  `save`/`restore` put the same chain back, so they compare true. Any
  `clip`, `eoclip`, `rectclip` after a capture makes a new chain, false even
  when the path is identical; `initclip` under a clip is false. Other graphics
  state (colour, line width, CTM) does not count.
- Exact: the gstate operand keeps its chain alive, so no new chain can reuse
  that allocation and compare equal by accident.
- Typecheck on a non-gstate operand (pushed back, as `setgstate` does),
  stackunderflow on none.
- **Name.** pscat's earlier extensions (`setalpha`, `setblendmode`, #47) use
  plain names. This one is dot-named, Ghostscript's convention for
  implementation-specific operators: it is an introspection hook, not a
  painting operator, and the dot keeps it out of the way of user names. gs
  10.07 has no `.sameclip` (`/.sameclip where` is false there), so there is no
  clash.

## drykit

- `dkPsameok` (`systemdict /.sameclip known`, at load) and `dkPsame`
  (`gstate -> bool`, `{ pop true }` without the operator). `dkPsame` is a
  procedure, not the operator itself, so a test can set `dkPsameok` false
  to preview the Ghostscript path.
- In `dkSrecord`'s batch branch, when `/pc` is a gstate and `dkPsame` says its
  clip is no longer in force, `/pc` goes to `/dirty`, and the existing line
  takes a new capture. Strokes under one clip still share one gstate, so
  grouping is unchanged (`plain_clips_in_a_batch_group_as_dkclip_regions_do`
  still counts 6 and 6 catches).
- The check runs at every stroke whose `/pc` is a gstate, so it catches any
  clip change drykit has no shadow for, not only a bound `grestore`: a bound
  `rectclip` after a seen, captured clip now reaches the next stroke too
  (a test row). Before #488 that stroke kept the earlier capture. A clip
  made by bound code while `/pc` is still null is not kept (below).
- The `/pcs` stack needs nothing: the bound wrapper's `gsave` was unseen too,
  so the stack has no entry for it, and the next seen `grestore` pops the
  right one.
- The issue's open question, the `/pc null` case (a clip made by bound code
  and never seen): left alone. Comparing the current clip with the batch's
  opening gstate (`/g`) would fire for every `dkclip` region too (dkclip
  clips the real graphics state while its procedure runs), and the fix
  would have to tell the two apart; it would also change what a batch draws
  for bound code that clips and strokes, which today keeps the documented
  "the clip in force when the batch ends" rule. No committed render needs it
  and no one has asked; not filed.
- Cost: one `type` test and one `.sameclip` per `dkstroke` in a batch whose
  `/pc` is a gstate (only batches with a plain clip inside). No timing taken.

## Evidence

- `tests/gstate.rs` `sameclip_tests_clip_identity`: the true/false cases
  above, the typecheck and the underflow.
- `tests/drykit.rs` `a_plain_clip_in_a_batch_renders_as_at_once` gains a
  fourteenth row, the bare bound wrapper `{ RING CL } dktwrap FREE`: now
  byte-identical in a batch to the same program drawn at once. Control in the
  test: with `dkPsameok false` (drykit's gs path) the same row leaks, 2,488
  pixels, the figure #459 measured. A fifteenth row, a bound `rectclip`
  after a seen clip and its stroke, is byte-identical too, and differs
  without `.sameclip` (360 pixels). Its strokes are kept apart: two
  overlapping strokes under different clips layer differently in a batch
  (level-major) than drawn at once whatever the clip, which a first try
  with the shared `CL` stroke showed (1,031 pixels with `.sameclip`).
- Ghostscript (correctness only): the bare-wrapper program at 144 dpi, png16m,
  with artkit, paintkit and drykit prepended, runs without error,
  `dkPsameok` is false, and the batch differs from the at-once render (2,422
  pixels): the documented limit, unchanged. The same program in pscat at 144
  dpi: `dkPsameok` true, batch and at-once byte-identical.
- Identity: `scripts/identity_check.sh` with `--base-bin`/`--head-bin`,
  `--loader lib/drykit.ps --loader lib/artkit.ps`, and a control (the bare
  bound wrapper in a batch) that must differ: 97 of 97 page renders
  identical across 81 files (every file that loads drykit or artkit, the
  dry-media libraries, gallery pieces and self-test drivers included); the
  control differs between the trees, as it must.
