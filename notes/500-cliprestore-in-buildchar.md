# #500: a Type 3 `BuildChar` runs in its own `gsave` level (2026-10-14)

Follow-up to `notes/494-clipsave-cliprestore.md` and
`notes/505-grestore-bottom-currentgray.md`. Ghostscript 10.07.1 is the
reference; every case below was run through gs first.

## Behaviour (gs)

gs runs a `BuildChar`/`BuildGlyph` inside a `gsave`. So, in the procedure:

- A `cliprestore` with no `clipsave` of its own gives back the clip at
  `show`, and does not pop an entry made before the `show` (the glyph starts
  with an empty stack). The caller's entry is still there after the `show`.
- A `clipsave` made in the glyph does not outlive it.
- An unmatched `grestore` gives back the state at `show` (line width,
  clip, ...). A second one pops the caller's `gsave`.
- A `gsave`/`save` in the glyph opens a level of its own on top of that
  one, as usual. `grestoreall` goes to the bottom, as everywhere.
- A glyph that leaks a `gsave` is sealed when it ends.

## What changed

pscat's glyph context was a snapshot (`Gfx::glyph_snapshot`) with no
`gsave` level, so an empty-stack `cliprestore` fell back to the newest
outer `gsave`'s clip, and an unmatched `grestore` popped the caller's
`gsave` (or, after #505, reset to the job's initial state at the bottom).

`Gfx::glyph_enter` (used in `src/font.rs` when it starts a
glyph) takes the same snapshot and then does a `gsave`, which also clears
the `clipsave` stack for the new level. Nothing else was needed: the
existing `cliprestore` fallback (`saved.last()`) and `grestore` now find
the state at `show`. `restore_glyph_snapshot` still truncates the stack and
puts the snapshot back at the end of the glyph, so sealing is unchanged.
`do_save`'s own snapshot is untouched.

## Still different

After a glyph whose procedure did an unmatched `grestore`, gs's state is
what the glyph's pops left (the end-of-glyph `grestore` pops one level
too many: the caller's line width came back as the job's initial `1.0` in
the probe). pscat keeps sealing the glyph, so the caller's state is the
one at `show`. The same goes for `grestoreall` in a glyph: gs leaves a
page-wide clip after the `show`, pscat the clip at `show` (the pinned case
stops printing inside the glyph). Deliberate: the sealing is what makes a buggy
`BuildChar` harmless, and no committed file unbalances one.

## Evidence

- `tests/glyph_cliprestore.rs`: 17 programs pinned to gs output (including a glyph that errors under `stopped`: no level is left behind), a
  local-only gs re-run, and a painting check (the second fill after a
  glyph's `cliprestore` stays under the clip at `show`). Without the
  change the pinned-lines test and the painting test fail.
- Identity: see the PR (`scripts/identity_check.sh --base-bin/--head-bin`).
