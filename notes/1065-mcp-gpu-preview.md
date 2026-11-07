## pscat-mcp: a gpu_preview option for kernelimage renders (issue #1065, 2026-11-06)

`render_postscript` and `start_render` take `gpu_preview` (a boolean). `true`
passes `--gpu-preview` to the `pscat` child, so compiled `kernelimage`
procedures run on the GPU in f32. Through the server on an Apple M4,
`gallery/mandelbulb.ps` takes 0.8 s with it and 8.0 s without.

**The design question.** pscat-mcp always runs its child under `--max-time`
and `--max-steps`. Until now, `kernelimage` refused the GPU under any limit,
because a GPU dispatch cannot be interrupted. Passing the flag through as it
was would have done nothing under MCP. There were three options:

1. Pass `--gpu-preview` and document that it has no effect under limits.
   That is honest but useless, since every MCP call has limits.
2. Drop the interpreter's time and step limits from a `gpu_preview` child,
   and make the process kill at `--timeout` the guard. This bounds a
   runaway, but badly. A runaway anywhere in the program, such as a plain
   `{ } loop` before the `kernelimage`, would then come back as a bare
   `timeout` with no page, not as `limit_exceeded` with the partial page.
   The step limit, the bound that does not depend on the machine's speed,
   would be gone. The whole render would lose its guard for the sake of one
   operator.
3. **Chosen:** keep every limit on the child, and make the GPU path obey
   the limits. `gpu::run` already submitted the image in dispatches of
   `GPU_CHUNK_PIXELS` (65536) pixels and waited for each one. It now calls
   the CPU path's `RunCtl::poll` before each dispatch:
   - **Time.** A limit that has fired stops the image with the error the
     CPU path raises (`TimeExceeded`, `StepsExceeded`): the same
     `limit_exceeded` and partial page.
   - **Steps.** Each dispatch is charged the kernel's straight-line size
     per pixel, `px * k.size()`. Loop trips are not counted; the shader's
     per-pixel fuel caps them at `GPU_LOOP_CAP` (2^20).
   - **The process kill.** One dispatch is at most 65536 pixels × 2^20
     trips. It is the only part that cannot be interrupted, and the server's
     wall-clock kill covers it. The interpreter's time limit is the timeout
     less a quarter of it, at most 5 s, so the kill comes later.

   Measured with a kernel that runs the whole loop budget on every pixel (a
   loop whose result is the pixel, so Metal cannot drop it, and whose exit
   is never taken):
   - 360,000 pixels took 3.0 s, about 0.5 s per dispatch.
   - At 2000×2000 under `--timeout 8`, the time limit fired at 6.4 s with
     `limitexceeded (time)` and the partial page, the same as the CPU
     (6.0 s).
   - Under `--max-steps 1000000`, the step limit fired before the first
     dispatch.

   This changes the CLI as well: `--gpu-preview` with `--max-time` or
   `--max-steps` now uses the GPU. Keeping two rules for one operator
   seemed worse. `docs/guides/kernels.md` and `pscat --help` say so.

**Memory.** The GPU path now checks the image buffer against
`--max-memory` before it runs. Over the limit, it leaves the image to the
CPU path, which handles that case.

**Builds without the feature.** In such a build, `gpu_preview: true` is a
`gpu_unavailable` tool error. It names the argument
(`structuredContent.error.argument`) and the cargo feature, and
`start_render` refuses it before any job starts. The server knows the
child's features from its own: `pscat_bin()` runs the `pscat` beside
`pscat-mcp`, from the same cargo build, so `cfg!(feature = "gpu")` holds for
both. For a hand-copied mismatched pair, the child's own refusal ("--gpu-preview
needs a pscat built ...") with no pages maps to the same error. `false` is
a plain render in every build. The argument's description and both tool
descriptions say which build the server is.

**Not on `review_sheet`.** A sheet promises the bytes `pscat sheet` writes,
and GPU pixels are approximate. `handwrite` and `chalkboard_math` draw no
kernels.

**Bands.** `gpu_preview` works with `bands`. Each band's `kernelimage`
computes only its own rows, and the bands share the one GPU context through
its mutex.

**Tests.**
- `tests/mcp.rs`: the schema and the descriptions, refusals of non-booleans,
  `false` as a plain render, and (by feature) the `gpu_unavailable`
  refusal, or a GPU render plus a runaway kernel stopped by the step limit.
- `tests/gpu_preview.rs` (gpu builds only): the time and step limits stop a
  runaway GPU kernel, and a normal kernel completes under limits.

**Deferred.**
- The steps charged on the GPU are a lower bound. They are not comparable
  with the CPU's count for kernels with loops.
- A GPU failure after some dispatches has already charged their steps, and
  the CPU run then charges its own.
- **A runaway loop is an image on the GPU, not an error.** The GPU stops
  each pixel at the loop cap, and the image comes back. The CPU path stops
  the same program with `limit_exceeded`. The GPU gives that error only when
  the capped work runs past a limit. A loop with no `exit` at all compiles
  to a constant colour; the shader compiler then drops the loop, and the
  picture is black. The lowering is from #742, but this branch is the first
  to let MCP users, and runs under limits, meet it. The docs and the
  argument's description say so. A fix for the owner to weigh: under a
  limit, have `lower` refuse (`Failed`, so the CPU runs it) any loop with
  no reachable `Break`. That covers only the no-exit case. A shader-side
  "ran out of fuel" flag, painted magenta like `Abort`, would cover the
  rest.
