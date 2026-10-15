# #494: Level 3 `clipsave` and `cliprestore` (2026-10-14)

## Decision

pscat now has `clipsave` and `cliprestore`, with Ghostscript 10.07.1's
semantics, pinned case by case before any code was written. The stack is
a field of `GraphicsState` (`clip_stack: Vec<Option<ClipState>>`, `Rc`
clones, so a `clipsave` copies no mask).

## Semantics (all probed in gs first)

- `clipsave` pushes the clip in force onto the current level's stack.
  `cliprestore` pops the newest entry and makes it the clip. Only the clip
  changes: the CTM, colour and current path are untouched.
- **Each `gsave` starts a new, empty level.** The copy `gsave` pushes keeps
  the outer entries, so `grestore` brings them back, and entries made
  inside the level go away with it. (gs's `gs_gsave` clears the new
  state's clip stack.)
- **`save` starts a new level too** (gs's `save` does a `gsave`). `restore`
  brings back the entries from save time and drops the ones made since.
- **A `cliprestore` with no entry at its level is not a no-op.** It sets
  the clip to the one the level started with: the clip at the `gsave`, or
  at the `save` if no `gsave` has opened since, or no clip at all at the
  top level. So at the top level, `10 10 50 50 rectclip cliprestore`
  clears the clip back to the whole page. This **corrects** the issue body
  and `notes/454-cliprestore-in-a-batch.md`, which called an unpaired
  `cliprestore` "a silent no-op". That holds only when no clip was made
  since the level opened. drykit's shadow marks the batch `/dirty` on
  every `cliprestore`, so the shadow was already right for this case.
- The stack travels with gstate objects: `gstate` and `currentgstate`
  capture it, `setgstate` puts it back, including an empty one.
- `initclip` and `initgraphics` keep the stack. `grestoreall` empties it
  (gs's ends in a fresh `gsave` level); probed at a save boundary, where a
  `cliprestore` after it gives back the clip at `save`, not the entry
  `clipsave` made before the `save`.
- Both are `operatortype` in systemdict.

## What shipped

- `src/gfx.rs`: the field. `Gfx::clipsave`, `Gfx::cliprestore(boundary)`
  and `Gfx::clear_clip_stack`. `gsave` clears the new level, and
  `init_graphics` carries the stack across.
- `src/interp.rs`: `do_save` clears the stack after its snapshot.
  `do_cliprestore` passes the innermost save's depth and state. The
  fallback uses `saved.len() <= depth`, not `==`, because pscat's
  `grestore` can pop below a save's depth (below).
- `src/ops/graphics.rs`: the two operators.

## Not done

- **Type 3 `BuildChar`.** gs runs it inside a `gsave`, so a `cliprestore`
  in a glyph procedure with no entry there gives back the clip at `show`.
  pscat's glyph context is a snapshot, not a `gsave`, so it falls back to
  the newest outer `gsave` instead. Nobody calls `cliprestore` in a glyph,
  so this is not filed.
- **`grestore` at a boundary still differs from gs**, as it did before this
  change. At the bottom of the stack pscat's `grestore` is a no-op, while
  gs's restores the job's initial state. At a `save` boundary pscat pops
  below the save. Two probes (P1, P3) differ for that reason alone, so
  they are left out of the test.
- `currentgray` prints `0.49999999999999994` for `0.5 setgray` in pscat,
  where gs prints `0.5`. This difference is older than this change, and the
  test rounds the value.

## Evidence

- `tests/clipsave.rs`:
  - `clipsave_and_cliprestore_print_what_ghostscript_prints`: 20 programs
    (pairs, nesting, levels opened by `gsave` and `save`, `restore`,
    `grestoreall`, gstate objects, `initclip`, `initgraphics`, the CTM,
    colour and path left alone, operator types). They print the clip's
    bbox, and pscat's output matches the pinned gs lines.
  - `ghostscript_prints_the_pinned_lines`: re-runs the same programs
    through gs. It runs locally only.
  - `cliprestore_brings_back_a_nested_eoclip_exactly`: the restored nested
    even-odd clip fills byte-identically to the clip itself, and
    `.sameclip` (#488) is true, so the very chain comes back.
- `tests/drykit.rs` `a_cliprestore_in_a_batch_renders_as_at_once` now uses
  the real operators (the stand-in procedure pair is gone). drykit
  installs the shadow in pscat, its probe asserts that, and the three-way
  check (shadow alone, `.sameclip` alone, neither as the control) still
  holds.
- Exporters: a page with an outer `rectclip`, `clipsave`, an `eoclip`
  ring, a fill, `cliprestore` and another fill. `--pdf` rasterised by gs
  is byte-identical to gs rendering the program directly. `--svg` puts the
  second fill under the outer rect's `clipPath` id alone.
- Identity: see the PR (`scripts/identity_check.sh --base-bin/--head-bin`).
