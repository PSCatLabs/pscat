# Handoff: compute-kernels epic (merged 2026-11-12)

Session: https://claude.ai/code/session_016y8NtRr7b9NWRg1SctzFfp. It wrote
the 2026-11-10 and 2026-11-11 handoffs and this update. The 2026-11-09
handoff was session_016y8NtToNBqdyScG3LMGriu.

## Where things stand

- **The epic is merged.** PR #1061 was squash-merged to main on 2026-11-12 as
  `e68cb99`, with the owner's go-ahead once CI passed. The push-to-main
  runs were green. It closed #742, #1062–#1066, #1091–#1094, #1099, #1101
  and #1102. The epic branch and its 13 `feature/compute-kernels-*` side
  branches are deleted.
- **Merging main in** brought two fixes: a text conflict in
  `plugin/skills/pscat/references/mcp.md` (both sides had added a section)
  and a missing `site/llms.txt` entry for `docs/guides/kernels.md`. Main's
  site check (#1125) requires every guide to be listed, so it failed.
- **Notes per issue:** `notes/1101-faster-executor.md`,
  `notes/1102-kernelarray.md`, `notes/1100-simd-and-vector-math.md`.
- **#1100 stays open**:
  - **x86 timings:** to be measured once the cloud MCP server is deployed
    on x86.
  - **AVX-512:** it needs Rust 1.89, above `rust-version` 1.88.
  - **`/fast` is now the default** (owner's decision, 2026-11-12):
    `.kernelmath /fast` is on unless `PSCAT_KERNEL_MATH=exact`, and a
    program's own `.kernelmath` wins over either. Its output is the exact
    path's bits: the four kernel gallery stills are byte-identical, and
    Mandelbulb runs in 72.8G cycles against 89.2G.
- **Test release for fractal_clouds:** 97b481d (CI run 43122833835), which
  fractal_clouds validated for bytes, steps and speed (fc#117 comment
  6810569860). It stays on that release until a real one is cut.
- **fractal_clouds' fc#16 native-noise notes (2026-11-12):**
  - Its scenes have no interpreter fallbacks.
  - alpine_forest's forest-bake grid (`kernelarrays`, 4 outputs) is at
    398,033 of the 400,000 inline cap, with 0 subroutines.
- **Follow-ups, not filed:** exact vector noise, divergence, kernelfill,
  two-pass antialiasing, `--gpu-preview` over MCP, and #1044's buffer API.

## Next executor levers (only if wanted)

- **Exact vector noise.** fractal_clouds' cost is mostly noise and the
  block loop, not trigonometry.
- **Divergence.** 8.3–12.3 of 16 lanes are active on average. Coherence or
  compaction are the candidates.
- **Moves.** Coalescing them is worth about 3%.
- **`Gather1`** is still scalar inside its handler.
- **`pow`/`ln`/`log` vectorised.** They are table-driven, about 2–3% of the
  pieces measured.
- What was learnt is in `notes/1101-faster-executor.md`: the cost is
  instructions per dispatch, and lane arrays get scalarised inside
  `Machine::block`. Keep new handlers' bodies out of `block`: inline, they
  made it 6% larger (#1100).

## How to test against fractal_clouds

- **Temp files go on external storage,** under
  `/Volumes/YOTUO/dev-cache/pscat_gpu/`, not `/tmp` or the session
  scratchpad (the user's rule; the scratchpad was wiped mid-session).
  `env.sh` there sets `TMPDIR`. The test kit is in `kernels/`:
  - `bench.sh PSCAT W H SCENE...` renders compiled and `/interpret`
    versions, then runs `cmp` and prints compile, run, total time and RSS.
    Run it from a fractal_clouds checkout made with `git -C
    ~/dev/fractal_clouds archive <commit> | tar -x -C <dir>`, plus `mk.sh`
    and `mk2.sh` copied into its `renders/k117b/`. `NOINTERP=1` reuses
    existing references.
  - `samelook.py a.png b.png [diff.png]` is fractal_clouds' SAME-LOOK
    metric: PSNR ≥ 40 dB, p99.9 ≤ 8, mean ≤ 1.5.
  - `build1-747ccee/` and `build2-46ad11b/` are the builds fractal_clouds
    verified. build1's SHA-256 is `9adc198b…b4f`.
  - `ab/runk.sh BIN TAG SCENE W H` renders one compiled scene with
    `/usr/bin/time -l`. It prints the run, the steps, the instructions
    retired and the cycles.
    - The cycle counts are steadier than wall time on a loaded machine.
      The prelude alone (a 16×10 kernel) is about 72G cycles for
      golden_coast and 49G for mountains: subtract it.
    - `ab/baseline.sh BIN TAG` runs the whole #1101 acceptance set, and
      `ab/gal.sh` the gallery pieces.
    - `ab/fc9.sh BASE NEW TAG` renders the 9 fractal_clouds scenes at
      160×100 with both builds, then compares the PNGs and the step
      counts.
  - An x86_64 build runs here under Rosetta (SSE4.2, no AVX):
    `rustup target add x86_64-apple-darwin`, then build or test with
    `--target x86_64-apple-darwin`. Use a separate `CARGO_TARGET_DIR`.
  - `microbench/microbench.rs` is a ns-per-dispatch harness. Copy it into
    `src/kernel/` and add `mod microbench;` to use it, then remove it again.
- **Drivers** come from fractal_clouds' `renders/k117b/mk.sh` and `mk2.sh`:
  `mk2.sh SCENE NAME W H`. A `_interp` twin appends `/interpret
  .kernelmode` to line 1.
- **Always compare** against a fresh `/interpret` render from the same
  fractal_clouds commit.
- **fractal_clouds' bar is "looks the same"** (SAME-LOOK), not exact
  bytes. The user agreed that close output is fine when the deviation is
  understood and documented. But pscat's default path stays exact, so any
  approximation must be opt-in. (`/fast` is the default because it is not
  an approximation: the same bits.)
- **The fractal-clouds Claude session** has been the test partner. It runs
  the 8-scene check plus 640×400 and 1280×800 of mountains and golden_coast
  on any build path and SHA-256 you send it. It can also change
  fractal_clouds PostScript (the owner allowed it) when a branch won't
  compile. Its fixes so far: fc#142 (`FsMixWt`) and fc#143 (the trunk
  `aload`).

Environment variables: `PSCAT_KERNEL_TIMING=1` (timings, sizes, every
fallback reason), `PSCAT_KERNEL_DUMP` (the IR), `PSCAT_KERNEL_THREADS=N`,
and `/interpret .kernelmode` for the reference.
