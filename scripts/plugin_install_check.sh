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
#    hooks/hooks.json, and nothing else of the repo (plugin/ also carries the
#    Codex manifest and mcp.json; scripts/codex_plugin_install_check.sh).
# 4. `claude mcp list` with only the given bin dir added to PATH: the
#    plugin's server must connect. Then without pscat-mcp on PATH it must fail
#    (the negative case the plugin's SessionStart hook explains).
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
extra=$(cd "$copy" && find . -mindepth 1 -maxdepth 1 ! -name skills ! -name .claude-plugin ! -name README.md \
    ! -name hooks ! -name plugin.json ! -name mcp.json)
[ -z "$extra" ] && ok "installed copy holds only the plugin" || bad "installed copy has extra entries: $extra"
"$claude" plugin details pscat@pscat

# Any other pscat on PATH (~/.cargo/bin, say) is dropped so the result is
# about the given binaries.
clean_path=$(dirname "$claude"):/usr/bin:/bin
node=$(command -v node || true)
[ -n "$node" ] && clean_path=$clean_path:$(dirname "$node")
# Positive case first: Claude Code caches a failed connection for a while,
# keyed to the server config, so a failure first could mask a success.
out=$(PATH="$bin:$clean_path" "$claude" mcp list 2>&1 || true)
echo "$out" | grep -q 'plugin:pscat:pscat.*Connected' \
    && ok "with pscat-mcp on PATH the server connects" || bad "mcp list: $out"

if PATH="$clean_path" command -v pscat-mcp >/dev/null; then
    bad "pscat-mcp is still on the stripped PATH; cannot run the negative case"
else
    out=$(PATH="$clean_path" "$claude" mcp list 2>&1 || true)
    echo "$out" | grep -q 'plugin:pscat:pscat.*Failed' \
        && ok "without pscat-mcp the server fails to start" || bad "negative case: $out"
fi

exit $fail
