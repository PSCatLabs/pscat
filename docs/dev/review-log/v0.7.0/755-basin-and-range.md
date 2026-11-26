## gallery: Basin and Range, a band-aware fbm2/ridged2 relief map (PR #815, issue #755, 2026-10-25)

- **What changed / what to look at:** a new gallery piece, `gallery/basin_and_range.ps`: a shaded-relief map from the native noise operators. New piece, so there is no before. Look at the land and sea reading as terrain, and at the mountain belts against the lowlands.
- **Perceptual check:** the issue has none. Its intent: "one small piece, in Learn or the gallery, that uses `fbm2` and `ridged2` for terrain or a cloud field and states its render time."
- **Reviewer:** PASS, round 1 (see the PR's "Perceptual review, round 1" comment). polish: the shoreline is slightly jagged at 2x; the white peak caps are a little grainy.

| before | after |
|---|---|
| (new piece, no before) | ![after](https://github.com/PSCatLabs/pscat/blob/289abbe0bce919c7363dbdb34f2c757091355f88/gallery/renders/basin_and_range.png?raw=true) |
