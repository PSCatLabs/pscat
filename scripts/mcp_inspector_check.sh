#!/usr/bin/env bash
# Drive pscat-mcp with the official MCP Inspector CLI over stdio (issue
# #568): a client we did not write negotiates the protocol, lists the
# tools, and calls them. tests/mcp.rs covers the protocol edge cases
# (version refusal, malformed input, confinement, limits); this checks
# that a real client stack accepts what the server sends.
#
# usage: scripts/mcp_inspector_check.sh path/to/pscat-mcp
# Needs node/npx and jq. INSPECTOR overrides the pinned package version.
set -euo pipefail

server=${1:?usage: $0 path/to/pscat-mcp}
server=$(cd "$(dirname "$server")" && pwd)/$(basename "$server")
inspector=${INSPECTOR:-@modelcontextprotocol/inspector@2.9.0}

# Run in an empty directory: the server's default output root.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"

fail=0
# `--format json`: one JSON object per line on stdout. A tool result with
# isError:true is printed and then followed by a `tool_is_error` line and
# exit 5 (its message goes to stderr), so a case that expects an error tolerates that exit and reads
# the first line only.
run() {
    local out status=0
    out=$(npx -y "$inspector" --cli "$server" --format json "$@") || status=$?
    if [ "$status" -ne 0 ] && [ "$status" -ne 5 ]; then
        echo "FAIL inspector exited $status for: $*" >&2
        echo "$out" >&2
        touch "$work/inspector-failed" # run() is called in $( ), a subshell
    fi
    head -n 1 <<<"$out"
}
check() {
    local what=$1 filter=$2 json=$3
    if jq -e "$filter" >/dev/null 2>&1 <<<"$json"; then
        echo "ok   $what"
    else
        echo "FAIL $what"
        echo "$json" | head -c 2000
        echo
        fail=1
    fi
}
names='["render_postscript","handwrite","chalkboard_math","review_sheet","describe_art_capabilities","eval_postscript","start_render","render_status","render_result","cancel_render"]'

json=$(run --method tools/list)
check "tools/list names the ten tools" "[.result.tools[].name] == $names" "$json"

# --strict: the inspector's tool-schema portability check (exit 6 on an
# error-severity problem, which run() reports).
json=$(run --strict --method tools/list)
check "tool schemas pass the inspector's --strict check" "[.result.tools[].name] == $names" "$json"
# --strict also prints portability warnings (exit 0) on stderr: a type array
# on chalkboard_math's `ghosting` was one (#712), now an anyOf. None may return.
warnings=$(npx -y "$inspector" --cli "$server" --format json --strict --method tools/list 2>&1 >/dev/null || true)
if grep -q '^Warning:' <<<"$warnings"; then
    echo "FAIL --strict warns about a tool schema"
    echo "$warnings" | head -c 2000
    echo
    fail=1
else
    echo "ok   --strict reports no schema warnings"
fi

# The dual-era handshake Claude Code uses: probe 2026-07-28's
# server/discover, fall back to initialize on "method not found".
json=$(run --protocol-era auto --method tools/list)
check "a dual-era client falls back to the legacy handshake" "[.result.tools[].name] == $names" "$json"

json=$(run --method tools/call --tool-name render_postscript \
    --tool-args-json '{"source":"0 0 40 40 rectfill showpage","width":40,"height":40}')
check "render_postscript returns a PNG" \
    '.result.isError == false and .result.content[0].type == "image" and .result.content[0].mimeType == "image/png" and (.result.content[0].data | startswith("iVBORw0KGgo"))' "$json"

json=$(run --method tools/call --tool-name eval_postscript --tool-args-json '{"source":"3 4 add ="}')
check "eval_postscript prints" '.result.isError == false and (.result.content[0].text | contains("7"))' "$json"

json=$(run --method tools/call --tool-name eval_postscript --tool-args-json '{"source":"1 2 bogus"}')
check "eval_postscript reports a structured PostScript error" \
    '.result.isError == true and .result.structuredContent.error.ps_error == "undefined" and .result.structuredContent.error.offending_command == "bogus" and (.result.content[0].text | contains("undefined"))' "$json"

json=$(run --method tools/call --tool-name render_postscript \
    --tool-args-json '{"source":"showpage","out_path":"../escape.png"}')
check "out_path outside the root is refused" \
    '.result.isError == true and .result.structuredContent.error.kind == "out_path_refused"' "$json"
json=$(run --method tools/call --tool-name render_postscript \
    --tool-args-json '{"source":"showpage","out_path":"Makefile"}')
check "out_path without the render extension is refused" \
    '.result.isError == true and .result.structuredContent.error.kind == "out_path_refused" and .result.structuredContent.error.required_extension == ".png"' "$json"
[ ! -e "$work/Makefile" ] || { echo "FAIL a Makefile was written"; fail=1; }

[ ! -e "$work/../escape.png" ] || { echo "FAIL a file escaped the root"; fail=1; }

json=$(run --method tools/call --tool-name handwrite --tool-args-json '{"text":"hello","seed":3}')
check "handwrite returns a PNG" '.result.isError == false and .result.content[0].type == "image"' "$json"

# chalkboard_math's anyOf `board` and `ghosting` (#712): each form a client
# may send, on a tiny board (the default one is 1600x800 with grain).
chalk='"latex":"x^2","width":200,"height":100,"dpi":36'
for args in \
    '"board":"slate","ghosting":0' \
    '"board":false' \
    '"board":[0.1,0.2,0.3],"ghosting":true' \
    '"board":{"tone":"greenboard"}' \
    '"board":"greenboard","ghosting":0.8'; do
    json=$(run --method tools/call --tool-name chalkboard_math --tool-args-json "{$chalk,$args}")
    check "chalkboard_math takes {$args}" \
        '.result.isError == false and .result.content[0].type == "image" and .result.content[0].mimeType == "image/png"' "$json"
done
json=$(run --method tools/call --tool-name chalkboard_math \
    --tool-args-json "{$chalk,\"board\":false,\"ghosting\":0.5}")
check "chalkboard_math refuses ghosting without a board" \
    '.result.isError == true and (.result.content[0].text | contains("ghosting needs a board"))' "$json"

# review_sheet's dpi lists hold numbers and {dpi, zoom} objects (anyOf items).
json=$(run --method tools/call --tool-name review_sheet --tool-args-json \
    '{"entries":[{"source":"0 0 40 40 rectfill showpage","label":"square"}],"width":60,"height":60,"dpi":[72,{"dpi":9,"zoom":2}]}')
check "review_sheet takes a dpi list of numbers and {dpi, zoom} objects" \
    '.result.isError == false and .result.content[0].type == "image" and .result.content[0].mimeType == "image/png"' "$json"

[ ! -e "$work/inspector-failed" ] || fail=1
exit "$fail"
