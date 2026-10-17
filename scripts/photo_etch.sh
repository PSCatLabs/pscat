#!/usr/bin/env bash
# photo_etch.sh — photo in, line-engraving out (issue #15).
#
#   ./scripts/photo_etch.sh --max-dim 800 -o etching.svg photo.jpg
#
# A thin wrapper over `pscat photo-etch` (issue #600): the options, the PostScript
# driver and the rendering all live in the pscat binary now, which carries
# lib/ inside it and needs no checkout, bash, awk or sed. Run `pscat photo-etch
# --help` for the options. This script only finds a pscat binary and passes
# its arguments on ($PSCAT_BIN picks the binary), so existing docs and
# skills keep working.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)

# BIN resolution covers the layouts this same script file runs from
# unmodified: an installed bundle (pscat at the bundle root, this script one
# level down in scripts/), a dev checkout (cargo build --release), or pscat on
# PATH. Falls through to "trigger the cargo-build fallback below" if none
# exists.
if [[ -n "${PSCAT_BIN:-}" ]]; then
  BIN="$PSCAT_BIN"
elif [[ -x "$ROOT/pscat" ]]; then
  BIN="$ROOT/pscat"
elif [[ -x "$ROOT/target/release/pscat" ]]; then
  BIN="$ROOT/target/release/pscat"
elif command -v pscat >/dev/null 2>&1; then
  BIN="$(command -v pscat)"
else
  BIN="$ROOT/target/release/pscat"
fi
[[ -x "$BIN" || -n "${PSCAT_BIN:-}" ]] || cargo build --release --manifest-path "$ROOT/Cargo.toml"

exec "$BIN" photo-etch "$@"
