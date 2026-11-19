# 1251: hybrid shadings need /Extend [true true] after #1040

Since #1040 a shading dict with no `/Extend` means `[false false]` (PLRM, and gs). Two hybrid examples
relied on the old always-extend behaviour; both now say `/Extend [true true]`:

- `examples/hybrid/sky_talk.ps` `vramp` (used by the `/PscatOnly` sky and sea). Without it the 3 pt
  overlap at the horizon is bare paper: a one-pixel white row (246, 246, 247) at row 499 of
  `docs/guides/hybrid/sky_talk_compare.png`, panel (b). The comment above `vramp` now says what
  `/Extend` does.
- `examples/hybrid/alpine_trail.ps`, the `/Stage /Reference` sky `shfill`. Its `/Coords` run exactly
  over the clip (y 380 .. PH), so no pixel changes today; the key is there so the clip can grow.

Renders checked with a release build of this branch (no source change under `src/` or `lib/`):

- `docs/guides/hybrid/sky_talk_compare.png` re-rendered with the command in
  `examples/hybrid/compare_sky_talk.ps`: byte-identical to the committed sheet, which predates #1040
  and so has no seam. The same command on the tree without the fix differs from it only in the
  seam row (bbox x 426..823, y 497..601 of panel (b)). So the committed sheet needed no change.
- `gallery/renders/hybrid_sky_talk.png`, `hybrid_alpine_trail.png`: the Hybrid stage draws neither
  shading, so re-rendering gives byte-identical files.
- `examples/hybrid/alpine_trail_ref.png` (the Reference stage): fix and no-fix render the same
  pixels. The committed file differs from both in rows 3..729; that predates this change and is
  left alone, because it is the image the image model was given.
- `plugin/.claude-plugin/icon.png` and `art/logo/renders/pscat-logo-d-512.png` re-render
  byte-identical (the logo uses no shading).

A grep of `lib/`, `examples/` and `gallery/` found no other shading dict without `/Extend`.
