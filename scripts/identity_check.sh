#!/usr/bin/env bash
# identity_check.sh -- shared cross-tree PNG byte-identity check for paintkit
# slices (issue #263).
#
#   scripts/identity_check.sh BASE_WORKTREE [BIN] [options]
#
# Every slice of the paintkit epics (#112, #182, #183) promises "a render with
# the new keys unset is byte-identical to before". L1, S1 and L2 each proved it
# by hand; this script is that proof, written once. It renders every tracked
# file that loads lib/paintkit.ps -- `examples/*.ps`, `gallery/*.ps` and
# `selftest/drivers/*.ps` -- from two trees with ONE binary, and compares the
# PNGs byte for byte.
#
#   BASE_WORKTREE  a checkout of the baseline. It MUST be at the branch's base
#                  commit (what the branch was last rebased onto, i.e. after
#                  work-issue step 5.5), NOT a newer origin/main: files a later
#                  merge changed would show up as differences that are not the
#                  branch's. `git worktree add --detach <path> origin/main` right
#                  after the step 5.5 rebase is the usual way to make one.
#   BIN            the pscat binary. Default: `$CARGO_TARGET_DIR/release/pscat`
#                  (else `<this repo>/target/release/pscat`), built first with
#                  `cargo build --release --bin pscat`. A BIN you pass is used
#                  as-is and never built. The same binary renders both trees, so
#                  a difference can only come from the trees' own files.
#
# The "branch" tree is the repo this script lives in. Each render runs with that
# tree as its working directory, because `(lib/paintkit.ps) run` resolves
# relative to the cwd: this is what makes each tree load its own library.
#
# What is compared
#   * PNG only. SVG output varies from run to run within one tree (#258).
#   * `--dpi N` (default 72) for examples and drivers. Gallery pieces render at
#     `--gallery-dpi N` (default 144): the committed gallery/renders/*.png stills
#     are 2x supersampled, so this is "the committed still's resolution".
#   * The page comes from the file's `%%SelfTestPage: WxH` (drivers) or
#     `%%BoundingBox: 0 0 W H`, as each file's own header documents.
#   * One line per page: `identical`, `DIFFERS` or `ERROR`. A pscat run that
#     exits nonzero in either tree is an ERROR even when both trees wrote the
#     same partial canvas, because pscat writes the canvas after an error and a
#     file that fails identically would otherwise read as identical.
#   * Files come from `git ls-files` in the branch tree, so untracked scratch
#     files never join in. A file absent from the base (a new demo) is listed
#     as `new` and is not compared.
#
# Options
#   --control FILE        negative control. A tiny driver that uses a key only
#                         the branch has. It is rendered from both trees and the
#                         PNGs must DIFFER, which proves each tree really loaded
#                         its own library. Without it the script warns and skips.
#   --control-page WxH    page for the control (default: its own header, else
#                         pscat's default page).
#   --gallery-still       also require gallery/alpine_lake.ps to equal the
#                         committed gallery/renders/alpine_lake.png at 2x, in
#                         each tree (needs --gallery-dpi 144).
#   --dpi N               examples and drivers (default 72)
#   --gallery-dpi N       gallery pieces (default 144)
#   --out DIR             scratch directory (default: a fresh mktemp -d under
#                         ${TMPDIR:-/tmp}, removed on success). Never inside
#                         either tree.
#   --keep                keep the scratch directory
#   -j N                  parallel renders (default: CPU count)
#
# Exit status: 0 all identical (and the control differed); 1 a page DIFFERS or
# ERRORs; 2 the control did NOT differ (or was unusable), so the pages that
# looked identical prove nothing; 64 a usage or harness error (bad arguments,
# no binary, the render driver itself failed), not a result.
#
# Run time on the current corpus is about one minute (alpine_lake is the long
# pole); the renders run in parallel.
#
# Bash 3.2 compatible (macOS /bin/bash).

set -euo pipefail

SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/$(basename "${BASH_SOURCE[0]}")"
ROOT="$(cd "$(dirname "$SELF")/.." && pwd -P)"

die() { echo "identity_check.sh: $*" >&2; exit 64; }

# ---- internal: render one file in one tree, record its exit status ---------
# identity_check.sh --render-one BIN TREE OUTDIR PAGE DPI FILE
if [ "${1:-}" = "--render-one" ]; then
    [ $# -eq 7 ] || die "--render-one takes 6 arguments"
    bin=$2; tree=$3; outdir=$4; page=$5; dpi=$6; file=$7
    mkdir -p "$outdir"
    set +e
    if [ "$page" != "-" ]; then
        (cd "$tree" && "$bin" --page "$page" --dpi "$dpi" --png "$outdir/page.png" "$file") \
            > "$outdir/log" 2>&1
    else
        (cd "$tree" && "$bin" --dpi "$dpi" --png "$outdir/page.png" "$file") \
            > "$outdir/log" 2>&1
    fi
    echo $? > "$outdir/rc"
    exit 0
fi

# ---- arguments ---------------------------------------------------------------
BASE=""
BIN=""
CONTROL=""
CONTROL_PAGE=""
GALLERY_STILL=0
DPI=72
GALLERY_DPI=144
OUT=""
KEEP=0
JOBS=""
positional=0

while [ $# -gt 0 ]; do
    case "$1" in
        --control)       [ $# -ge 2 ] || die "--control needs a file"; CONTROL=$2; shift 2 ;;
        --control-page)  [ $# -ge 2 ] || die "--control-page needs WxH"; CONTROL_PAGE=$2; shift 2 ;;
        --gallery-still) GALLERY_STILL=1; shift ;;
        --dpi)           [ $# -ge 2 ] || die "--dpi needs a number"; DPI=$2; shift 2 ;;
        --gallery-dpi)   [ $# -ge 2 ] || die "--gallery-dpi needs a number"; GALLERY_DPI=$2; shift 2 ;;
        --out)           [ $# -ge 2 ] || die "--out needs a directory"; OUT=$2; shift 2 ;;
        --keep)          KEEP=1; shift ;;
        -j)              [ $# -ge 2 ] || die "-j needs a number"; JOBS=$2; shift 2 ;;
        -h|--help)       sed -n '2,/^set -euo/p' "$SELF" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*)              die "unknown option $1 (see --help)" ;;
        *)
            positional=$((positional + 1))
            case $positional in
                1) BASE=$1 ;;
                2) BIN=$1 ;;
                *) die "too many arguments (see --help)" ;;
            esac
            shift ;;
    esac
done

[ -n "$BASE" ] || die "usage: identity_check.sh BASE_WORKTREE [BIN] [options]"
[ -d "$BASE" ] || die "BASE_WORKTREE '$BASE' is not a directory"
BASE="$(cd "$BASE" && pwd -P)"
[ -f "$BASE/lib/paintkit.ps" ] || die "$BASE has no lib/paintkit.ps"
[ -f "$ROOT/lib/paintkit.ps" ] || die "$ROOT has no lib/paintkit.ps"
if [ "$BASE" = "$ROOT" ]; then
    die "BASE_WORKTREE is this tree itself; pass a separate checkout of the base commit"
fi
case "$DPI$GALLERY_DPI" in *[!0-9]*) die "--dpi and --gallery-dpi take whole numbers" ;; esac
if [ "$GALLERY_STILL" -eq 1 ] && [ "$GALLERY_DPI" -ne 144 ]; then
    die "--gallery-still compares at 2x, so it needs --gallery-dpi 144"
fi
if [ -n "$CONTROL" ]; then
    [ -f "$CONTROL" ] || die "--control file '$CONTROL' not found"
    CONTROL="$(cd "$(dirname "$CONTROL")" && pwd -P)/$(basename "$CONTROL")"
fi
if [ -z "$JOBS" ]; then
    JOBS=$(sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo 4)
fi
case "$JOBS" in ''|*[!0-9]*|0) die "-j takes a positive number" ;; esac

# ---- binary ------------------------------------------------------------------
if [ -z "$BIN" ]; then
    BIN="${CARGO_TARGET_DIR:-$ROOT/target}/release/pscat"
    echo "building the release pscat (cargo build --release --bin pscat)..." >&2
    cargo build --release --bin pscat --manifest-path "$ROOT/Cargo.toml" >&2
fi
[ -x "$BIN" ] || die "pscat binary '$BIN' is not executable"
BIN="$(cd "$(dirname "$BIN")" && pwd -P)/$(basename "$BIN")"

# ---- scratch -----------------------------------------------------------------
made_out=0
if [ -z "$OUT" ]; then
    OUT=$(mktemp -d "${TMPDIR:-/tmp}/identity_check.XXXXXX")
    made_out=1
else
    # Check before creating anything, so a refused path leaves nothing behind.
    out_parent="$OUT"
    while [ ! -d "$out_parent" ]; do out_parent=$(dirname "$out_parent"); done
    out_real="$(cd "$out_parent" && pwd -P)${OUT#"$out_parent"}"
    case "$out_real/" in
        "$ROOT"/*|"$BASE"/*) die "--out '$OUT' is inside one of the trees; use a directory outside the repo" ;;
    esac
    mkdir -p "$OUT"
fi
OUT="$(cd "$OUT" && pwd -P)"
case "$OUT/" in
    "$ROOT"/*|"$BASE"/*) die "--out '$OUT' is inside one of the trees; use a directory outside the repo" ;;
esac
rm -rf "$OUT/base" "$OUT/head"
mkdir -p "$OUT/base" "$OUT/head"

FAILED=0
CONTROL_FAILED=0
cleanup() {
    if [ "$made_out" -eq 1 ] && [ "$KEEP" -eq 0 ] && [ "$FAILED" -eq 0 ] && [ "$CONTROL_FAILED" -eq 0 ]; then
        rm -rf "$OUT"
    else
        echo "scratch renders kept in $OUT" >&2
    fi
}
trap cleanup EXIT

# ---- the file list -----------------------------------------------------------
# page_of FILE: WxH from %%SelfTestPage, else %%BoundingBox 0 0 W H, else empty.
page_of() {
    local p
    p=$(sed -n 's/^%%SelfTestPage:[[:space:]]*\([0-9][0-9]*x[0-9][0-9]*\).*/\1/p' "$1" | head -1)
    if [ -z "$p" ]; then
        p=$(sed -n 's/^%%BoundingBox:[[:space:]]*0[[:space:]][[:space:]]*0[[:space:]][[:space:]]*\([0-9][0-9]*\)[[:space:]][[:space:]]*\([0-9][0-9]*\).*/\1x\2/p' "$1" | head -1)
    fi
    printf '%s' "$p"
}
key_of() { printf '%s' "$1" | sed 's|/|__|g'; }

FILES="$OUT/files.txt"
: > "$FILES"
while IFS= read -r f; do
    [ -n "$f" ] || continue
    if grep -qE '\(lib/paintkit\.ps\)' "$ROOT/$f"; then
        printf '%s\n' "$f" >> "$FILES"
    fi
done <<EOF_LIST
$(git -C "$ROOT" ls-files -- 'examples/*.ps' 'gallery/*.ps' 'selftest/drivers/*.ps')
EOF_LIST
[ -s "$FILES" ] || die "no tracked file under examples/, gallery/ or selftest/drivers/ loads (lib/paintkit.ps)"

# ---- the job list (gallery first: alpine_lake is the long pole) --------------
JOBLIST="$OUT/jobs.nul"
: > "$JOBLIST"
add_job() { # tree-name tree-path key page dpi file
    # xargs -0 drops empty arguments, so "no page" travels as "-".
    printf '%s\0%s\0%s\0%s\0%s\0' "$2" "$OUT/$1/$3" "${4:--}" "$5" "$6" >> "$JOBLIST"
}
COMPARE="$OUT/compare.txt"   # lines: file key
: > "$COMPARE"
NEWFILES="$OUT/new.txt"
: > "$NEWFILES"

queue() { # file
    local f=$1 key page dpi
    key=$(key_of "$f")
    page=$(page_of "$ROOT/$f")
    case "$f" in gallery/*) dpi=$GALLERY_DPI ;; *) dpi=$DPI ;; esac
    if [ -z "$page" ]; then
        echo "identity_check.sh: warning: $f declares no page size; using pscat's default page" >&2
    fi
    if [ ! -f "$BASE/$f" ]; then
        printf '%s\n' "$f" >> "$NEWFILES"
        return
    fi
    add_job base "$BASE" "$key" "$page" "$dpi" "$f"
    add_job head "$ROOT" "$key" "$page" "$dpi" "$f"
    printf '%s %s\n' "$f" "$key" >> "$COMPARE"
}
while IFS= read -r f; do case "$f" in gallery/*) queue "$f" ;; esac; done < "$FILES"
while IFS= read -r f; do case "$f" in gallery/*) ;; *) queue "$f" ;; esac; done < "$FILES"

CONTROL_KEY="__control__"
if [ -n "$CONTROL" ]; then
    cpage=$CONTROL_PAGE
    [ -n "$cpage" ] || cpage=$(page_of "$CONTROL")
    add_job base "$BASE" "$CONTROL_KEY" "$cpage" "$DPI" "$CONTROL"
    add_job head "$ROOT" "$CONTROL_KEY" "$cpage" "$DPI" "$CONTROL"
else
    echo "identity_check.sh: WARNING: no --control given. Without a negative control nothing proves each tree" >&2
    echo "  loaded its own lib/paintkit.ps, so an all-identical result could mean both used the same one." >&2
fi

# Each job is 5 NUL-separated fields after the fixed `--render-one BIN`; the
# tree name is encoded in OUTDIR, so xargs passes: TREE OUTDIR PAGE DPI FILE.
START=$(date +%s)
nfiles=$(wc -l < "$COMPARE" | tr -d ' ')
echo "rendering $nfiles files x 2 trees with $BIN (-j $JOBS)" >&2
xargs -0 -n 5 -P "$JOBS" "$SELF" --render-one "$BIN" < "$JOBLIST" \
    || die "the render driver failed (see $OUT); this is a harness error, not a difference"

# ---- compare -----------------------------------------------------------------
pngs_of() { # dir: png basenames, one per line
    (cd "$1" 2>/dev/null && ls -1 page*.png 2>/dev/null) || true
}
rc_of() { cat "$1/rc" 2>/dev/null || echo "?"; }

# compare_one LABEL KEYDIR: prints the per-page lines; returns 0 identical,
# 1 differs or error.
npages=0
nident=0
nbad=0
compare_one() {
    local label=$1 key=$2 b="$OUT/base/$2" h="$OUT/head/$2"
    local brc hrc bl hl p i st=0
    brc=$(rc_of "$b"); hrc=$(rc_of "$h")
    bl=$(pngs_of "$b"); hl=$(pngs_of "$h")
    if [ "$hrc" != "0" ]; then
        echo "ERROR      $label (pscat exited $hrc in the branch tree; see $h/log)"
        return 1
    fi
    if [ "$brc" != "0" ]; then
        echo "ERROR      $label (pscat exited $brc in the base tree; see $b/log)"
        return 1
    fi
    if [ -z "$bl" ] || [ -z "$hl" ]; then
        echo "ERROR      $label (no PNG written)"
        return 1
    fi
    if [ "$bl" != "$hl" ]; then
        echo "DIFFERS    $label (page count: base $(printf '%s\n' "$bl" | wc -l | tr -d ' '), branch $(printf '%s\n' "$hl" | wc -l | tr -d ' '))"
        return 1
    fi
    i=0
    for p in $hl; do
        i=$((i + 1))
        npages=$((npages + 1))
        if cmp -s "$b/$p" "$h/$p"; then
            echo "identical  $label page $i"
            nident=$((nident + 1))
        else
            echo "DIFFERS    $label page $i"
            nbad=$((nbad + 1))
            st=1
        fi
    done
    return $st
}

while read -r f key; do
    if ! compare_one "$f" "$key"; then
        FAILED=1
    fi
done < "$COMPARE"

while IFS= read -r f; do
    [ -n "$f" ] || continue
    echo "new        $f (not in the base tree, not compared)"
done < "$NEWFILES"

# ---- the gallery still ---------------------------------------------------------
if [ "$GALLERY_STILL" -eq 1 ]; then
    still=gallery/alpine_lake.ps
    skey=$(key_of "$still")
    for t in base head; do
        tree=$BASE; [ "$t" = head ] && tree=$ROOT
        committed="$tree/gallery/renders/alpine_lake.png"
        got="$OUT/$t/$skey/page.png"
        if [ ! -f "$committed" ] || [ ! -f "$got" ]; then
            echo "ERROR      alpine_lake at 2x vs committed still ($t): missing file"
            FAILED=1
        elif cmp -s "$committed" "$got"; then
            echo "identical  alpine_lake at 2x vs the committed still ($t tree)"
        else
            echo "DIFFERS    alpine_lake at 2x vs the committed still ($t tree)"
            FAILED=1
        fi
    done
fi

# ---- the negative control -------------------------------------------------------
if [ -n "$CONTROL" ]; then
    b="$OUT/base/$CONTROL_KEY"; h="$OUT/head/$CONTROL_KEY"
    hrc=$(rc_of "$h")
    if [ "$hrc" != "0" ]; then
        echo "CONTROL BROKEN  the control driver exited $hrc in the branch tree (see $h/log)"
        CONTROL_FAILED=1
    elif [ -z "$(pngs_of "$h")" ]; then
        echo "CONTROL BROKEN  the control driver wrote no PNG in the branch tree"
        CONTROL_FAILED=1
    elif [ "$(pngs_of "$b")" != "$(pngs_of "$h")" ]; then
        echo "control    differs between the trees, as it must (page count differs)"
    else
        cdiff=0
        for p in $(pngs_of "$h"); do
            cmp -s "$b/$p" "$h/$p" || cdiff=1
        done
        if [ "$cdiff" -eq 1 ]; then
            echo "control    differs between the trees, as it must"
        else
            echo "CONTROL FAILED  the control rendered identically in both trees: each tree may not have loaded its own library"
            CONTROL_FAILED=1
        fi
    fi
fi

ELAPSED=$(( $(date +%s) - START ))
echo
if [ "$FAILED" -eq 0 ]; then
    echo "identity_check.sh: $nident of $npages page renders identical across $nfiles files (${ELAPSED}s)"
else
    echo "identity_check.sh: FAILED, $nbad pages differ, plus any ERROR lines above ($nident of $npages identical, ${ELAPSED}s)"
fi
if [ "$FAILED" -ne 0 ]; then
    exit 1
fi
if [ "$CONTROL_FAILED" -ne 0 ]; then
    echo "identity_check.sh: the negative control did not hold; the identical result above proves nothing" >&2
    exit 2
fi
exit 0
