#!/bin/sh
# pscat plugin launcher (issue #853): runs pscat-mcp for Claude Code, Cowork
# and Codex, installing pscat first if it is missing. No Rust or Node needed.
#
#   sh pscat-mcp.sh [pscat-mcp args]   find or install pscat, then exec pscat-mcp
#   sh pscat-mcp.sh --locate           find or install pscat, print its directory
#   sh pscat-mcp.sh --hook             SessionStart hook: report, never download
#
# Lookup, in order: $PSCAT_MCP_BIN, pscat-mcp on PATH, ~/.cargo/bin,
# /opt/homebrew/bin, /usr/local/bin. A candidate counts only if
# `pscat-mcp --version` is at least the version of the pinned release (the
# tag in ../pscat-release, pre-release suffix dropped). Then the cached copy
# of the pinned release. Otherwise download that release's archive for this
# platform from GitHub, check it against the release's SHA256SUMS, unpack it
# into the cache and run it from there (the binaries find lib/ beside them).
#
# Cache: $PSCAT_CACHE_DIR, else ~/Library/Caches/pscat/plugin (macOS) or
# ${XDG_CACHE_HOME:-~/.cache}/pscat/plugin. One directory for every host, so
# a download made for Claude Code also serves Codex, and the hook can name a
# path the skills can run. $PSCAT_RELEASE_BASE_URL replaces
# https://github.com/PSCatLabs/pscat/releases/download (tests use file://).
#
# stdout is the MCP stdio channel: everything this script says goes to
# stderr, except --locate's directory and --hook's JSON.
set -u

REPO_URL=https://github.com/PSCatLabs/pscat
HERE=$(cd "$(dirname "$0")" 2>/dev/null && pwd -P) || HERE=.
PLUGIN_DIR=$(dirname "$HERE")

say() { printf 'pscat plugin: %s\n' "$*" >&2; }
die() { say "$*"; exit 1; }

# PSCAT_MCP_BIN pointing back at this script would recurse forever.
[ -n "${PSCAT_PLUGIN_LAUNCHER:-}" ] && die "PSCAT_MCP_BIN must name pscat-mcp itself, not this launcher"
PSCAT_PLUGIN_LAUNCHER=1
export PSCAT_PLUGIN_LAUNCHER

# --- the pinned release ------------------------------------------------------
TAG=
[ -r "$PLUGIN_DIR/pscat-release" ] && TAG=$(sed -n '/^[[:space:]]*#/d; s/[[:space:]]//g; /./{p;q;}' "$PLUGIN_DIR/pscat-release")
case "$TAG" in
  v[0-9]*.[0-9]*.[0-9]*) ;;
  *) TAG= ;;
esac
# 0.7.0 from v0.7.0-rc.1: the lowest installed version this plugin accepts.
MIN_VERSION=$(printf '%s\n' "${TAG#v}" | sed 's/[-+].*//')
BASE_URL=${PSCAT_RELEASE_BASE_URL:-$REPO_URL/releases/download}
BASE_URL=${BASE_URL%/}

# --- platform ----------------------------------------------------------------
OS=$(uname -s 2>/dev/null)
ARCH=$(uname -m 2>/dev/null)
case "$ARCH" in
  arm64|aarch64) ARCH=aarch64 ;;
  x86_64|amd64) ARCH=x86_64 ;;
esac
TRIPLE=
case "$OS" in
  Darwin)
    # A Rosetta (x86_64) shell on Apple silicon still gets the native build.
    [ "$ARCH" = x86_64 ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" = 1 ] && ARCH=aarch64
    case "$ARCH" in aarch64|x86_64) TRIPLE=$ARCH-apple-darwin ;; esac ;;
  Linux)
    case "$ARCH" in aarch64|x86_64) TRIPLE=$ARCH-unknown-linux-musl ;; esac ;;
esac

if [ -n "${PSCAT_CACHE_DIR:-}" ]; then
  CACHE=$PSCAT_CACHE_DIR
elif [ "$OS" = Darwin ]; then
  CACHE=${HOME:-/tmp}/Library/Caches/pscat/plugin
else
  CACHE=${XDG_CACHE_HOME:-${HOME:-/tmp}/.cache}/pscat/plugin
fi

# --- version check -------------------------------------------------------------
# version_ok A B: true if version A (X.Y.Z, any suffix ignored) >= B.
version_ok() {
  [ -z "$2" ] && return 0
  printf '%s\n%s\n' "$1" "$2" | sed 's/[-+].*//' | awk -F. '
    NR == 1 { for (i = 1; i <= 3; i++) a[i] = $i + 0 }
    NR == 2 { for (i = 1; i <= 3; i++) b[i] = $i + 0 }
    END {
      for (i = 1; i <= 3; i++) {
        if (a[i] > b[i]) exit 0
        if (a[i] < b[i]) exit 1
      }
      exit 0
    }'
}

FOUND_DIR=
FOUND_HOW=
OLD_SEEN=
# try_candidate PATH HOW: accept PATH (a pscat-mcp) if it runs and is new enough.
try_candidate() {
  [ -n "$FOUND_DIR" ] && return 0
  [ -f "$1" ] && [ -x "$1" ] || return 1
  # An old pscat-mcp without --version would start serving; stdin at EOF ends it.
  v=$("$1" --version </dev/null 2>/dev/null | sed -n 's/^pscat-mcp \([0-9][0-9.]*[^ ]*\).*/\1/p' | head -n 1)
  if [ -z "$v" ] || ! version_ok "$v" "$MIN_VERSION"; then
    OLD_SEEN="$1 (${v:-unknown version})"
    return 1
  fi
  FOUND_DIR=$(cd "$(dirname "$1")" && pwd -P)
  FOUND_HOW=$2
}

# The cached copy of the pinned release, published by a symlink (see install).
cached_dir() {
  [ -n "$TAG" ] && [ -n "$TRIPLE" ] && [ -x "$CACHE/$TAG/$TRIPLE/pscat-mcp" ] &&
    printf '%s\n' "$CACHE/$TAG/$TRIPLE"
}

# lookup: set FOUND_DIR to a directory holding a usable pscat-mcp (and pscat).
lookup() {
  FOUND_DIR=
  [ -n "${PSCAT_MCP_BIN:-}" ] && try_candidate "$PSCAT_MCP_BIN" "PSCAT_MCP_BIN"
  p=$(command -v pscat-mcp 2>/dev/null) && try_candidate "$p" "PATH"
  for d in "${HOME:-/nonexistent}/.cargo/bin" /opt/homebrew/bin /usr/local/bin; do
    try_candidate "$d/pscat-mcp" "$d"
  done
  if [ -z "$FOUND_DIR" ] && d=$(cached_dir); then
    FOUND_DIR=$d
    FOUND_HOW=cache
  fi
  [ -n "$FOUND_DIR" ]
}

# --- download and install ------------------------------------------------------
fetch() { # fetch URL FILE
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 2 --connect-timeout 20 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -T 20 -O "$2" "$1"
  else
    say "neither curl nor wget is installed, so pscat cannot be downloaded"
    return 1
  fi
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print $1}'
  elif command -v openssl >/dev/null 2>&1; then openssl dgst -sha256 -r "$1" | awk '{print $1}'
  else return 1
  fi
}

offline_help() {
  say "install pscat yourself instead (cargo install pscat, or an archive from $REPO_URL/releases put on PATH), then restart your agent"
}

WORK=
LOCK=
cleanup() {
  [ -n "$WORK" ] && rm -rf "$WORK"
  [ -n "$LOCK" ] && rm -rf "$LOCK"
  WORK=
  LOCK=
}

# Only one process downloads at a time. The lock is an optimisation: the
# publish step below is safe without it.
take_lock() {
  LOCK=$CACHE/.lock-$TAG-$TRIPLE
  waited=0
  nopid=0
  while ! mkdir "$LOCK" 2>/dev/null; do
    [ -n "$(cached_dir)" ] && { LOCK=; return 0; }
    pid=$(cat "$LOCK/pid" 2>/dev/null)
    if [ -n "$pid" ]; then
      nopid=0
      if ! kill -0 "$pid" 2>/dev/null && ! ps -p "$pid" >/dev/null 2>&1; then
        say "removing a stale download lock (process $pid is gone)"
        rm -rf "$LOCK"
        continue
      fi
    else
      nopid=$((nopid + 1))
      [ "$nopid" -ge 10 ] && { rm -rf "$LOCK"; nopid=0; continue; }
    fi
    [ "$waited" = 0 ] && say "another pscat download is running; waiting for it"
    waited=$((waited + 1))
    [ "$waited" -ge 600 ] && { say "gave up waiting for the download lock $LOCK"; LOCK=; return 1; }
    sleep 1
  done
  echo $$ > "$LOCK/pid"
}

install() {
  [ -n "$TRIPLE" ] || { say "there is no prebuilt pscat for $OS $ARCH"; offline_help; return 1; }
  [ -n "$TAG" ] || { say "no release pinned in $PLUGIN_DIR/pscat-release"; offline_help; return 1; }
  mkdir -p "$CACHE/$TAG" || { say "cannot create the cache directory $CACHE/$TAG"; return 1; }
  trap 'cleanup; exit 1' INT TERM HUP
  take_lock || return 1
  if [ -n "$(cached_dir)" ]; then cleanup; return 0; fi
  # Work dirs left by a killed download (an hour old; never a published *.d).
  find "$CACHE/$TAG" -maxdepth 1 -name '.dl.*' ! -name '*.d' -mmin +60 -exec rm -rf {} + 2>/dev/null

  WORK=$(mktemp -d "$CACHE/$TAG/.dl.XXXXXX") || { say "cannot create a temporary directory in $CACHE/$TAG"; cleanup; return 1; }
  sums_url=$BASE_URL/$TAG/SHA256SUMS
  if ! fetch "$sums_url" "$WORK/SHA256SUMS"; then
    say "could not download $sums_url"
    say "pscat is not installed yet and this computer seems to be offline (or GitHub is unreachable); connect and restart your agent"
    offline_help
    cleanup
    return 1
  fi
  line=$(awk -v suf="-$TRIPLE.tar.gz" '
    { n = $2; sub(/^\*/, "", n)
      if (substr(n, 1, 6) == "pscat-" && length(n) > length(suf) &&
          substr(n, length(n) - length(suf) + 1) == suf) { print $1, n; exit } }' "$WORK/SHA256SUMS")
  want=${line%% *}
  asset=${line#* }
  case "$asset" in
    *[!A-Za-z0-9._-]*|'') say "release $TAG has no archive for $TRIPLE"; offline_help; cleanup; return 1 ;;
  esac
  url=$BASE_URL/$TAG/$asset
  say "downloading $asset from $REPO_URL ($TAG); this happens once"
  if ! fetch "$url" "$WORK/$asset"; then
    say "could not download $url (offline, or GitHub is unreachable); connect and restart your agent"
    offline_help
    cleanup
    return 1
  fi
  got=$(sha256_of "$WORK/$asset") || { say "no sha256sum, shasum or openssl to check the download with"; cleanup; return 1; }
  if [ "$got" != "$want" ]; then
    say "checksum mismatch for $asset: SHA256SUMS says $want, the download is $got; not installing it"
    cleanup
    return 1
  fi
  { mkdir "$WORK/x" && tar -xzf "$WORK/$asset" -C "$WORK/x"; } || { say "could not unpack $asset"; cleanup; return 1; }
  rm -f "$WORK/$asset"
  bundle=$WORK/x/${asset%.tar.gz}
  [ -x "$bundle/pscat-mcp" ] || { say "$asset holds no ${asset%.tar.gz}/pscat-mcp"; cleanup; return 1; }

  # Publish: keep the unpacked tree under a unique name and point
  # $TAG/$TRIPLE at it with one symlink. `ln -s` fails if the name exists, so
  # two racing installers cannot both win, and a reader sees either nothing
  # or a whole bundle, never a half-unpacked one.
  keep=$(basename "$WORK")
  mv "$WORK/x" "$CACHE/$TAG/$keep.d" || { say "could not move the bundle into $CACHE/$TAG"; cleanup; return 1; }
  if ln -s "$keep.d/${asset%.tar.gz}" "$CACHE/$TAG/$TRIPLE" 2>/dev/null; then
    say "installed pscat $TAG in $CACHE/$TAG/$TRIPLE"
  else
    rm -rf "$CACHE/$TAG/$keep.d"
  fi
  # Stable names for people and skills: $CACHE/bin/pscat and pscat-mcp.
  mkdir -p "$CACHE/bin" 2>/dev/null
  for b in pscat pscat-mcp; do
    ln -s "$CACHE/$TAG/$TRIPLE/$b" "$CACHE/bin/.$b.$$" 2>/dev/null &&
      mv -f "$CACHE/bin/.$b.$$" "$CACHE/bin/$b" 2>/dev/null
    rm -f "$CACHE/bin/.$b.$$"
  done
  cleanup
  trap - INT TERM HUP
  [ -n "$(cached_dir)" ]
}

# ensure: lookup, else install, else (offline) any earlier cached version.
ensure() {
  lookup && return 0
  [ -n "$OLD_SEEN" ] && say "ignoring $OLD_SEEN: this plugin needs pscat $MIN_VERSION or newer"
  if install && lookup; then return 0; fi
  if [ -x "$CACHE/bin/pscat-mcp" ]; then
    FOUND_DIR=$(cd "$(dirname "$(readlink "$CACHE/bin/pscat-mcp" 2>/dev/null || echo "$CACHE/bin/pscat-mcp")")" && pwd -P)
    FOUND_HOW=cache
    say "using the previously downloaded pscat in $FOUND_DIR instead"
    return 0
  fi
  return 1
}

# --- SessionStart hook -----------------------------------------------------------
json_str() { # a JSON string literal for $1 (paths and plain prose only)
  printf '"%s"' "$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g' | tr '\n\t' '  ')"
}

hook() {
  if lookup; then
    if [ "$FOUND_HOW" = PATH ] && command -v pscat >/dev/null 2>&1; then
      exit 0 # the usual case: nothing to say
    fi
    ctx="pscat plugin: the pscat command-line tool is not on PATH in this session. Run it by its full path, $FOUND_DIR/pscat (pscat-mcp is beside it)."
    printf '{"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": %s}}\n' "$(json_str "$ctx")"
    exit 0
  fi
  launcher="sh '$HERE/pscat-mcp.sh' --locate"
  if [ -z "$TRIPLE" ]; then
    msg="pscat plugin: pscat / pscat-mcp not found, and there is no prebuilt pscat for $OS $ARCH, so the pscat MCP tools will not start. Install them with: cargo install pscat (or a release from $REPO_URL/releases), then restart your agent."
    ctx=$msg
  else
    msg="pscat plugin: pscat is not installed yet. The pscat MCP server downloads it (release ${TAG:-?}, about 6 MB, checked against the release's SHA256SUMS) from $REPO_URL the first time it starts, into $CACHE. If the pscat tools do not appear, check the internet connection and restart; or install pscat yourself: cargo install pscat."
    [ -n "$OLD_SEEN" ] && msg="$msg (Found $OLD_SEEN, older than the $MIN_VERSION this plugin needs.)"
    ctx="pscat plugin: pscat is not on PATH. To use the pscat command line, run: $launcher  (it downloads pscat if needed and prints the directory holding pscat and pscat-mcp), then run that directory's pscat by its full path."
  fi
  printf '{"systemMessage": %s, "hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": %s}}\n' \
    "$(json_str "$msg")" "$(json_str "$ctx")"
  exit 0
}

case "${1:-}" in
  --hook) hook ;;
  --locate)
    ensure || exit 1
    printf '%s\n' "$FOUND_DIR"
    exit 0 ;;
esac

if ! ensure; then
  say "pscat-mcp is not available, so the pscat tools cannot start"
  exit 1
fi
[ "$FOUND_HOW" = cache ] || [ "$FOUND_HOW" = PATH ] || say "using $FOUND_DIR/pscat-mcp"
unset PSCAT_PLUGIN_LAUNCHER
exec "$FOUND_DIR/pscat-mcp" "$@"
