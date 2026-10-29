//! pscat-mcp integration (Stage 14; protocol contract and bounds, #568):
//! drive the real binary over stdio with newline-delimited JSON-RPC, the
//! way an MCP client would.

mod common;
use common::CommandDeadline as _;

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Server {
    child: Child,
    reader: BufReader<ChildStdout>,
    /// Kills a hung server after the shared deadline (see `tests/common`).
    watchdog: Option<common::Watchdog>,
}

/// Every environment variable `pscat-mcp` reads for a limit (#756).
const LIMIT_ENV_VARS: &[&str] = &[
    "PSCAT_MCP_TIMEOUT",
    "PSCAT_MCP_JOB_TIMEOUT",
    "PSCAT_MCP_MAX_JOBS",
    "PSCAT_MCP_JOB_TTL",
    "PSCAT_MCP_MAX_BANDS",
    "PSCAT_MCP_MAX_INPUT_BYTES",
    "PSCAT_MCP_MAX_IMAGE_BYTES",
    "PSCAT_MCP_MAX_PIXELS",
    "PSCAT_MCP_MAX_LATEX_BYTES",
    "PSCAT_MCP_MAX_STEPS",
    "PSCAT_MCP_MAX_MEMORY",
];

impl Server {
    /// A server whose working directory (so its default output root) is a
    /// fresh scratch directory.
    fn start() -> Self {
        Self::start_in(&common::scratch_dir("mcp-cwd"), &[])
    }

    fn start_in(cwd: &Path, args: &[&str]) -> Self {
        Self::spawn(Command::new(env!("CARGO_BIN_EXE_pscat-mcp")), cwd, args)
    }

    /// The same server reached as `pscat mcp`, the command the MCP
    /// Registry's cargo entry runs (#569).
    fn start_subcommand() -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pscat"));
        cmd.arg("mcp");
        Self::spawn(cmd, &common::scratch_dir("mcp-sub-cwd"), &[])
    }

    /// A server started with these environment variables (issue #756).
    fn start_with_env(cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pscat-mcp"));
        cmd.envs(env.iter().copied());
        Self::spawn(cmd, cwd, args)
    }

    fn spawn(mut cmd: Command, cwd: &Path, args: &[&str]) -> Self {
        // A developer's own PSCAT_MCP_* settings must not change what a test
        // sees; a test that wants one sets it through `start_with_env`
        // (after this, so it is not removed).
        for var in LIMIT_ENV_VARS {
            if cmd.get_envs().all(|(k, _)| k != *var) {
                cmd.env_remove(var);
            }
        }
        let mut child = cmd
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn pscat-mcp");
        let reader = BufReader::new(child.stdout.take().expect("stdout"));
        // A hung server fails the test (EOF on `reader`) instead of hanging it.
        let watchdog = Some(common::Watchdog::new(&child, common::default_deadline()));
        Server {
            child,
            reader,
            watchdog,
        }
    }

    fn send_raw(&mut self, bytes: &[u8]) {
        let stdin = self.child.stdin.as_mut().expect("stdin");
        stdin.write_all(bytes).expect("write request");
        stdin.write_all(b"\n").expect("write newline");
        stdin.flush().expect("flush");
    }

    /// The next stdout line, which must be one JSON-RPC 2.0 message: the
    /// "stdout carries clean JSON-RPC" check runs on every reply read.
    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("read response");
        let v: Value = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line:?}"));
        assert_eq!(v["jsonrpc"], "2.0", "not a JSON-RPC message: {line}");
        assert!(
            v.get("result").is_some() != v.get("error").is_some(),
            "exactly one of result/error: {line}"
        );
        v
    }

    fn call(&mut self, msg: Value) -> Value {
        self.send_raw(msg.to_string().as_bytes());
        self.read()
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        self.call(json!({
            "jsonrpc": "2.0", "id": 100, "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }))["result"]
            .clone()
    }

    /// Send a `tools/call` without reading its reply.
    fn request(&mut self, id: Value, name: &str, arguments: Value) {
        let msg = json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        });
        self.send_raw(msg.to_string().as_bytes());
    }

    #[cfg(unix)]
    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Close stdin, as a client does to stop a stdio server, and read what
    /// is left on stdout up to its end.
    fn close_and_drain(&mut self) -> Vec<Value> {
        drop(self.child.stdin.take());
        let mut rest = Vec::new();
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line).expect("read") == 0 {
                return rest;
            }
            rest.push(serde_json::from_str(&line).expect("JSON line"));
        }
    }

    fn ping_ok(&mut self) {
        let resp = self.call(json!({ "jsonrpc": "2.0", "id": "ping-1", "method": "ping" }));
        assert_eq!(resp["id"], "ping-1");
        assert_eq!(resp["result"], json!({}));
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.watchdog.take()); // before the pid is reaped
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn initialize(server: &mut Server, version: Value) -> Value {
    server.call(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": version, "capabilities": {},
                    "clientInfo": { "name": "test", "version": "0" } }
    }))
}

fn initialized(server: &mut Server) {
    let resp = initialize(server, json!("2025-11-25"));
    assert_eq!(resp["result"]["serverInfo"]["name"], "pscat");
    assert_eq!(
        resp["result"]["serverInfo"]["version"],
        common::built_version()
    );
    assert_eq!(resp["result"]["protocolVersion"], "2025-11-25");
    server.send_raw(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
}

fn texts(result: &Value) -> Vec<String> {
    result["content"]
        .as_array()
        .expect("content array")
        .iter()
        .filter_map(|c| c["text"].as_str().map(str::to_owned))
        .collect()
}

fn error_kind(result: &Value) -> &str {
    assert_eq!(result["isError"], true, "expected a tool error: {result}");
    result["structuredContent"]["error"]["kind"]
        .as_str()
        .expect("structured error kind")
}

// --- protocol version contract -----------------------------------------

#[test]
fn supported_versions_are_answered_as_asked() {
    // 2025-11-25: Claude Code and the MCP Inspector; 2025-06-18: Codex.
    for v in ["2025-11-25", "2025-06-18"] {
        let mut s = Server::start();
        let resp = initialize(&mut s, json!(v));
        assert_eq!(resp["result"]["protocolVersion"], v);
        assert_eq!(resp["result"]["capabilities"], json!({ "tools": {} }));
        assert!(resp["result"]["instructions"].is_string());
    }
}

#[test]
fn an_unsupported_revision_is_never_echoed() {
    // Spec: a server that does not support the requested revision answers
    // with one it does support (its newest); the client decides.
    for v in ["2024-11-05", "2025-03-26", "2026-07-28", "2099-01-01"] {
        let mut s = Server::start();
        let resp = initialize(&mut s, json!(v));
        assert_eq!(
            resp["result"]["protocolVersion"], "2025-11-25",
            "requested {v}: {resp}"
        );
    }
}

#[test]
fn a_malformed_or_missing_version_is_refused_with_the_supported_list() {
    for v in [json!("1.0.0"), json!(""), json!(20251125), json!(null)] {
        let mut s = Server::start();
        let resp = initialize(&mut s, v.clone());
        assert_eq!(resp["error"]["code"], -32602, "requested {v}: {resp}");
        assert_eq!(
            resp["error"]["data"]["supported"],
            json!(["2025-11-25", "2025-06-18"])
        );
        assert_eq!(resp["error"]["data"]["requested"], v);
        s.ping_ok();
    }
}

#[test]
fn server_discover_is_method_not_found_so_clients_fall_back() {
    // Claude Code probes 2026-07-28's `server/discover` first and falls
    // back to `initialize` on any error that is not a modern one (-32022
    // would claim 2026-07-28 support).
    let mut s = Server::start();
    let resp = s.call(json!({
        "jsonrpc": "2.0", "id": "server-discover-probe-1", "method": "server/discover",
        "params": { "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } }
    }));
    assert_eq!(resp["id"], "server-discover-probe-1");
    assert_eq!(resp["error"]["code"], -32601);
    initialized(&mut s);
}

// --- tools ---------------------------------------------------------------

#[test]
fn handshake_and_tool_list() {
    let mut s = Server::start();
    initialized(&mut s);
    let resp = s.call(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let names: Vec<&str> = resp["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        [
            "render_postscript",
            "handwrite",
            "chalkboard_math",
            "review_sheet",
            "describe_art_capabilities",
            "eval_postscript",
            "start_render",
            "render_status",
            "render_result",
            "cancel_render"
        ],
        "the advertised tool set"
    );
    for tool in resp["result"]["tools"].as_array().unwrap() {
        assert!(
            tool["inputSchema"]["required"].is_array(),
            "every tool declares required params"
        );
    }
}

/// Walk a schema, calling `f` on every object that is itself a schema node
/// (the root, `properties` values, `items`, `anyOf` branches).
fn schema_nodes<'a>(node: &'a Value, f: &mut dyn FnMut(&'a Value)) {
    f(node);
    if let Some(props) = node.get("properties").and_then(Value::as_object) {
        props.values().for_each(|p| schema_nodes(p, f));
    }
    if let Some(items) = node.get("items") {
        schema_nodes(items, f);
    }
    if let Some(branches) = node.get("anyOf").and_then(Value::as_array) {
        branches.iter().for_each(|b| schema_nodes(b, f));
    }
}

/// The shape of the input schemas clients have to accept (#712). Codex
/// 0.164.0 and Claude Code 2.1.309 both took `chalkboard_math`'s `board`
/// and `ghosting` unions as written; the MCP Inspector's `--strict` check
/// warns on a `type` array, which some clients map onto a single-`type`
/// dialect (Gemini's), so unions are `anyOf` branches that each carry one
/// `type`. Pinned so a "tidy" does not bring back what a client may reject.
#[test]
fn tool_schemas_use_only_the_portable_union_shape() {
    let mut s = Server::start();
    initialized(&mut s);
    let resp = s.call(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let tools = resp["result"]["tools"].as_array().expect("tools array");

    for tool in tools {
        schema_nodes(&tool["inputSchema"], &mut |n| {
            let name = tool["name"].as_str().unwrap();
            assert!(!n["type"].is_array(), "{name}: a `type` array in {n}");
            for key in ["$ref", "$defs", "oneOf", "allOf", "not"] {
                assert!(n.get(key).is_none(), "{name}: `{key}` in {n}");
            }
            if let Some(branches) = n.get("anyOf").and_then(Value::as_array) {
                for b in branches {
                    assert!(
                        b["type"].is_string(),
                        "{name}: anyOf branch without one type: {b}"
                    );
                }
            }
        });
    }

    let chalk = tools
        .iter()
        .find(|t| t["name"] == "chalkboard_math")
        .expect("chalkboard_math is listed");
    let props = &chalk["inputSchema"]["properties"];
    assert_eq!(
        props["board"]["anyOf"],
        json!([
            { "type": "boolean" },
            { "type": "string", "enum": ["slate", "greenboard"] },
            { "type": "array", "items": { "type": "number", "minimum": 0, "maximum": 1 }, "minItems": 3, "maxItems": 3 },
            { "type": "object" }
        ]),
        "board: bool, name, [r, g, b] or an object"
    );
    assert_eq!(
        props["ghosting"]["anyOf"],
        json!([
            { "type": "number", "minimum": 0, "maximum": 1 },
            { "type": "boolean" }
        ]),
        "ghosting: a number or a bool"
    );
    assert!(
        props["ghosting"].get("type").is_none() && props["board"].get("type").is_none(),
        "the unions carry no outer `type`"
    );

    // review_sheet's dpi lists take a number or a {dpi, zoom} object per item.
    let sheet = tools.iter().find(|t| t["name"] == "review_sheet").unwrap();
    let item = &sheet["inputSchema"]["properties"]["dpi"]["items"];
    let kinds: Vec<&str> = item["anyOf"]
        .as_array()
        .expect("dpi items are an anyOf")
        .iter()
        .map(|b| b["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["number", "object"]);

    // The description must not claim `&` aligns every multi-row input
    // (#699: gather rows are centred).
    let desc = chalk["description"].as_str().unwrap();
    assert!(desc.contains("gather") && desc.contains("centre"), "{desc}");
    assert!(!desc.contains("& aligns them"), "{desc}");
}

/// Every tool carries all five annotation hints (#687). The table is the
/// contract: a tool missing from it fails this test, so a new tool must say
/// whether it is read-only before it ships. `destructive` is false for the
/// writers on purpose (see `annotations` in src/mcp_server.rs).
#[test]
fn every_tool_has_annotations() {
    // (name, readOnlyHint, idempotentHint): only start_render is not
    // idempotent, since each call starts another job.
    let expected = [
        ("render_postscript", false, true),
        ("handwrite", false, true),
        ("chalkboard_math", false, true),
        ("review_sheet", false, true),
        ("describe_art_capabilities", true, true),
        ("eval_postscript", true, true),
        ("start_render", false, false),
        ("render_status", true, true),
        ("render_result", true, true),
        ("cancel_render", false, true),
    ];
    let mut s = Server::start();
    initialized(&mut s);
    let resp = s.call(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let tools = resp["result"]["tools"].as_array().expect("tools array");
    assert_eq!(
        tools.len(),
        expected.len(),
        "a tool with no row in this table"
    );
    for tool in tools {
        let name = tool["name"].as_str().expect("name");
        let &(_, read_only, idempotent) = expected
            .iter()
            .find(|(n, _, _)| *n == name)
            .unwrap_or_else(|| panic!("{name} has no row in the annotations table"));
        let a = tool["annotations"]
            .as_object()
            .unwrap_or_else(|| panic!("{name}: annotations must be an object"));
        for key in [
            "readOnlyHint",
            "destructiveHint",
            "idempotentHint",
            "openWorldHint",
        ] {
            assert!(a[key].is_boolean(), "{name}: {key} must be set explicitly");
        }
        assert_eq!(a["readOnlyHint"], read_only, "{name}: readOnlyHint");
        assert_eq!(a["destructiveHint"], false, "{name}: destructiveHint");
        assert_eq!(a["idempotentHint"], idempotent, "{name}: idempotentHint");
        assert_eq!(a["openWorldHint"], false, "{name}: no network, ever");
        assert_eq!(a["title"], tool["title"], "{name}: annotations.title");
    }
}

#[test]
fn pscat_mcp_subcommand_is_the_same_server() {
    let mut s = Server::start_subcommand();
    initialized(&mut s);
    let resp = s.call(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    assert_eq!(resp["result"]["tools"].as_array().expect("tools").len(), 10);
    // A tool call works too: the subcommand finds pscat (itself) to shell out to.
    let result = s.tool("eval_postscript", json!({ "source": "(hi) =" }));
    assert_eq!(result["isError"], false, "{result}");
}

#[test]
fn render_returns_a_png_image() {
    let mut s = Server::start();
    initialized(&mut s);
    let result = s.tool(
        "render_postscript",
        json!({ "source": "0 0 40 40 rectfill showpage", "width": 40, "height": 40 }),
    );
    let content = &result["content"][0];
    assert_eq!(content["type"], "image");
    assert_eq!(content["mimeType"], "image/png");
    // Base64 of the PNG signature: any PNG starts iVBORw0KGgo.
    let data = content["data"].as_str().expect("base64 data");
    assert!(data.starts_with("iVBORw0KGgo"), "PNG magic in base64");
    assert_eq!(result["isError"], false);
    assert!(result.get("structuredContent").is_none());
}

#[test]
fn eval_reports_prints_and_structured_errors() {
    let mut s = Server::start();
    initialized(&mut s);
    let result = s.tool("eval_postscript", json!({ "source": "3 4 add ==" }));
    assert_eq!(result["isError"], false);
    assert!(texts(&result)[0].contains('7'));
    // A PostScript error comes back as a tool error: the standard name and
    // the operand-stack post-mortem as text, and the same as fields.
    let result = s.tool("eval_postscript", json!({ "source": "1 2\nbogus" }));
    assert_eq!(error_kind(&result), "postscript_error");
    let text = &texts(&result)[0];
    assert!(text.contains("undefined"), "error named: {text}");
    assert!(text.contains("Operand stack"), "post-mortem: {text}");
    let err = &result["structuredContent"]["error"];
    assert_eq!(err["ps_error"], "undefined");
    assert_eq!(err["offending_command"], "bogus");
    assert_eq!(err["line"], 2);
}

#[test]
fn handwrite_returns_a_note() {
    let mut s = Server::start();
    initialized(&mut s);
    let result = s.tool(
        "handwrite",
        json!({ "text": "hello from mcp", "size": 24, "seed": 3 }),
    );
    assert_eq!(result["isError"], false, "handwrite failed: {result}");
    assert_eq!(result["content"][0]["type"], "image");
}

#[test]
fn render_stays_silent_about_lint_when_the_page_is_clean() {
    let mut s = Server::start();
    initialized(&mut s);
    let result = s.tool(
        "render_postscript",
        json!({ "source": "0 0 40 40 rectfill showpage", "width": 40, "height": 40 }),
    );
    assert_eq!(result["isError"], false);
    assert!(
        texts(&result).iter().all(|t| !t.starts_with("Lint:")),
        "no Lint block on a clean render: {result}"
    );
}

#[test]
fn render_of_an_errored_program_reports_the_error_once_not_a_lint_verdict() {
    // Issue #369: `--lint` now prints a `[run-error]` line for a run that
    // errored; the `PostScript error:` block already says so, so the
    // `Lint:` block must not repeat it.
    let mut s = Server::start();
    initialized(&mut s);
    let result = s.tool(
        "render_postscript",
        json!({ "source": "0 0 40 40 rectfill pop showpage", "width": 40, "height": 40 }),
    );
    let texts = texts(&result);
    let err = texts
        .iter()
        .find(|t| t.starts_with("PostScript error:"))
        .expect("an error block");
    assert!(err.contains("stackunderflow"), "{err}");
    assert!(!err.contains("run-error"), "{err}");
    assert!(
        texts.iter().all(|t| !t.contains("run-error")),
        "no run-error line in any block: {texts:?}"
    );
}

#[test]
fn render_surfaces_lint_findings_for_a_blank_page() {
    let mut s = Server::start();
    initialized(&mut s);
    let result = s.tool(
        "render_postscript",
        json!({ "source": "showpage", "width": 40, "height": 40 }),
    );
    assert_eq!(result["isError"], false);
    let texts = texts(&result);
    let lint = texts
        .iter()
        .find(|t| t.starts_with("Lint:"))
        .expect("a Lint block for a blank page");
    assert!(lint.contains("blank-page"), "lint block: {lint}");
}

#[test]
fn unknown_method_and_unknown_tool_are_jsonrpc_errors() {
    let mut s = Server::start();
    initialized(&mut s);
    let resp = s.call(json!({ "jsonrpc": "2.0", "id": 7, "method": "resources/list" }));
    assert_eq!(resp["error"]["code"], -32601);
    let resp = s.call(json!({
        "jsonrpc": "2.0", "id": 8, "method": "tools/call",
        "params": { "name": "rm_rf", "arguments": {} }
    }));
    assert_eq!(resp["error"]["code"], -32602);
}

// --- out_path confinement --------------------------------------------------

const SQUARE: &str = "0 0 20 20 rectfill showpage";

fn render_to(s: &mut Server, out_path: &str) -> Value {
    s.tool(
        "render_postscript",
        json!({ "source": SQUARE, "width": 20, "height": 20, "out_path": out_path }),
    )
}

#[test]
fn out_path_inside_the_root_is_written_relative_or_absolute() {
    let root = common::scratch_dir("mcp-root");
    std::fs::create_dir(root.join("sub")).unwrap();
    let mut s = Server::start_in(&root, &[]);
    initialized(&mut s);

    let result = render_to(&mut s, "sub/a.png");
    assert_eq!(result["isError"], false, "{result}");
    assert!(root.join("sub/a.png").is_file());

    let abs = root.join("b.png");
    let result = render_to(&mut s, abs.to_str().unwrap());
    assert_eq!(result["isError"], false, "{result}");
    assert!(abs.is_file());

    // An existing regular file is replaced (documented).
    std::fs::write(root.join("c.png"), b"old").unwrap();
    let result = render_to(&mut s, "c.png");
    assert_eq!(result["isError"], false, "{result}");
    assert!(
        std::fs::read(root.join("c.png"))
            .unwrap()
            .starts_with(b"\x89PNG")
    );

    // A multi-page render numbers its files inside the root.
    let result = s.tool(
        "render_postscript",
        json!({ "source": "showpage showpage", "width": 20, "height": 20, "out_path": "pages.png" }),
    );
    assert_eq!(result["isError"], false, "{result}");
    assert!(root.join("pages-001.png").is_file() && root.join("pages-002.png").is_file());
    // No temp files are left behind.
    let leftovers: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn out_path_traversal_and_outside_absolute_paths_are_refused() {
    let base = common::scratch_dir("mcp-confine");
    let root = base.join("root");
    std::fs::create_dir(&root).unwrap();
    let outside = base.join("outside.png");
    let mut s = Server::start_in(&root, &[]);
    initialized(&mut s);

    for bad in [
        "../outside.png".to_string(),
        "sub/../../outside.png".to_string(),
        "./../outside.png".to_string(),
        outside.to_str().unwrap().to_string(),
        root.join("..")
            .join("outside.png")
            .to_str()
            .unwrap()
            .to_string(),
        "/etc/pscat-mcp-should-not-exist.png".to_string(),
        "missing-dir/x.png".to_string(),
        "".to_string(),
        "dir-only/".to_string(),
    ] {
        let result = render_to(&mut s, &bad);
        assert_eq!(error_kind(&result), "out_path_refused", "{bad}: {result}");
        assert!(
            texts(&result)[0].contains("refused"),
            "the text says so too: {result}"
        );
    }
    assert!(!outside.exists(), "nothing written outside the root");
    // handwrite shares the check.
    let result = s.tool(
        "handwrite",
        json!({ "text": "hi", "out_path": "../outside.png" }),
    );
    assert_eq!(error_kind(&result), "out_path_refused");
    assert!(!outside.exists());
    s.ping_ok();
}

#[cfg(unix)]
#[test]
fn out_path_symlink_escapes_are_refused() {
    use std::os::unix::fs::symlink;
    let base = common::scratch_dir("mcp-symlink");
    let root = base.join("root");
    let outside = base.join("outside");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    // A directory link leading out, and a file link pointing at a file out.
    symlink(&outside, root.join("door")).unwrap();
    let victim = outside.join("victim.png");
    std::fs::write(&victim, b"keep me").unwrap();
    symlink(&victim, root.join("trap.png")).unwrap();
    // A link to a directory inside the root is fine.
    std::fs::create_dir(root.join("real")).unwrap();
    symlink(root.join("real"), root.join("alias")).unwrap();

    let mut s = Server::start_in(&root, &[]);
    initialized(&mut s);
    let result = render_to(&mut s, "door/escaped.png");
    assert_eq!(error_kind(&result), "out_path_refused", "{result}");
    assert!(!outside.join("escaped.png").exists());

    let result = render_to(&mut s, "trap.png");
    assert_eq!(error_kind(&result), "out_path_refused", "{result}");
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep me");

    let result = render_to(&mut s, "alias/ok.png");
    assert_eq!(result["isError"], false, "{result}");
    assert!(root.join("real/ok.png").is_file());
}

/// The refusal names the extension a retry needs.
fn assert_refused_with_extension(result: &Value, ext: &str) {
    assert_eq!(error_kind(result), "out_path_refused", "{result}");
    assert_eq!(
        result["structuredContent"]["error"]["required_extension"], ext,
        "{result}"
    );
}

#[test]
fn out_path_needs_the_extension_of_the_output_format() {
    let root = common::scratch_dir("mcp-ext");
    let mut s = Server::start_in(&root, &[]);
    initialized(&mut s);

    // Wrong, missing, or no extension at all.
    for bad in [
        "Makefile",
        ".envrc",
        "pre-commit",
        "x.txt",
        "x.png.sh",
        "x.svg",
        ".png",
        "x.",
        "x.pdf",
    ] {
        let result = render_to(&mut s, bad);
        assert_refused_with_extension(&result, ".png");
        assert!(!root.join(bad).exists(), "{bad} must not be written");
    }
    // The extension follows the requested format, case aside.
    let svg = |s: &mut Server, out_path: &str| {
        s.tool(
            "render_postscript",
            json!({ "source": SQUARE, "width": 20, "height": 20, "format": "svg", "out_path": out_path }),
        )
    };
    assert_refused_with_extension(&svg(&mut s, "x.png"), ".svg");
    assert_eq!(svg(&mut s, "x.svg")["isError"], false);
    assert_eq!(svg(&mut s, "Y.SVG")["isError"], false);
    assert!(root.join("x.svg").is_file() && root.join("Y.SVG").is_file());
    let pdf = s.tool(
        "render_postscript",
        json!({ "source": SQUARE, "width": 20, "height": 20, "format": "pdf", "out_path": "x.png" }),
    );
    assert_refused_with_extension(&pdf, ".pdf");
    let pdf = s.tool(
        "render_postscript",
        json!({ "source": SQUARE, "width": 20, "height": 20, "format": "pdf", "out_path": "x.pdf" }),
    );
    assert_eq!(pdf["isError"], false, "{pdf}");
    // handwrite makes PNGs.
    let result = s.tool("handwrite", json!({ "text": "hi", "out_path": "note.svg" }));
    assert_refused_with_extension(&result, ".png");
    let result = s.tool("handwrite", json!({ "text": "hi", "out_path": "note.png" }));
    assert_eq!(result["isError"], false, "{result}");
    assert!(root.join("note.png").is_file());
    // A multi-page render is checked on the name asked for.
    let result = s.tool(
        "render_postscript",
        json!({ "source": "showpage showpage", "width": 20, "height": 20, "out_path": "pages" }),
    );
    assert_refused_with_extension(&result, ".png");
    assert!(!root.join("pages-001").exists());
    s.ping_ok();
}

#[test]
fn out_path_refuses_dot_directories() {
    let root = common::scratch_dir("mcp-dotdir");
    // The directories exist, so a refusal is the rule, not a missing dir.
    std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
    std::fs::create_dir_all(root.join("sub/.claude")).unwrap();
    std::fs::create_dir(root.join("real")).unwrap();
    let mut s = Server::start_in(&root, &[]);
    initialized(&mut s);

    for bad in [
        ".git/hooks/x.png",
        ".git/x.png",
        "sub/.claude/x.png",
        "./.git/x.png",
    ] {
        let result = render_to(&mut s, bad);
        assert_eq!(error_kind(&result), "out_path_refused", "{bad}: {result}");
        assert!(
            texts(&result)[0].contains("dot-directory"),
            "{bad}: {result}"
        );
    }
    let abs = root.join(".git/hooks/y.png");
    let result = render_to(&mut s, abs.to_str().unwrap());
    assert_eq!(error_kind(&result), "out_path_refused", "{result}");
    assert!(
        std::fs::read_dir(root.join(".git/hooks"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(
        std::fs::read_dir(root.join("sub/.claude"))
            .unwrap()
            .next()
            .is_none()
    );

    // A visible directory is fine.
    assert_eq!(render_to(&mut s, "real/x.png")["isError"], false);

    // A link to a dot-directory is judged by where it leads.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join(".git"), root.join("alias")).unwrap();
        let result = render_to(&mut s, "alias/x.png");
        assert_eq!(error_kind(&result), "out_path_refused", "{result}");
        assert!(!root.join(".git/x.png").exists());
    }
}

#[test]
fn out_root_may_itself_sit_under_a_dot_directory() {
    let base = common::scratch_dir("mcp-dotroot");
    let root = base.join(".cache/renders");
    std::fs::create_dir_all(&root).unwrap();
    let mut s = Server::start_in(&base, &["--out-root", root.to_str().unwrap()]);
    initialized(&mut s);
    assert_eq!(render_to(&mut s, "x.png")["isError"], false);
    assert!(root.join("x.png").is_file());
}

#[test]
fn out_path_overwrites_only_existing_render_files() {
    let root = common::scratch_dir("mcp-overwrite");
    for (name, bytes) in [
        ("Makefile", &b"all:\n"[..]),
        (".envrc", b"export X=1\n"),
        ("notes.txt", b"keep me"),
        ("png", b"no dot"),
    ] {
        std::fs::write(root.join(name), bytes).unwrap();
    }
    std::fs::write(root.join("old.png"), b"old").unwrap();
    let mut s = Server::start_in(&root, &[]);
    initialized(&mut s);

    // An existing render file of the same extension is replaced.
    let result = render_to(&mut s, "old.png");
    assert_eq!(result["isError"], false, "{result}");
    assert!(
        std::fs::read(root.join("old.png"))
            .unwrap()
            .starts_with(b"\x89PNG")
    );

    // Anything else is refused and left byte for byte as it was.
    for (name, bytes) in [
        ("Makefile", &b"all:\n"[..]),
        (".envrc", b"export X=1\n"),
        ("notes.txt", b"keep me"),
        ("png", b"no dot"),
    ] {
        let result = render_to(&mut s, name);
        assert_refused_with_extension(&result, ".png");
        assert_eq!(std::fs::read(root.join(name)).unwrap(), bytes, "{name}");
    }

    // An existing file of another render extension is not replaced by a png
    // render either (its name has the wrong extension).
    std::fs::write(root.join("doc.pdf"), b"%PDF keep").unwrap();
    let result = render_to(&mut s, "doc.pdf");
    assert_refused_with_extension(&result, ".png");
    assert_eq!(std::fs::read(root.join("doc.pdf")).unwrap(), b"%PDF keep");

    // A directory named like a render is not a file to replace.
    std::fs::create_dir(root.join("dir.png")).unwrap();
    let result = render_to(&mut s, "dir.png");
    assert_eq!(error_kind(&result), "out_path_refused", "{result}");
    assert!(root.join("dir.png").is_dir());
}

#[test]
fn out_root_flag_moves_the_root() {
    let cwd = common::scratch_dir("mcp-cwd2");
    let root = common::scratch_dir("mcp-flagroot");
    let mut s = Server::start_in(&cwd, &["--out-root", root.to_str().unwrap()]);
    initialized(&mut s);
    assert_eq!(render_to(&mut s, "x.png")["isError"], false);
    assert!(root.join("x.png").is_file());
    let result = render_to(&mut s, cwd.join("y.png").to_str().unwrap());
    assert_eq!(error_kind(&result), "out_path_refused");
}

#[cfg(unix)]
#[test]
fn out_path_is_off_when_started_in_the_filesystem_root() {
    let mut s = Server::start_in(Path::new("/"), &[]);
    initialized(&mut s);
    let result = render_to(&mut s, "tmp/x.png");
    assert_eq!(error_kind(&result), "out_path_disabled", "{result}");
    // Rendering inline still works.
    let result = s.tool(
        "render_postscript",
        json!({ "source": SQUARE, "width": 20, "height": 20 }),
    );
    assert_eq!(result["isError"], false);
}

// --- read roots (issue #763) -------------------------------------------------

/// The PostScript error `eval_postscript` reported, or "" for success.
fn eval_error(s: &mut Server, source: &str) -> String {
    let result = s.tool("eval_postscript", json!({ "source": source }));
    if result["isError"] == false {
        return String::new();
    }
    result["structuredContent"]["error"]["ps_error"]
        .as_str()
        .unwrap_or_else(|| panic!("no ps_error: {result}"))
        .to_string()
}

#[test]
fn reads_are_confined_to_the_working_directory_and_allow_read() {
    let base = common::scratch_dir("mcp-read-roots");
    let cwd = base.join("project");
    let extra = base.join("extra");
    let outside = base.join("outside");
    for d in [&cwd, &extra, &outside] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(cwd.join("a.ps"), "(in cwd) =").unwrap();
    std::fs::write(extra.join("b.ps"), "(in extra) =").unwrap();
    std::fs::write(outside.join("away.ps"), "(leaked) =").unwrap();
    let extra_arg = extra.to_str().unwrap();
    let mut s = Server::start_in(&cwd, &["--allow-read", extra_arg]);
    let init = initialize(&mut s, json!("2025-11-25"));
    let instructions = init["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("read roots"), "{instructions}");
    assert!(instructions.contains("extra"), "{instructions}");
    s.send_raw(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let p = |d: &Path, f: &str| common::ps_path(&d.join(f));
    // Inside: relative, absolute, and an --allow-read directory.
    assert_eq!(eval_error(&mut s, "(a.ps) run"), "");
    assert_eq!(
        eval_error(&mut s, &format!("({}) run", p(&cwd, "a.ps"))),
        ""
    );
    assert_eq!(
        eval_error(&mut s, &format!("({}) run", p(&extra, "b.ps"))),
        ""
    );
    // The bundled lib/ is always there.
    assert_eq!(eval_error(&mut s, "(lib/artkit.ps) run"), "");
    // Outside, `..`, a missing file outside, and readreals.
    let away = p(&outside, "away.ps");
    for src in [
        format!("({away}) run"),
        format!("({away}) (r) file"),
        format!("({}) (r) file", p(&outside, "missing.ps")),
        "(../outside/away.ps) run".to_string(),
        format!("({away}) 1 readreals"),
    ] {
        assert_eq!(eval_error(&mut s, &src), "invalidfileaccess", "{src}");
    }
    // A symlink in the working directory pointing out.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.join("away.ps"), cwd.join("door.ps")).unwrap();
        assert_eq!(eval_error(&mut s, "(door.ps) run"), "invalidfileaccess");
    }
    // filenameforall lists inside, and nothing outside.
    let list = |s: &mut Server, template: &str| {
        let src = format!("({template}) {{ = }} 400 string filenameforall (end) =");
        let result = s.tool("eval_postscript", json!({ "source": src }));
        assert_eq!(result["isError"], false, "{result}");
        texts(&result)[0].clone()
    };
    let inside = list(&mut s, &p(&cwd, "*.ps"));
    assert!(inside.contains("a.ps"), "{inside}");
    assert!(
        !inside.contains("door.ps"),
        "the symlink out isn't listed: {inside}"
    );
    let out = list(&mut s, &p(&outside, "*"));
    assert!(!out.contains("away"), "{out}");
    let up = list(&mut s, "../outside/*");
    assert!(!up.contains("away"), "{up}");
}

/// `pscat-mcp` run to completion with these arguments and environment (and no
/// other `PSCAT_MCP_*` variable).
fn run_server(args: &[&str], env: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pscat-mcp"));
    for var in LIMIT_ENV_VARS {
        cmd.env_remove(var);
    }
    cmd.envs(env.iter().copied())
        .args(args)
        .stdin(Stdio::null())
        .output_deadline()
        .unwrap()
}

#[test]
fn an_env_var_raises_a_limit_and_a_flag_overrides_it() {
    // Issue #756: PSCAT_MCP_MAX_STEPS is read at startup like --max-steps.
    let cwd = common::scratch_dir("mcp-env-steps");
    let mut s = Server::start_with_env(&cwd, &[], &[("PSCAT_MCP_MAX_STEPS", "1000000")]);
    initialized(&mut s);
    let result = s.tool("eval_postscript", json!({ "source": "{ } loop" }));
    assert_eq!(error_kind(&result), "limit_exceeded", "{result}");
    assert_eq!(
        result["structuredContent"]["error"]["limit"], "steps",
        "{result}"
    );
    // The flag wins: a loop of 5 million iterations passes under a flag of
    // 0 (no limit) although the variable says 1 million.
    let mut s = Server::start_with_env(
        &cwd,
        &["--max-steps", "0"],
        &[("PSCAT_MCP_MAX_STEPS", "1000000")],
    );
    initialized(&mut s);
    let result = s.tool(
        "eval_postscript",
        json!({ "source": "0 1 5000000 { pop } for (done) =" }),
    );
    assert_eq!(result["isError"], false, "{result}");
}

#[test]
fn a_bad_env_var_is_a_startup_error_but_help_still_works() {
    for (var, bad) in [
        ("PSCAT_MCP_TIMEOUT", "soon"),
        ("PSCAT_MCP_MAX_PIXELS", "99999999999"),
    ] {
        let out = run_server(&[], &[(var, bad)]);
        assert_eq!(out.status.code(), Some(2), "{var}={bad}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains(var) && err.contains("whole number"), "{err}");
        // `--help` is not blocked by it.
        let out = run_server(&["--help"], &[(var, bad)]);
        assert!(out.status.success(), "{var}={bad}");
        assert!(String::from_utf8_lossy(&out.stdout).contains("PSCAT_MCP_"));
    }
}

#[test]
fn a_bad_allow_read_is_a_startup_error() {
    let missing = common::scratch_dir("mcp-allow-read").join("nope");
    let out = Command::new(env!("CARGO_BIN_EXE_pscat-mcp"))
        .arg("--allow-read")
        .arg(&missing)
        .stdin(Stdio::null())
        .output_deadline()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--allow-read"));
}

#[cfg(unix)]
#[test]
fn the_filesystem_root_is_never_a_read_root() {
    // Started in `/` (some desktop apps do), the working directory is not a
    // read root, so an absolute path anywhere is refused, and a relative one
    // finds nothing there (existing or not: no probing).
    let mut s = Server::start_in(Path::new("/"), &[]);
    initialized(&mut s);
    assert_eq!(
        eval_error(&mut s, "(/etc/hosts) (r) file"),
        "invalidfileaccess"
    );
    for name in ["etc/hosts", "etc/no-such-file"] {
        let src = format!("({name}) (r) file");
        assert_eq!(eval_error(&mut s, &src), "undefinedfilename", "{src}");
    }
    assert_eq!(eval_error(&mut s, "(lib/artkit.ps) run"), "");
}

#[test]
fn handwrite_and_chalk_still_render_under_read_roots() {
    // A scratch working directory: the libraries come from the bundled copy.
    let mut s = Server::start();
    initialized(&mut s);
    let result = s.tool("handwrite", json!({ "text": "roots", "size": 18 }));
    assert_eq!(result["isError"], false, "{result}");
    let result = s.tool(
        "chalkboard_math",
        json!({ "latex": "x", "width": 400, "height": 200, "dpi": 72 }),
    );
    assert_eq!(result["isError"], false, "{result}");
}

// --- resource bounds ---------------------------------------------------------

#[test]
fn a_runaway_program_is_stopped_by_the_interpreter_with_a_structured_error() {
    // Issue #604: the interpreter's own limits stop `{ } loop` inside the
    // server's timeout, with a `limit_exceeded` error naming the limit, not
    // the `timeout` kill. (The test asserts the error, not the elapsed time.)
    // A low step cap and a long timeout make the step limit trip first and
    // deterministically; the clock has its own test below.
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-limit"),
        &["--timeout", "30", "--max-steps", "1000000"],
    );
    initialized(&mut s);
    let result = s.tool("eval_postscript", json!({ "source": "{ } loop" }));
    assert_eq!(error_kind(&result), "limit_exceeded", "{result}");
    let err = &result["structuredContent"]["error"];
    assert_eq!(err["ps_error"], "limitexceeded");
    assert_eq!(err["limit"], "steps", "the limit is named: {err}");
    // `stopped` cannot swallow it.
    let result = s.tool(
        "eval_postscript",
        json!({ "source": "{ { { } loop } stopped pop } loop" }),
    );
    assert_eq!(error_kind(&result), "limit_exceeded", "{result}");
    // A render keeps the partial canvas, with the error beside it.
    let result = s.tool(
        "render_postscript",
        json!({ "source": "1 0 0 setrgbcolor 0 0 50 50 rectfill { } loop", "width": 60, "height": 60 }),
    );
    assert_eq!(result["isError"], false, "{result}");
    let content = result["content"].as_array().unwrap();
    assert!(content.iter().any(|c| c["type"] == "image"), "{result}");
    assert!(
        texts(&result).iter().any(|t| t.contains("limitexceeded")),
        "{result}"
    );
    s.ping_ok();
}

#[test]
fn step_and_memory_limits_are_server_flags() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-limit-flags"),
        &["--max-steps", "100000", "--max-memory", "100"],
    );
    initialized(&mut s);
    let result = s.tool("eval_postscript", json!({ "source": "{ 1 pop } loop" }));
    assert_eq!(error_kind(&result), "limit_exceeded", "{result}");
    assert_eq!(result["structuredContent"]["error"]["limit"], "steps");
    let result = s.tool(
        "eval_postscript",
        json!({ "source": "/d 100000 dict def 0 1 99999 { d exch 100000 array put } for" }),
    );
    assert_eq!(error_kind(&result), "limit_exceeded", "{result}");
    assert_eq!(result["structuredContent"]["error"]["limit"], "memory");
    // A program inside the limits is unaffected.
    let result = s.tool(
        "eval_postscript",
        json!({ "source": "1 1 1000 { pop } for (ok) =" }),
    );
    assert_eq!(result["isError"], false, "{result}");
}

#[test]
fn handwrite_is_bounded_by_the_step_limit_not_only_the_timeout() {
    // Issue #701 (found by #711): `pscat handwrite` did not get the server's
    // step, time and memory limits, so only `--timeout`'s kill bounded it. A
    // library that never finishes stands in for `lib/handscript.ps`
    // (`$PSCAT_LIB`, which the child inherits); the step limit must stop it
    // long before the 60 s kill, with the structured `limit_exceeded`.
    let dir = common::scratch_dir("mcp-handwrite-limit");
    let lib = dir.join("runaway-lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(lib.join("handscript.ps"), "{ } loop\n").unwrap();
    let mut s = Server::start_with_env(
        &dir,
        &["--timeout", "60", "--max-steps", "100000"],
        &[("PSCAT_LIB", lib.to_str().unwrap())],
    );
    initialized(&mut s);
    let result = s.tool("handwrite", json!({ "text": "hello" }));
    // The 60 s kill would be a `timeout` error, so `limit_exceeded` naming
    // the steps is what shows the interpreter's own limit stopped it.
    assert_eq!(error_kind(&result), "limit_exceeded", "{result}");
    let err = &result["structuredContent"]["error"];
    assert_eq!(err["limit"], "steps", "the step limit stopped it: {err}");
    s.ping_ok();
}

#[test]
fn zero_turns_a_limit_off_and_the_clock_still_stops_the_program() {
    // `--timeout 6` gives the child 4.5 s (a quarter, at most 5 s, is kept
    // back for the child's start and exit), a margin wide enough for a loaded
    // CI runner. Only the clock is left once the other two are off.
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-limit-off"),
        &["--max-steps", "0", "--max-memory", "0", "--timeout", "6"],
    );
    initialized(&mut s);
    let result = s.tool("eval_postscript", json!({ "source": "{ 1 pop } loop" }));
    assert_eq!(error_kind(&result), "limit_exceeded", "{result}");
    assert_eq!(
        result["structuredContent"]["error"]["limit"], "time",
        "only the clock is left: {result}"
    );
}

#[cfg(unix)]
#[test]
fn a_whole_file_read_that_never_ends_hits_the_limits_not_the_timeout_kill() {
    // Issue #702. A read of a FIFO nobody writes to, or of `/dev/zero`, used
    // to sit inside one operator, past every limit, until the server's own
    // timeout killed the child. The read now looks at the limits as it goes,
    // so the call fails with `limit_exceeded` (the render keeps its partial
    // page) and the kill stays only as the backstop.
    let dir = common::scratch_dir("mcp-backstop");
    let fifo = dir.join("stuck.fifo");
    let _ = std::fs::remove_file(&fifo);
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo")
            .success(),
        "make a FIFO"
    );
    let mut s = Server::start_in(&dir, &["--timeout", "2"]);
    initialized(&mut s);
    let started = std::time::Instant::now();
    let source = format!("({}) (r) file pop", common::ps_path(&fifo));
    let result = s.tool("eval_postscript", json!({ "source": source }));
    let took = started.elapsed();
    assert_eq!(error_kind(&result), "limit_exceeded", "{result}");
    assert_eq!(
        result["structuredContent"]["error"]["limit"], "time",
        "{result}"
    );
    assert!(
        took < std::time::Duration::from_secs(15),
        "stopped promptly: {took:?}"
    );
    // `/dev/zero` is outside the read roots (issue #763), so it is refused
    // before any read; the CLI-side limit tests still read it to the cap.
    let result = s.tool(
        "eval_postscript",
        json!({ "source": "(/dev/zero) (r) file pop" }),
    );
    assert_eq!(error_kind(&result), "postscript_error", "{result}");
    assert_eq!(
        result["structuredContent"]["error"]["ps_error"], "invalidfileaccess",
        "{result}"
    );
    s.ping_ok();
}

#[test]
fn a_program_that_prints_forever_is_cut_off() {
    let mut s = Server::start();
    initialized(&mut s);
    let result = s.tool(
        "eval_postscript",
        json!({ "source": "{ (xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx) print } loop" }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert!(
        texts(&result)[0].len() < 10_000,
        "the reply itself is bounded"
    );
    s.ping_ok();
}

#[test]
fn oversized_input_is_a_tool_error_on_its_own_id() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-input"),
        &["--max-input-bytes", "1000"],
    );
    initialized(&mut s);
    let big = format!("{} showpage", "% padding\n".repeat(200));
    let result = s.tool("eval_postscript", json!({ "source": big }));
    assert_eq!(error_kind(&result), "input_too_large", "{result}");
    let result = s.tool("handwrite", json!({ "text": "a".repeat(1001) }));
    assert_eq!(error_kind(&result), "input_too_large", "{result}");
    // A line far past the transport cap is dropped unbuffered, with a
    // JSON-RPC error, and the session continues.
    let huge = format!(
        r#"{{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{{"name":"eval_postscript","arguments":{{"source":"{}"}}}}}}"#,
        "x".repeat(200_000)
    );
    s.send_raw(huge.as_bytes());
    let resp = s.read();
    assert_eq!(resp["error"]["code"], -32600, "{resp}");
    s.ping_ok();
}

#[test]
fn inline_images_over_the_cap_are_refused_but_out_path_still_gets_the_file() {
    let root = common::scratch_dir("mcp-imgcap");
    let mut s = Server::start_in(&root, &["--max-image-bytes", "200"]);
    initialized(&mut s);
    // Noise does not compress: well over 200 bytes of PNG.
    let noisy = "1 srand 0 1 99 { /y exch def 0 1 99 { y 1 1 rand 2 mod setgray rectfill } for } for showpage";
    let result = s.tool(
        "render_postscript",
        json!({ "source": noisy, "width": 100, "height": 100 }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert!(
        result["content"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["type"] == "text")
    );

    let result = s.tool(
        "render_postscript",
        json!({ "source": noisy, "width": 100, "height": 100, "out_path": "noise.png" }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert!(texts(&result)[0].contains("noise.png"), "{result}");
    assert!(root.join("noise.png").is_file());
}

#[test]
fn bad_arguments_are_structured_tool_errors() {
    let mut s = Server::start();
    initialized(&mut s);
    for (tool, args) in [
        ("render_postscript", json!({})),
        ("render_postscript", json!({ "source": 42 })),
        (
            "render_postscript",
            json!({ "source": SQUARE, "width": -5 }),
        ),
        (
            "render_postscript",
            json!({ "source": SQUARE, "width": 99999 }),
        ),
        (
            "render_postscript",
            json!({ "source": SQUARE, "height": "tall" }),
        ),
        ("render_postscript", json!({ "source": SQUARE, "dpi": 1e9 })),
        (
            "render_postscript",
            json!({ "source": SQUARE, "format": "gif" }),
        ),
        (
            "render_postscript",
            json!({ "source": SQUARE, "format": "pdf" }),
        ),
        (
            "render_postscript",
            json!({ "source": SQUARE, "halftone": "yes" }),
        ),
        (
            "render_postscript",
            json!({ "source": SQUARE, "out_path": 7 }),
        ),
        ("handwrite", json!({ "text": "hi", "seed": 1.5 })),
        ("handwrite", json!({ "text": "hi", "paper": "papyrus" })),
        ("handwrite", json!({ "text": "hi", "size": "big" })),
        ("eval_postscript", json!({ "source": null })),
    ] {
        let result = s.tool(tool, args.clone());
        assert_eq!(
            error_kind(&result),
            "invalid_argument",
            "{tool} {args}: {result}"
        );
    }
    // `arguments` that is not an object.
    let resp = s.call(json!({
        "jsonrpc": "2.0", "id": 11, "method": "tools/call",
        "params": { "name": "eval_postscript", "arguments": "3 4 add" }
    }));
    assert_eq!(error_kind(&resp["result"]), "invalid_argument");
    // A bad ink is caught by `pscat handwrite` itself.
    let result = s.tool("handwrite", json!({ "text": "hi", "ink": "1,0,0) pop (" }));
    assert_eq!(error_kind(&result), "handwrite_failed", "{result}");
    s.ping_ok();
}

// --- malformed input smoke corpus ---------------------------------------------

/// Each line gets at most one reply, every reply is clean JSON-RPC, and
/// the server is still serving afterwards.
#[test]
fn malformed_input_corpus() {
    let mut s = Server::start();
    initialized(&mut s);
    let corpus: &[(&[u8], Option<i64>)] = &[
        (b"{", Some(-32700)),
        (b"not json at all", Some(-32700)),
        (b"\xff\xfe\x00garbage", Some(-32700)),
        (
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"\xff}",
            Some(-32700),
        ),
        (b"[]", Some(-32600)),
        (
            br#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#,
            Some(-32600),
        ),
        (b"42", Some(-32600)),
        (b"\"ping\"", Some(-32600)),
        (b"null", Some(-32600)),
        (br#"{"jsonrpc":"2.0","id":3}"#, Some(-32600)),
        (br#"{"jsonrpc":"1.0","id":3,"method":"ping"}"#, Some(-32600)),
        (br#"{"id":3,"method":"ping"}"#, Some(-32600)),
        (
            br#"{"jsonrpc":"2.0","id":{"a":1},"method":"ping"}"#,
            Some(-32600),
        ),
        (
            br#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
            Some(-32600),
        ),
        (br#"{"jsonrpc":"2.0","id":3,"method":7}"#, Some(-32600)),
        (
            br#"{"jsonrpc":"2.0","id":3,"method":"ping","params":[1]}"#,
            Some(-32602),
        ),
        (
            br#"{"jsonrpc":"2.0","id":3,"method":"tools/call"}"#,
            Some(-32602),
        ),
        (
            br#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":5}}"#,
            Some(-32602),
        ),
        (
            br#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope","arguments":"x"}}"#,
            Some(-32602),
        ),
        (
            br#"{"jsonrpc":"2.0","id":3,"method":"no/such"}"#,
            Some(-32601),
        ),
        // Notifications and stray responses: no reply at all.
        (
            br#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
            None,
        ),
        (br#"{"jsonrpc":"2.0","id":5,"result":{}}"#, None),
        (
            br#"{"jsonrpc":"2.0","id":5,"error":{"code":1,"message":"x"}}"#,
            None,
        ),
        (b"   ", None),
    ];
    for (line, want) in corpus {
        s.send_raw(line);
        if let Some(code) = want {
            let resp = s.read();
            assert_eq!(
                resp["error"]["code"],
                *code,
                "{}: {resp}",
                String::from_utf8_lossy(line)
            );
        }
        // A ping after every line proves nothing extra was written (the
        // next reply is the ping's) and the server is alive.
        s.ping_ok();
    }
    // A PostScript program full of hostile bytes is just a program.
    let result = s.tool(
        "eval_postscript",
        json!({ "source": "(\u{0}\u{1b}[2J\n{\"jsonrpc\":\"2.0\"}) print" }),
    );
    assert_eq!(result["isError"], false, "{result}");
    s.ping_ok();
}

#[test]
fn stray_child_output_never_reaches_stdout() {
    // Everything a program prints is inside a tool result, never a bare line.
    let mut s = Server::start();
    initialized(&mut s);
    let result = s.tool(
        "render_postscript",
        json!({ "source": "(hello\n) print 0 0 20 20 rectfill showpage", "width": 20, "height": 20 }),
    );
    assert_eq!(result["isError"], false);
    s.ping_ok();
}

#[test]
fn a_finite_flood_that_exits_is_still_over_the_cap() {
    // About 5 MB printed, then a clean exit: the overflow must be seen even
    // when the child is gone before the server's next poll.
    let mut s = Server::start();
    initialized(&mut s);
    let result = s.tool(
        "eval_postscript",
        json!({ "source": "1 1 100000 { pop (xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx) print } for" }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
}

#[test]
fn more_pages_than_the_inline_cap_are_refused_before_reading() {
    let root = common::scratch_dir("mcp-pagecap");
    let mut s = Server::start_in(&root, &[]);
    initialized(&mut s);
    let source = "1 1 21 { pop showpage } for";
    let result = s.tool(
        "render_postscript",
        json!({ "source": source, "width": 10, "height": 10 }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert_eq!(result["structuredContent"]["error"]["pages"], 21);
    let result = s.tool(
        "render_postscript",
        json!({ "source": source, "width": 10, "height": 10, "out_path": "many.png" }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert!(root.join("many-021.png").is_file());
}

/// The pixel cap (#711): `render_postscript` and `handwrite` refuse a page
/// over `--max-pixels` with `output_too_large` and the numbers, before any
/// canvas is allocated.
#[test]
fn render_postscript_and_handwrite_refuse_pages_over_the_pixel_cap() {
    let mut s = Server::start();
    initialized(&mut s);
    // The default cap, 40 million: each of these is over it (or 8000 a side).
    for (w, h, dpi) in [(8000, 8000, 72), (7000, 7000, 72), (600, 900, 1200)] {
        let args = json!({ "source": "showpage", "width": w, "height": h, "dpi": dpi });
        let result = s.tool("render_postscript", args);
        assert_eq!(error_kind(&result), "output_too_large", "{w}x{h}@{dpi}");
        let err = &result["structuredContent"]["error"];
        assert_eq!(err["limit_pixels"], 40_000_000, "{err}");
        assert!(err["pixels"].as_u64().unwrap() > 40_000_000, "{err}");
    }
    // Every format draws on the same raster.
    let args = json!({ "source": "showpage", "width": 7000, "height": 7000,
                       "format": "svg" });
    let result = s.tool("render_postscript", args);
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    // A page under the cap still renders.
    let result = s.tool(
        "render_postscript",
        json!({ "source": "showpage", "width": 100, "height": 100 }),
    );
    assert_eq!(result["isError"], false, "{result}");

    // handwrite: the width is checked in the server, the area in the child.
    let result = s.tool(
        "handwrite",
        json!({ "text": "hi", "width": 8000, "dpi": 100 }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    let result = s.tool(
        "handwrite",
        json!({ "text": "hi", "width": 700, "dpi": 1200 }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert_eq!(
        result["structuredContent"]["error"]["limit_pixels"],
        40_000_000
    );

    // A long note at 144 dpi is under the area cap (about 13 million pixels)
    // but over 8000 pixels tall: refused by name, not as a failed render.
    let long = "a b c d e f g h i j ".repeat(200);
    let result = s.tool("handwrite", json!({ "text": long, "dpi": 144 }));
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    let err = &result["structuredContent"]["error"];
    assert_eq!(err["width_px"], 1224, "{err}");
    assert!(err["height_px"].as_u64().unwrap() > 8000, "{err}");
}

#[test]
fn a_lowered_pixel_cap_applies_to_every_canvas_tool() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-pixcap"),
        &["--max-pixels", "300000"],
    );
    initialized(&mut s);
    // 612 x 792 is 484,704 pixels; 400 x 700 is 280,000.
    let result = s.tool("render_postscript", json!({ "source": "showpage" }));
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    let err = &result["structuredContent"]["error"];
    assert_eq!(err["limit_pixels"], 300_000);
    assert_eq!(err["pixels"], 484_704);
    assert_eq!(
        (err["width_px"].clone(), err["height_px"].clone()),
        (json!(612), json!(792))
    );
    let ok = json!({ "source": "showpage", "width": 400, "height": 700 });
    assert_eq!(s.tool("render_postscript", ok)["isError"], false);

    // A short note fits; a long one is auto-sized taller than the cap.
    let result = s.tool("handwrite", json!({ "text": "hi" }));
    assert_eq!(result["isError"], false, "{result}");
    let long = "a b c d e f g h i j ".repeat(40);
    let result = s.tool("handwrite", json!({ "text": long }));
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    let err = &result["structuredContent"]["error"];
    assert_eq!(err["limit_pixels"], 300_000);
    assert_eq!(err["width_px"], 612);
    assert!(err["pixels"].as_u64().unwrap() > 300_000, "{err}");

    // chalkboard_math and review_sheet use the same cap.
    let args = json!({ "latex": "x", "width": 800, "height": 400, "dpi": 72 });
    let result = s.tool("chalkboard_math", args);
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert_eq!(
        result["structuredContent"]["error"]["limit_pixels"],
        300_000
    );
    let args = json!({ "entries": [{ "source": "showpage", "width": 800, "height": 400 }] });
    let result = s.tool("review_sheet", args);
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert_eq!(
        result["structuredContent"]["error"]["limit_pixels"],
        300_000
    );

    // The limit is in the tool descriptions.
    let list = s.call(json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/list" }));
    let text = list["result"]["tools"].to_string();
    assert!(text.contains("300000 pixels"), "{text}");
}

#[test]
fn max_latex_bytes_is_a_server_flag() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-latexcap"),
        &["--max-latex-bytes", "10"],
    );
    initialized(&mut s);
    let result = s.tool("chalkboard_math", small_board("x+x+x+x+x+x"));
    assert_eq!(error_kind(&result), "input_too_large", "{result}");
    assert_eq!(result["structuredContent"]["error"]["limit"], 10);
    let result = s.tool("chalkboard_math", small_board("x^2"));
    assert_eq!(result["isError"], false, "{result}");

    // Raised above the 4096 default, a long input is no longer refused for
    // its length (a comment-only one is an empty formula, at once).
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-latexcap2"),
        &["--max-latex-bytes", "8192"],
    );
    initialized(&mut s);
    let result = s.tool("chalkboard_math", small_board(&"% ".repeat(2100)));
    assert_eq!(error_kind(&result), "latex_error", "{result}");
}

#[test]
fn the_cap_flags_reject_values_out_of_range() {
    for args in [
        &["--max-pixels", "0"][..],
        &["--max-pixels", "64000001"],
        &["--max-pixels", "many"],
        &["--max-latex-bytes", "0"],
        &["--max-latex-bytes"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_pscat-mcp"))
            .args(args)
            .current_dir(common::scratch_dir("mcp-badflag"))
            .stdin(Stdio::null())
            .output_deadline()
            .expect("run pscat-mcp");
        assert!(!out.status.success(), "{args:?} must be refused");
        assert!(out.stdout.is_empty(), "{args:?}: nothing on stdout");
    }
}

/// A render that succeeds can still have something to say on stderr (a font
/// falling back to a built-in face, #728, exits 0): it comes back as a
/// `Warnings:` text item, once per distinct line, and not the child's own
/// receipt. The line here is a caught error that the program reports with
/// `handleerror`, a generic stderr line that needs no font pack.
#[test]
fn render_postscript_returns_stderr_warnings_on_success() {
    let mut s = Server::start();
    initialized(&mut s);
    let source =
        "3 { { 1 0 div } stopped { handleerror } if } repeat 100 100 50 50 rectfill showpage";
    let result = s.tool(
        "render_postscript",
        json!({ "source": source, "width": 200, "height": 200 }),
    );
    assert_eq!(result["isError"], false, "{result}");
    let warnings: Vec<_> = texts(&result)
        .into_iter()
        .filter(|t| t.starts_with("Warnings:"))
        .collect();
    assert_eq!(warnings.len(), 1, "{result}");
    assert_eq!(
        warnings[0].matches("undefinedresult").count(),
        1,
        "three identical lines shown once: {}",
        warnings[0]
    );
    assert!(!warnings[0].contains("pscat: wrote"), "{}", warnings[0]);
    assert!(
        result["content"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["type"] == "image"),
        "the image is still there"
    );

    // A quiet render has no Warnings item.
    let result = s.tool(
        "render_postscript",
        json!({ "source": "0 0 50 50 rectfill showpage", "width": 100, "height": 100 }),
    );
    assert!(
        texts(&result).iter().all(|t| !t.starts_with("Warnings:")),
        "{result}"
    );

    // review_sheet: the same, from a cell that succeeds.
    let args = json!({ "entries": [{ "source": source, "width": 100, "height": 100 }],
                       "dpi": [72] });
    let result = s.tool("review_sheet", args);
    assert_eq!(result["isError"], false, "{result}");
    assert!(
        texts(&result).iter().any(|t| t.starts_with("Warnings:")),
        "{result}"
    );
}

/// `eval_postscript` returns a successful run's stderr as a `Warnings:` item
/// after the output: the program's own `%stderr` writes, once per distinct
/// line. A quiet run, and one that only prints, has none; a failed run keeps
/// its stderr in the error.
#[test]
fn eval_postscript_returns_stderr_warnings_on_success() {
    let mut s = Server::start();
    initialized(&mut s);
    let source = "(hello) = (%stderr) (w) file 3 { dup (note: careful\\n) writestring } repeat pop \
                  3 { { 1 0 div } stopped { handleerror } if } repeat";
    let result = s.tool("eval_postscript", json!({ "source": source }));
    assert_eq!(result["isError"], false, "{result}");
    let all = texts(&result);
    assert_eq!(all[0].trim(), "hello", "{result}");
    let warnings: Vec<_> = all.iter().filter(|t| t.starts_with("Warnings:")).collect();
    assert_eq!(warnings.len(), 1, "{result}");
    assert_eq!(
        warnings[0].matches("note: careful").count(),
        1,
        "{}",
        warnings[0]
    );
    assert_eq!(
        warnings[0].matches("undefinedresult").count(),
        1,
        "{}",
        warnings[0]
    );
    assert!(!warnings[0].contains("pscat:"), "{}", warnings[0]);

    let result = s.tool("eval_postscript", json!({ "source": "(hi) =" }));
    assert_eq!(texts(&result).len(), 1, "{result}");
    let result = s.tool("eval_postscript", json!({ "source": "1 2 add" }));
    assert_eq!(texts(&result), ["(ran clean, printed nothing)"], "{result}");
}

#[test]
fn version_flag_prints_the_build_version_and_exits_without_serving() {
    // A closed stdin: were the flag ignored the server would see EOF and
    // exit with empty stdout, so the assertion fails instead of hanging.
    let want = format!("pscat-mcp {}\n", common::built_version());
    for flag in ["--version", "-V"] {
        let out = Command::new(env!("CARGO_BIN_EXE_pscat-mcp"))
            .arg(flag)
            .current_dir(common::scratch_dir("mcp-version"))
            .stdin(Stdio::null())
            .output_deadline()
            .expect("run pscat-mcp");
        assert!(out.status.success(), "{flag} must exit 0");
        assert_eq!(String::from_utf8_lossy(&out.stdout), want, "{flag}");
        assert!(out.stderr.is_empty(), "{flag} stderr: {:?}", out.stderr);
    }
}

// --- chalkboard_math (#688) ----------------------------------------------------

/// FNV-1a, as `tests/chalk.rs` pins its boards: a hash of the pixels, not of
/// the PNG bytes, so an encoder change does not move it.
fn fnv1a(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

const QUADRATIC: &str = r"x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}";

/// `tests/chalk.rs`'s small board, as flat tool arguments.
fn small_board(latex: &str) -> Value {
    json!({ "latex": latex, "width": 400, "height": 200, "dpi": 72, "size": 40, "seed": 1 })
}

/// The quadratic formula at seed 1 comes back as the same board the library
/// and `pscat chalk` draw (`tests/chalk.rs` pins the same hash), from a
/// server whose working directory is not a checkout, and twice the same.
#[test]
fn chalkboard_math_draws_the_pinned_board() {
    let root = common::scratch_dir("mcp-chalk");
    let mut s = Server::start_in(&root, &[]);
    initialized(&mut s);
    let mut args = small_board(QUADRATIC);
    args["out_path"] = json!("q1.png");
    let result = s.tool("chalkboard_math", args.clone());
    assert_eq!(result["isError"], false, "{result}");
    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["mimeType"], "image/png");
    assert!(texts(&result)[0].contains("q1.png"), "{result}");
    args["out_path"] = json!("q2.png");
    assert_eq!(s.tool("chalkboard_math", args)["isError"], false);

    let png = std::fs::read(root.join("q1.png")).unwrap();
    assert_eq!(
        png,
        std::fs::read(root.join("q2.png")).unwrap(),
        "same seed"
    );
    let pm = tiny_skia::Pixmap::decode_png(&png).expect("decode");
    assert_eq!((pm.width(), pm.height()), (400, 200));
    assert_eq!(fnv1a(pm.data()), 0x7746bc8158136515, "the board moved");
    let cfg = pscat::chalk::Config::from_json(
        r#"{"layout": {"width": 400, "height": 200, "dpi": 72, "size": 40}, "seed": 1}"#,
    )
    .unwrap();
    let lib = pscat::chalk::render(QUADRATIC, &cfg, pscat::chalk::Format::Png).unwrap();
    assert_eq!(png, lib, "the tool draws exactly what the library does");
}

/// No board: chalk alone on a transparent page; `config` carries the keys
/// that have no flat argument.
#[test]
fn chalkboard_math_without_a_board_is_transparent() {
    let root = common::scratch_dir("mcp-chalk-clear");
    let mut s = Server::start_in(&root, &[]);
    initialized(&mut s);
    let mut args = small_board("x^2");
    args["board"] = json!(false);
    args["config"] = json!({ "chalk": { "dust": 0 } });
    args["out_path"] = json!("clear.png");
    let result = s.tool("chalkboard_math", args);
    assert_eq!(result["isError"], false, "{result}");
    let pm = tiny_skia::Pixmap::decode_png(&std::fs::read(root.join("clear.png")).unwrap())
        .expect("decode");
    assert_eq!(pm.pixel(0, 0).unwrap().alpha(), 0, "a transparent corner");
    assert!(pm.pixels().iter().any(|p| p.alpha() > 0), "some chalk");
}

#[test]
fn chalkboard_math_errors_are_one_line_and_structured() {
    let mut s = Server::start();
    initialized(&mut s);
    // TeX outside mathkit's subset: the command, named, in one line.
    let result = s.tool("chalkboard_math", small_board(r"a \oplus b"));
    assert_eq!(error_kind(&result), "latex_error", "{result}");
    let err = &result["structuredContent"]["error"];
    assert_eq!(err["code"], "unsupported_command");
    assert_eq!(err["token"], r"\oplus");
    let text = &texts(&result)[0];
    assert!(
        text.starts_with(r"unsupported LaTeX command \oplus"),
        "{text}"
    );
    assert!(!text.contains('\n') && !text.contains("%%["), "{text}");
    assert_eq!(result["content"].as_array().unwrap().len(), 1, "no image");

    // A multi-byte character survives the trip through the child's report.
    let result = s.tool("chalkboard_math", small_board("x = \u{2603}"));
    assert_eq!(error_kind(&result), "latex_error", "{result}");
    assert_eq!(result["structuredContent"]["error"]["token"], "\u{2603}");
    assert!(texts(&result)[0].ends_with("(U+2603)"), "{result}");

    let result = s.tool("chalkboard_math", json!({ "latex": "% nothing" }));
    assert_eq!(error_kind(&result), "latex_error");
    assert_eq!(result["structuredContent"]["error"]["code"], "empty");

    // The caps (#688): checked before anything renders.
    let result = s.tool("chalkboard_math", json!({ "latex": "x+".repeat(3000) }));
    assert_eq!(error_kind(&result), "input_too_large", "{result}");
    assert_eq!(result["structuredContent"]["error"]["limit"], 4096);
    for (w, h, dpi) in [(8000, 8000, 72), (1920, 1200, 301), (8000, 100, 144)] {
        let args = json!({ "latex": "x", "width": w, "height": h, "dpi": dpi });
        let result = s.tool("chalkboard_math", args);
        assert_eq!(error_kind(&result), "output_too_large", "{w}x{h}@{dpi}");
        assert_eq!(
            result["structuredContent"]["error"]["limit_pixels"],
            40_000_000
        );
    }

    // Bad arguments name the key the caller sent.
    for (args, says) in [
        (json!({ "latex": "x", "size": 0 }), "size must be"),
        (
            json!({ "latex": "x", "color": "mauve" }),
            "color must be one of",
        ),
        (json!({ "latex": "x", "board": "chalk" }), "board must be"),
        (json!({ "latex": "x", "board": 3 }), "board must be"),
        (json!({ "latex": "x", "seed": 1.5 }), "seed must be"),
        (
            json!({ "latex": "x", "board": false, "ghosting": 0.3 }),
            "ghosting needs a board",
        ),
        (
            json!({ "latex": "x", "config": { "colour": "red" } }),
            "unknown key colour",
        ),
        (
            json!({ "latex": "x", "config": [] }),
            "config must be an object",
        ),
        (json!({ "latex": "x", "format": "svg" }), "needs out_path"),
        (json!({ "latex": "x", "format": "gif" }), "format must be"),
        (
            json!({ "latex": "x", "width": 60, "config": { "layout": { "margin": 40 } } }),
            "no room",
        ),
        (json!({ "latex": 7 }), "latex must be a string"),
    ] {
        let result = s.tool("chalkboard_math", args.clone());
        assert_eq!(error_kind(&result), "invalid_argument", "{args}: {result}");
        assert!(texts(&result)[0].contains(says), "{args}: {result}");
    }
    s.ping_ok();
}

/// The board runs under the server's interpreter limits, as every render does.
#[test]
fn chalkboard_math_runs_under_the_interpreter_limits() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-chalk-lim"),
        &["--max-steps", "100000"],
    );
    initialized(&mut s);
    let result = s.tool("chalkboard_math", small_board(QUADRATIC));
    assert_eq!(error_kind(&result), "limit_exceeded", "{result}");
    assert_eq!(result["structuredContent"]["error"]["limit"], "steps");
}

// --- review_sheet (#708) --------------------------------------------------------

/// Standard base64, as an image content carries its PNG.
fn unbase64(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => panic!("not base64: {c}"),
    };
    let mut out = Vec::new();
    for chunk in text.trim_end_matches('=').as_bytes().chunks(4) {
        let n = chunk
            .iter()
            .fold(0u32, |n, &c| n << 6 | u32::from(value(c)))
            << (6 * (4 - chunk.len()));
        out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
    }
    out
}

/// The PNG an image content of a result carries.
fn sheet_png(result: &Value) -> Vec<u8> {
    assert_eq!(result["isError"], false, "{result}");
    let images: Vec<&Value> = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "image")
        .collect();
    assert_eq!(images.len(), 1, "one sheet: {result}");
    assert_eq!(images[0]["mimeType"], "image/png");
    unbase64(images[0]["data"].as_str().unwrap())
}

/// A logo-like program: a square and a disc in `rgb`, at the page's size.
fn mark(rgb: &str) -> String {
    format!(
        "{rgb} setrgbcolor 0.1 0.1 scale 100 100 translate 0 0 800 800 rectfill 1 setgray 500 500 200 0 360 arc fill"
    )
}

const BAD_PROGRAM: &str = "0 0 1 setrgbcolor 0 0 30 30 rectfill nosuchoperator";
const TWO_PAGES: &str =
    "1 0 0 setrgbcolor 0 0 40 40 rectfill showpage 0 0.6 0 setrgbcolor 0 0 20 40 rectfill showpage";

/// `pscat sheet` over the programs as files, in a scratch directory the same
/// way a user would run it. Returns the PNG and whether it exited cleanly.
fn cli_sheet(tag: &str, files: &[(&str, &str)], flags: &[&[&str]]) -> (Vec<u8>, bool) {
    let dir = common::scratch_dir(tag);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pscat"));
    cmd.current_dir(&dir).arg("sheet").arg("-o").arg("cli.png");
    for ((name, source), flags) in files.iter().zip(flags) {
        std::fs::write(dir.join(name), source).unwrap();
        cmd.args(*flags).arg(name);
    }
    let out = cmd.output_deadline().expect("run pscat sheet");
    (
        std::fs::read(dir.join("cli.png")).expect("the sheet"),
        out.status.success(),
    )
}

/// The acceptance check of #708: the tool's sheet is the CLI's, byte for
/// byte, for the same programs. Mixed page sizes and dpis, an enlarged cell,
/// accented labels, and the options that turn the labels and captions off.
#[test]
fn review_sheet_is_byte_identical_to_pscat_sheet() {
    let (a, b, c) = (
        mark("0.8 0.2 0.1"),
        mark("0.1 0.3 0.7"),
        mark("0.2 0.6 0.3"),
    );
    let mut s = Server::start();
    initialized(&mut s);
    let cases: [(&str, bool, bool); 3] = [
        ("all", true, true),
        ("bare", false, false),
        ("sizes", false, true),
    ];
    for (tag, labels, sizes) in cases {
        let mut extra: Vec<&str> = Vec::new();
        if !labels {
            extra.push("--no-labels");
        }
        if !sizes {
            extra.push("--no-sizes");
        }
        let dir = common::scratch_dir(&format!("mcp-sheet-{tag}"));
        for (name, source) in [("a.ps", &a), ("b.ps", &b), ("c.ps", &c)] {
            std::fs::write(dir.join(name), source).unwrap();
        }
        let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
            .current_dir(&dir)
            .arg("sheet")
            .args(["-o", "cli.png"])
            .args(&extra)
            .args([
                "--page",
                "120x120",
                "--dpi",
                "96,9,9x4",
                "--label",
                "Café ñandú",
                "a.ps",
            ])
            .args([
                "--page",
                "60x90",
                "--dpi",
                "72,200",
                "--label",
                "B: second",
                "b.ps",
            ])
            .args(["--dpi", "40", "--label", "program 3", "c.ps"])
            .output_deadline()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let cli = std::fs::read(dir.join("cli.png")).unwrap();
        let result = s.tool(
            "review_sheet",
            json!({
                "entries": [
                    { "source": a, "label": "Café ñandú", "width": 120, "height": 120,
                      "dpi": [96, 9, { "dpi": 9, "zoom": 4 }] },
                    { "source": b, "label": "B: second", "width": 60, "height": 90, "dpi": [72, 200] },
                    // No width or height here: the top-level page is the CLI's
                    // sticky `--page`, the one the file before it gave.
                    { "source": c, "label": "program 3", "width": 60, "height": 90, "dpi": [40] },
                ],
                "labels": labels,
                "sizes": sizes,
            }),
        );
        let mine = sheet_png(&result);
        assert_eq!(
            mine, cli,
            "{tag}: the tool's sheet differs from pscat sheet's"
        );
        let pm = tiny_skia::Pixmap::decode_png(&mine).expect("decode");
        assert!(pm.width() > 100 && pm.height() > 100);
    }
}

/// Defaults: the page 612x792 at 72 dpi, row labels `program N`, the
/// top-level page and dpi list for entries that give none.
#[test]
fn review_sheet_defaults_match_the_cli_defaults() {
    let (a, b) = (mark("0.8 0.2 0.1"), mark("0.1 0.3 0.7"));
    let (cli, ok) = cli_sheet(
        "mcp-sheet-defaults",
        &[("a.ps", &a), ("b.ps", &b)],
        &[
            &["--label", "program 1", "--page", "50x50", "--dpi", "72,144"],
            &["--label", "program 2"],
        ],
    );
    assert!(ok);
    let mut s = Server::start();
    initialized(&mut s);
    let result = s.tool(
        "review_sheet",
        json!({ "entries": [{ "source": a }, { "source": b }], "width": 50, "height": 50, "dpi": [72, 144] }),
    );
    assert_eq!(sheet_png(&result), cli);
}

/// A cell whose program fails is drawn as far as it got and the error comes
/// back beside the sheet, as the CLI does (it exits non-zero, with the file
/// written); a multi-page program contributes its last page.
#[test]
fn review_sheet_draws_a_failing_cell_and_a_multi_page_cell_like_the_cli() {
    let (cli, ok) = cli_sheet(
        "mcp-sheet-fail",
        &[("bad.ps", BAD_PROGRAM), ("two.ps", TWO_PAGES)],
        &[
            &["--page", "40x40", "--label", "bad"],
            &["--label", "two pages"],
        ],
    );
    assert!(!ok, "the CLI exits non-zero when a cell fails");
    let mut s = Server::start();
    initialized(&mut s);
    let result = s.tool(
        "review_sheet",
        json!({
            "entries": [
                { "source": BAD_PROGRAM, "label": "bad" },
                { "source": TWO_PAGES, "label": "two pages" },
            ],
            "width": 40, "height": 40,
        }),
    );
    assert_eq!(sheet_png(&result), cli);
    let said = texts(&result).join("\n");
    assert!(
        said.contains("PostScript error in bad at 72 dpi failed"),
        "{said}"
    );
    assert!(said.contains("nosuchoperator"), "{said}");
    assert!(
        !said.contains("two pages"),
        "only the failing cell is reported: {said}"
    );
}

/// The cells run under the interpreter's limits: a runaway cell is stopped,
/// drawn as far as it got, and named, and the server carries on.
#[test]
fn review_sheet_cells_run_under_the_interpreter_limits() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-sheet-lim"),
        &["--timeout", "30", "--max-steps", "1000000"],
    );
    initialized(&mut s);
    let result = s.tool(
        "review_sheet",
        json!({
            "entries": [
                { "source": "1 0 0 setrgbcolor 0 0 30 30 rectfill { } loop", "label": "loops" },
                { "source": "0 0 1 setrgbcolor 0 0 30 30 rectfill", "label": "fine" },
            ],
            "width": 40, "height": 40, "dpi": [72],
        }),
    );
    let png = sheet_png(&result);
    assert!(
        texts(&result)
            .iter()
            .any(|t| t.contains("limitexceeded") && t.contains("loops")),
        "{result}"
    );
    let pm = tiny_skia::Pixmap::decode_png(&png).expect("decode");
    // The red the runaway cell drew before it was stopped is on the sheet.
    assert!(
        pm.pixels()
            .iter()
            .any(|p| p.red() == 255 && p.green() == 0 && p.blue() == 0),
        "the partial cell is drawn"
    );
    s.ping_ok();
}

/// One time budget for the whole sheet, not one per cell: cells that each
/// run to their own clock limit cannot stretch a call past `--timeout`.
#[test]
fn review_sheet_has_one_time_budget_for_all_cells() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-sheet-budget"),
        &["--max-steps", "0", "--max-memory", "0", "--timeout", "3"],
    );
    initialized(&mut s);
    let started = std::time::Instant::now();
    let entry = json!({ "source": "{ 1 pop } loop" });
    let result = s.tool(
        "review_sheet",
        json!({ "entries": [entry, entry, entry, entry], "width": 20, "height": 20 }),
    );
    assert_eq!(error_kind(&result), "timeout", "{result}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(12),
        "{:?}",
        started.elapsed()
    );
    s.ping_ok();
}

/// `--timeout 1` is a supported value: a small sheet still draws, and the
/// budget left for a cell is never rounded to "no limit".
#[test]
fn review_sheet_works_under_the_smallest_timeout() {
    let mut s = Server::start_in(&common::scratch_dir("mcp-sheet-1s"), &["--timeout", "1"]);
    initialized(&mut s);
    let result = s.tool(
        "review_sheet",
        json!({ "entries": [{ "source": mark("0.8 0.2 0.1"), "label": "x" }], "width": 50, "height": 50 }),
    );
    assert_eq!(sheet_png(&result)[..4], [0x89, b'P', b'N', b'G']);
}

#[test]
fn review_sheet_caps_are_structured_errors_before_anything_runs() {
    let mut s = Server::start();
    initialized(&mut s);
    // Too many cells.
    let dpi: Vec<u32> = vec![72; 33];
    let result = s.tool(
        "review_sheet",
        json!({ "entries": [{ "source": "" }], "dpi": dpi }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert_eq!(result["structuredContent"]["error"]["limit_cells"], 32);
    // A cell past 8000 px on a side, and one past it only once enlarged.
    for (args, says) in [
        (
            json!({ "entries": [{ "source": "" }], "width": 8000, "height": 100, "dpi": [100] }),
            "8000 pixels on a side",
        ),
        (
            json!({ "entries": [{ "source": "" }], "width": 800, "height": 100, "dpi": [{ "dpi": 720, "zoom": 2 }] }),
            "shown at 2x",
        ),
    ] {
        let result = s.tool("review_sheet", args.clone());
        assert_eq!(error_kind(&result), "output_too_large", "{args}: {result}");
        assert!(texts(&result)[0].contains(says), "{args}: {result}");
    }
    // The cells' own pixels in all: 5 cells of 3000x3000 are 45 million.
    let big = json!({ "source": "", "width": 3000, "height": 3000 });
    let result = s.tool(
        "review_sheet",
        json!({ "entries": [big, big, big, big, big] }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert_eq!(
        result["structuredContent"]["error"]["limit_pixels"],
        40_000_000
    );
    // A sheet past the pixel cap whose cells are small: a wide strip over a
    // tall one makes the sheet their bounding box, over 40 million pixels.
    let result = s.tool(
        "review_sheet",
        json!({ "entries": [
            { "source": "", "width": 7000, "height": 50 },
            { "source": "", "width": 50, "height": 7000 },
        ] }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    let err = &result["structuredContent"]["error"];
    assert_eq!(err["limit_pixels"], 40_000_000);
    assert!(err["width_px"].as_u64().unwrap() > 7000, "{err}");
    // The sheet past 8000 px a side is Layout's own refusal, same kind.
    let wide = json!({ "source": "", "width": 6000, "height": 10 });
    let result = s.tool(
        "review_sheet",
        json!({ "entries": [wide], "dpi": [72, 72] }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert_eq!(result["structuredContent"]["error"]["limit_side_px"], 8000);
    s.ping_ok();
}

#[test]
fn review_sheet_bad_arguments_are_structured_tool_errors() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-sheet-bad"),
        &["--max-input-bytes", "100"],
    );
    initialized(&mut s);
    let one = json!([{ "source": "0 setgray" }]);
    for (args, says) in [
        (json!({}), "entries must be a non-empty list"),
        (json!({ "entries": [] }), "entries must be a non-empty list"),
        (
            json!({ "entries": "x" }),
            "entries must be a non-empty list",
        ),
        (json!({ "entries": [7] }), "entries[0] must be an object"),
        (json!({ "entries": [{}] }), "entries[0].source is required"),
        (
            json!({ "entries": [{ "source": 7 }] }),
            "entries[0].source must be a string",
        ),
        (
            json!({ "entries": [{ "source": "x", "label": 7 }] }),
            "label must be a string",
        ),
        (
            json!({ "entries": [{ "source": "x", "label": "a".repeat(201) }] }),
            "over 200 characters",
        ),
        (
            json!({ "entries": [{ "source": "x", "width": 0 }] }),
            "width must be a whole number",
        ),
        (
            json!({ "entries": one, "height": 9000 }),
            "height must be a whole number",
        ),
        (
            json!({ "entries": one, "dpi": 72 }),
            "dpi must be a non-empty list",
        ),
        (
            json!({ "entries": one, "dpi": [] }),
            "dpi must be a non-empty list",
        ),
        (json!({ "entries": one, "dpi": [8] }), "from 9 to 1200"),
        (json!({ "entries": one, "dpi": [1201] }), "from 9 to 1200"),
        (
            json!({ "entries": one, "dpi": ["96"] }),
            "a cell is a dpi number",
        ),
        (
            json!({ "entries": one, "dpi": [{ "zoom": 2 }] }),
            "a dpi must be a number",
        ),
        (
            json!({ "entries": one, "dpi": [{ "dpi": 9, "zoom": 1 }] }),
            "zoom must be a whole number from 2 to 16",
        ),
        (
            json!({ "entries": one, "dpi": [{ "dpi": 9, "zoom": 17 }] }),
            "zoom must be a whole number from 2 to 16",
        ),
        (
            json!({ "entries": [{ "source": "x", "dpi": [5] }] }),
            "entries[0].dpi",
        ),
        (
            json!({ "entries": one, "labels": "yes" }),
            "labels must be true or false",
        ),
        (
            json!({ "entries": one, "sizes": 1 }),
            "sizes must be true or false",
        ),
    ] {
        let result = s.tool("review_sheet", args.clone());
        assert_eq!(error_kind(&result), "invalid_argument", "{args}: {result}");
        assert!(texts(&result)[0].contains(says), "{args}: {result}");
    }
    // The sources together, not each, are held to the input cap.
    let result = s.tool(
        "review_sheet",
        json!({ "entries": [{ "source": "a".repeat(60) }, { "source": "a".repeat(60) }] }),
    );
    assert_eq!(error_kind(&result), "input_too_large", "{result}");
    assert_eq!(result["structuredContent"]["error"]["size"], 120);
    // Programs are inline: a file name is a program, never a file to read.
    let result = s.tool(
        "review_sheet",
        json!({ "entries": [{ "path": "/etc/passwd" }] }),
    );
    assert_eq!(error_kind(&result), "invalid_argument", "{result}");
    s.ping_ok();
}

/// `out_path` is the server's usual one: confined to the root, `.png` only,
/// checked before anything renders; the file holds the inline image's bytes.
#[test]
fn review_sheet_out_path_follows_the_server_rules() {
    let root = common::scratch_dir("mcp-sheet-out");
    let mut s = Server::start_in(&root, &[]);
    initialized(&mut s);
    let entries =
        json!([{ "source": mark("0.8 0.2 0.1"), "label": "x", "width": 50, "height": 50 }]);
    let result = s.tool(
        "review_sheet",
        json!({ "entries": entries, "out_path": "sheet.png" }),
    );
    assert!(texts(&result)[0].contains("sheet.png"), "{result}");
    assert_eq!(
        std::fs::read(root.join("sheet.png")).unwrap(),
        sheet_png(&result)
    );
    for bad in ["../x.png", "sheet.svg", "nodir/x.png", ".git/x.png"] {
        let result = s.tool(
            "review_sheet",
            json!({ "entries": entries, "out_path": bad }),
        );
        assert_eq!(error_kind(&result), "out_path_refused", "{bad}: {result}");
    }
    assert!(!root.join("x.png").exists());
}

/// The inline byte cap: over it the sheet is refused, and a file asked for
/// with `out_path` is still written first.
#[test]
fn review_sheet_over_the_inline_byte_cap_is_refused_and_out_path_keeps_it() {
    let root = common::scratch_dir("mcp-sheet-cap");
    let mut s = Server::start_in(&root, &["--max-image-bytes", "2000"]);
    initialized(&mut s);
    let entries = json!([{ "source": mark("0.8 0.2 0.1"), "width": 200, "height": 200 }]);
    let result = s.tool("review_sheet", json!({ "entries": entries }));
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert_eq!(result["structuredContent"]["error"]["limit_bytes"], 2000);
    let result = s.tool(
        "review_sheet",
        json!({ "entries": entries, "out_path": "kept.png" }),
    );
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    assert!(
        root.join("kept.png").exists(),
        "the file is written before the cap speaks"
    );
}

/// A label is the model's text, drawn by an interpreter inside the server.
/// It is a string literal there, escaped; a label that tries to close the
/// literal and run something draws as text, prints nothing on the protocol's
/// stdout, and gives the CLI's sheet for the same `--label`.
#[test]
fn review_sheet_labels_cannot_inject_postscript() {
    let hostile = [
        ") print (b",
        r"\) = (x",
        "(((",
        r"a\",
        "line one\nline two\r(\u{7}",
        ") pstack (",
        ") {} loop (",
    ];
    let a = mark("0.8 0.2 0.1");
    let mut s = Server::start();
    initialized(&mut s);
    for (i, label) in hostile.iter().enumerate() {
        let (cli, ok) = cli_sheet(
            &format!("mcp-sheet-inject-{i}"),
            &[("a.ps", &a)],
            &[&["--page", "50x50", "--label", label]],
        );
        assert!(ok);
        let result = s.tool(
            "review_sheet",
            json!({ "entries": [{ "source": a, "label": label }], "width": 50, "height": 50 }),
        );
        assert_eq!(sheet_png(&result), cli, "{label:?}");
    }
    s.ping_ok();
}

// --- concurrency, cancellation and render jobs (#622, #741) ---------------
//
// No assertion here is on elapsed time. Waits are a bounded number of looks
// (`eventually`), each bound far beyond what is waited for, and a render
// that must still be running is held at a FIFO the test opens, not raced.

/// Look at `cond` up to `tries` times, 50 ms apart.
fn eventually(tries: usize, mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..tries {
        if cond() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    cond()
}

/// The children of `pid` as (pid, state), zombies included: `ps`, because
/// macOS's `pgrep -P` leaves zombies out.
#[cfg(unix)]
fn children(pid: u32) -> Vec<(u32, String)> {
    let out = Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,stat="])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (f.len() >= 3 && f[1].parse::<u32>().ok() == Some(pid))
                .then(|| (f[0].parse().expect("pid"), f[2].to_string()))
        })
        .collect()
}

/// Whether `pid` is a live process. A zombie is not: on Linux `kill(pid, 0)`
/// succeeds on one, which would make a test wait on init's reaping.
#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    // Signal 0 only asks whether the process is there.
    if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
        return false;
    }
    let out = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .expect("ps");
    let stat = String::from_utf8_lossy(&out.stdout);
    !stat.trim().is_empty() && !stat.trim_start().starts_with('Z')
}

#[cfg(unix)]
fn mkfifo(path: &Path) {
    let _ = std::fs::remove_file(path);
    assert!(
        Command::new("mkfifo")
            .arg(path)
            .status()
            .expect("mkfifo")
            .success(),
        "make a FIFO"
    );
}

fn job_facts(result: &Value) -> Value {
    assert_eq!(result["isError"], false, "{result}");
    let facts = result["structuredContent"].clone();
    let text: Value = serde_json::from_str(&texts(result)[0]).expect("the text is the JSON");
    assert_eq!(text, facts, "text and structuredContent say the same");
    facts
}

fn start_job(s: &mut Server, arguments: Value) -> String {
    let facts = job_facts(&s.tool("start_render", arguments));
    assert_eq!(facts["state"], "queued", "{facts}");
    facts["job_id"].as_str().expect("job_id").to_string()
}

fn status(s: &mut Server, job: &str) -> Value {
    job_facts(&s.tool("render_status", json!({ "job_id": job })))
}

fn images(result: &Value) -> Vec<String> {
    result["content"]
        .as_array()
        .expect("content")
        .iter()
        .filter(|c| c["type"] == "image")
        .map(|c| c["data"].as_str().expect("data").to_string())
        .collect()
}

/// #622: a call that runs for minutes no longer holds the server: `ping`
/// and another call are answered beside it, and `notifications/cancelled`
/// kills its child (reaped: no zombie) long before its 600 s limit. The
/// cancelled call is never answered, as the spec asks.
#[test]
fn ping_and_other_calls_answer_during_a_long_call_and_cancel_kills_it() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-cancel-call"),
        &["--timeout", "600", "--max-steps", "0"],
    );
    initialized(&mut s);
    s.request(
        json!("long"),
        "eval_postscript",
        json!({ "source": "{ } loop" }),
    );
    s.ping_ok();
    let beside = s.tool("eval_postscript", json!({ "source": "(beside) =" }));
    assert_eq!(texts(&beside), ["beside\n"], "{beside}");
    #[cfg(unix)]
    {
        let pid = s.pid();
        assert!(
            eventually(400, || children(pid).len() == 1),
            "the long call's child runs: {:?}",
            children(pid)
        );
    }
    s.send_raw(
        br#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"long","reason":"test"}}"#,
    );
    #[cfg(unix)]
    {
        let pid = s.pid();
        assert!(
            eventually(400, || children(pid).is_empty()),
            "the cancel killed and reaped the child: {:?}",
            children(pid)
        );
    }
    s.ping_ok();
    let rest = s.close_and_drain();
    assert!(
        rest.iter().all(|m| m["id"] != "long"),
        "a cancelled call is not answered: {rest:?}"
    );
}

/// Replies from several threads at once are whole lines: renders whose
/// replies are hundreds of KB, with pings in between, all come back as
/// clean JSON-RPC (`Server::read` checks each line), each id exactly once.
#[test]
fn concurrent_replies_never_interleave() {
    let mut s = Server::start();
    initialized(&mut s);
    let noisy = "1 1 3000 { pop rand 100 mod 100 div rand 100 mod 100 div rand 100 mod 100 div \
                 setrgbcolor rand 600 mod rand 780 mod 9 9 rectfill } for showpage";
    let mut want = std::collections::BTreeSet::new();
    for i in 0..3 {
        let id = format!("render-{i}");
        s.request(json!(id), "render_postscript", json!({ "source": noisy }));
        want.insert(id);
        for j in 0..10 {
            let id = format!("ping-{i}-{j}");
            s.send_raw(
                json!({ "jsonrpc": "2.0", "id": id, "method": "ping" })
                    .to_string()
                    .as_bytes(),
            );
            want.insert(id);
        }
    }
    let mut got = std::collections::BTreeSet::new();
    let mut big = 0;
    for _ in 0..want.len() {
        let reply = s.read();
        let id = reply["id"].as_str().expect("string id").to_string();
        if id.starts_with("render-") {
            assert_eq!(reply["result"]["isError"], false, "{reply}");
            big += 1;
        }
        assert!(got.insert(id.clone()), "{id} answered twice");
    }
    assert_eq!(got, want);
    assert_eq!(big, 3);
    s.ping_ok();
}

/// #741's acceptance, on a short time scale: a render that outlasts the
/// per-call `--timeout` (1 s here) runs as a job, answers `render_status`
/// and `ping` meanwhile with its pages so far, and its result is what
/// `render_postscript` returns for the same program. The program waits on a
/// FIFO between its two pages, so it is still running for as long as the
/// test keeps the FIFO's write end open.
#[cfg(unix)]
#[test]
fn a_render_job_outlives_the_call_limit_and_is_polled_and_fetched() {
    let dir = common::scratch_dir("mcp-job-gate");
    let fifo = dir.join("gate.fifo");
    mkfifo(&fifo);
    // The write end is held open from the start, read-write so the open does
    // not wait for a reader: the render's open then returns at once and its
    // read waits for the end of the file, which is this handle's close. (A
    // writer that opens after the reader and closes at once can be missed by
    // a reader still waking in `open` on macOS, which then waits for good.)
    let gate_writer = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&fifo)
        .expect("hold the FIFO open");
    let mut s = Server::start_in(&dir, &["--timeout", "1", "--job-timeout", "120"]);
    initialized(&mut s);
    let first = "0 0 1 setrgbcolor 10 10 50 50 rectfill showpage 0 1 3000 { pop } for";
    let second = "1 0 0 setrgbcolor 20 20 40 40 rectfill showpage";
    let gate = format!("({}) (r) file pop", common::ps_path(&fifo));
    let job = start_job(
        &mut s,
        json!({ "source": format!("{first} {gate} {second}"), "width": 100, "height": 100 }),
    );
    assert!(
        eventually(400, || status(&mut s, &job)["pages_done"] == 1),
        "the first page is counted while the job waits"
    );
    assert_eq!(status(&mut s, &job)["state"], "running");
    s.ping_ok();
    // Past the per-call limit: a job has its own.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let facts = status(&mut s, &job);
    assert_eq!(facts["state"], "running", "{facts}");
    let early = s.tool("render_result", json!({ "job_id": job }));
    assert_eq!(error_kind(&early), "job_not_finished", "{early}");
    // Open the gate: closing the only writer is the end of the file.
    drop(gate_writer);
    assert!(
        eventually(600, || status(&mut s, &job)["state"] == "done"),
        "the job finishes once the gate opens: {}",
        status(&mut s, &job)
    );
    let facts = status(&mut s, &job);
    assert_eq!(facts["pages_done"], 2, "{facts}");
    let result = s.tool("render_result", json!({ "job_id": job }));
    assert_eq!(result["isError"], false, "{result}");
    let direct = s.tool(
        "render_postscript",
        json!({ "source": format!("{first} {second}"), "width": 100, "height": 100 }),
    );
    assert_eq!(images(&result).len(), 2, "{result}");
    assert_eq!(
        images(&result),
        images(&direct),
        "the same pages as render_postscript"
    );
    // The result can be fetched again until it expires.
    let again = s.tool("render_result", json!({ "job_id": job }));
    assert_eq!(images(&again), images(&result));
}

/// #741: `cancel_render` stops a running job's child long before its 600 s
/// limit; the job reads `cancelled` only once the child is reaped.
#[test]
fn cancel_render_stops_the_child() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-job-cancel"),
        &["--job-timeout", "600", "--max-steps", "0"],
    );
    initialized(&mut s);
    let job = start_job(&mut s, json!({ "source": "{ } loop" }));
    assert!(eventually(400, || status(&mut s, &job)["state"] == "running"));
    #[cfg(unix)]
    {
        let pid = s.pid();
        assert!(
            eventually(400, || children(pid).len() == 1),
            "the job's child runs"
        );
    }
    let facts = job_facts(&s.tool("cancel_render", json!({ "job_id": job })));
    assert_eq!(facts["job_id"], json!(job));
    assert!(
        eventually(400, || status(&mut s, &job)["state"] == "cancelled"),
        "cancelled: {}",
        status(&mut s, &job)
    );
    #[cfg(unix)]
    assert!(
        children(s.pid()).is_empty(),
        "cancelled means killed and reaped: {:?}",
        children(s.pid())
    );
    let result = s.tool("render_result", json!({ "job_id": job }));
    assert_eq!(error_kind(&result), "cancelled", "{result}");
    s.ping_ok();
}

/// `--max-jobs 1`: a second job waits, queued, while the first runs; a
/// queued job can be cancelled without ever starting; the slot passes on.
#[test]
fn jobs_past_the_running_cap_queue_and_can_be_cancelled_there() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-job-queue"),
        &[
            "--max-jobs",
            "1",
            "--job-timeout",
            "600",
            "--max-steps",
            "0",
        ],
    );
    initialized(&mut s);
    let small = json!({ "source": "0 0 10 10 rectfill showpage", "width": 20, "height": 20 });
    let forever = start_job(&mut s, json!({ "source": "{ } loop" }));
    assert!(eventually(400, || status(&mut s, &forever)["state"] == "running"));
    let waiting = start_job(&mut s, small.clone());
    let after = start_job(&mut s, small.clone());
    assert_eq!(status(&mut s, &waiting)["state"], "queued");
    assert_eq!(status(&mut s, &after)["state"], "queued");
    let facts = job_facts(&s.tool("cancel_render", json!({ "job_id": waiting })));
    assert_eq!(facts["cancel_requested"], true, "{facts}");
    assert!(eventually(400, || status(&mut s, &waiting)["state"] == "cancelled"));
    assert_eq!(
        status(&mut s, &after)["state"],
        "queued",
        "still behind the first"
    );
    s.tool("cancel_render", json!({ "job_id": forever }));
    assert!(
        eventually(600, || status(&mut s, &after)["state"] == "done"),
        "the freed slot goes to the next in line: {}",
        status(&mut s, &after)
    );
    // Cancelling a finished job leaves it as it is.
    let facts = job_facts(&s.tool("cancel_render", json!({ "job_id": after })));
    assert_eq!(facts["state"], "done", "{facts}");
    assert_eq!(
        images(&s.tool("render_result", json!({ "job_id": after }))).len(),
        1
    );
}

#[test]
fn job_tools_refuse_bad_arguments_and_unknown_jobs_at_once() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-job-args"),
        &["--max-image-bytes", "100"],
    );
    initialized(&mut s);
    // `render_postscript`'s checks run before a job exists.
    for (args, kind) in [
        (json!({}), "invalid_argument"),
        (
            json!({ "source": "showpage", "format": "pdf" }),
            "invalid_argument",
        ),
        (
            json!({ "source": "showpage", "format": "tiff" }),
            "invalid_argument",
        ),
        (
            json!({ "source": "showpage", "dpi": 1200, "width": 8000 }),
            "output_too_large",
        ),
        (
            json!({ "source": "showpage", "out_path": "../x.png" }),
            "out_path_refused",
        ),
    ] {
        let result = s.tool("start_render", args.clone());
        assert_eq!(error_kind(&result), kind, "{args}: {result}");
    }
    for tool in ["render_status", "render_result", "cancel_render"] {
        let result = s.tool(tool, json!({}));
        assert_eq!(error_kind(&result), "invalid_argument", "{tool}: {result}");
        let result = s.tool(tool, json!({ "job_id": "job-99" }));
        assert_eq!(error_kind(&result), "unknown_job", "{tool}: {result}");
    }
    // A program that errors is a done job, as render_postscript returns
    // its partial page with the error text.
    let job = start_job(
        &mut s,
        json!({ "source": "nosuchop", "width": 20, "height": 20 }),
    );
    assert!(eventually(600, || status(&mut s, &job)["state"] == "done"));
    let result = s.tool("render_result", json!({ "job_id": job }));
    assert_eq!(result["isError"], false, "{result}");
    assert!(
        texts(&result)[0].starts_with("PostScript error:"),
        "{result}"
    );
    // A render that fails (here: past the inline image cap) is a failed
    // job, its error in the status, its result the same error.
    let noisy =
        "1 1 200 { pop rand 255 mod 255 div setgray rand 90 mod rand 90 mod 7 7 rectfill } for";
    let job = start_job(
        &mut s,
        json!({ "source": noisy, "width": 100, "height": 100 }),
    );
    assert!(eventually(600, || status(&mut s, &job)["state"] == "failed"));
    assert_eq!(status(&mut s, &job)["error"]["kind"], "output_too_large");
    let result = s.tool("render_result", json!({ "job_id": job }));
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
}

/// `--job-ttl 1`: a finished job is gone a second after it finished.
#[test]
fn a_finished_job_expires_after_the_ttl() {
    let mut s = Server::start_in(&common::scratch_dir("mcp-job-ttl"), &["--job-ttl", "1"]);
    initialized(&mut s);
    let job = start_job(
        &mut s,
        json!({ "source": "showpage", "width": 20, "height": 20 }),
    );
    // Each look either sees the job still there or, once done, gone after
    // its second: it is never seen in any other state.
    assert!(
        eventually(600, || {
            let result = s.tool("render_status", json!({ "job_id": job }));
            if result["isError"] == true {
                assert_eq!(error_kind(&result), "unknown_job", "{result}");
                return true;
            }
            let state = &result["structuredContent"]["state"];
            assert!(
                state == "queued" || state == "running" || state == "done",
                "{result}"
            );
            false
        }),
        "the finished job expires"
    );
}

/// No job outlives the server: closing stdin (how a client stops a stdio
/// server) cancels and reaps every job before the server exits.
#[cfg(unix)]
#[test]
fn closing_stdin_stops_every_job() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-job-shutdown"),
        &["--job-timeout", "600", "--max-steps", "0"],
    );
    initialized(&mut s);
    start_job(&mut s, json!({ "source": "{ } loop" }));
    let pid = s.pid();
    assert!(eventually(400, || children(pid).len() == 1));
    let child = children(pid)[0].0;
    s.close_and_drain();
    assert!(
        eventually(400, || matches!(s.child.try_wait(), Ok(Some(_)))),
        "the server exits once stdin closes"
    );
    assert!(!process_exists(child), "the job's child went with it");
}

/// ... and a server killed outright (no chance to cancel) takes its jobs
/// with it too: each child runs with `--exit-with-parent`.
#[cfg(unix)]
#[test]
fn a_killed_server_leaves_no_render_behind() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-job-killed"),
        &["--job-timeout", "600", "--max-steps", "0"],
    );
    initialized(&mut s);
    start_job(&mut s, json!({ "source": "{ } loop" }));
    let pid = s.pid();
    assert!(eventually(400, || children(pid).len() == 1));
    let child = children(pid)[0].0;
    s.child.kill().expect("SIGKILL the server");
    s.child.wait().expect("reap the server");
    assert!(
        eventually(400, || !process_exists(child)),
        "the orphaned render stopped itself"
    );
}

/// Several starts in one write, below `--max-jobs`: every job runs. (Taking
/// a slot makes the next job first in line; without a wake-up then, a job
/// sat queued beside a free slot. Without the wake-up this failed about one
/// run in ten here; with it, never.)
#[test]
fn jobs_started_together_all_run_up_to_the_cap() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-job-together"),
        &[
            "--max-jobs",
            "4",
            "--job-timeout",
            "600",
            "--max-steps",
            "0",
        ],
    );
    initialized(&mut s);
    for i in 0..4 {
        s.request(json!(i), "start_render", json!({ "source": "{ } loop" }));
    }
    let mut jobs = Vec::new();
    for _ in 0..4 {
        let reply = s.read();
        jobs.push(
            job_facts(&reply["result"])["job_id"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    for job in &jobs {
        assert!(
            eventually(400, || status(&mut s, job)["state"] == "running"),
            "{job} runs: {}",
            status(&mut s, job)
        );
    }
    for job in &jobs {
        s.tool("cancel_render", json!({ "job_id": job }));
    }
    for job in &jobs {
        assert!(eventually(400, || status(&mut s, job)["state"] == "cancelled"));
    }
}

/// A one-shot client (one request piped in, then stdin closed) still gets
/// its reply: calls in flight finish and answer at shutdown.
#[test]
fn a_call_in_flight_at_shutdown_is_still_answered() {
    let mut s = Server::start();
    initialized(&mut s);
    s.request(
        json!("last"),
        "eval_postscript",
        json!({ "source": "(bye) =" }),
    );
    let rest = s.close_and_drain();
    let reply = rest
        .iter()
        .find(|m| m["id"] == "last")
        .unwrap_or_else(|| panic!("no reply for the last call: {rest:?}"));
    assert_eq!(texts(&reply["result"]), ["bye\n"], "{reply}");
}

// --- bands (#843) ---------------------------------------------------------

/// A band-aware program (`tests/bands.rs`' FBM_AWARE): an fbm2 field drawn
/// through a colorimage procedure that computes only the image rows its band
/// needs, one row of margin each side. No `forall` over names, so the band
/// preamble's name ids cannot reorder anything.
const BAND_AWARE: &str = "%!PS
/W 120 def /H 90 def /IX 20 def /IY 15 def /IW 240 def /IH 180 def
/rh IH H div def
/BandBBox where
  { pop /blo BandBBox 1 get rh sub def /bhi BandBBox 3 get rh add def }
  { /blo -1e9 def /bhi 1e9 def } ifelse
/row W 3 mul string def /j 0 def
/need {
  dup H exch sub rh mul IY add blo ge
  exch H exch sub 1 sub rh mul IY add bhi le and } def
IX IY translate IW IH scale
W H 8 [W 0 0 H neg 0 H]
{ j need {
    0 1 W 1 sub { /i exch def
      i 0.04 mul j 0.04 mul 5 0.5 2 fbm2 0.5 mul 0.5 add
      dup 0 lt { pop 0 } if dup 1 gt { pop 1 } if 255 mul cvi /v exch def
      row i 3 mul v put row i 3 mul 1 add 255 v sub put row i 3 mul 2 add v 2 idiv put
    } for } if
  /j j 1 add def row }
false 3 colorimage
";

/// Run a start_render job to the end and return its result.
fn job_result_of(s: &mut Server, arguments: Value) -> Value {
    let job = start_job(s, arguments);
    assert!(
        eventually(1200, || status(s, &job)["state"] == "done"),
        "the job finishes: {}",
        status(s, &job)
    );
    s.tool("render_result", json!({ "job_id": job }))
}

/// #843's acceptance test: `start_render` with `bands: 4` renders a
/// band-aware program to the same PNG bytes as `bands: 1` (a plain render).
/// The control below shows the bands really ran: a program that reads
/// `/BandCount` paints differently under bands.
#[test]
fn start_render_with_bands_matches_a_plain_render() {
    let mut s = Server::start();
    initialized(&mut s);
    let args =
        |bands: u64| json!({ "source": BAND_AWARE, "width": 280, "height": 210, "bands": bands });

    let plain = job_result_of(&mut s, args(1));
    let banded = job_result_of(&mut s, args(4));
    assert_eq!(plain["isError"], false, "{plain}");
    assert_eq!(banded["isError"], false, "{banded}");
    let (plain, banded) = (images(&plain), images(&banded));
    assert_eq!(plain.len(), 1);
    assert!(plain == banded, "bands 4 differs from bands 1");

    // The same through render_postscript.
    let call = s.tool("render_postscript", args(4));
    assert!(images(&call) == plain, "render_postscript bands 4 differs");

    // Control: the flag reaches the child.
    let probe = |s: &mut Server, bands: u64| {
        images(&s.tool(
            "render_postscript",
            json!({
                "source": "/BandCount where { pop 1 0 0 } { 0 0 1 } ifelse setrgbcolor 0 0 20 20 rectfill",
                "width": 20, "height": 20, "bands": bands
            }),
        ))
    };
    assert!(probe(&mut s, 1) != probe(&mut s, 4), "bands 4 ran banded");
}

/// A banded render has its `Lint:` block back (#847): silent when clean (the
/// `note:` line alone is not a finding), a block with the band note and the
/// finding when the page looks wrong, for both tools.
#[test]
fn a_banded_render_has_a_lint_block_only_when_there_are_findings() {
    let mut s = Server::start();
    initialized(&mut s);
    let lint_of = |s: &mut Server, tool: &str, source: &str| -> Option<String> {
        let args = json!({ "source": source, "width": 40, "height": 40, "bands": 2 });
        let result = if tool == "render_postscript" {
            s.tool(tool, args)
        } else {
            job_result_of(s, args)
        };
        assert_eq!(result["isError"], false, "{result}");
        texts(&result).into_iter().find(|t| t.starts_with("Lint:"))
    };
    for tool in ["render_postscript", "start_render"] {
        let clean = lint_of(&mut s, tool, "0 0 40 40 rectfill showpage");
        assert!(
            clean.is_none(),
            "{tool}: no Lint block when clean: {clean:?}"
        );
        let blank = lint_of(&mut s, tool, "showpage").expect("a Lint block for a blank page");
        assert!(blank.contains("blank-page"), "{tool}: {blank}");
        assert!(
            blank.contains("lint: note:"),
            "{tool} says which band: {blank}"
        );
    }
}

/// What `pscat --bands` would refuse is refused up front, naming `bands`:
/// a non-PNG format, more bands than the server's `--max-bands` or the
/// page's rows, and anything but a whole number from 1.
#[test]
fn bands_refusals_name_the_argument() {
    let mut s = Server::start_in(&common::scratch_dir("mcp-bands"), &["--max-bands", "16"]);
    initialized(&mut s);
    let refused = |s: &mut Server, extra: Value| {
        let mut args = json!({ "source": SQUARE });
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        for tool in ["render_postscript", "start_render"] {
            let result = s.tool(tool, args.clone());
            assert_eq!(error_kind(&result), "invalid_argument", "{tool}: {result}");
            let text = texts(&result).join("\n");
            assert!(text.contains("bands"), "{tool}: {text}");
        }
    };
    refused(&mut s, json!({ "bands": 2, "format": "svg" }));
    refused(&mut s, json!({ "bands": 17 }));
    refused(&mut s, json!({ "bands": 9, "width": 8, "height": 8 }));
    refused(&mut s, json!({ "bands": 0 }));
    refused(&mut s, json!({ "bands": 2.5 }));
    refused(&mut s, json!({ "bands": "2" }));

    // The cap is advertised in the schema, and 1 is fine for any format.
    let list = s.call(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    for tool in list["result"]["tools"].as_array().unwrap() {
        if ["render_postscript", "start_render"].contains(&tool["name"].as_str().unwrap()) {
            assert_eq!(tool["inputSchema"]["properties"]["bands"]["maximum"], 16);
        }
    }
    let svg = s.tool(
        "render_postscript",
        json!({ "source": SQUARE, "format": "svg", "bands": 1 }),
    );
    assert_eq!(svg["isError"], false, "{svg}");
}

/// A banded render keeps the server's bounds: the step limit stops a
/// runaway program in every band, as in a plain render (the queue stops at
/// the first band to hit it), and reads stay confined to the read roots
/// (#763). Both come back as a plain render's do: the partial page plus a
/// `PostScript error:` block.
#[test]
fn a_banded_render_keeps_the_limits_and_read_confinement() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-bands-bounds"),
        &["--max-steps", "200000"],
    );
    initialized(&mut s);
    let mut cases = vec![("0 0 20 20 rectfill { } loop", "limitexceeded (steps)")];
    if cfg!(unix) {
        cases.push((
            "0 0 20 20 rectfill (/etc/hosts) (r) file pop",
            "invalidfileaccess",
        ));
    }
    for (source, expect) in cases {
        for bands in [1, 2] {
            let result = s.tool(
                "render_postscript",
                json!({ "source": source, "width": 40, "height": 40, "bands": bands }),
            );
            let text = texts(&result).join("\n");
            assert!(
                text.contains("PostScript error:") && text.contains(expect),
                "bands {bands}: {source}: {text}"
            );
            assert_eq!(images(&result).len(), 1, "the partial page: {result}");
        }
    }
}

/// #921: with no width/height, render_postscript and start_render take an
/// EPSF file's %%BoundingBox as the page (any file's with `bbox`), and the
/// pixel cap checks that page, not the letter default.
#[test]
fn render_tools_take_the_page_from_the_bounding_box() {
    let mut s = Server::start_in(
        &common::scratch_dir("mcp-bbox"),
        &["--max-pixels", "300000"],
    );
    initialized(&mut s);
    let plain = "%!PS-Adobe-3.0\n%%BoundingBox: 0 0 400 300\n0 0 1 setrgbcolor 0 0 400 300 rectfill showpage\n";
    let eps = plain.replace("%!PS-Adobe-3.0\n", "%!PS-Adobe-3.0 EPSF-3.0\n");
    let dims = |result: &Value| {
        let png = sheet_png(result);
        let pm = tiny_skia::Pixmap::decode_png(&png).expect("decode");
        (pm.width(), pm.height())
    };
    // 400 x 300 is under the 300,000-pixel cap; letter (484,704) is not.
    let direct = s.tool("render_postscript", json!({ "source": eps }));
    assert_eq!(dims(&direct), (400, 300));
    let with_bbox = s.tool(
        "render_postscript",
        json!({ "source": plain, "bbox": true }),
    );
    assert_eq!(dims(&with_bbox), (400, 300));
    let result = s.tool("render_postscript", json!({ "source": plain }));
    assert_eq!(error_kind(&result), "output_too_large", "{result}");
    // An explicit size wins, and asking for both is refused.
    let sized = s.tool(
        "render_postscript",
        json!({ "source": eps, "width": 50, "height": 40 }),
    );
    assert_eq!(dims(&sized), (50, 40));
    let both = s.tool(
        "render_postscript",
        json!({ "source": plain, "bbox": true, "width": 50 }),
    );
    assert_eq!(error_kind(&both), "invalid_argument", "{both}");
    // A box that cannot be used is a warning beside the render.
    let bad = eps.replace("0 0 400 300\n0", "0 0 abc 300\n0");
    // (Letter at 36 dpi, 306 x 396, fits the cap.)
    let result = s.tool("render_postscript", json!({ "source": bad, "dpi": 36 }));
    assert_eq!(dims(&result), (306, 396));
    assert!(
        texts(&result)
            .iter()
            .any(|t| t.starts_with("Warnings:") && t.contains("ignoring the bounding box")),
        "{result}"
    );

    let job = start_job(&mut s, json!({ "source": eps }));
    assert!(
        eventually(600, || status(&mut s, &job)["state"] == "done"),
        "{}",
        status(&mut s, &job)
    );
    let result = s.tool("render_result", json!({ "job_id": job }));
    assert_eq!(
        images(&result),
        images(&direct),
        "the same page as render_postscript"
    );
}
