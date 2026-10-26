# --sweep-seed also seeds noise (#753)

The native noise operators (#734) read their own table, built by
`noiseseed`, so `--sweep-seed` (which only overrides `srand`) gave every
cell of a noise-only piece the same picture.

Decision: a sweep seed also seeds noise, by default; no second flag. A seed
sweep means "vary the randomness", and a separate `--sweep-noise-seed` would
leave the obvious command quietly doing nothing for noise art.

- `Interp::set_seed_override(Some(s))` builds the noise table from `s`, as
  `s noiseseed` would, before the program runs (`noise_from_override`).
  That is the one hook both `pscat FILE --sweep-seed` and `pscat sheet
  --sweep-seed` use, so they agree.
- A program that calls `noiseseed` keeps its own table: its call clears
  `noise_from_override`. (Unlike `srand`, which the override replaces: found
  art hardcodes `N srand` and is meant to be re-rolled; art that seeds its
  noise on purpose has chosen its look.)
- Sampling the override-seeded table counts as the override firing, so the
  "never called srand or sampled noise" warning stays quiet for a noise-only
  piece and still fires for one that seeds its own noise and never calls
  `srand`.
- `set_seed_override(None)` restores the default table only while the table
  is still the override's.
- Byte-identity: programs that use no noise render the same sweep cells
  (checked with a base binary against the head binary, `cmp` on the PNGs). No
  tracked lib, gallery or example file samples native noise yet, so no
  existing sheet changes.
- `review_sheet` (MCP) has no sweep input today: out of scope here, tracked in #744.
