#!/usr/bin/env bash
# build_wasm.sh — build the browser build (Stage 16) and put the
# module where web/pscat.js expects it.
#
#   ./scripts/build_wasm.sh
#   python3 -m http.server -d web      # then open http://localhost:8000
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
# The crate is an rlib for everyone else (issue #565: a `cdylib` in
# Cargo.toml's crate-type made every downstream crate build one too), so ask
# for the cdylib here only. `--no-default-features`: the `window` feature's
# winit/softbuffer are native-only and skipped on wasm32 regardless.
cargo rustc --lib --release --target wasm32-unknown-unknown \
  --no-default-features --crate-type cdylib \
  --manifest-path "$ROOT/Cargo.toml"
# Copy atomically: a concurrent reader (another `cargo test` in this worktree,
# tests/wasm.rs) must never see a half-written module. The temp file is in
# web/ so the rename stays on one filesystem, and its name has no `.wasm`
# suffix, so a `web/*.wasm` glob never picks it up.
WASM_TMP=""
trap 'if [[ -n "$WASM_TMP" ]]; then rm -f "$WASM_TMP"; fi' EXIT
WASM_TMP=$(mktemp "$ROOT/web/.pscat.wasm.XXXXXX")
cp "$ROOT/target/wasm32-unknown-unknown/release/pscat.wasm" "$WASM_TMP"
chmod a+r "$WASM_TMP" # mktemp creates 0600; cp would have given the usual 0644
mv -f "$WASM_TMP" "$ROOT/web/pscat.wasm"
WASM_TMP=""
ls -lh "$ROOT/web/pscat.wasm"
