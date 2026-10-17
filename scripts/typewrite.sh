#!/usr/bin/env bash
# typewrite.sh — a text file in, a typewritten page out (#213).
#
#   ./scripts/typewrite.sh memo.txt                       # typed.png
#   ./scripts/typewrite.sh -o memo.pdf --spacing 2 memo.txt
#   ./scripts/typewrite.sh --memo-to "All Staff" --memo-from "Records" \
#       --memo-subject "Carbon copies" --memo-date "Oct 14, 1962" -o m.png memo.txt
#   echo "Dear Sir," | ./scripts/typewrite.sh --height fit -o note.png -
#
# Types the text with the /Typewriter font (lib/typewriter.ps): word-wrapped
# to the margins, with tab stops, line spacing, underlines and strikeovers
# by markup, an optional memo header and page numbers, in a machine's own
# per-strike variation and ribbon ink. All business logic lives in the
# PostScript library (tw-type); this script builds the options dict and runs
# pscat. `--height fit` runs a headless tw-pageheight pre-pass first, so the
# page is as tall as its text. `--ps PATH` writes the self-contained driver
# instead of rendering (it runs in Ghostscript with `gs -dNOSAFER`).
#
# Markup in the text (no escapes needed): <u>underlined</u>,
# <x>crossed out</x>, <o>old|new</o> (new overtyped on old), <r>redacted</r>
# (typed, then covered with a black bar; lib/released.ps has a marker's bar,
# see its rel-redact and the /Redactor key). Typographic
# quotes, dashes and the ellipsis become their ASCII forms; any other
# non-ASCII character becomes "?": the machine has no such key.
set -euo pipefail

usage() {
  cat <<'EOF2'
usage: typewrite.sh [options] [FILE | -]      (text from FILE, stdin, or --text)

  -o, --out PATH        output: .png (default typed.png) or .pdf
      --text STR        the text, instead of FILE
      --reflow          join the lines of each paragraph (a paragraph ends at
                        a blank line), for text wrapped by an editor
      --ps PATH         write the self-contained PostScript driver to PATH
                        and stop (it runs in Ghostscript: gs -dNOSAFER)

  page
      --width N         page width in points (default 612)
      --height N|fit    page height in points, or "fit" to size the page to
                        its text (default 792); longer text runs onto more
                        pages (PNG: out-001.png, out-002.png, ...)
      --margin N        all four margins in points (default 72)
      --margin-left N  --margin-right N  --margin-top N  --margin-bottom N
      --spacing F       1 single, 1.5, 2 double (default 1)
      --indent N        first-line paragraph indent in columns (default 0)
      --tabs N|A,B,C    a stop every N columns, or at these columns (default 5)
      --page-numbers P  none | top | bottom (default none)

  memo header (any of these types one above the text)
      --memo            a header with just the title and rule
      --memo-to S  --memo-from S  --memo-cc S  --memo-subject S  --memo-date S
      --memo-title S    default MEMORANDUM; "" for none
      --no-rule         leave out the underscore rule under the header

  machine
      --pitch N         10 or pica, 12 or elite (default pica)
      --size N          type size in points (default 12.5)
      --machine N       the machine: its keys' fixed quirks (default 1)
      --seed N          the document: same seeds, same page (default 1)
      --wear F          0 perfect .. 1 good manual .. 1.5 old (default 1)
      --preset P        manual | electric | worn
      --ink R,G,B       ribbon colour, components 0..1
      --ribbon F        ribbon wetness 0..1 (default 0.7)
      --dpi N           render resolution (default 72)
  -h, --help            this text

Without --reflow newlines break lines; a blank line stays blank. Set PSCAT_BIN=/path/to/pscat
to pick the interpreter.
EOF2
}

ROOT=$(cd "$(dirname "$0")/.." && pwd)
# Point pscat at this tree's lib/ and fonts/catalog/ (src/paths.rs): a
# release binary has no checkout heuristics, and its embedded lib/ may be
# older than the files beside this script.
export PSCAT_HOME="${PSCAT_HOME:-${PSCAT_ROOT:-$ROOT}}"

# BIN resolution covers the layouts this same script runs from unmodified:
# an installed bundle (pscat at the bundle root, this script one level down),
# a dev checkout (cargo build --release), or pscat on PATH. $PSCAT_BIN
# overrides all three.
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

die() { echo "typewrite.sh: $*" >&2; exit 1; }
num() { [[ "$2" =~ ^-?[0-9]+(\.[0-9]+)?$ ]] || die "$1 needs a number, got: $2"; }

TEXT="" HAVE_TEXT="" REFLOW="" FILE="" OUT="typed.png" PSOUT=""
WIDTH=612 HEIGHT=792 MARGIN="" ML="" MR="" MT="" MB="" SPACING="" INDENT="" TABS="" PN=""
MEMO="" M_TO="" M_FROM="" M_CC="" M_SUBJECT="" M_DATE="" M_TITLE="" M_TITLE_SET="" M_RULE=""
PITCH="" SIZE="" MACHINE="" SEED="" WEAR="" PRESET="" INK="" RIBBON="" DPI=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    -o|--out) OUT=$2; shift 2 ;;
    --text) TEXT=$2; HAVE_TEXT=1; shift 2 ;;
    --ps) PSOUT=$2; shift 2 ;;
    --reflow) REFLOW=1; shift ;;
    --width) num "$1" "$2"; WIDTH=$2; shift 2 ;;
    --height) if [[ "$2" == fit ]]; then HEIGHT=fit; else num "$1" "$2"; HEIGHT=$2; fi; shift 2 ;;
    --margin) num "$1" "$2"; MARGIN=$2; shift 2 ;;
    --margin-left) num "$1" "$2"; ML=$2; shift 2 ;;
    --margin-right) num "$1" "$2"; MR=$2; shift 2 ;;
    --margin-top) num "$1" "$2"; MT=$2; shift 2 ;;
    --margin-bottom) num "$1" "$2"; MB=$2; shift 2 ;;
    --spacing) num "$1" "$2"; SPACING=$2; shift 2 ;;
    --indent) num "$1" "$2"; INDENT=$2; shift 2 ;;
    --tabs) [[ "$2" =~ ^[0-9]+(,[0-9]+)*$ ]] || die "--tabs needs N or A,B,C columns, got: $2"; TABS=$2; shift 2 ;;
    --page-numbers) PN=$2; shift 2 ;;
    --memo) MEMO=1; shift ;;
    --memo-to) M_TO=$2; MEMO=1; shift 2 ;;
    --memo-from) M_FROM=$2; MEMO=1; shift 2 ;;
    --memo-cc) M_CC=$2; MEMO=1; shift 2 ;;
    --memo-subject) M_SUBJECT=$2; MEMO=1; shift 2 ;;
    --memo-date) M_DATE=$2; MEMO=1; shift 2 ;;
    --memo-title) M_TITLE=$2; M_TITLE_SET=1; MEMO=1; shift 2 ;;
    --no-rule) M_RULE=false; MEMO=1; shift ;;
    --pitch)
      case "$2" in pica) PITCH=10 ;; elite) PITCH=12 ;; *) num "$1" "$2"; PITCH=$2 ;; esac
      shift 2 ;;
    --size) num "$1" "$2"; SIZE=$2; shift 2 ;;
    --machine) num "$1" "$2"; MACHINE=$2; shift 2 ;;
    --seed) num "$1" "$2"; SEED=$2; shift 2 ;;
    --wear) num "$1" "$2"; WEAR=$2; shift 2 ;;
    --preset) PRESET=$2; shift 2 ;;
    --ink) [[ "$2" =~ ^[0-9.]+,[0-9.]+,[0-9.]+$ ]] || die "--ink needs R,G,B, got: $2"; INK=$2; shift 2 ;;
    --ribbon) num "$1" "$2"; RIBBON=$2; shift 2 ;;
    --dpi) num "$1" "$2"; DPI=$2; shift 2 ;;
    --) shift; [[ $# -gt 0 ]] && { FILE=$1; shift; } ;;
    -) FILE=- ; shift ;;
    -*) echo "typewrite.sh: unknown option: $1" >&2; usage >&2; exit 1 ;;
    *)
      [[ -z "$FILE" ]] || die "more than one input file"
      FILE=$1; shift ;;
  esac
done

case "$PN" in ""|none|top|bottom) ;; *) die "--page-numbers must be none, top or bottom, got: $PN" ;; esac
case "$PRESET" in ""|manual|electric|worn) ;; *) die "--preset must be manual, electric or worn, got: $PRESET" ;; esac

if [[ -z "$HAVE_TEXT" ]]; then
  if [[ -n "$FILE" && "$FILE" != - ]]; then
    [[ -r "$FILE" ]] || die "cannot read $FILE"
    TEXT=$(cat "$FILE"; printf x); TEXT=${TEXT%x}   # keep trailing blank lines
  elif [[ "$FILE" == - || ! -t 0 ]]; then
    TEXT=$(cat; printf x); TEXT=${TEXT%x}
  fi
fi
[[ -n "$TEXT" || -n "$MEMO" ]] || die "no text given (a FILE, - for stdin, or --text)"

# a paragraph's lines joined by a space; its first line keeps any indent
reflow() {
  awk 'NF == 0 { if (p != "") print p; p = ""; print ""; next }
       { if (p == "") p = $0; else { sub(/^[ \t]+/, ""); p = p " " $0 } }
       END { if (p != "") print p }'
}
if [[ -n "$REFLOW" ]]; then TEXT=$(printf '%s' "$TEXT" | reflow; printf x); TEXT=${TEXT%x}; fi

# ASCII only: the typewriter has no other keys. Typographic punctuation
# becomes its plain form, any other character a "?".
ascii() {
  LC_ALL=C sed \
    -e $'s/\xe2\x80[\x98\x99]/\'/g' -e $'s/\xe2\x80[\x9c\x9d]/"/g' \
    -e $'s/\xe2\x80\x94/--/g' -e $'s/\xe2\x80\x93/-/g' -e $'s/\xe2\x80\xa6/.../g' \
    -e $'s/\xe2\x80\xa2/*/g' -e $'s/\xc2\xa0/ /g' -e $'s/\r//g' \
    -e $'s/[\xc2-\xf4][\x80-\xbf]*/?/g'
}
# escape \ ( ) for a PostScript string literal; embedded newlines pass through
pstr() { printf '%s' "$1" | ascii | sed -e 's/\\/\\\\/g' -e 's/(/\\(/g' -e 's/)/\\)/g'; }

opts_dict() { # $1 = page height to declare
  echo "/TWOpts <<"
  local t; t=$(pstr "$TEXT"; printf x); t=${t%x}   # keep trailing blank lines
  echo "  /Text ($t)"
  echo "  /Width $WIDTH  /Height $1"
  [[ -z "$MARGIN" ]] || echo "  /Margin $MARGIN"
  [[ -z "$ML" ]] || echo "  /MarginLeft $ML"
  [[ -z "$MR" ]] || echo "  /MarginRight $MR"
  [[ -z "$MT" ]] || echo "  /MarginTop $MT"
  [[ -z "$MB" ]] || echo "  /MarginBottom $MB"
  [[ -z "$SPACING" ]] || echo "  /Spacing $SPACING"
  [[ -z "$INDENT" ]] || echo "  /Indent $INDENT"
  if [[ -n "$TABS" ]]; then
    if [[ "$TABS" == *,* ]]; then echo "  /Tabs [ ${TABS//,/ } ]"; else echo "  /Tabs $TABS"; fi
  fi
  [[ -z "$PN" ]] || echo "  /PageNumbers /$PN"
  if [[ -n "$MEMO" ]]; then
    echo "  /Memo <<"
    [[ -z "$M_TO" ]] || echo "    /To ($(pstr "$M_TO"))"
    [[ -z "$M_FROM" ]] || echo "    /From ($(pstr "$M_FROM"))"
    [[ -z "$M_CC" ]] || echo "    /Cc ($(pstr "$M_CC"))"
    [[ -z "$M_SUBJECT" ]] || echo "    /Subject ($(pstr "$M_SUBJECT"))"
    [[ -z "$M_DATE" ]] || echo "    /Date ($(pstr "$M_DATE"))"
    [[ -z "$M_TITLE_SET" ]] || echo "    /Title ($(pstr "$M_TITLE"))"
    [[ -z "$M_RULE" ]] || echo "    /Rule $M_RULE"
    echo "  >>"
  fi
  [[ -z "$PITCH" ]] || echo "  /Pitch $PITCH"
  [[ -z "$SIZE" ]] || echo "  /Size $SIZE"
  [[ -z "$MACHINE" ]] || echo "  /Machine $MACHINE"
  [[ -z "$SEED" ]] || echo "  /Seed $SEED"
  [[ -z "$WEAR" ]] || echo "  /Wear $WEAR"
  [[ -z "$PRESET" ]] || echo "  /Preset /$PRESET"
  [[ -z "$INK" ]] || echo "  /Ink [ ${INK//,/ } ]"
  [[ -z "$RIBBON" ]] || echo "  /Ribbon $RIBBON"
  echo ">> def"
}

LIBS=("$ROOT/lib/typewriter_outlines.ps" "$ROOT/lib/typewriter.ps")
for f in "${LIBS[@]}"; do [[ -r "$f" ]] || die "missing library file: $f"; done

TMP=$(mktemp -d "${TMPDIR:-/tmp}/typewrite.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

if [[ "$HEIGHT" == fit ]]; then
  [[ -x "$BIN" || -n "${PSCAT_BIN:-}" ]] || cargo build --release --manifest-path "$ROOT/Cargo.toml"
  # Pre-pass: the page height that holds the text on one page. Width, not
  # height, decides the wrapping, so a placeholder height is fine here.
  { cat "${LIBS[@]}"; opts_dict 792; echo "TWOpts tw-pageheight ="; } > "$TMP/fit.ps"
  HEIGHT=$("$BIN" --headless "$TMP/fit.ps")
  [[ "$HEIGHT" =~ ^[0-9]+$ ]] || die "could not size the page (tw-pageheight gave: $HEIGHT)"
fi

# the page size is declared in the driver, so `gs -dNOSAFER file.ps` needs no -g
{ cat "${LIBS[@]}"; echo "/setpagedevice where { pop << /PageSize [ $WIDTH $HEIGHT ] >> setpagedevice } if"; opts_dict "$HEIGHT"
  echo "TWOpts tw-type showpage"; } > "$TMP/render.ps"

if [[ -n "$PSOUT" ]]; then
  cp "$TMP/render.ps" "$PSOUT"
  echo "typewrite.sh: wrote $PSOUT (${WIDTH}x${HEIGHT} points; run with gs -dNOSAFER, or pscat --page ${WIDTH}x${HEIGHT})" >&2
  exit 0
fi

[[ -x "$BIN" || -n "${PSCAT_BIN:-}" ]] || cargo build --release --manifest-path "$ROOT/Cargo.toml"
case "$OUT" in
  *.pdf|*.PDF) FORMAT=--pdf ;;
  *) FORMAT=--png ;;
esac
"$BIN" --page "${WIDTH}x${HEIGHT}" ${DPI:+--dpi "$DPI"} "$FORMAT" "$OUT" "$TMP/render.ps"
