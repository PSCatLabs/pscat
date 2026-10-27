# #877: `--bands` prints stderr once

Every band runs the whole program, so `--bands 4` (8 bands by default for
a 100-pixel page) printed one `(%stderr)` line, `handleerror` report and
font stand-in note 8 times. From closed PR #872 only the stderr half was
taken; `--band-cuts` stays shelved (#869).

## What changed

- `Interp::quiet_stderr()` sets a flag; `Interp::write_stderr` is the one
  place program stderr output goes (the `%stderr` file sink in
  `ops/file.rs`, `handleerror`, `findfont`'s stand-in note) and drops it
  when the flag is set. Notes still collect in `font_warnings`.
- `bands.rs` sets the flag on every band except 0, as it already captured
  their stdout. `print_font_notes` prints, once and after the run, the
  stand-in notes lower bands raised that band 0 did not.

## Errors only in a lower band

- An uncaught error: unchanged and already correct. `run_source` returns
  `Err`, the band hands its report back, and `run` prints the first failing
  band's (`pscat: band 3/4 failed:` then the report) once and fails the
  exit code. It never went through `handleerror`, so quieting stderr does
  not touch it.
- An error the program catches itself and writes about (`stopped` plus
  `handleerror`, or its own `(%stderr)` write) in a lower band only: dropped.
  That is the program's own output from a copy whose page is cut away, the
  same call as for lower-band stdout. Collecting it would need a per-band
  log and a rule for dedupe against band 0's different text; not worth it
  until someone asks.

## Tests

`tests/bands.rs`: `stderr_prints_once`, `font_notes_are_deduplicated`
(one note raised in every band, one only in band 2), and
`an_error_in_a_lower_band_is_reported_once`.
