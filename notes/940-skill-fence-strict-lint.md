## Skill fences run under strict lint (#940, #932), 2026-10-29

- **The charts never leaked.** `barchart`, `linechart` and `areachart` leave
  nothing on the stack (probe: all three, then `count` is 0). The stray `0`
  the QA pass saw came from the psviz examples, which passed seven numbers to
  the six-argument `setdvframe` (`y0 y1 px py pw ph`). `lib/dataviz.ps` is
  unchanged. The `{ dup sin }` plotfn example left 201 operands; the contract
  is `{ x -> y }`, so it is `{ sin }`. Rendered PNGs are byte-identical.
- **`tests/skill_blocks.rs` adds `--lint-strict`** to every runnable fence
  (stack-leak, blank-page). `partial` (not run) is the opt-out for a fence
  that deliberately draws nothing. The bare `(lib/...) run` fence in
  `paint.md` is now `partial`; the `setalpha` fence in `pscat/references/
  extensions.md` now draws a rectangle and pops its read-backs.
- **#932.** `paint.md` shows `x y w h << ... >> pkpaper` with a runnable
  line. `pkpaper` now checks `count 5 lt` first and raises
  `pkpaper-needs-x-y-w-h-and-an-options-dict` (the file's own undefined-name
  convention), so a dict-only call no longer leaves `/pvh` on the stack.
  Renders are byte-identical (`identity_check.sh --loader lib/paintkit.ps`).
  Not done: `pkwash` and the other presets may leak the same way (not checked) on too few
  operands.
