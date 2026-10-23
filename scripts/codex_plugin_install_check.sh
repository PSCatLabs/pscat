#!/usr/bin/env bash
# Install the Codex plugin into a throwaway Codex home and check what a user
# would get (issue #569 part b). Needs the `codex` CLI; no login (marketplace
# add and plugin add from a local marketplace work without one). Never
# touches your real config: CODEX_HOME points at a temp dir deleted on exit.
#
# usage: scripts/codex_plugin_install_check.sh path/to/dir-with-pscat-and-pscat-mcp
#
# 1. `codex plugin marketplace add <this checkout>` reads
#    .agents/plugins/marketplace.json; `codex plugin add pscat@pscat`.
# 2. The installed copy sits under plugins/cache/pscat/pscat/<version> with
#    the version from plugin/plugin.json, holds the three skills as real
#    files (Codex skips symlinks when it copies a plugin), mcp.json,
#    hooks/hooks.json and the hook script hooks/check-pscat.sh, and nothing
#    else of the repo. The hook command is run with CLAUDE_PLUGIN_ROOT set
#    (Codex sets it for compatibility): the missing-pscat message on a PATH
#    without pscat, nothing with it.
# 3. The installed mcp.json's pscat server, started the way Codex starts it
#    (its command on PATH, stdio), answers initialize and tools/list. Codex
#    has no login-free command that starts a plugin's MCP server, so this
#    speaks the protocol directly. Without pscat-mcp on PATH the command is
#    not found (the case hooks/hooks.json explains).
set -euo pipefail

bin=${1:?usage: $0 path/to/bin-dir-with-pscat-mcp}
bin=$(cd "$bin" && pwd)
repo=$(cd "$(dirname "$0")/.." && pwd)
test -x "$bin/pscat-mcp" && test -x "$bin/pscat" || { echo "FAIL no pscat/pscat-mcp in $bin" >&2; exit 1; }
codex=$(command -v codex) || { echo "FAIL codex CLI not on PATH" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export CODEX_HOME="$work/home"
mkdir -p "$CODEX_HOME" "$work/project"
cd "$work/project"

fail=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*" >&2; fail=1; }

"$codex" --version
"$codex" plugin marketplace add "$repo" && ok "marketplace added" || bad "marketplace add"
"$codex" plugin add pscat@pscat && ok "plugin added" || bad "plugin add"

version=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' \
    "$repo/plugin/plugin.json")
copy="$CODEX_HOME/plugins/cache/pscat/pscat/$version"
[ -d "$copy" ] && ok "installed under version $version" \
    || bad "no $copy ($(ls "$CODEX_HOME/plugins/cache/pscat/pscat" 2>&1))"
for s in pscat psart psviz; do
    [ -f "$copy/skills/$s/SKILL.md" ] && ok "installed copy has skills/$s/SKILL.md" \
        || bad "installed copy lacks skills/$s/SKILL.md"
done
for f in plugin.json mcp.json hooks/hooks.json hooks/check-pscat.sh; do
    [ -f "$copy/$f" ] && ok "installed copy has $f" || bad "installed copy lacks $f"
done
echo "info hooks/check-pscat.sh mode in the Codex cache: $(ls -l "$copy/hooks/check-pscat.sh" 2>&1 | cut -c1-10)"
links=$(find "$copy" -type l 2>/dev/null)
[ -z "$links" ] && ok "installed copy has no symlinks" || bad "symlinks in installed copy: $links"
extra=$(cd "$copy" 2>/dev/null && find . -mindepth 1 -maxdepth 1 ! -name skills ! -name hooks \
    ! -name plugin.json ! -name mcp.json ! -name README.md ! -name .claude-plugin ! -name .codex-plugin)
[ -z "$extra" ] && ok "installed copy holds only the plugin" || bad "installed copy has extra entries: $extra"

# The SessionStart hook, run as the agent runs it: the command string from
# the installed hooks.json with CLAUDE_PLUGIN_ROOT set to the installed copy
# (Claude Code and Codex both set it; a real Codex hook run needs the /hooks
# trust review, #654, so this stands in for it). #684.
hook_cmd=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["hooks"]["SessionStart"][0]["hooks"][0]["command"])' \
    "$copy/hooks/hooks.json")
hook_out() { CLAUDE_PLUGIN_ROOT="$copy" PATH="$1" sh -c "$hook_cmd"; }
hook_clean=/usr/bin:/bin
if PATH="$hook_clean" command -v pscat-mcp >/dev/null; then
    bad "pscat-mcp is still on the stripped PATH; cannot run the hook's negative case"
else
    out=$(hook_out "$hook_clean") && printf '%s' "$out" | python3 -c 'import json,sys; assert "pscat-mcp" in json.load(sys.stdin)["systemMessage"]' \
        && ok "hook without pscat on PATH prints the systemMessage" || bad "hook negative case: $out"
fi
out=$(hook_out "$bin:$hook_clean") && [ -z "$out" ] \
    && ok "hook with pscat and pscat-mcp on PATH prints nothing" || bad "hook positive case: $out"

# Start the server as the installed mcp.json describes it.
handshake() {
    PATH="$1" python3 - "$copy/mcp.json" <<'EOF'
import json, shutil, subprocess, sys
cfg = json.load(open(sys.argv[1]))["mcpServers"]["pscat"]
assert cfg.get("type") == "stdio", cfg
exe = shutil.which(cfg["command"])
if exe is None:
    print(f"command {cfg['command']!r} not on PATH")
    sys.exit(3)
msgs = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "codex-plugin-check", "version": "0"}}},
    {"jsonrpc": "2.0", "method": "notifications/initialized"},
    {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
]
stdin = "".join(json.dumps(m) + "\n" for m in msgs)
p = subprocess.run([exe, *cfg.get("args", [])], input=stdin, capture_output=True,
                   text=True, timeout=60)
replies = {r.get("id"): r for r in map(json.loads, filter(None, p.stdout.splitlines()))}
tools = sorted(t["name"] for t in replies.get(2, {}).get("result", {}).get("tools", []))
print("tools:", " ".join(tools))
sys.exit(0 if "render_postscript" in tools else 1)
EOF
}
clean_path=/usr/bin:/bin
handshake "$bin:$clean_path" && ok "with pscat-mcp on PATH the server answers tools/list" \
    || bad "MCP handshake failed"
if PATH="$clean_path" command -v pscat-mcp >/dev/null; then
    bad "pscat-mcp is still on the stripped PATH; cannot run the negative case"
else
    rc=0; handshake "$clean_path" || rc=$?
    [ "$rc" = 3 ] && ok "without pscat-mcp the command is not found" || bad "negative case exit $rc"
fi

exit $fail
