#!/usr/bin/env bash
# build_base_bin.sh -- build the BASELINE pscat for identity_check.sh --base-bin
# (issue #335).
#
#   scripts/build_base_bin.sh BASE_WORKTREE OUT_BIN
#
# BASE_WORKTREE is a checkout of the branch's base commit, usually
#   git worktree add --detach ../<repo>-idbase origin/main
# made right after the work-issue step 5.5 rebase. The release binary is built
# INSIDE it, so tree and binary match by construction, then copied to OUT_BIN
# (a path outside both trees). The copy matters: a shared CARGO_TARGET_DIR or a
# `build.target-dir` setting makes the head build overwrite target/release/pscat.
#
# A cold release build takes minutes: run it with run_in_background.
# Then: scripts/identity_check.sh BASE_WORKTREE --base-bin OUT_BIN --head-bin HEAD_BIN ...
# Bash 3.2 compatible.
set -euo pipefail
die() { echo "build_base_bin.sh: $*" >&2; exit 64; }
[ $# -eq 2 ] || die "usage: build_base_bin.sh BASE_WORKTREE OUT_BIN"
[ -f "$1/Cargo.toml" ] || die "'$1' is not a pscat checkout"
BASE="$(cd "$1" && pwd -P)"
mkdir -p "$(dirname "$2")"
OUT="$(cd "$(dirname "$2")" && pwd -P)/$(basename "$2")"
case "$OUT" in "$BASE"/*) die "OUT_BIN must be outside BASE_WORKTREE" ;; esac
cargo build --release --bin pscat --manifest-path "$BASE/Cargo.toml" >&2
TARGET_DIR=$(cargo metadata --format-version 1 --no-deps --manifest-path "$BASE/Cargo.toml" 2>/dev/null \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
SRC="${TARGET_DIR:-${CARGO_TARGET_DIR:-$BASE/target}}/release/pscat"
[ -x "$SRC" ] || die "built binary '$SRC' not found"
cp "$SRC" "$OUT"
echo "build_base_bin.sh: $OUT (from $BASE at $(git -C "$BASE" rev-parse --short HEAD))" >&2
