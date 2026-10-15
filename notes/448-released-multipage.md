# #448: multi-page overlays for rel-page

Follows #214 (`notes/214-released-look.md`). `rel-page` wrapped one drawing: its
copy chain is a stack of clipped, transformed frames, and `tw-type` ends a full
page with `showpage`, which resets the CTM and the clip and so cuts the chain
in two. Only the paper (`rel-paper` in `/PageStart`) reached later pages.

## What shipped

- `lib/typewriter.ps`: `tw-type /PageEnd` (`lyPE`, guard
  `tw-pageend-must-be-a-procedure`), run as `/PageStart` is (`gsave`, `TWState`
  ended, `grestore`), after the page number of every page, the last included,
  before the `showpage` that starts the next page. With none set nothing runs,
  so no render changes.
- `lib/released.ps`: `rel-page /Paged true`, plus `rel-page-start` and
  `rel-page-end` as the two hooks. `rlRun` is split (`rlSheetUp`) and
  `rlOriginal` is split (`rlOrigHead`, `rlOrigTail`) so the single-page path and
  the paged path share the same operators in the same order. A one-page paged
  document is byte-identical to the unpaged `rel-page` (a test, and checked on
  `gallery/released_memo.ps`).
- `/Page` on a stamp or note dict: that page of a paged document only; none, every
  page. The staple and the clips are on the first sheet only (they hold the
  stack at its top sheet). `rel-stamp` and `rel-note` ignore `/Page`.
- Each sheet has its own seed (`/Seed` plus 101 a sheet, the first unchanged), so
  its specks, shifts and blotches differ from the last one's.
- `examples/released_pages.ps`, a two-page memo; renders in
  `docs/dev/design/released_pages_1.png` and `_2.png`.

## The one trick

The frame (scanner tilt, sheet clips of every generation, dropout clip) has to be
in force while `tw-type` types the page, but `tw-type` calls a hook as
`gsave hook grestore`. `rel-page-start` therefore begins with a `grestore` (the
hook's own gsave), builds the frame, and ends with a `gsave` that `tw-type`'s
`grestore` pops, leaving the frame in place; `rel-page-end` does the reverse. The
hooks must be the whole hook procedure and called by `tw-type` (a `gsave` before
them would make the first `grestore` pop the wrong level). `showpage` between
pages is at the caller's level, where the stack is balanced.

## Left alone

- Marks repeat on every page unless a `/Page` is given; there is no per-page
  variation of a stamp's own seed.
- `/Paged` is for `tw-type`: another multi-page drawing can call the hooks too, if
  it runs them as tw-type does.
