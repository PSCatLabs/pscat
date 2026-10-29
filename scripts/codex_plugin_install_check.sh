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
#    hooks/hooks.json, the launcher scripts/pscat-mcp.sh and its pin
#    pscat-release, and nothing else of the repo. The hook command is run
#    with CLAUDE_PLUGIN_ROOT set (Codex sets it for compatibility): it says
#    pscat will be downloaded on a PATH without pscat, nothing with it.
# 3. The pscat server as Codex resolved it (`codex mcp get pscat --json`:
#    command, args with ${PLUGIN_ROOT} expanded, env, cwd; #853) answers
#    initialize and tools/list. Codex has no login-free command that starts
#    a plugin's MCP server, so this starts that command itself and speaks the
#    protocol directly, three ways and with no network: pscat-mcp on PATH;
#    not on PATH, so the launcher downloads a fake release made from the given
#    binaries (scripts/fake_plugin_release.py, file://); and not on PATH with
#    the release unreachable, which must fail. HOME and the launcher's cache
#    are temp dirs.
# 4. Saving renders through that server (#910): with the installed copy
#    read-only and as the working directory (where Codex starts it), PNG, SVG
#    and PDF saves land in the out-root from ~/.config/pscat/plugin.conf, or
#    with no setting in pscat's per-user renders directory; never in the copy.
set -euo pipefail

bin=${1:?usage: $0 path/to/bin-dir-with-pscat-mcp}
bin=$(cd "$bin" && pwd)
repo=$(cd "$(dirname "$0")/.." && pwd)
test -x "$bin/pscat-mcp" && test -x "$bin/pscat" || { echo "FAIL no pscat/pscat-mcp in $bin" >&2; exit 1; }
codex=$(command -v codex) || { echo "FAIL codex CLI not on PATH" >&2; exit 1; }

work=$(mktemp -d)
trap 'chmod -R u+w "$work" 2>/dev/null; rm -rf "$work"' EXIT
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
for f in plugin.json mcp.json hooks/hooks.json scripts/pscat-mcp.sh pscat-release; do
    [ -f "$copy/$f" ] && ok "installed copy has $f" || bad "installed copy lacks $f"
done
echo "info scripts/pscat-mcp.sh mode in the Codex cache: $(ls -l "$copy/scripts/pscat-mcp.sh" 2>&1 | cut -c1-10)"
links=$(find "$copy" -type l 2>/dev/null)
[ -z "$links" ] && ok "installed copy has no symlinks" || bad "symlinks in installed copy: $links"
extra=$(cd "$copy" 2>/dev/null && find . -mindepth 1 -maxdepth 1 ! -name skills ! -name hooks \
    ! -name plugin.json ! -name mcp.json ! -name README.md ! -name .claude-plugin ! -name .codex-plugin \
    ! -name scripts ! -name pscat-release)
[ -z "$extra" ] && ok "installed copy holds only the plugin" || bad "installed copy has extra entries: $extra"

# The SessionStart hook, run as the agent runs it: the command string from
# the installed hooks.json with CLAUDE_PLUGIN_ROOT set to the installed copy
# (Claude Code and Codex both set it; a real Codex hook run needs the /hooks
# trust review, #654, so this stands in for it). #684.
hook_cmd=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["hooks"]["SessionStart"][0]["hooks"][0]["command"])' \
    "$copy/hooks/hooks.json")
mkdir -p "$work/home"
offline="http://127.0.0.1:9"  # connection refused, like being offline
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

# The server as Codex resolved it from the installed mcp.json.
"$codex" mcp get pscat --json > "$work/server.json" && ok "codex resolves the plugin's pscat server" \
    || bad "codex mcp get pscat"
if python3 - "$work/server.json" "$copy" <<'EOF'
import json, os, sys
t = json.load(open(sys.argv[1]))["transport"]
assert t["command"] == "sh", t
real = os.path.realpath  # Codex canonicalizes the root: /var -> /private/var on macOS
assert [real(a) for a in t["args"]] == [real(sys.argv[2] + "/scripts/pscat-mcp.sh")], t
EOF
then ok "Codex expands \${PLUGIN_ROOT} to the installed launcher"
else bad "resolved server config: $(cat "$work/server.json")"
fi

# handshake PATH CACHE BASE_URL: start the resolved server, list its tools.
handshake() {
    PATH="$1" HOME="$work/home" PSCAT_CACHE_DIR="$2" PSCAT_RELEASE_BASE_URL="$3" \
        python3 - "$work/server.json" <<'EOF'
import json, os, shutil, subprocess, sys
t = json.load(open(sys.argv[1]))["transport"]
exe = shutil.which(t["command"])
msgs = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "codex-plugin-check", "version": "0"}}},
    {"jsonrpc": "2.0", "method": "notifications/initialized"},
    {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
]
stdin = "".join(json.dumps(m) + "\n" for m in msgs)
env = {**os.environ, **(t.get("env") or {})}
p = subprocess.run([exe, *t.get("args", [])], input=stdin, capture_output=True,
                   text=True, timeout=120, env=env, cwd=t.get("cwd"))
replies = {r.get("id"): r for r in map(json.loads, filter(None, p.stdout.splitlines()))}
tools = sorted(t["name"] for t in replies.get(2, {}).get("result", {}).get("tools", []))
print("tools:", " ".join(tools) or "(none)", "| exit", p.returncode, "|", p.stderr.strip()[-300:])
sys.exit(0 if "render_postscript" in tools else 1)
EOF
}
clean_path=/usr/bin:/bin
handshake "$bin:$clean_path" "$work/cache-unused" "$offline" \
    && ok "with pscat-mcp on PATH the server answers tools/list" || bad "MCP handshake failed"
[ ! -e "$work/cache-unused" ] && ok "with pscat-mcp on PATH nothing is downloaded" \
    || bad "the launcher downloaded although pscat-mcp is on PATH"
if PATH="$clean_path" command -v pscat-mcp >/dev/null; then
    bad "pscat-mcp is still on the stripped PATH; cannot run the negative cases"
else
    tag=$(grep -v '^#' "$copy/pscat-release" | tr -d '[:space:]')
    python3 "$repo/scripts/fake_plugin_release.py" "$bin" "$work/release" "$tag"
    handshake "$clean_path" "$work/cache" "file://$work/release" \
        && ok "without pscat-mcp the launcher downloads the release ($tag, local) and serves" \
        || bad "download case"
    if handshake "$clean_path" "$work/cache-offline" "$offline"; then
        bad "offline case served tools"
    else
        ok "without pscat-mcp and offline the server fails to start"
    fi
fi

# Saving renders (#910). save_check PATH HOME ROOT: through the resolved
# server, started in the installed copy as Codex starts it, a PNG, an SVG and
# a PDF saved by out_path land in ROOT, the instructions name ROOT, and no
# file appears in the copy.
save_check() {
    PATH="$1" HOME="$2" PSCAT_CACHE_DIR="$work/cache-unused" PSCAT_RELEASE_BASE_URL="$offline" \
        python3 - "$work/server.json" "$3" "$copy" <<'EOF'
import json, os, shutil, subprocess, sys
t = json.load(open(sys.argv[1]))["transport"]
root, copy = os.path.realpath(sys.argv[2]), sys.argv[3]
if os.path.realpath(t.get("cwd") or "/") != os.path.realpath(copy):
    print(f"note: Codex resolved cwd {t.get('cwd')!r}, not the installed copy")
files = lambda: sorted(os.path.join(d, f) for d, _, fs in os.walk(copy) for f in fs)  # noqa: E731
before = files()
src = "newpath 10 10 moveto 50 50 lineto stroke showpage"
msgs = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "codex-plugin-check", "version": "0"}}},
    {"jsonrpc": "2.0", "method": "notifications/initialized"},
] + [{"jsonrpc": "2.0", "id": 2 + i, "method": "tools/call", "params": {
        "name": "render_postscript", "arguments": {
            "source": src, "width": 60, "height": 60, "format": fmt, "out_path": f"saved.{fmt}"}}}
     for i, fmt in enumerate(("png", "svg", "pdf"))]
stdin = "".join(json.dumps(m) + "\n" for m in msgs)
env = {**os.environ, **(t.get("env") or {})}
p = subprocess.run([shutil.which(t["command"]), *t.get("args", [])], input=stdin, capture_output=True,
                   text=True, timeout=120, env=env, cwd=copy)
replies = {r.get("id"): r for r in map(json.loads, filter(None, p.stdout.splitlines()))}
instr = replies.get(1, {}).get("result", {}).get("instructions", "")
problems = []
if f"out_path writes only inside {root}" not in instr:
    problems.append(f"instructions do not name {root}: {instr[-400:]!r}")
for i, (fmt, magic) in enumerate((("png", b"\x89PNG"), ("svg", b"<"), ("pdf", b"%PDF"))):
    r = replies.get(2 + i, {}).get("result", {})
    f = os.path.join(root, f"saved.{fmt}")
    if r.get("isError") or not os.path.isfile(f) or not open(f, "rb").read(512).lstrip().startswith(magic):
        problems.append(f"{fmt}: {json.dumps(r)[:300]}")
if files() != before:
    problems.append(f"files appeared in the installed copy: {sorted(set(files()) - set(before))}")
print("saved png, svg, pdf in", root, "|", problems or "ok", "|", p.stderr.strip()[-300:])
sys.exit(1 if problems else 0)
EOF
}
chmod -R a-w "$copy"
mkdir -p "$work/save-home/.config/pscat" "$work/workspace"
printf 'out-root = %s\n' "$work/workspace" > "$work/save-home/.config/pscat/plugin.conf"
save_check "$bin:$clean_path" "$work/save-home" "$work/workspace" \
    && ok "read-only install: PNG, SVG and PDF saves land in the configured out-root" \
    || bad "saves with a configured out-root"
rm "$work/save-home/.config/pscat/plugin.conf"
case "$(uname -s)" in
    Darwin) renders="$work/save-home/Library/Application Support/pscat/renders" ;;
    *) renders="$work/save-home/.local/share/pscat/renders" ;;
esac
save_check "$bin:$clean_path" "$work/save-home" "$renders" \
    && ok "read-only install, no setting: the saves land in pscat's renders directory, not the plugin" \
    || bad "saves with no out-root setting"
chmod -R u+w "$copy"

exit $fail
