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
#      packs installed stands in as a built-in face, with a note naming the
#      pack (issue #728), and the run succeeds;
#   2. a corrupted pack is refused and leaves nothing installed;
#   3. `pscat fonts install --from <index>` installs every pack, `fonts
#      list` re-verifies them, and they report pinned (EXPECT_PINNED=1) or
#      user-selected;
#   3b. a fonts/catalog/ override wins over the installed packs and is
#      reported as user-selected;
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

echo "==> 1. missing packs stand in as a built-in face and name the pack"
for triple in Bangers:extras:Helvetica CourierPrime:extras:Courier NotoSansKR-Regular:cjk-thai:Helvetica; do
  IFS=: read -r face pack stand_in <<<"$triple"
  err=$("$PSCAT" -e "/$face findfont /FontName get ==" 2>&1 >"$SCRATCH/stdout.txt") ||
    { echo "$err" >&2; fail "/$face findfont failed with no packs"; }
  [[ $(cat "$SCRATCH/stdout.txt") == "/$stand_in" ]] || fail "/$face did not stand in as $stand_in"
  grep -q "is in the $pack font pack" <<<"$err" || { echo "$err" >&2; fail "/$face: note does not name $pack"; }
  grep -q "pscat fonts install --pack $pack" <<<"$err" || { echo "$err" >&2; fail "/$face: note lacks the install command"; }
done
[[ $("$PSCAT" -e "/NoSuchFace findfont /FontName get ==") == "/Helvetica" ]] ||
  fail "an unknown non-pack name no longer substitutes"
# The standard 35 are compiled in (issue #757): no pack, no note.
err=$("$PSCAT" -e "/Palatino-Roman findfont /FontName get == /ZapfDingbats findfont /FontName get ==" 2>&1 >"$SCRATCH/stdout.txt") ||
  { echo "$err" >&2; fail "a standard-35 face failed with no packs"; }
[[ $(tr '\n' ' ' <"$SCRATCH/stdout.txt") == "/texgyrepagella-regular /D050000L " ]] ||
  fail "Palatino-Roman/ZapfDingbats are not the compiled-in faces: $(cat "$SCRATCH/stdout.txt")"
[[ -z "$err" ]] || { echo "$err" >&2; fail "a standard-35 face printed a note"; }

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
for pack in extras cjk-thai; do
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
assert names == ["cjk-thai", "extras"], names
assert all(p["status"] == sys.argv[1] for p in fp["installed"]), fp
assert fp["overrides"] == [], fp
' "$want" || fail "--capabilities font_packs is wrong"

echo "==> 3b. a fonts/catalog override beats the installed (pinned) packs"
# Bangers-Regular, overridden with Liberation Mono's bytes: "i" is then
# 600/1000 em wide, where the real Bangers is much narrower.
mkdir -p "$SCRATCH/ovr/fonts/catalog/Bangers"
cp "$ROOT/fonts/LiberationMono-Regular.ttf" "$SCRATCH/ovr/fonts/catalog/Bangers/Bangers-Regular.ttf"
W=$("$PSCAT" -e '/Bangers findfont 1000 scalefont setfont (i) stringwidth pop round cvi ==')
[[ "$W" != 600 ]] || fail "the real Bangers already measures 600; pick another probe"
W=$(PSCAT_HOME="$SCRATCH/ovr" "$PSCAT" -e '/Bangers findfont 1000 scalefont setfont (i) stringwidth pop round cvi ==')
[[ "$W" == 600 ]] || fail "the override did not win over the installed pack (width $W)"
# Here-strings, not pipes: `grep -q` exiting early would SIGPIPE pscat
# and fail the pipeline under pipefail.
OVR_LIST=$(PSCAT_HOME="$SCRATCH/ovr" "$PSCAT" fonts list)
grep -q "^override: $SCRATCH/ovr/fonts/catalog " <<<"$OVR_LIST" ||
  { echo "$OVR_LIST" >&2; fail "fonts list does not report the override"; }
OVR_FONTS=$(PSCAT_HOME="$SCRATCH/ovr" "$PSCAT" --fonts)
grep -q "^# font-override $SCRATCH/ovr/fonts/catalog user-selected" <<<"$OVR_FONTS" ||
  fail "--fonts does not report the override as user-selected"

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
"$PSCAT" fonts remove extras
[[ $("$PSCAT" -e "/Bangers findfont /FontName get ==" 2>/dev/null) == "/Helvetica" ]] ||
  fail "Bangers does not stand in as Helvetica after removing extras"
"$PSCAT" fonts install --from "$PACKS/font-index.json" --pack extras
[[ $("$PSCAT" -e "/Bangers findfont /FontName get ==" 2>&1) != "/Helvetica" ]] ||
  fail "Bangers still stands in as Helvetica after reinstalling"
"$PSCAT" fonts list >/dev/null || fail "fonts list verification failed after reinstall"
# The retired texgyre pack (issue #757): asking for it succeeds and says so.
err=$("$PSCAT" fonts install --from "$PACKS/font-index.json" --pack texgyre 2>&1) ||
  { echo "$err" >&2; fail "installing the retired texgyre pack failed"; }
grep -q "texgyre font pack is retired" <<<"$err" || { echo "$err" >&2; fail "no retired-pack note"; }

echo "==> font pack smoke OK"
