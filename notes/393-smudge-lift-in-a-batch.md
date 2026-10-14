# #393: dhsmudge and dhlift inside a dkbatch (2026-10-14)

## Decision

A smudge or a lift made inside a `dkbatch` is drawn **after** the batch, in
call order, each on its own, exactly as if it had been made just after the
batch closed. It keeps its own colour, `/Under`, CTM and `dkclip` region.

The issue asked whether a smudge or lift in a batch should open a separate
batch after the caller's (ordering), or stay outside batches. Splitting the
caller's batch at the smudge would keep call order, but it would silently
break the level-major crossings the batch exists for, for every stroke after
the split. A smudge or a lift only makes sense on top of the charcoal it
works, so "after the batch" is the natural place. The cost: in
`{ hatch A; lift; hatch B } dkbatch` the lift also lands on B. That is
documented in both summaries, the dry-media guide, DRYMEDIA.md and the psart
skill: to hatch over a lift, hatch in a later batch.

What this buys an artist is composability: a routine that hatches, smudges
and lifts (a face, a fold) can now be called inside a caller's batch, next
to other hatched regions, instead of raising
`dhsmudge-cannot-run-inside-a-dkbatch`.

## What shipped

- `lib/drykit.ps`: a new `dkstroke` key, `/After [boolean]` (default false;
  `dkstroke-after-must-be-a-boolean`). Inside a batch, an `/After` stroke is
  recorded as usual (same `/clip` rule as #312) but goes to the batch's
  `/after` list: it neither checks nor sets the batch's pigment. `dkbatch`
  renders the main records, then each `/After` record with `[ rec ] dkrender`,
  in call order, under the same `dkZbase`. Outside a batch the key is
  ignored. With no `/After` stroke the render path is unchanged (one extra
  key lookup per stroke, an empty `forall` per batch).
- `lib/drycharcoal.ps`: `dhnobatch` removed; `dhsmudge` and `dhlift` put
  `/After true` into the options they pass to `dkstroke` (after the merge, so
  a caller's `/After false` cannot make a paper-coloured lift join a charcoal
  batch and raise `dkbatch-mixes-pigments`).
- Self-tests: drykit's `dkbatch-guards` gains `/After` strokes of another
  colour first and last, a batch of only `/After` strokes, and `/After` with
  no batch; `dkstroke-rejects-malformed-options` gains the boolean guard.
  drycharcoal's guards replace the two `cannot-run-inside-a-dkbatch` checks
  with a lift before hatching and a smudge after it in one batch, and a lift
  inside a `dkclip` in a batch.

## Evidence

- `tests/drycharcoal.rs` `a_smudge_and_a_lift_in_a_batch_draw_after_it`:
  `{ h1 smudge lift h2 } dkbatch` is byte-identical to
  `{ h1 h2 } dkbatch smudge lift`; control: the batch split at the smudge
  differs. A lift inside a `dkclip` (a triangle) in the batch is
  byte-identical to the same lift under `gsave region clip ... grestore`
  after the batch; control: the lift unclipped differs.
- `tests/drykit.rs`: `/After` is in the every-documented-parameter test
  (inside a batch with a later crossing stroke, it changes the render).
- Ghostscript (correctness only): a 160 pt scene, two overlapping hatched
  squares with a smudge and a lift in the batch, renders byte-identical in
  gs (png16m, 144 dpi) to the same scene with the smudge and lift after the
  batch. Same in pscat.
- Identity: see the PR. `scripts/identity_check.sh --loader lib/drykit.ps
  --loader lib/drycharcoal.ps`, with a control (an `/After` stroke in a batch
  before a crossing stroke) that must differ between the trees.

## Not done

- drychalk's `dcboard` still refuses a batch (`dcboard-cannot-run-inside-a-dkbatch`);
  it draws a whole board, so it is not a stroke to defer.
- No specimen row: nothing renders differently unless a caller puts a
  smudge or lift in a batch.
