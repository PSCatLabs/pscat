#!/usr/bin/env bash
# build_npm.sh — assemble the npm package in npm/ (#566) and, with --pack,
# `npm pack` it and check the tarball.
#
#   ./scripts/build_npm.sh                         # build npm/ in place (tests run against it)
#   ./scripts/build_npm.sh --pack DIR              # also write DIR/pscat-X.Y.Z.tgz and check it
#   ./scripts/build_npm.sh --notices FILE --pack DIR   # reuse a THIRD_PARTY_LICENSES.html
#   ./scripts/build_npm.sh --no-notices            # skip cargo-about (not with --pack)
#
# The wasm comes from scripts/build_wasm.sh, the same build as the site's
# playground, and the low-level wrapper is web/pscat.js (copied in as
# core.js). Licence files are copied from the repository root, and the
# dependency notices are cargo-about's (the same command as the release
# workflow's), so the package carries the same terms as the crate and the
# binaries. The release workflow's `verify` job runs this with --pack into
# release-inputs/, and the npm stage of scripts/release_stages.py publishes
# that exact tarball.
#
# Checks with --pack: package.json's version is Cargo.toml's (lockstep), the
# tarball holds exactly the expected files, and it fits the size budget.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
NPM="$ROOT/npm"

# Size budget (bytes). Measured for 0.6.0: tarball ~3.4 MB, unpacked ~7.9 MB,
# almost all of it the embedded Liberation fonts (4.2 MB) and lib/ (2.6 MB).
# A breach means something new got embedded; decide on purpose, then raise it.
MAX_TARBALL=$((4 * 1024 * 1024))
MAX_UNPACKED=$((9 * 1024 * 1024))

NOTICES=""
NO_NOTICES=0
PACK_DIR=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --notices) NOTICES="$2"; shift 2 ;;
    --no-notices) NO_NOTICES=1; shift ;;
    --pack) PACK_DIR="$2"; shift 2 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "build_npm.sh: unknown argument $1" >&2; exit 2 ;;
  esac
done
if [[ -n "$PACK_DIR" && $NO_NOTICES -eq 1 ]]; then
  echo "build_npm.sh: --pack needs the dependency notices; drop --no-notices" >&2
  exit 2
fi

CARGO_VERSION=$(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import release_stages as r; print(r.cargo_version())' "$ROOT/scripts")
NPM_VERSION=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["version"])' "$NPM/package.json")
if [[ "$CARGO_VERSION" != "$NPM_VERSION" ]]; then
  echo "build_npm.sh: npm/package.json is $NPM_VERSION, Cargo.toml is $CARGO_VERSION (they move in lockstep: scripts/release_stages.py bump-manifests)" >&2
  exit 1
fi

"$ROOT/scripts/build_wasm.sh"
cp "$ROOT/web/pscat.wasm" "$NPM/pscat.wasm"
cp "$ROOT/web/pscat.js" "$NPM/core.js"
mkdir -p "$NPM/fonts" "$NPM/licenses"
cp "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" "$ROOT/LICENSE-OFL" "$ROOT/THIRD_PARTY_NOTICES.md" "$NPM/"
cp "$ROOT/fonts/LICENSE" "$NPM/fonts/LICENSE"
cp "$ROOT/licenses/assets.json" "$NPM/licenses/assets.json"

if [[ -n "$NOTICES" ]]; then
  cp "$NOTICES" "$NPM/THIRD_PARTY_LICENSES.html"
elif [[ $NO_NOTICES -eq 0 ]]; then
  command -v cargo-about >/dev/null || {
    echo "build_npm.sh: cargo-about is not installed (cargo install cargo-about), or pass --notices FILE / --no-notices" >&2
    exit 1
  }
  (cd "$ROOT" && cargo about generate --locked --all-features --fail about.hbs -o "$NPM/THIRD_PARTY_LICENSES.html")
fi

[[ -n "$PACK_DIR" ]] || exit 0

mkdir -p "$PACK_DIR"
PACK_DIR=$(cd "$PACK_DIR" && pwd)
PACK_JSON=$(cd "$NPM" && npm pack --json --pack-destination "$PACK_DIR")
python3 - "$PACK_JSON" "$PACK_DIR" "$MAX_TARBALL" "$MAX_UNPACKED" <<'EOF'
import json, os, sys
info = json.loads(sys.argv[1])[0]
pack_dir, max_tar, max_unpacked = sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
expected = {
    "package.json", "README.md", "index.js", "node.js", "renderer.js", "core.js", "index.d.ts",
    "pscat.wasm", "LICENSE-MIT", "LICENSE-APACHE", "LICENSE-OFL", "fonts/LICENSE",
    "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_LICENSES.html", "licenses/assets.json",
}
got = {f["path"] for f in info["files"]}
problems = []
if got - expected:
    problems.append(f"unexpected files in the tarball: {sorted(got - expected)}")
if expected - got:
    problems.append(f"missing from the tarball: {sorted(expected - got)}")
tarball = os.path.join(pack_dir, info["filename"])
size = os.path.getsize(tarball)
if size > max_tar:
    problems.append(f"tarball is {size} bytes, over the {max_tar} budget")
if info["unpackedSize"] > max_unpacked:
    problems.append(f"unpacked size is {info['unpackedSize']} bytes, over the {max_unpacked} budget")
print(f"{tarball}: {size} bytes packed, {info['unpackedSize']} unpacked, {len(got)} files")
print(f"integrity {info['integrity']}")
if problems:
    for p in problems:
        print(f"build_npm.sh: {p}", file=sys.stderr)
    sys.exit(1)
EOF
