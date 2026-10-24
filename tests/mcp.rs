//! pscat-mcp integration (Stage 14; protocol contract and bounds, #568):
//! drive the real binary over stdio with newline-delimited JSON-RPC, the
//! way an MCP client would.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};

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
        Self::spawn(Command::new(env!("CARGO_BIN_EXE_pscat-mcp")), cwd, args)
    }

    /// The same server reached as `pscat mcp`, the command the MCP
    /// Registry's cargo entry runs (#569).
    fn start_subcommand() -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pscat"));
        cmd.arg("mcp");
        Self::spawn(cmd, &common::scratch_dir("mcp-sub-cwd"), &[])
    }

    fn spawn(mut cmd: Command, cwd: &Path, args: &[&str]) -> Self {
        let mut child = cmd
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
            "chalkboard_math",
            "review_sheet",
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

/// Every tool carries all five annotation hints (#687). The table is the
/// contract: a tool missing from it fails this test, so a new tool must say
/// whether it is read-only before it ships. `destructive` is false for the
/// writers on purpose (see `annotations` in src/mcp_server.rs).
#[test]
fn every_tool_has_annotations() {
    // (name, readOnlyHint)
    let expected = [
        ("render_postscript", false),
        ("handwrite", false),
        ("chalkboard_math", false),
        ("review_sheet", false),
        ("describe_art_capabilities", true),
        ("eval_postscript", true),
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
        let &(_, read_only) = expected
            .iter()
            .find(|(n, _)| *n == name)
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
        assert_eq!(a["idempotentHint"], true, "{name}: idempotentHint");
        assert_eq!(a["openWorldHint"], false, "{name}: no network, ever");
        assert_eq!(a["title"], tool["title"], "{name}: annotations.title");
    }
}

#[test]
fn pscat_mcp_subcommand_is_the_same_server() {
    let mut s = Server::start_subcommand();
    initialized(&mut s);
    let resp = s.call(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    assert_eq!(resp["result"]["tools"].as_array().expect("tools").len(), 6);
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
fn a_program_stuck_in_one_operator_is_killed_by_the_timeout_backstop() {
    // The interpreter's limits are read between steps, so a program stuck
    // inside one operator (here: opening a FIFO nobody writes to, which blocks
    // in the file read) is outside them. The server's own timeout kills the
    // child and the server keeps serving (SECURITY.md, "Limits that cannot be
    // enforced").
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
    assert_eq!(error_kind(&result), "timeout", "{result}");
    assert!(
        took < std::time::Duration::from_secs(15),
        "stopped promptly: {took:?}"
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

#[test]
fn version_flag_prints_the_crate_version_and_exits_without_serving() {
    // A closed stdin: were the flag ignored the server would see EOF and
    // exit with empty stdout, so the assertion fails instead of hanging.
    let want = format!("pscat-mcp {}\n", env!("CARGO_PKG_VERSION"));
    for flag in ["--version", "-V"] {
        let out = Command::new(env!("CARGO_BIN_EXE_pscat-mcp"))
            .arg(flag)
            .current_dir(common::scratch_dir("mcp-version"))
            .stdin(Stdio::null())
            .output()
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
    assert_eq!(fnv1a(pm.data()), 0xba9ec8c0d6b5ce8b, "the board moved");
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
    let out = cmd.output().expect("run pscat sheet");
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
            .output()
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

/// A server started where `out_path` is off still returns the sheet inline.
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
