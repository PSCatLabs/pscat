#!/usr/bin/env bash
# package_release.sh — build pscat + pscat-mcp and assemble a
# standalone, installable bundle: the binaries plus the runtime
# assets they need (lib/), with no git checkout or cargo required to
# use it afterward. The handwrite, typewrite and photo-etch tools are
# `pscat` subcommands (issue #600), so the bundle carries no scripts.
#
#   ./scripts/package_release.sh
#   ./scripts/package_release.sh /tmp/out         # custom output dir
#
# The bundle layout is deliberate: pscat/pscat-mcp sit at the bundle
# root so `src/paths.rs`'s exe-sibling resolution finds lib/ with
# zero configuration — unzip, add the bundle dir
# to PATH, done. $PSCAT_HOME (or $PSCAT_ROOT) still works as an explicit override if
# the binaries get moved elsewhere. examples/ and gallery/ are left
# out — nothing at runtime (lib/*.ps, src/) depends on them; they're
# demo material for the checkout, not the installed tool. The font
# catalog is not bundled either: it ships as separately versioned font
# packs that `pscat fonts install` downloads (issue #599).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT_BASE="${1:-$ROOT/dist}"

VERSION=$(awk -F'"' '/^version/{print $2; exit}' "$ROOT/Cargo.toml")
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
NAME="pscat-${VERSION}-${OS}-${ARCH}"
BUNDLE="$OUT_BASE/$NAME"

echo "==> Building pscat and pscat-mcp (release)"
cargo build --release --manifest-path "$ROOT/Cargo.toml"

echo "==> Assembling $BUNDLE"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE"
cp "$ROOT/target/release/pscat" "$BUNDLE/pscat"
cp "$ROOT/target/release/pscat-mcp" "$BUNDLE/pscat-mcp"
cp -R "$ROOT/lib" "$BUNDLE/lib"

cat > "$BUNDLE/README.md" <<EOF
# pscat $VERSION ($OS/$ARCH)

A standalone pscat install — no git checkout or cargo needed.

## Use

Add this directory to your \`PATH\`, then run \`pscat\` from anywhere:

    export PATH="\$PWD:\$PATH"
    pscat --png out.png some_file.ps

\`lib/\` is found automatically as long as it stays
next to the \`pscat\` binary (this directory). If you move the binary
elsewhere, set \`PSCAT_HOME\` to this directory instead. (The
libraries are also compiled into \`pscat\`, so \`lib/\` keeps working
even then.)

## Fonts

The twelve Liberation faces (Helvetica, Times, Courier) are built in.
The rest of the catalog (Symbol, ZapfDingbats, Palatino and the other
standard-35 faces, the display faces, Korean/Japanese/Thai) comes as
font packs:

    pscat fonts install      # the packs this pscat release pins
    pscat fonts list

A program that asks for a face from a pack that is not installed stops
with an error naming the pack.

## What's here

- \`pscat\`, \`pscat-mcp\` — the interpreter and its MCP server
- \`lib/\` — artkit.ps, handscript.ps, etching.ps, the Type 3 font programs, style packs

The handwritten-note, typewritten-page and photo-to-line-etching tools are
subcommands of the binary: \`pscat handwrite\`, \`pscat typewrite\` and
\`pscat photo-etch\` (add \`--help\`). They need nothing but \`pscat\`.

Full docs, source, and the \`examples/\`/\`gallery/\` demo material:
https://github.com/jeromebanks/postscript_interpreter
EOF

echo "==> Taring"
mkdir -p "$OUT_BASE"
tar -czf "$OUT_BASE/$NAME.tar.gz" -C "$OUT_BASE" "$NAME"

echo "==> Verifying the bundle resolves lib/ standalone, and needs font packs"
# From /tmp (not the bundle, not the checkout) with no $PSCAT_HOME set,
# so exe-sibling resolution (src/paths.rs) must find lib/ next to the
# binary: the scenario "unzip + add to PATH"
# puts a user in. The binary also carries an embedded lib/, so the lib
# checks assert on the resolver trace that the bundle's copy served it.
cd /tmp
bare() { env -u PSCAT_ROOT -u PSCAT_HOME -u PSCAT_LIB PSCAT_TRACE_RESOURCES=1 "$@"; }
# With an empty HOME (so no font packs the packager installed), a pack
# face must stop with an error naming its pack, never substitute.
EMPTY_HOME=$(mktemp -d)
OUT=$(HOME="$EMPTY_HOME" XDG_DATA_HOME="$EMPTY_HOME" bare "$BUNDLE/pscat" -e '/Bangers findfont pop' 2>&1) &&
  { echo "FAIL: /Bangers resolved with no font packs installed" >&2; exit 1; }
grep -q "is in the extras font pack" <<<"$OUT" ||
  { echo "$OUT" >&2; echo "FAIL: missing face did not name its font pack" >&2; exit 1; }
rm -rf "$EMPTY_HOME"
for lib in lib/artkit.ps lib/etching.ps; do
  TRACE=$(bare "$BUNDLE/pscat" -e "($lib) run" 2>&1) || true
  grep -q "resource $lib <- exe-dir" <<<"$TRACE" ||
    { echo "FAIL: $lib did not resolve from the bundle" >&2; exit 1; }
done

# The three tool subcommands need nothing but the binary (issue #600): no
# scripts/ beside it, no checkout, no shell. The photo is test input from
# the checkout, not an install dependency.
TOOLS=$(mktemp -d)
printf 'bundle check\n' > "$TOOLS/note.txt"
bare "$BUNDLE/pscat" handwrite -o "$TOOLS/h.png" "bundle check" >/dev/null ||
  { echo "FAIL: pscat handwrite did not run from the bundle" >&2; exit 1; }
bare "$BUNDLE/pscat" typewrite -o "$TOOLS/t.png" "$TOOLS/note.txt" >/dev/null ||
  { echo "FAIL: pscat typewrite did not run from the bundle" >&2; exit 1; }
bare "$BUNDLE/pscat" photo-etch -o "$TOOLS/e.png" "$ROOT/tests/data/sphere.jpg" >/dev/null ||
  { echo "FAIL: pscat photo-etch did not run from the bundle" >&2; exit 1; }
for f in h t e; do
  [[ -s "$TOOLS/$f.png" ]] || { echo "FAIL: a tool subcommand wrote no PNG ($f)" >&2; exit 1; }
done
rm -rf "$TOOLS"

# The other realistic install shape: a symlink into some bin/ dir
# pointing back at the bundle (what `brew`, and this project's own
# install.sh, do) rather than the bundle itself being on PATH.
# current_exe() can return the symlink's own path rather than the
# real binary's — caught a real bug here during development — so
# this is worth pinning at packaging time, not just trusting the
# non-symlinked case above.
SYMLINK_DIR=$(mktemp -d)
ln -s "$BUNDLE/pscat" "$SYMLINK_DIR/pscat"
TRACE=$(bare "$SYMLINK_DIR/pscat" -e '(lib/artkit.ps) run' 2>&1) || true
grep -q "resource lib/artkit.ps <- exe-dir" <<<"$TRACE" ||
  { echo "FAIL: lib/artkit.ps did not resolve via a symlinked pscat" >&2; exit 1; }
rm -rf "$SYMLINK_DIR"
cd "$ROOT"

echo "==> Done: $OUT_BASE/$NAME.tar.gz"
du -sh "$OUT_BASE/$NAME.tar.gz"
