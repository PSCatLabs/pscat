## Paint loading L4: pkfan adopts `/Colors`, `/ColorMix`, `/Light` (issue #224, 2026-10-13)

Slice L4 of #183's design (`docs/PAINT_LOADING.md`, sections 2-3 and 6). `pkfan`
now takes a double-loaded brush, with `pfub` as the head position. With the keys
unset every render is byte-identical: each new branch tests `pfColorRec null
ne`, and nothing draws a random number.

- The jitter draw is stored, the colour applied per run. Each bristle still takes
  its one `pfColorJitter pkjit` draw where it always did; loaded, it goes into
  `pfcj` instead of being applied. `pfcolor` sets the colour just before the
  `pkribbon` call in `pfrun` and `pfray`: `puat` at `pfub` and the run's flip,
  `puwet`, then `1 pfcj add shade`. Colour is resolved once per run, not once
  per bristle as section 3.3 sketches, because the flip depends on the run's
  subpath under `/Light`; the render is the same, and the cost is a handful of
  array reads per run.
- `/Light` flips per subpath with the same `puflips` as L2 (`/Angle` 0, pitch
  `/Pitch`). A bristle's runs span every subpath of the path, which is why the
  colour could not stay per bristle. A pressed fan (a bare `moveto`) is a
  degenerate subpath and is never flipped; its rays radiate through 140 degrees
  and `/Colors` runs across that arc, the first colour on the left of the axis.
- Tests: `fan_loading_unset_is_byte_identical` (nine scenarios pinned against
  `origin/main` at f1eae7d), `pkfan` joins `LOAD_PRESETS` so the design's tests
  2-5, 9 and the per-subpath flip run on it too (dense, whole-pixel bristles so a
  hard `/ColorMix 0` step lands on a pixel edge), plus a uniform load with broken
  runs and pressed rays, the pressed-fan arc, one bristle's runs flipping by
  subpath, no-draws with broken runs and the advertised keys. Mutation-checked:
  colouring every run from the first subpath's flip fails the two flip tests, and
  dropping `puwet` fails both `pkwet` tests.
- Not here: `/Touch` on `pkfan` (S3), `pkdab`/`pkoil` (L5), the specimen (L5b,
  #227, which is where a loaded fan gets its demo).

