# Handoff: compute-kernels epic (2026-11-09)

Session: https://claude.ai/code/session_016y8NtToNBqdyScG3LMGriu

## Where things stand

- Epic branch `feature/compute-kernels` is at `8d75160` and pushed. It is
  PR PSCatLabs/pscat#1061 (open), which addresses #742 (GPU/SIMD
  acceleration, asked for by ArtColony/fractal_clouds).
- Workflow: each follow-up is a feature branch off the epic, done by a
  subagent in a worktree, reviewed, then merged back into the epic. Codex
  is usage-limited until **Nov 11, 9:34 AM**; until then a fresh-agent
  review stood in.
- Merged follow-ups: #1062 (loop names), #1063 (rand), #1064 (shared
  subroutines, `.kernelshare`), #1065 (MCP/CLI `--gpu-preview`), #1066
  (`lib/kernel.ps` shim), and the four filed by the fractal_clouds
  session: #1091–#1094. Each has a `notes/NNNN-*.md` file.
- #1091–#1094 are still **open** on GitHub: they are merged into the
  epic, not into main. Close them when #1061 merges, or comment with
  the merge commit if you prefer.

| issue | what changed | branch tip |
|---|---|---|
| #1091 | Size cap counts only subs charged on the kept path; DCE per sub; `MAX_INSNS` 400k; `MAX_SUB_MADE` 1.6M (bounds memory); `/auto` falls back to the first pass | `feature/compute-kernels-1091-size-count` |
| #1092 | Retire reasons per site (`exec::Why`, `Kernel::sites`); soft branches read off the finished kernel; bool arrays compile as 0/1 tables. It also fixed a bug that could give wrong output | `feature/compute-kernels-1092-retire-reasons` |
| #1093 | Per-pixel fallback when the kernel's reachable code is pure (`src/kernel/pure.rs`) | `feature/compute-kernels-1093-pure-fallback` |
| #1094 | Compile time 8–14× faster: indexed scopes, copy-on-write scope names, one register allocation | `feature/compute-kernels-1094-compile-time` (`8d75160`) |

#1093 has no effect on the fractal_clouds scenes yet: their setup helpers
are reachable by name and use `copy`, `astore`, `cvx` and `begin`, so
the kernel's code counts as impure.

## Last verification (on `8d75160`, release build)

- `--test kernels` (89 tests) and `--test kernel_shim` (11) pass.
- `fmt --check`, `clippy --all-targets --release -D warnings` and
  `build --release --features gpu` are clean.
- All four gallery kernel pieces are byte-identical to `gallery/renders`.
- fractal_clouds at `34a977f`, 160×100, each `cmp`-identical to a fresh
  `/interpret .kernelmode` render:
  - golden_coast: compile 605 ms, 74,066 instructions in 182 shared
    subroutines, frame 0.99 s (about 28 s interpreted; 9.1 s before
    #1094).
  - mountains: compile 25 ms, 35,636 instructions, frame 233 ms.
- **Always compare against a fresh interpreted render** made from the
  same fractal_clouds commit. An old reference caused one false alarm.
  Verify before pushing.

## To do

1. **Full `cargo test --profile ci`** on the epic. It has not run since
   these merges; the earlier attempt and its watchers were killed under
   memory pressure. Two failures are known and are not from this work:
   - a typewrite test that also fails on clean `origin/main`;
   - the MCP concurrent test, which depends on the environment.
2. **Codex review of #1091–#1094**, and #1064's last round, after
   Nov 11 9:34 AM. Use `/codex:rescue`, or the codex plugin's review
   path.
3. **fractal_clouds-side changes, not yet filed.** Ask the user before
   filing an issue there. Two places in `lib/fc_trees.ps` make pixels
   retire to the interpreter:
   - `fc-tree-species` / `fc-crowns` write into the outside array
     `FsMixWt`. Make it a per-call local array. The note reads "put into
     an outside array or string … in fc-crowns > fc-tree-cell >
     fc-tree-species".
   - At about line 1618, an `ifelse` in `fc-tree-shade-trunk` leaves
     different stack types on its two branches. Rewrite it in the
     `aload pop` form.
   - Effect: pnw_coast retires 5,596 pixels and tropical_island retires
     all 16,000. golden_coast also reports the first note, but it costs
     little there.
   - `notes/1092-kernel-retire-reasons.md` has the exact rewrites.
4. **Blocked comment on ArtColony/fractal_clouds#124.** The auto-mode
   permission check denied it as an external write. The user needs to
   post it, or allow it.
5. **Optional: a new test build for fractal_clouds.** The existing one is
   dry-run release run 42407578124, built from `34e9d58`; its artifacts
   expire 2027-02-05. It predates #1091–#1094. A new
   `workflow_dispatch` dry run of `release.yml` on the epic would give
   fractal_clouds the fixes, and fc#117 holds the test steps to reuse.
   A dry run makes artifacts only, no release. Real releases come only
   from a `v*` tag on main, with owner approval.
6. Clean up the agent worktrees under `.claude/worktrees/` (some are
   locked) once their branches are no longer needed.

## Useful paths

These are in the session scratchpad and may be gone:
`/private/tmp/claude-501/-Volumes-YOTUO-dev-pscat-gpu/eec74d6d-79d7-4148-a83c-3ce46938927c/scratchpad/`.

- `fc/`: a fractal_clouds clone. It holds the drivers `t_km_160.ps` and
  `t_km_160_interp.ps` (mountains), and `renders/k117/` with
  `gc.ps`/`gc_interp.ps` (golden_coast), the other scenes, and the
  driver generators `mk.sh`/`mk2.sh`.
- `w1094/`: bench and IR-dump compare scripts (`bench.sh`,
  `dumpcmp.py`, `gallery.sh`).

The mountains driver recipe is in fc#117, step 3. Build it again from
there if the scratchpad is gone.

Useful environment variables:
- `PSCAT_KERNEL_TIMING=1`: compile and run timings, plus size notes.
- `PSCAT_KERNEL_DUMP`: dumps the IR.
- `/interpret .kernelmode`: gives the interpreter reference.
