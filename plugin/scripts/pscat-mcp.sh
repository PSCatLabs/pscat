#!/bin/sh
# pscat plugin launcher (issue #853): runs pscat-mcp for Claude Code, Cowork
# and Codex, installing pscat first if it is missing. No Rust or Node needed.
#
#   sh pscat-mcp.sh [pscat-mcp args]   find or install pscat, then exec pscat-mcp
#   sh pscat-mcp.sh --locate           find or install pscat, link it onto PATH,
#                                      print the directory holding pscat
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
# The one stable path is $CACHE/bin/pscat (and pscat-mcp beside it): symlinks
# that always name the release in use, so the versioned directories stay an
# implementation detail. --locate, the hook and the skills all say that path.
#
# PATH (issue #919): when the cached copy is in use, the launcher also links
# pscat and pscat-mcp into ~/.local/bin or ~/bin, whichever is already on PATH
# and writable, so a plain `pscat` works in the agent's shell. It never
# creates a directory, never touches a system directory, and never overwrites
# a pscat it did not make (ours: a symlink to $CACHE/bin/<name>, or the
# marked shim below). Before the first download the hook leaves a small shim
# as `pscat` that downloads on first use and then replaces itself with the
# symlink. PSCAT_NO_PATH_LINK=1 turns all of this off.
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
FOUND_BIN=
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
  FOUND_BIN=$1 # as given: PSCAT_MCP_BIN may name pscat-mcp-0.6.3, say
  FOUND_DIR=$(cd "$(dirname "$1")" && pwd -P)
  FOUND_HOW=$2
}

# The cached copy of the pinned release, published by a symlink (see install).
cached_dir() {
  [ -n "$TAG" ] && [ -n "$TRIPLE" ] && [ -x "$CACHE/$TAG/$TRIPLE/pscat-mcp" ] &&
    printf '%s\n' "$CACHE/$TAG/$TRIPLE"
}

# publish_bin DIR: make $CACHE/bin/pscat and pscat-mcp symlinks to DIR's. Atomic
# (tmp link + mv) and a no-op when they already point there.
publish_bin() {
  mkdir -p "$CACHE/bin" 2>/dev/null || return 1
  for b in pscat pscat-mcp; do
    [ "$(readlink "$CACHE/bin/$b" 2>/dev/null)" = "$1/$b" ] && continue
    ln -sn "$1/$b" "$CACHE/bin/.$b.$$" 2>/dev/null &&
      mv -f "$CACHE/bin/.$b.$$" "$CACHE/bin/$b" 2>/dev/null
    rm -f "$CACHE/bin/.$b.$$"
  done
  # Both must name DIR now: an unwritable bin/ left pointing at an earlier
  # release must not pass for this one.
  [ "$(readlink "$CACHE/bin/pscat" 2>/dev/null)" = "$1/pscat" ] &&
    [ "$(readlink "$CACHE/bin/pscat-mcp" 2>/dev/null)" = "$1/pscat-mcp" ] &&
    [ -x "$CACHE/bin/pscat" ] && [ -x "$CACHE/bin/pscat-mcp" ]
}

# lookup: set FOUND_DIR to a directory holding a usable pscat-mcp (and pscat).
lookup() {
  FOUND_DIR=
  [ -n "${PSCAT_MCP_BIN:-}" ] && try_candidate "$PSCAT_MCP_BIN" "PSCAT_MCP_BIN"
  # Not our own link (see link_on_path): it names whatever release is in the
  # cache, and only the cache branch below follows the pin (an rc would
  # otherwise satisfy the final release's check for ever).
  p=$(command -v pscat-mcp 2>/dev/null) &&
    [ "$(readlink "$p" 2>/dev/null)" != "$CACHE/bin/pscat-mcp" ] && try_candidate "$p" "PATH"
  for d in "${HOME:-/nonexistent}/.cargo/bin" /opt/homebrew/bin /usr/local/bin; do
    try_candidate "$d/pscat-mcp" "$d"
  done
  if [ -z "$FOUND_DIR" ] && d=$(cached_dir); then
    FOUND_BIN=$d/pscat-mcp # run the real file; people and skills use bin/
    FOUND_HOW=cache
    # bin/ must name the release just validated, not an earlier one.
    if publish_bin "$d"; then FOUND_DIR=$CACHE/bin; else FOUND_DIR=$d; fi
  fi
  [ -n "$FOUND_DIR" ]
}

# --- download and install ------------------------------------------------------
CURL_OR_WGET=
fetch() { # fetch URL FILE; returns curl's or wget's exit status
  if command -v curl >/dev/null 2>&1; then
    CURL_OR_WGET=curl
    curl -fsSL --retry 2 --connect-timeout 20 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    CURL_OR_WGET=wget
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
  fetch "$sums_url" "$WORK/SHA256SUMS"
  rc=$?
  if [ "$rc" != 0 ]; then
    say "could not download $sums_url"
    case "$CURL_OR_WGET:$rc" in
      curl:22|curl:37|wget:8)
        # The server answered (HTTP error) or the file is missing: not offline.
        say "release $TAG has no SHA256SUMS there: is the release published (not a draft), and is the tag in $PLUGIN_DIR/pscat-release right?" ;;
      *)
        say "pscat is not installed yet and this computer seems to be offline (or GitHub is unreachable); connect and restart your agent" ;;
    esac
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
  # $TAG/$TRIPLE at it with one symlink. `ln -sn` fails if the name exists, so
  # two racing installers cannot both win, and a reader sees either nothing
  # or a whole bundle, never a half-unpacked one.
  keep=$(basename "$WORK")
  mv "$WORK/x" "$CACHE/$TAG/$keep.d" || { say "could not move the bundle into $CACHE/$TAG"; cleanup; return 1; }
  # -n: an existing link to a directory must make ln fail, not be followed.
  if ln -sn "$keep.d/${asset%.tar.gz}" "$CACHE/$TAG/$TRIPLE" 2>/dev/null; then
    say "installed pscat $TAG in $CACHE/$TAG/$TRIPLE"
  else
    rm -rf "$CACHE/$TAG/$keep.d"
  fi
  cleanup
  trap - INT TERM HUP
  [ -n "$(cached_dir)" ] # lookup publishes bin/
}

# ensure: lookup, else install, else (offline) any earlier cached version.
ensure() {
  lookup && return 0
  [ -n "$OLD_SEEN" ] && say "ignoring $OLD_SEEN: this plugin needs pscat $MIN_VERSION or newer"
  if install && lookup; then return 0; fi
  if [ -x "$CACHE/bin/pscat-mcp" ]; then
    FOUND_DIR=$CACHE/bin
    FOUND_BIN=$CACHE/bin/pscat-mcp
    FOUND_HOW=cache
    say "using the previously downloaded pscat in $CACHE/bin instead"
    return 0
  fi
  return 1
}

# --- pscat on PATH (issue #919) ---------------------------------------------------
SHIM_MARK='# pscat-plugin-shim'
LINK_DIR=   # the directory pscat was linked into
LINK_STATE= # on-path | foreign (another pscat is on PATH) | nodir | off

# The first of ~/.local/bin and ~/bin that exists, is writable and is on PATH.
# Never a system directory (a package manager owns those), never created.
path_link_dir() {
  [ -n "${HOME:-}" ] || return 1
  for cand in "$HOME/.local/bin" "$HOME/bin"; do
    [ -d "$cand" ] && [ -w "$cand" ] || continue
    ( set -f; IFS=:
      for e in $PATH; do [ "${e%/}" = "$cand" ] && exit 0; done
      exit 1 ) && { printf '%s\n' "$cand"; return 0; }
  done
  return 1
}

# is_ours FILE NAME: FILE is a symlink to $CACHE/bin/NAME, or (pscat only) our shim.
is_ours() {
  if [ -L "$1" ]; then
    [ "$(readlink "$1" 2>/dev/null)" = "$CACHE/bin/$2" ]
  else
    [ "$2" = pscat ] && [ -f "$1" ] && [ "$(head -c 256 "$1" 2>/dev/null | sed -n 2p)" = "$SHIM_MARK" ]
  fi
}

sq() { printf '%s' "$1" | sed "s/'/'\\\\''/g"; } # for single-quoting in the shim

# The shim stands in for pscat until the release is downloaded: the first run
# downloads it through this launcher, then link_on_path swaps the shim for a
# symlink. A shell script, so a pscat started before that keeps working.
write_shim() { # write_shim FILE
  {
    printf '#!/bin/sh\n%s\n' "$SHIM_MARK"
    printf '# Written by the pscat plugin. It downloads pscat on first use and then\n'
    printf '# replaces itself with a link. Delete it if you remove the plugin.\n'
    printf "launcher='%s'\n" "$(sq "$HERE/pscat-mcp.sh")"
    printf "bin='%s'\n" "$(sq "$CACHE/bin/pscat")"
    cat <<'SHIM'
if [ -x "$bin" ]; then exec "$bin" "$@"; fi
# --locate downloads if needed, or finds a pscat installed since (cargo, say),
# and prints the directory holding it.
dir=
[ -f "$launcher" ] && dir=$(sh "$launcher" --locate)
if [ -n "$dir" ] && [ -x "$dir/pscat" ]; then exec "$dir/pscat" "$@"; fi
echo "pscat: not installed, and the pscat plugin could not download it (is the plugin still installed, and is this computer online?). Install pscat with: cargo install pscat" >&2
exit 127
SHIM
  } > "$1"
  chmod 755 "$1"
}

# put_link NAME TARGET: make TARGET ours: a symlink to $CACHE/bin/NAME when the
# cached copy exists, else (pscat only) the shim. TARGET is absent or ours.
# A new name is made with one `ln` that fails if the name appeared meanwhile;
# an existing one of ours is replaced atomically (tmp + mv).
put_link() {
  n=$1
  t=$2
  tmp=$(dirname "$t")/.$n.$$
  rm -f "$tmp"
  if [ -x "$CACHE/bin/$n" ]; then
    [ "$(readlink "$t" 2>/dev/null)" = "$CACHE/bin/$n" ] && return 0
    if [ -e "$t" ] || [ -L "$t" ]; then
      ln -s "$CACHE/bin/$n" "$tmp" 2>/dev/null && mv -f "$tmp" "$t" 2>/dev/null
    else
      ln -sn "$CACHE/bin/$n" "$t" 2>/dev/null
    fi
  else
    [ "$n" = pscat ] || return 0
    write_shim "$tmp" 2>/dev/null || { rm -f "$tmp"; return 1; }
    if [ -e "$t" ] || [ -L "$t" ]; then
      if [ -f "$t" ] && [ ! -L "$t" ] && cmp -s "$tmp" "$t"; then rm -f "$tmp"; return 0; fi
      mv -f "$tmp" "$t" 2>/dev/null
    else
      ln "$tmp" "$t" 2>/dev/null
    fi
  fi
  rc=$?
  rm -f "$tmp"
  return $rc
}

# link_on_path: make plain `pscat` and `pscat-mcp` work in a shell. Sets
# LINK_STATE and LINK_DIR. Silent on stdout.
link_on_path() {
  LINK_STATE=
  LINK_DIR=
  [ -n "${PSCAT_NO_PATH_LINK:-}" ] && { LINK_STATE=off; return 0; }
  have=
  [ -x "$CACHE/bin/pscat" ] && [ -x "$CACHE/bin/pscat-mcp" ] && have=1
  # Before the download only the shim can be made, and only where one release exists.
  [ -n "$have" ] || { [ -n "$TRIPLE" ] && [ -n "$TAG" ]; } || return 0
  dir=$(path_link_dir) || { LINK_STATE=nodir; return 0; }
  LINK_DIR=$dir
  for b in pscat pscat-mcp; do
    [ -z "$have" ] && [ "$b" = pscat-mcp ] && continue
    target=$dir/$b
    found=$(command -v "$b" 2>/dev/null)
    if [ -n "$found" ]; then
      case "$found" in /*) ;; *) found= ;; esac # a function or alias
    fi
    if [ -n "$found" ]; then
      is_ours "$found" "$b" || continue # someone else's: leave it alone
      target=$found
    elif [ -e "$target" ] || [ -L "$target" ]; then
      is_ours "$target" "$b" || { LINK_STATE=foreign; continue; }
    fi
    if put_link "$b" "$target"; then
      [ "$target" = "$found" ] || say "linked $b into $dir"
    fi
  done
  p=$(command -v pscat 2>/dev/null)
  if [ -z "$p" ]; then LINK_STATE=nodir
  elif is_ours "$p" pscat; then LINK_STATE=on-path
  else LINK_STATE=foreign
  fi
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
    if [ "$FOUND_HOW" = cache ]; then
      link_on_path
      [ "$LINK_STATE" = on-path ] && exit 0 # linked: a plain pscat works, nothing to say
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
    msg="pscat plugin: pscat is not installed yet. The pscat MCP server downloads it (release ${TAG:-?}, about 14 MB, checked against the release's SHA256SUMS) from $REPO_URL the first time it starts, into $CACHE. If the pscat tools do not appear, check the internet connection and restart; or install pscat yourself: cargo install pscat."
    [ -n "$OLD_SEEN" ] && msg="$msg (Found $OLD_SEEN, older than the $MIN_VERSION this plugin needs.)"
    link_on_path
    if [ -x "$CACHE/bin/pscat" ]; then
      ctx="pscat plugin: release ${TAG:-?} is not downloaded yet (about 14 MB, once); the earlier pscat in $CACHE/bin still works. To update it run: $launcher"
    elif [ "$LINK_STATE" = on-path ]; then
      ctx="pscat plugin: pscat is not downloaded yet (release ${TAG:-?}, about 14 MB, once). Running pscat in a shell downloads it first (pscat is linked into $LINK_DIR). Or run: $launcher  (downloads it and prints the directory holding pscat). After that the stable path is $CACHE/bin/pscat."
    else
      ctx="pscat plugin: pscat is not downloaded yet and is not on PATH. To use the pscat command line, run: $launcher  (downloads release ${TAG:-?}, about 14 MB, once, and prints the directory holding pscat). After that the stable path is $CACHE/bin/pscat."
    fi
  fi
  printf '{"systemMessage": %s, "hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": %s}}\n' \
    "$(json_str "$msg")" "$(json_str "$ctx")"
  exit 0
}

case "${1:-}" in
  --hook) hook ;;
  --locate)
    ensure || exit 1
    if [ "$FOUND_DIR" = "$CACHE/bin" ]; then
      link_on_path
      case "$LINK_STATE" in
        on-path) say "pscat is on PATH (linked into $LINK_DIR); the stable path is $FOUND_DIR/pscat" ;;
        foreign) say "another pscat is on PATH and was left alone; the plugin's copy is $FOUND_DIR/pscat" ;;
        nodir)   say "no writable ~/.local/bin or ~/bin on PATH to link pscat into; run it as $FOUND_DIR/pscat" ;;
        *)       say "run pscat as $FOUND_DIR/pscat" ;;
      esac
    fi
    printf '%s\n' "$FOUND_DIR"
    exit 0 ;;
esac

if ! ensure; then
  say "pscat-mcp is not available, so the pscat tools cannot start"
  exit 1
fi
[ "$FOUND_HOW" = cache ] || [ "$FOUND_HOW" = PATH ] || say "using $FOUND_BIN"
[ "$FOUND_HOW" = cache ] && link_on_path
unset PSCAT_PLUGIN_LAUNCHER
exec "$FOUND_BIN" "$@"
