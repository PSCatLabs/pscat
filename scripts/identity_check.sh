#!/usr/bin/env bash
# identity_check.sh -- shared cross-tree PNG byte-identity check for paintkit
# slices (issue #263), and (--loader / --files, #303) for any lib change that
# must render identically.
#
#   scripts/identity_check.sh BASE_WORKTREE [BIN] [options]
#   scripts/identity_check.sh BASE_WORKTREE --base-bin PATH --head-bin PATH [options]
#
# Every slice of the paintkit epics (#112, #182, #183) promises "a render with
# the new keys unset is byte-identical to before". L1, S1 and L2 each proved it
# by hand; this script is that proof, written once. It renders every tracked
# file that loads lib/paintkit.ps -- `examples/*.ps`, `gallery/*.ps` and
# `selftest/drivers/*.ps` -- from two trees with ONE binary (or, for an
# interpreter change, one binary per tree: --base-bin / --head-bin, below), and
# compares the PNGs byte for byte. Any other lib change that must render identically uses
# `--loader lib/<name>.ps` (below) to pick the files instead.
#
#   BASE_WORKTREE  a checkout of the baseline. It MUST be at the branch's base
#                  commit (what the branch was last rebased onto, i.e. after
#                  work-issue step 5.5), NOT a newer origin/main: files a later
#                  merge changed would show up as differences that are not the
#                  branch's. `git worktree add --detach <path> origin/main` right
#                  after the step 5.5 rebase is the usual way to make one.
#   BIN            the pscat binary. Default: the release binary in Cargo's
#                  target directory (from `cargo metadata`), built first with
#                  `cargo build --release --bin pscat`. A BIN you pass is used
#                  as-is and never built. The same binary renders both trees, so
#                  a difference can only come from the trees' own files.
#   --base-bin PATH  \ two binaries, for a change under src/ (issue #335). The base
#   --head-bin PATH  / tree renders with PATH of the first, the head tree with the
#                  second. Give both or neither, and not together with BIN. Neither
#                  is built here. Make the base one with
#                  scripts/build_base_bin.sh BASE_WORKTREE OUT_BIN (BASE_WORKTREE is
#                  the same tree you pass as the first argument), the head one is the
#                  release binary of this tree. A difference can now come from the
#                  binary or from the trees' files. For a pure src/ change the files
#                  are the same in both trees, so a difference is the binary's.
#                  The default file pick (files that load lib/paintkit.ps) is narrow
#                  for an interpreter change: add --loader lib/artkit.ps (about 63
#                  files) or a --files LIST.
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
#   * One line per page: `identical`, `DIFFERS`, `ERROR` or `SKIPPED`. A pscat
#     run that exits nonzero in either tree is an ERROR (pscat writes the
#     canvas after an error, so a partly drawn page must not read as
#     identical), with one exception: a file that fails IDENTICALLY in both
#     trees is `SKIPPED` (#504). Identically means the same nonzero exit
#     status, the same log (the scratch path aside) and byte-equal canvases.
#     Such a failure is the file's own (tw_proto.ps runs a generated
#     twglyphs.ps that is not tracked), not a difference between the trees,
#     so it must not fail every whole-corpus run. It is never silent: the
#     SKIPPED line, a closing WARNING listing every skipped file, and "K
#     skipped" in the summary. A failure that differs in any way (one tree
#     only, another status, another message, another canvas) stays an ERROR.
#     If every file is skipped nothing was compared, and that exits 1.
#   * Files come from `git ls-files` in the branch tree, so untracked scratch
#     files never join in. A file absent from the base (a new demo) is listed
#     as `new` and is not compared.
#   * A multi-page file is compared page by page: pscat writes page-001.png,
#     page-002.png, ... (a single page is page.png). The page lists must match
#     in both trees, then each page is compared and reported as `page N`.
#
# Options
#   --loader LIB          pick the files that load LIB (e.g. lib/artkit.ps)
#                         instead of lib/paintkit.ps. Repeatable. The pick is
#                         TRANSITIVE: a lib that loads LIB (`(lib/x.ps) run`,
#                         also in a `% @requires:` line) counts as a loader too,
#                         so --loader lib/artkit.ps also covers every file that
#                         reaches artkit through paintkit, drykit, ... The
#                         libs found are listed on stderr. A file mentioned
#                         only inside a comment is over-included, never missed.
#                         Without --loader (or --files) the pick is exactly
#                         the old one: files that load lib/paintkit.ps.
#   --files LIST          also (or instead, with no --loader) render the files
#                         listed in LIST: one repo-relative path per line, blank
#                         lines and `#` comments ignored, each tracked in the
#                         branch tree. Use it for a slice no loader describes.
#   --expect-diff LIST    files that are MEANT to render differently (issue #674): an
#                         opt-in change, a new default, a deliberate fix. Same format
#                         as --files (tracked paths, one per line, `#` comments). A
#                         listed file that differs prints `EXPECTED` and does not fail
#                         the run; a listed file that comes out identical prints
#                         `UNCHANGED` and fails it (exit 1), since the change did not
#                         take effect; a listed file outside the compared set (not
#                         picked, or new in the branch) is a usage error (exit 64), so
#                         a typo cannot pass silently. Every file NOT listed must still
#                         be identical. A run that needs this flag is a visible change
#                         (SDLC.md "Who merges", tier b), not a proof of identity.
#   --control FILE        negative control. A tiny driver that uses a key only
#                         the branch has. It is rendered from both trees and the
#                         PNGs must DIFFER, which proves each tree really loaded
#                         its own library. Without it the script warns and skips.
#                         With --base-bin/--head-bin it is the same test for the
#                         binaries: a driver that uses behaviour only the head
#                         binary has (for an operator fix, the call that used to
#                         raise an error). The two renders must differ, which
#                         proves each tree ran its own binary.
#   --control-page WxH    page for the control (default: its own header, else
#                         pscat's default page).
#   --gallery-still       also require gallery/alpine_lake.ps to equal the
#                         committed gallery/renders/alpine_lake.png at 2x, in
#                         each tree (needs --gallery-dpi 144).
#   --dpi N               examples and drivers (default 72)
#   --gallery-dpi N       gallery pieces (default 144)
#   --out DIR             where the scratch directory goes (default
#                         ${TMPDIR:-/tmp}). The script makes its own fresh
#                         identity_check.XXXXXX inside it and deletes only
#                         that, on success. Never inside either tree.
#   --keep                keep the scratch directory
#   -j N                  parallel renders (default: CPU count)
#
# Exit status: 0 all identical, SKIPPED or EXPECTED (and the control differed); 1 a page
# DIFFERS or ERRORs, an --expect-diff file is UNCHANGED, or nothing at all was compared; 2 the control did NOT differ (or was unusable), so the pages that
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
BASE_BIN=""
HEAD_BIN=""
CONTROL=""
CONTROL_PAGE=""
LOADERS=""       # newline-separated --loader libs (empty = lib/paintkit.ps)
FILES_LIST=""    # --files LIST
EXPECT_LIST=""   # --expect-diff LIST
GALLERY_STILL=0
DPI=72
GALLERY_DPI=144
OUT=""
KEEP=0
JOBS=""
positional=0

while [ $# -gt 0 ]; do
    case "$1" in
        --loader)        [ $# -ge 2 ] || die "--loader needs a lib path such as lib/artkit.ps"
                         LOADERS="${LOADERS:+$LOADERS
}$2"; shift 2 ;;
        --files)         [ $# -ge 2 ] || die "--files needs a list file"; FILES_LIST=$2; shift 2 ;;
        --expect-diff)   [ $# -ge 2 ] || die "--expect-diff needs a list file"; EXPECT_LIST=$2; shift 2 ;;
        --base-bin)      [ $# -ge 2 ] || die "--base-bin needs a pscat binary"; BASE_BIN=$2; shift 2 ;;
        --head-bin)      [ $# -ge 2 ] || die "--head-bin needs a pscat binary"; HEAD_BIN=$2; shift 2 ;;
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

[ -n "$BASE" ] || die "usage: identity_check.sh BASE_WORKTREE [BIN | --base-bin PATH --head-bin PATH] [options]"
TWO_BINS=0
if [ -n "$BASE_BIN" ] || [ -n "$HEAD_BIN" ]; then
    [ -n "$BASE_BIN" ] && [ -n "$HEAD_BIN" ] || die "--base-bin and --head-bin go together; give both"
    [ -z "$BIN" ] || die "BIN and --base-bin/--head-bin are alternatives; give one or the other"
    TWO_BINS=1
fi
[ -d "$BASE" ] || die "BASE_WORKTREE '$BASE' is not a directory"
BASE="$(cd "$BASE" && pwd -P)"
# The libs that pick the files. Default: lib/paintkit.ps (the old behaviour).
DEFAULT_LOADER=0
if [ -z "$LOADERS" ] && [ -z "$FILES_LIST" ]; then
    LOADERS="lib/paintkit.ps"
    DEFAULT_LOADER=1
fi
while IFS= read -r l; do
    [ -n "$l" ] || continue
    case "$l" in lib/*.ps) ;; *) die "--loader '$l' must look like lib/<name>.ps" ;; esac
    [ -f "$BASE/$l" ] || die "$BASE has no $l"
    [ -f "$ROOT/$l" ] || die "$ROOT has no $l"
done <<EOF_LOADERS
$LOADERS
EOF_LOADERS
if [ -n "$FILES_LIST" ]; then
    [ -f "$FILES_LIST" ] || die "--files list '$FILES_LIST' not found"
    FILES_LIST="$(cd "$(dirname "$FILES_LIST")" && pwd -P)/$(basename "$FILES_LIST")"
    # Check the paths now, before any scratch directory exists, so a typo leaves nothing behind.
    while IFS= read -r f || [ -n "$f" ]; do
        f="${f%%#*}"
        f=$(printf '%s' "$f" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')
        [ -n "$f" ] || continue
        [ -f "$ROOT/$f" ] && git --literal-pathspecs -C "$ROOT" ls-files --error-unmatch -- "$f" >/dev/null 2>&1 \
            || die "--files: '$f' is not a tracked file in $ROOT"
    done < "$FILES_LIST"
fi
EXPECTED=""      # newline-separated --expect-diff paths
if [ -n "$EXPECT_LIST" ]; then
    [ -f "$EXPECT_LIST" ] || die "--expect-diff list '$EXPECT_LIST' not found"
    while IFS= read -r f || [ -n "$f" ]; do
        f="${f%%#*}"
        f=$(printf '%s' "$f" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')
        [ -n "$f" ] || continue
        [ -f "$ROOT/$f" ] && git --literal-pathspecs -C "$ROOT" ls-files --error-unmatch -- "$f" >/dev/null 2>&1 \
            || die "--expect-diff: '$f' is not a tracked file in $ROOT"
        EXPECTED="${EXPECTED:+$EXPECTED
}$f"
    done < "$EXPECT_LIST"
    [ -n "$EXPECTED" ] || die "--expect-diff list '$EXPECT_LIST' names no file"
fi
is_expected() { [ -n "$EXPECTED" ] && printf '%s\n' "$EXPECTED" | grep -qxF -- "$1"; }
LOADER_DESC=$(printf '%s' "$LOADERS" | tr '\n' ' ' | sed 's/ *$//')
[ -n "$LOADER_DESC" ] || LOADER_DESC="library"
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
if [ "$TWO_BINS" -eq 1 ]; then
    for b in "$BASE_BIN" "$HEAD_BIN"; do
        [ -x "$b" ] && [ -f "$b" ] || die "pscat binary '$b' is not an executable file"
    done
    BASE_BIN="$(cd "$(dirname "$BASE_BIN")" && pwd -P)/$(basename "$BASE_BIN")"
    HEAD_BIN="$(cd "$(dirname "$HEAD_BIN")" && pwd -P)/$(basename "$HEAD_BIN")"
    BIN=$HEAD_BIN
    if cmp -s "$BASE_BIN" "$HEAD_BIN"; then
        echo "identity_check.sh: WARNING: --base-bin and --head-bin are byte-identical files, so the" >&2
        echo "  binaries cannot make the control differ; the check only compares the trees." >&2
    fi
else
    BASE_BIN=""
fi
if [ "$TWO_BINS" -eq 0 ] && [ -z "$BIN" ]; then
    echo "building the release pscat (cargo build --release --bin pscat)..." >&2
    cargo build --release --bin pscat --manifest-path "$ROOT/Cargo.toml" >&2
    # Ask Cargo where the binary went (scripts/selftest.sh does the same): a
    # `build.target-dir` in config.toml would otherwise leave us running a
    # stale binary from $ROOT/target.
    TARGET_DIR=$(cargo metadata --format-version 1 --no-deps \
        --manifest-path "$ROOT/Cargo.toml" 2>/dev/null \
        | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
    BIN="${TARGET_DIR:-${CARGO_TARGET_DIR:-$ROOT/target}}/release/pscat"
fi
[ -x "$BIN" ] || die "pscat binary '$BIN' is not executable"
BIN="$(cd "$(dirname "$BIN")" && pwd -P)/$(basename "$BIN")"
[ -n "$BASE_BIN" ] || BASE_BIN=$BIN
HEAD_BIN=$BIN

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
# Work in a fresh directory this run owns, never in --out itself: nothing
# below ever deletes anything it did not create, so --out may safely be a
# directory that holds other things (even a baseline checkout).
if [ "$made_out" -eq 0 ]; then
    OUT=$(mktemp -d "$OUT/identity_check.XXXXXX")
fi
mkdir -p "$OUT/base" "$OUT/head"

FAILED=0
CONTROL_FAILED=0
cleanup() {
    local rc=$?   # any nonzero exit (a diff, a bad control, a harness error) keeps the logs
    if [ "$KEEP" -eq 0 ] && [ "$rc" -eq 0 ]; then
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
if [ -n "$LOADERS" ]; then
    # The libs that load a --loader lib, directly or through another lib. Only in
    # --loader mode, where a change to a base lib (artkit) reaches most files
    # through the libs built on it. The default (no --loader) stays the old direct
    # match on lib/paintkit.ps.
    SET="$LOADERS"
    if [ "$DEFAULT_LOADER" -eq 0 ]; then
        changed=1
        while [ "$changed" -eq 1 ]; do
            changed=0
            for cand in $(git -C "$ROOT" ls-files -- 'lib/*.ps'); do
                case "$cand" in lib/*/*) continue ;; esac
                if printf '%s\n' "$SET" | grep -qxF "$cand"; then continue; fi
                while IFS= read -r m; do
                    if grep -qF "($m)" "$ROOT/$cand"; then
                        SET="$SET
$cand"
                        changed=1
                        break
                    fi
                done <<EOF_SET
$SET
EOF_SET
            done
        done
        echo "identity_check.sh: files that load any of: $(printf '%s' "$SET" | tr '\n' ' ')" >&2
    fi
    while IFS= read -r f; do
        [ -n "$f" ] || continue
        while IFS= read -r m; do
            if grep -qF "($m)" "$ROOT/$f"; then
                printf '%s\n' "$f" >> "$FILES"
                break
            fi
        done <<EOF_SET2
$SET
EOF_SET2
    done <<EOF_LIST
$(git -C "$ROOT" ls-files -- 'examples/*.ps' 'gallery/*.ps' 'selftest/drivers/*.ps')
EOF_LIST
fi
if [ -n "$FILES_LIST" ]; then
    while IFS= read -r f || [ -n "$f" ]; do
        f="${f%%#*}"
        f=$(printf '%s' "$f" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')
        [ -n "$f" ] || continue
        printf '%s\n' "$f" >> "$FILES"
    done < "$FILES_LIST"
    # drop duplicates, keep order
    awk '!seen[$0]++' "$FILES" > "$FILES.u" && mv "$FILES.u" "$FILES"
fi
[ -s "$FILES" ] || die "no tracked file under examples/, gallery/ or selftest/drivers/ loads ($LOADER_DESC)"
if [ "$GALLERY_STILL" -eq 1 ] && ! grep -qx 'gallery/alpine_lake.ps' "$FILES"; then
    die "--gallery-still needs gallery/alpine_lake.ps among the files, and this pick ($LOADER_DESC) leaves it out"
fi

# ---- the job list (gallery first: alpine_lake is the long pole) --------------
JOBLIST="$OUT/jobs.nul"
: > "$JOBLIST"
add_job() { # tree-name tree-path key page dpi file
    # xargs -0 drops empty arguments, so "no page" travels as "-". The binary is
    # part of the job: the base tree renders with BASE_BIN, the head tree with HEAD_BIN.
    local bin=$HEAD_BIN
    [ "$1" = base ] && bin=$BASE_BIN
    printf '%s\0%s\0%s\0%s\0%s\0%s\0' "$bin" "$2" "$OUT/$1/$3" "${4:--}" "$5" "$6" >> "$JOBLIST"
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

# Every --expect-diff file must be one this run compares: a path the pick left out, or
# one that is new in the branch, can never show its difference, so it is a usage error.
if [ -n "$EXPECTED" ]; then
    while IFS= read -r f; do
        [ -n "$f" ] || continue
        if grep -qxF -- "$f" "$NEWFILES"; then
            die "--expect-diff: $f is new in this branch (not in the base tree), so there is nothing to compare"
        fi
        cut -d' ' -f1 "$COMPARE" | grep -qxF -- "$f" \
            || die "--expect-diff: $f is not among the files compared (add it with --files, or check --loader)"
    done <<EOF_EXPECTED
$EXPECTED
EOF_EXPECTED
fi

CONTROL_KEY="__control__"
if [ -n "$CONTROL" ]; then
    cpage=$CONTROL_PAGE
    [ -n "$cpage" ] || cpage=$(page_of "$CONTROL")
    add_job base "$BASE" "$CONTROL_KEY" "$cpage" "$DPI" "$CONTROL"
    add_job head "$ROOT" "$CONTROL_KEY" "$cpage" "$DPI" "$CONTROL"
else
    echo "identity_check.sh: WARNING: no --control given. Without a negative control nothing proves each tree" >&2
    if [ "$TWO_BINS" -eq 1 ]; then
        echo "  ran its own binary, so an all-identical result could mean both used the same one." >&2
    else
        echo "  loaded its own $LOADER_DESC, so an all-identical result could mean both used the same one." >&2
    fi
fi

# Each job is 6 NUL-separated fields after the fixed `--render-one`; the tree
# name is encoded in OUTDIR, so xargs passes: BIN TREE OUTDIR PAGE DPI FILE.
START=$(date +%s)
nfiles=$(wc -l < "$COMPARE" | tr -d ' ')
if [ "$TWO_BINS" -eq 1 ]; then
    echo "rendering $nfiles files x 2 trees: base with $BASE_BIN, head with $HEAD_BIN (-j $JOBS)" >&2
else
    echo "rendering $nfiles files x 2 trees with $BIN (-j $JOBS)" >&2
fi
xargs -0 -n 6 -P "$JOBS" "$SELF" --render-one < "$JOBLIST" \
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
SKIPPED_FILES=""
nskipped=0
nexpected=0

# fails_identically BDIR HDIR: 0 when the two renders of one file failed the
# same way (#504): same log with the scratch path masked, same PNG names, same
# PNG bytes. The caller has already checked the exit statuses.
fails_identically() {
    local b=$1 h=$2 bl hl p bpat="$OUT/base/" hpat="$OUT/head/"
    # (a variable holds the pattern: bash 3.2 mis-parses a quoted "$OUT/base/" inside ${..//..})
    bl=$(cat "$b/log" 2>/dev/null || true); bl=${bl//"$bpat"/OUT/}
    hl=$(cat "$h/log" 2>/dev/null || true); hl=${hl//"$hpat"/OUT/}
    [ "$bl" = "$hl" ] || return 1
    [ "$(pngs_of "$b")" = "$(pngs_of "$h")" ] || return 1
    for p in $(pngs_of "$h"); do
        cmp -s "$b/$p" "$h/$p" || return 1
    done
    return 0
}
compare_one() {
    local label=$1 key=$2 b="$OUT/base/$2" h="$OUT/head/$2"
    local brc hrc bl hl p i st=0
    brc=$(rc_of "$b"); hrc=$(rc_of "$h")
    bl=$(pngs_of "$b"); hl=$(pngs_of "$h")
    # A numeric status only: a missing rc file reads "?" in both trees and must stay an ERROR.
    if [ "$hrc" != "0" ] && [ "$brc" = "$hrc" ] && [ -z "${hrc//[0-9]/}" ] && fails_identically "$b" "$h" && is_expected "$label"; then
        echo "ERROR      $label (listed in --expect-diff, but pscat exited $hrc identically in both trees; see $h/log)"
        return 1
    fi
    if [ "$hrc" != "0" ] && [ "$brc" = "$hrc" ] && [ -z "${hrc//[0-9]/}" ] && fails_identically "$b" "$h"; then
        echo "SKIPPED    $label (pscat exited $hrc identically in both trees, same log, same canvas: nothing to compare; see $h/log)"
        nskipped=$((nskipped + 1))
        SKIPPED_FILES="$SKIPPED_FILES  $label
"
        return 0
    fi
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
        if is_expected "$label"; then
            echo "EXPECTED   $label (page count: base $(printf '%s\n' "$bl" | wc -l | tr -d ' '), branch $(printf '%s\n' "$hl" | wc -l | tr -d ' '); --expect-diff)"
            nexpected=$((nexpected + 1))
            return 0
        fi
        echo "DIFFERS    $label (page count: base $(printf '%s\n' "$bl" | wc -l | tr -d ' '), branch $(printf '%s\n' "$hl" | wc -l | tr -d ' '))"
        return 1
    fi
    local moved=0
    i=0
    for p in $hl; do
        i=$((i + 1))
        npages=$((npages + 1))
        if cmp -s "$b/$p" "$h/$p"; then
            echo "identical  $label page $i"
            nident=$((nident + 1))
        elif is_expected "$label"; then
            echo "EXPECTED   $label page $i (--expect-diff)"
            nexpected=$((nexpected + 1))
            moved=1
        else
            echo "DIFFERS    $label page $i"
            nbad=$((nbad + 1))
            st=1
        fi
    done
    if is_expected "$label" && [ "$moved" -eq 0 ]; then
        echo "UNCHANGED  $label (listed in --expect-diff, but every page is identical: the change did not take effect)"
        st=1
    fi
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
    elif [ -z "$(pngs_of "$b")" ]; then
        echo "CONTROL BROKEN  the control driver wrote no PNG in the base tree (exit $(rc_of "$b"), see $b/log); nothing was compared"
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
            if [ "$TWO_BINS" -eq 1 ]; then
                echo "CONTROL FAILED  the control rendered identically in both trees: it may not use anything only the head binary has, or the binaries may not differ"
            else
                echo "CONTROL FAILED  the control rendered identically in both trees: each tree may not have loaded its own library"
            fi
            CONTROL_FAILED=1
        fi
    fi
fi

ELAPSED=$(( $(date +%s) - START ))
SKIP_NOTE=""
[ "$nexpected" -eq 0 ] || SKIP_NOTE=", $nexpected expected differences (--expect-diff)"
if [ "$nskipped" -gt 0 ]; then
    SKIP_NOTE="$SKIP_NOTE, $nskipped skipped"
    echo >&2
    echo "identity_check.sh: WARNING: $nskipped file(s) failed identically in both trees and were NOT compared:" >&2
    printf '%s' "$SKIPPED_FILES" >&2
    # Nothing compared is not a pass: every file failing the same way points at
    # the harness (the binary, the page setup), not at the files.
    if [ "$nident" -eq 0 ] && [ "$nbad" -eq 0 ] && [ "$nexpected" -eq 0 ] && [ "$FAILED" -eq 0 ]; then
        echo "identity_check.sh: every file was skipped, so nothing was compared" >&2
        FAILED=1
    fi
fi
echo
if [ "$FAILED" -eq 0 ]; then
    echo "identity_check.sh: $nident of $npages page renders identical across $nfiles files${SKIP_NOTE} (${ELAPSED}s)"
else
    echo "identity_check.sh: FAILED, $nbad pages differ, plus any ERROR or UNCHANGED lines above ($nident of $npages identical${SKIP_NOTE}, ${ELAPSED}s)"
fi
if [ "$FAILED" -ne 0 ]; then
    exit 1
fi
if [ "$CONTROL_FAILED" -ne 0 ]; then
    echo "identity_check.sh: the negative control did not hold; the identical result above proves nothing" >&2
    exit 2
fi
exit 0
