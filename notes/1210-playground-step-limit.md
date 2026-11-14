## The playground caps the steps of one frame, not of the whole program (issue #1210, 2026-11-14)

### The premise, and what measuring it showed

#1208 made decoded filter output count as steps, a step per KiB, so a step
limit stops one operator draining a decompression bomb. The issue asked for a
default step limit for the browser playground, "chosen so the gallery pieces
and examples still finish". Measured in the wasm build under node, that
cannot work as a limit on the whole program:

| | steps | rate in wasm |
|---|---|---|
| heaviest piece the playground offers (`basin_and_range`, 720x480) | 84,469,712 | about 45 million steps a second |
| heaviest `examples/` program that runs in wasm (`paintkit_wet_pull_demo`) | 860,513,157 | |
| heaviest gallery piece that runs in wasm without kernels (`alpine_lake`) | 2,326,240,602 | |
| a two-layer Flate bomb draining | 1 step per KiB | about 90,000 steps a second (88 MiB/s) |

A bomb is about 500 times cheaper per step than ordinary work, so any cap that
lets `basin_and_range` finish (84 million steps) lets a bomb drain 80 GiB,
about 15 minutes, and a cap that lets every `examples/` program finish is
hours. It is also out of reach for the page itself: the playground runs
`speed` steps a frame (220 by default, 4000 at most), so a program cap of a
billion steps is 70 minutes of frames at the top speed.

What freezes a tab is one `step()` call, so the cap that works bounds one call.
`ps_set_max_steps` caps the steps from the moment it is called, for the program
in progress as well as later ones (it calls `Interp::set_limits`, which
restarts the count); the playground calls it again before every `step()`:

    ps.maxSteps = FRAME_MAX_STEPS;     // 500,000
    ps.step(Number(speed.value));

Called once before `begin()` it is a whole-program cap (`run()` in node, the
endless-loop check in `tests/wasm.rs`).

### The number: 500,000 steps a frame

Largest charge of any single `step()` call, measured with the cap re-armed
before each call at 500,000, at the slider's top speed (4000) and default
(220); every program that finishes with no cap finishes with the cap on:

| set | programs | finish | most one call charged |
|---|---|---|---|
| playground-offered: the 19 `playground: true` gallery entries, the dropdown, 20 Learn examples (39 files) | 39 | 39 (speed 4000 and 220) | 4,652 at speed 4000 (`basin_and_range`); 275 at 220 |
| every `gallery/*.ps` and `examples/*.ps` (138) and the 20 Learn examples | 158 | 137 | 5,306 (`evening_stream`) |

The 18 that do not finish are the same 18 that stop in wasm with no cap at all
(they read files, or need a font pack: `undefinedfilename`, `*-font-not-found`);
the cap changes no result. Three gallery pieces, none offered in the
playground, stop: `mirror_garden`, `mandelbulb` and `dreaming_network` each
charge 5.6 to 27 billion steps in one `kernelimage` call (4 to 29 seconds of
one frozen call with no cap), which is exactly what a frame cap is for.
`?maxsteps=0` runs them.

500,000 is about a hundred times the most any offered frame charges, and a
two-layer bomb of a few hundred bytes stops after 500 MiB of decoded data,
5.3 seconds here (a 684 MiB bomb, stopped at 499,976 steps; uncapped it
drains in 7.2 s, so a 1 GiB one takes about 11, and a bigger one longer). A
kernel that charges under 500,000 steps in one operator still runs.

### What changed

- `src/wasm.rs`: `ps_set_max_steps(f64)` and `ps_steps_used() -> f64`. The cap
  is remembered per module instance and applied at each `begin`, as fonts are.
  Off until set, so `pscat.js` and the npm package behave as before.
- `web/pscat.js`: `maxSteps` setter, `stepsUsed` getter (the npm `Renderer`
  has no option for it: proposed, not done).
- `site/playground.html`: re-arms 500,000 before every `step()`; a
  `limitexceeded (steps)` shows a sentence saying what happened, the cap, the
  usual cause and `?maxsteps=N` (0 for none), then the raw report; the status
  line reads `stopped: step limit`. `site/style.css`: the error box grows from
  6em to 10em. `web/index.html` does the same re-arm.
- `tests/wasm.rs`: `wasm_step_cap_stops_a_single_operator_decompression_bomb`
  (whole-program cap, per-call cap, endless loop, lifting, bad values) and
  `wasm_playground_frame_cap_stops_a_bomb_and_passes_a_frame`, which reads
  the number out of `site/playground.html` and drains a 684 MiB three-layer
  bomb under it.

### Not done

The wasm clock (`max_time`) is still unenforced. A decode-specific limit
(bytes, not steps) would let one cap cover both rates; not needed now.
`site/learn/` and the gallery pieces were measured with default fonts only, as
the playground runs them.
