#!/usr/bin/env bash
# Install the Claude Code plugin into a throwaway Claude config and check
# what a user would get (issue #569). Needs the `claude` CLI; no login (the
# plugin and `mcp list` commands work without one). Never touches your real
# config: CLAUDE_CONFIG_DIR points at a temp dir that is deleted on exit.
#
# usage: scripts/plugin_install_check.sh path/to/dir-with-pscat-and-pscat-mcp
#
# 1. `claude plugin validate --strict` on the marketplace and on plugin/skills/.
# 2. `plugin marketplace add <this checkout>` + `plugin install pscat@pscat`.
# 3. The installed copy holds the three skills as real files, the shared
#    hooks/hooks.json, the launcher scripts/pscat-mcp.sh and its release pin
#    pscat-release, and nothing else of the repo (plugin/ also carries the
#    Codex manifest and mcp.json; scripts/codex_plugin_install_check.sh). The
#    hook command from the installed hooks.json is run with
#    CLAUDE_PLUGIN_ROOT set: it says pscat will be downloaded on a PATH
#    without pscat, and nothing with it.
# 4. `claude mcp list`, which starts the plugin's server through the
#    launcher (#853), three ways, never touching the network: with the given
#    bin dir on PATH it connects; without it, the launcher downloads a fake
#    release made from the given binaries (scripts/fake_plugin_release.py,
#    served from file://) and connects; without it and with the release
#    unreachable, it fails. HOME and the launcher's cache are temp dirs, so
#    a pscat in your ~/.cargo/bin is not seen.
set -euo pipefail

bin=${1:?usage: $0 path/to/bin-dir-with-pscat-mcp}
bin=$(cd "$bin" && pwd)
repo=$(cd "$(dirname "$0")/.." && pwd)
test -x "$bin/pscat-mcp" && test -x "$bin/pscat" || { echo "FAIL no pscat/pscat-mcp in $bin" >&2; exit 1; }
claude=$(command -v claude) || { echo "FAIL claude CLI not on PATH" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export CLAUDE_CONFIG_DIR="$work/config"
mkdir -p "$CLAUDE_CONFIG_DIR" "$work/project"
cd "$work/project"

fail=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*" >&2; fail=1; }

"$claude" plugin validate --strict "$repo" && ok "marketplace validates" || bad "marketplace validate"
"$claude" plugin validate --strict "$repo/plugin/skills" && ok "skills validate" || bad "skills validate"

"$claude" plugin marketplace add "$repo"
"$claude" plugin install pscat@pscat

version=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' \
    "$repo/plugin/.claude-plugin/plugin.json")
copy="$CLAUDE_CONFIG_DIR/plugins/cache/pscat/pscat/$version"
for s in pscat psart psviz; do
    if [ -f "$copy/skills/$s/SKILL.md" ] && [ ! -L "$copy/skills" ]; then
        ok "installed copy has skills/$s/SKILL.md"
    else
        bad "installed copy lacks skills/$s/SKILL.md ($copy)"
    fi
done
[ -f "$copy/hooks/hooks.json" ] && ok "installed copy has hooks/hooks.json" || bad "installed copy lacks hooks/hooks.json"
[ -f "$copy/scripts/pscat-mcp.sh" ] && ok "installed copy has scripts/pscat-mcp.sh (mode $(ls -l "$copy/scripts/pscat-mcp.sh" | cut -c1-10))" || bad "installed copy lacks scripts/pscat-mcp.sh"
[ -f "$copy/pscat-release" ] && ok "installed copy has pscat-release ($(grep -v '^#' "$copy/pscat-release"))" || bad "installed copy lacks pscat-release"
extra=$(cd "$copy" && find . -mindepth 1 -maxdepth 1 ! -name skills ! -name .claude-plugin ! -name README.md \
    ! -name hooks ! -name scripts ! -name pscat-release ! -name plugin.json ! -name mcp.json)
[ -z "$extra" ] && ok "installed copy holds only the plugin" || bad "installed copy has extra entries: $extra"
"$claude" plugin details pscat@pscat

# The SessionStart hook, run as the agent runs it: the command string from
# the installed hooks.json with CLAUDE_PLUGIN_ROOT set to the installed copy
# (Claude Code and Codex both set it; a real Codex hook run needs the /hooks
# trust review, #654, so this stands in for it). #684.
hook_cmd=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["hooks"]["SessionStart"][0]["hooks"][0]["command"])' \
    "$copy/hooks/hooks.json")
mkdir -p "$work/home"
offline="file://$work/no-release"
hook_out() { CLAUDE_PLUGIN_ROOT="$copy" PATH="$1" HOME="$work/home" PSCAT_CACHE_DIR="$work/hook-cache" \
    PSCAT_RELEASE_BASE_URL="$offline" sh -c "$hook_cmd"; }
hook_clean=/usr/bin:/bin
if PATH="$hook_clean" command -v pscat-mcp >/dev/null; then
    bad "pscat-mcp is still on the stripped PATH; cannot run the hook's negative case"
else
    out=$(hook_out "$hook_clean") && printf '%s' "$out" | python3 -c 'import json,sys; assert "downloads it" in json.load(sys.stdin)["systemMessage"]' \
        && ok "hook without pscat on PATH says the server will download it" || bad "hook negative case: $out"
fi
out=$(hook_out "$bin:$hook_clean") && [ -z "$out" ] \
    && ok "hook with pscat and pscat-mcp on PATH prints nothing" || bad "hook positive case: $out"

# Any other pscat on PATH (~/.cargo/bin, say) is dropped so the result is
# about the given binaries.
clean_path=$(dirname "$claude"):/usr/bin:/bin
node=$(command -v node || true)
[ -n "$node" ] && clean_path=$clean_path:$(dirname "$node")
# Positive case first: Claude Code caches a failed connection for a while,
# keyed to the server config, so a failure first could mask a success.
mcp_list() { # mcp_list PATH CACHE BASE_URL
    PATH="$1" HOME="$work/home" PSCAT_CACHE_DIR="$2" PSCAT_RELEASE_BASE_URL="$3" \
        "$claude" mcp list 2>&1 || true
}
out=$(mcp_list "$bin:$clean_path" "$work/cache-unused" "$offline")
echo "$out" | grep -q 'plugin:pscat:pscat.*Connected' \
    && ok "with pscat-mcp on PATH the server connects" || bad "mcp list: $out"
[ ! -e "$work/cache-unused" ] && ok "with pscat-mcp on PATH nothing is downloaded" \
    || bad "the launcher downloaded although pscat-mcp is on PATH"

if PATH="$clean_path" command -v pscat-mcp >/dev/null; then
    bad "pscat-mcp is still on the stripped PATH; cannot run the negative cases"
else
    tag=$(grep -v '^#' "$copy/pscat-release" | tr -d '[:space:]')
    python3 "$repo/scripts/fake_plugin_release.py" "$bin" "$work/release" "$tag"
    out=$(mcp_list "$clean_path" "$work/cache" "file://$work/release")
    echo "$out" | grep -q 'plugin:pscat:pscat.*Connected' \
        && ok "without pscat-mcp the launcher downloads the release ($tag, local) and connects" \
        || bad "download case: $out"
    ls "$work/cache/$tag"/*/pscat-mcp >/dev/null 2>&1 \
        && ok "the download is cached under $tag" || bad "no cached download under $work/cache/$tag"
    out=$(mcp_list "$clean_path" "$work/cache-offline" "$offline")
    echo "$out" | grep -q 'plugin:pscat:pscat.*Failed' \
        && ok "without pscat-mcp and offline the server fails to start" || bad "offline case: $out"
fi

exit $fail
