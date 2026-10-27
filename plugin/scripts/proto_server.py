# PROTOTYPE (#866): a minimal MCP stdio server, to see the .cmd was chosen.
import json, sys
if sys.argv[1:2] == ["--hook"]:
    print(json.dumps({"systemMessage": "PROTO-CMD"}))
    sys.exit(0)
for line in sys.stdin:
    m = json.loads(line)
    if "id" not in m:
        continue
    if m["method"] == "initialize":
        r = {"protocolVersion": m["params"]["protocolVersion"], "capabilities": {"tools": {}},
             "serverInfo": {"name": "proto-cmd", "version": "0"}}
    elif m["method"] == "tools/list":
        r = {"tools": [{"name": "render_postscript", "description": "proto",
                        "inputSchema": {"type": "object"}}]}
    else:
        r = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": m["id"], "result": r}) + "\n")
    sys.stdout.flush()
