# #1013: PSCatLabs, not ArtColony, on the functional showcase

The functional PostScript showcase (#923) printed "ARTCOLONY" in three headers.
PSCatLabs is the brand (#927), so the headers now read:

- `examples/functional/common.ps`: `PSCATLABS PRESENTS  /  GENERATIVE SESSIONS No. 001` (poster, all colourways and the imperative control)
- `examples/functional/type-terrain.ps`: `PSCATLABS  /  FUNCTIONAL TYPE STUDY 02`
- `examples/functional/observable-press.ps`: `PSCATLABS  /  FUNCTIONAL TYPE STUDY 03`

`docs/functional-postscript/ARCHITECTURE.md`'s diagram now says "Artist / agent".
`docs/brand/org-profile-README.md` and the `notes/` mentions refer to the real
ArtColony org and stay.

## Rebuild

- `python3 scripts/functional_demo.py render --pscat target/ci/pscat` re-rendered the
  assets. Only the 12 PNGs that draw a header changed, and only inside the header line
  (checked pixel by pixel). `ground`, `detail` and `primitives` came out pixel-identical
  but with a new `pHYs` chunk, so the committed bytes are kept and their hashes are unchanged.
  The two Courier headers keep their width (same letter count, monospaced); the poster's
  centred Helvetica line re-centres.
- `render-manifest.json`: only the 12 `sha256` values changed. The poster `stdout` fields keep
  their original text, so a local absolute path does not leak in.
- `python3 scripts/build_functional_studio.py` rebuilt the offline studio payload, which also
  brings the WASM module up to today's engine (28 engine-source hashes, `lib/artkit.ps` and
  the JS API). `cargo test --profile ci --test functional -- --ignored` (saved PNGs and WASM
  vs native pixels) passes, as do the studio provenance checks.
- `MEASUREMENTS.json` is not re-benchmarked. Its `input_sha256` was already stale on `main`
  (`lib/functional.ps`, `lib/paintkit.ps`), and this change adds `examples/functional/common.ps`
  to the list. Only `--provenance`, which CI never runs, checks it. Rewriting the hashes
  without re-measuring would misstate what was measured.
