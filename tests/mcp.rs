//! pscat-mcp integration (Stage 14; protocol contract and bounds, #568):
//! drive the real binary over stdio with newline-delimited JSON-RPC, the
//! way an MCP client would.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

struct Server {
    child: Child,
    reader: BufReader<ChildStdout>,
}

impl Server {
    /// A server whose working directory (so its default output root) is a
    /// fresh scratch directory.
    fn start() -> Self {
        Self::start_in(&common::scratch_dir("mcp-cwd"), &[])
    }

    fn start_in(cwd: &Path, args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_pscat-mcp"))
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn pscat-mcp");
        let reader = BufReader::new(child.stdout.take().expect("stdout"));
        Server { child, reader }
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

    fn ping_ok(&mut self) {
        let resp = self.call(json!({ "jsonrpc": "2.0", "id": "ping-1", "method": "ping" }));
        assert_eq!(resp["id"], "ping-1");
        assert_eq!(resp["result"], json!({}));
    }
}

impl Drop for Server {
    fn drop(&mut self) {
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
            "describe_art_capabilities",
            "eval_postscript"
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

// --- resource bounds ---------------------------------------------------------

#[test]
fn a_runaway_program_times_out_and_the_server_keeps_serving() {
    let mut s = Server::start_in(&common::scratch_dir("mcp-timeout"), &["--timeout", "2"]);
    initialized(&mut s);
    let started = Instant::now();
    let result = s.tool("eval_postscript", json!({ "source": "{ } loop" }));
    let took = started.elapsed();
    assert_eq!(error_kind(&result), "timeout", "{result}");
    assert!(took < Duration::from_secs(15), "stopped promptly: {took:?}");
    let result = s.tool("render_postscript", json!({ "source": "{ } loop" }));
    assert_eq!(error_kind(&result), "timeout", "{result}");
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
