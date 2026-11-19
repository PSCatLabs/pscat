# #1030: the functional showcase PNG "drift" was a missing pHYs chunk

`scripts/functional_demo.py render` changed the bytes of `ground.png`,
`primitives.png` and `detail.png` on a fresh build, and nothing else.

Cause: those three were committed (#923) from a binary older than #957, the
change that makes PNG output write a `pHYs` (DPI) chunk. The other eleven were
rendered after it. Current output is **pixel-identical** to the committed
files (RGBA difference of zero, the compressed `IDAT` data identical); the only
difference is the 21-byte `pHYs` chunk. Two renders by one binary hash the same,
so there is no nondeterminism and no platform effect.

Fix: the three PNGs and their `render-manifest.json` hashes were regenerated.
The manifest also stopped recording the author's absolute path in the
`stdout` of the three posters (current binaries print the `pscat: wrote` line
on stderr, so the script's captured stdout no longer contains it).

Not done, deliberately: `MEASUREMENTS.json` still names stale hashes for
`lib/functional.ps`, `lib/artkit.ps` and `lib/paintkit.ps`. Its timings must be
re-measured with those hashes on a quiet machine (`functional_demo.py
benchmark`, load average under about 4); hashes are never rewritten without
re-measuring. `--provenance` stays out of CI: WASM-STUDIO.md explains that it
hashes engine and library files unrelated PRs change.
