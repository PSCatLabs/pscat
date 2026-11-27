## Transparency groups: the painted-row scan looks at the limits (issue #1421, 2026-11-27)

A run stopped on a limit with groups open drops each layer through
`Gfx::composite_layer`, which first scanned the whole layer for painted rows.
The 64-row limit checks came only in the composite loop after it, so a blank
layer (no painted row) was read to its end with no look at the limits: four
nested blank 8000 x 8000 groups were about 1 GB of reads after the stop.

- The scan is now a loop that looks at the limits every `GROUP_POLL_ROWS`
  (64) rows, with the pixels read so far as the work (a step per 1024). A
  stop returns that work, not the layer's size, and the group still closes
  (the caller had already swapped the page back and restores the `gsave`
  level), so the canvas, the saved-state stack and the page are as they were
  before `begingroup`.
- The list of painted rows is asked of `--max-memory` before it is made
  (8 bytes a row, `Vec::with_capacity` so the growth is the figure asked).
- With the time limit fired (`expire_group_probe`), each group now costs one
  look at the clock and no scan. Not done: a painted-rows high-water mark,
  which the poll makes unnecessary.
- No render changes: the scan answers the same rows on every run that is not
  stopped. Tests are unit tests in `src/gfx.rs` on a 4 x 100 000 canvas (many
  rows, few bytes), counting looks with `limits::CHECKS`.
- Codex round 1 on PR #1462: a group closed with no operator to report it
  (the end of the program, `showpage`, `copypage`, `erasepage`, `restore`,
  a stop out of the group) dropped the row-list refusal, so the group was
  silently not composited and the run went on. `Gfx::finish_group_uncharged`
  now latches a `MemoryExceeded` outcome in `mem_refused`, which the
  interpreter's between-slices check raises, as for the PDF recorder's
  refusal. `tests/groups_memory_implicit.rs` runs each closing form under a
  cap that holds the layer but not the list. A time limit needs no latch (the
  deadline stays past); a memory one does (the heap falls back once the layer
  is dropped, and the row list was only asked for).
