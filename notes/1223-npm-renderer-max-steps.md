## npm Renderer: maxSteps, frameMaxSteps and stepsUsed (issue #1223, 2026-11-27)

The wasm module and `web/pscat.js` already had a step cap (`ps_set_max_steps`,
`Pscat.maxSteps`, `ps_steps_used`, #1210); the playground re-arms it before each
`step()`. The npm `Renderer` exposed neither, so an npm consumer could not bound
an endless loop or a decompression bomb. It exposes no time or memory limit
either (the module has no clock), so there was no existing option shape to
copy; the page options (`width`, `dpi`, `svg`, ...) are the model.

### What shipped (npm only; no `src/` change)

- `PageOptions.maxSteps` (render, begin, frames): a cap on the whole program,
  counted from `begin()`. Off by default; 0, null and undefined are off, as
  `pscat --max-steps 0` is.
- `FrameOptions.frameMaxSteps`: the playground's shape, re-armed before every
  `step(budget)` in `frames()` so the count restarts each frame. It replaces
  `maxSteps` (the re-arm would drop the program cap), so giving both is a
  `TypeError`.
- `Renderer.stepsUsed`: the core's count (0 with no cap, 0 before `begin()`;
  trails by up to a slice; correct after an error and after a limit stop,
  where it is at least the cap).

### Decisions

- **`begin()` always arms the cap, 0 when absent.** The module keeps the last
  cap for every later program, so arming only when the option is present would
  let `render(a, { maxSteps: 1000 })` cap a following `render(b)`.
- **No public setter on `Renderer`.** A persistent `r.maxSteps = n` would clash
  with `begin()` resetting it; per-call capping is `frameMaxSteps`.
- **Validation in JS (`stepCap`)**, before any module call or state change: the
  f64 argument would otherwise turn `true` into 1 and `'100'` into 100. A whole
  number from 0 to `Number.MAX_SAFE_INTEGER`; larger, fractional, NaN, infinite
  or negative is a `RangeError`, a non-number a `TypeError`. A refused `begin()`
  leaves the program in progress alone.

### The cap is coarse (Codex round 1)

The runtime counts in slices of up to 4096 steps and enforces the cap between
slices; a full slice's overrun is deferred to the next call. `frames()` re-arms
(restarting the count) before that call, which erases a pending overrun. So
`r.frames('1 pop '.repeat(100), { budget: 11, frameMaxSteps: 10 })` finishes
with no error (19 frames, `stepsUsed` up to 11), while a runaway loop or a
bomb in a frame is stopped. Not changed on the Rust side; the contract is
documented (README, index.d.ts, browser guide) and the reproducer pinned in
`npm/test/steps.test.mjs`. `stepsUsed` is not clipped to the cap (1997 seen for
a cap of 1000). Both non-zero `maxSteps` and `frameMaxSteps` conflict; one of
them 0 is accepted.

### Deferred

- A `maxMemory` option (the Rust side has `Limits::max_memory`; whether it can be enforced inside the wasm module is not checked here).
- A way to read `stepsUsed` with no cap set.
