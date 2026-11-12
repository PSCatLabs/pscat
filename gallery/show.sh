#!/usr/bin/env bash
# Display the gallery, one piece at a time.
#
#   ./gallery/show.sh          step through the rendered PNGs; close each
#                              Quick Look window (or press q) to advance
#   ./gallery/show.sh --live   run each .ps in a live pscat window instead,
#                              so you watch every piece draw itself;
#                              close the window to move on
set -euo pipefail
cd "$(dirname "$0")/.."

# The pieces, their page sizes and drawing speeds come from the manifest
# (gallery/manifest.json), so a new piece shows up here without editing this file.
# Each piece's source is the manifest's path: most are gallery/<id>.ps, but
# a piece may live elsewhere (examples/hybrid/ reads JPEGs beside it).
PIECES=(); PAGES=(); SPEEDS=(); SOURCES=()
while IFS=$'\t' read -r id page speed source; do
  PIECES+=("$id"); PAGES+=("$page"); SPEEDS+=("$speed"); SOURCES+=("$source")
done < <(python3 scripts/gallery.py list)

if [[ "${1:-}" == "--live" ]]; then
  BIN=target/release/pscat
  [[ -x "$BIN" ]] || cargo build --release
  for i in "${!PIECES[@]}"; do
    echo ">> ${PIECES[$i]}  (close the window for the next piece)"
    "$BIN" --page "${PAGES[$i]}" --speed "${SPEEDS[$i]}" "${SOURCES[$i]}"
  done
else
  for p in "${PIECES[@]}"; do
    img="gallery/renders/$p.png"
    if [[ ! -f "$img" ]]; then
      echo "!! missing $img — see gallery/README.md to re-render" >&2
      continue
    fi
    echo ">> $p"
    if command -v qlmanage >/dev/null 2>&1; then
      # macOS Quick Look: blocks until the preview window closes
      qlmanage -p "$img" >/dev/null 2>&1
    else
      (xdg-open "$img" 2>/dev/null || open "$img") &
      read -r -p "   press enter for the next piece... "
    fi
  done
fi
echo "fin."
