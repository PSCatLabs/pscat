#!/usr/bin/env bash
# build_site.sh — assemble the GitHub Pages site into _site/ (Stage 17).
#
#   ./scripts/build_site.sh
#   python3 -m http.server -d _site       # preview locally
#
# Pieces: the authored pages (site/, with the shared header, footer and
# <head> from site/_partials/ applied last, by scripts/site_pages.py),
# the wasm build + JS library, the gallery (gallery.html and one page per
# piece, generated from gallery/manifest.json by scripts/gallery.py), the
# playground example sources (self-contained files only — the wasm build has
# no filesystem), the Learn page's examples (site/learn/, placed into the
# page by scripts/site_examples.py and checked in the wasm build by
# scripts/check_learn_examples.mjs), and PNG renders of every gallery piece
# and example, produced by pscat itself.
#
# Then check it: scripts/check_site_links.py (CI runs it after this).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT="$ROOT/_site"

rm -rf "$OUT"
mkdir -p "$OUT/assets/renders" "$OUT/assets/references" "$OUT/assets/learn" \
  "$OUT/examples/learn" "$OUT/lib"

# 1. the authored pages (the shell is applied in the last step)
cp "$ROOT"/site/*.html "$ROOT"/site/style.css "$ROOT"/site/source-viewer.js "$OUT/"
cp "$ROOT"/lib/graph.ps "$ROOT"/lib/dataviz.ps "$OUT/lib/"

# 2. the browser build
"$ROOT/scripts/build_wasm.sh"
cp "$ROOT/web/pscat.wasm" "$ROOT/web/pscat.js" "$OUT/"
# every Learn example must run in the browser build as the page says (#252)
if command -v node >/dev/null; then
  node "$ROOT/scripts/check_learn_examples.mjs"
else
  echo "build_site.sh: node not found, skipping the Learn examples' browser check" >&2
fi

# 3. the gallery, generated from gallery/manifest.json (#251): the check first,
# so a piece with no entry or a missing render stops the build; then
# gallery.html and one page per piece (with the shell markers, so the last step
# dresses them), the renders, reference photos, the playground sources
# (self-contained files only: the wasm build has no filesystem) and every
# program for the "View source" window (source/<repo path>, #461).
python3 "$ROOT/scripts/gallery.py" check
python3 "$ROOT/scripts/gallery.py" build "$OUT"
python3 "$ROOT/scripts/gallery.py" stage "$OUT"
cp "$ROOT"/site/learn/*.ps "$OUT/examples/learn/"

# 4. the font gallery: one card per face (site/fonts.html)
"$ROOT/scripts/build_font_gallery.sh" "$OUT/assets/fonts"

# 5. the specimens and examples that have no committed render: pscat draws
# them here at the page size the manifest gives
cargo build --release --manifest-path "$ROOT/Cargo.toml"
BIN="$ROOT/target/release/pscat"
while read -r src page id; do
  "$BIN" --page "$page" --png "$OUT/assets/renders/$id.png" "$ROOT/$src"
done < <(python3 "$ROOT/scripts/gallery.py" renders)
# the Learn examples that draw, at their %%BoundingBox size and twice the
# resolution, so they stay sharp on high-density screens
for f in "$ROOT"/site/learn/*.ps; do
  size=$(sed -n 's/^%%BoundingBox: *0 0 \([0-9]*\) \([0-9]*\) *$/\1x\2/p' "$f")
  [ -n "$size" ] || continue
  "$BIN" --page "$size" --dpi 144 --png "$OUT/assets/learn/$(basename "$f" .ps).png" "$f"
done

# 6. the Learn page's examples: source, "Try it" link and result
python3 "$ROOT/scripts/site_examples.py" "$OUT"

# 7. the shared shell, last, so pages generated above get it too
python3 "$ROOT/scripts/site_pages.py" "$OUT"

echo "site assembled: $OUT ($(du -sh "$OUT" | cut -f1))"
