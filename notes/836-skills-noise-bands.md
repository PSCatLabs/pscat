## Skills: native noise, `--bands`, `--profile`, `writereals` (issue #836, 2026-10-26)

By the time this was picked up, #780, #796, #808 and #813 had already put most
of it into `plugin/skills/pscat/SKILL.md`, and `psart` had a Noise bullet. The
gap was what an agent would *reach for*: nothing in `psart` said that a heavy
per-pixel render is one `fbm2` per pixel plus `--bands`, and nothing showed a
band-aware data procedure.

- `psart`: a "Heavy noise renders" section with one runnable band-aware
  `image` example (cloud.ps: plain, `--bands 3/4/7/8` at 72 and 150 dpi, and
  `--transparent` / `--halftone`, all byte-identical to the plain render),
  the `systemdict /fbm2 known` probe, and the `oct` range (0..32; more is
  `rangecheck`).
- `pscat`: the `--profile` paragraph was in the file twice (removed one);
  added the `--profile-ops` hint (an anonymous `colorimage` data procedure
  profiles as "0 of 0 procedures", so the per-operator table is the useful
  one), what `--bands` refuses, that `pscat-mcp` has no bands, and a short
  `writereals`/`readreals` example (fenced `partial`: it needs `--allow-write`).
- `tests/skill_blocks.rs` runs the new `psart` block on a bare install.
