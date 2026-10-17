#!/usr/bin/env bash
# font_pack_smoke.sh — prove the font packs work outside a checkout
# (issue #599). release-fonts.yml runs it on every build, dry run
# included; it needs only a built pscat and a directory of packs from
# `scripts/font_packs.py build`.
#
#   ./scripts/font_pack_smoke.sh <pscat binary> <packs dir>
#   EXPECT_PINNED=1 ./scripts/font_pack_smoke.sh ...   # binary pins this index
#
# From an empty directory, with HOME pointed at an empty scratch dir and
# no PSCAT_* set, it checks:
#   1. a pack face (/Symbol, /Palatino-Roman, /NotoSansKR-Regular) with no
#      packs installed is an invalidfont naming the pack, not a substitute;
#   2. a corrupted pack is refused and leaves nothing installed;
#   3. `pscat fonts install --from <index>` installs every pack, `fonts
#      list` re-verifies them, and they report pinned (EXPECT_PINNED=1) or
#      user-selected;
#   4. catalog-font examples render pixel-identically from the installed
#      packs and from the checkout's fonts/catalog (PSCAT_HOME=<checkout>),
#      the reproducibility contract in docs/guides/fonts.md;
#   5. remove and reinstall of one pack.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd -P)
PSCAT=$(cd "$(dirname "$1")" && pwd -P)/$(basename "$1")
PACKS=$(cd "$2" && pwd -P)
EXAMPLES=(font_catalog.ps international_text.ps graphing.ps circular_text.ps paragraph_layout.ps)

fail() { echo "FAIL: $*" >&2; exit 1; }

SCRATCH=$(mktemp -d "${TMPDIR:-/tmp}/pscat-font-smoke.XXXXXX")
SCRATCH=$(cd "$SCRATCH" && pwd -P)
case "$SCRATCH/" in "$ROOT"/*) fail "scratch dir is inside the checkout" ;; esac
if [[ -z "${KEEP:-}" ]]; then trap 'rm -rf "$SCRATCH"' EXIT; else echo "keeping $SCRATCH"; fi
mkdir -p "$SCRATCH/home" "$SCRATCH/work" "$SCRATCH/bad"
export HOME="$SCRATCH/home" XDG_DATA_HOME="$SCRATCH/home/.local/share"
unset PSCAT_HOME PSCAT_ROOT PSCAT_LIB APPDATA
cd "$SCRATCH/work"

PACKDIR=$("$PSCAT" fonts list | sed -n 's/^install directory: //p')
[[ -n "$PACKDIR" ]] || fail "fonts list printed no install directory"
case "$PACKDIR/" in "$SCRATCH"/*) ;; *) fail "install directory $PACKDIR is outside the scratch HOME" ;; esac

echo "==> 1. missing packs are errors that name the pack"
for pair in Symbol:extras ZapfDingbats:extras Palatino-Roman:texgyre NotoSansKR-Regular:cjk-thai; do
  face=${pair%%:*} pack=${pair##*:}
  if out=$("$PSCAT" -e "/$face findfont pop" 2>&1); then fail "/$face findfont succeeded with no packs"; fi
  grep -q "invalidfont" <<<"$out" || { echo "$out" >&2; fail "/$face: no invalidfont"; }
  grep -q "is in the $pack font pack" <<<"$out" || { echo "$out" >&2; fail "/$face: hint does not name $pack"; }
done
[[ $("$PSCAT" -e "/NoSuchFace findfont /FontName get ==") == "/Helvetica" ]] ||
  fail "an unknown non-pack name no longer substitutes"

echo "==> 2. a corrupted pack is refused, nothing partial left"
cp "$PACKS"/* "$SCRATCH/bad/"
EXTRAS=$(ls "$SCRATCH"/bad/extras-*.tar.zst)
printf 'X' | dd of="$EXTRAS" bs=1 seek=4096 conv=notrunc 2>/dev/null
if out=$("$PSCAT" fonts install --from "$SCRATCH/bad/font-index.json" 2>&1); then
  fail "a corrupted pack installed"
fi
grep -q "checksum mismatch" <<<"$out" || { echo "$out" >&2; fail "no checksum-mismatch message"; }
if [[ -d "$PACKDIR" ]] && [[ -n "$(ls -A "$PACKDIR")" ]]; then
  ls -la "$PACKDIR" >&2
  fail "a refused install left files behind"
fi

echo "==> 3. install from the index, verify, status"
"$PSCAT" fonts install --from "$PACKS/font-index.json"
LIST=$("$PSCAT" fonts list)
echo "$LIST"
want=user-selected
[[ -n "${EXPECT_PINNED:-}" ]] && want=pinned
for pack in extras texgyre cjk-thai; do
  grep -Eq "^installed: $pack fonts-[0-9.]+ $want at .*files verified$" <<<"$LIST" ||
    fail "$pack is not installed, verified and $want"
done
grep -q '^override:' <<<"$LIST" && fail "an override directory is visible; the test would not exercise the packs"
FONTS=$("$PSCAT" --fonts)
grep -qx "Bangers-Regular" <<<"$FONTS" || fail "--fonts does not list Bangers-Regular from the pack"
grep -q "^# font-pack extras .* $want " <<<"$FONTS" || fail "--fonts does not report extras as $want"
"$PSCAT" --capabilities | python3 -c '
import json, sys
fp = json.load(sys.stdin)["font_packs"]
names = sorted(p["name"] for p in fp["installed"])
assert names == ["cjk-thai", "extras", "texgyre"], names
assert all(p["status"] == sys.argv[1] for p in fp["installed"]), fp
assert fp["overrides"] == [], fp
' "$want" || fail "--capabilities font_packs is wrong"

echo "==> 4. pixels from the packs match pixels from the checkout catalog"
mkdir -p pack checkout
for ex in "${EXAMPLES[@]}"; do
  cp "$ROOT/examples/$ex" "$SCRATCH/work/$ex"
  png="${ex%.ps}.png" # multi-page programs write name-001.png, name-002.png, ...
  "$PSCAT" --png "pack/$png" "$ex" >/dev/null || fail "$ex did not render from the packs"
  PSCAT_HOME="$ROOT" "$PSCAT" --png "checkout/$png" "$ex" >/dev/null ||
    fail "$ex did not render from the checkout"
done
diff -r pack checkout || fail "renders from the packs differ from renders from fonts/catalog (above)"
echo "    $(ls pack | wc -l | tr -d ' ') pages from ${#EXAMPLES[@]} examples, byte-identical"

echo "==> 5. remove and reinstall one pack"
"$PSCAT" fonts remove texgyre
if "$PSCAT" -e "/Palatino-Roman findfont pop" 2>/dev/null; then fail "Palatino still resolves after removing texgyre"; fi
"$PSCAT" fonts install --from "$PACKS/font-index.json" --pack texgyre
"$PSCAT" -e "/Palatino-Roman findfont pop" || fail "Palatino does not resolve after reinstalling"
"$PSCAT" fonts list >/dev/null || fail "fonts list verification failed after reinstall"

echo "==> font pack smoke OK"
