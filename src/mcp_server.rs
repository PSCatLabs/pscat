//! pscat-mcp — pscat as MCP tools (Stage 14; hardened in #568).
//!
//! One server, two ways in: the `pscat-mcp` binary (`src/bin/pscat_mcp.rs`)
//! and `pscat mcp` (`src/main.rs`), which the MCP Registry entry runs
//! because a cargo package is launched by its crate name (#569). Both
//! binaries include this file with `#[path]` and call [`run`]; it is not
//! part of the library.
//!
//! An MCP server over stdio (newline-delimited JSON-RPC 2.0) exposing
//! five tools: `render_postscript`, `handwrite`, `chalkboard_math`,
//! `describe_art_capabilities`, `eval_postscript`. Works with any MCP
//! client — Claude Code, Codex, OpenClaw, Hermes; registration
//! one-liners are in `docs/guides/agents.md`.
//!
//! Design choice: the server **shells out to the pscat CLI** (its
//! `handwrite` subcommand included) rather than linking the interpreter.
//! PostScript programs print to stdout (`=`, `print`, `pstack`), and
//! in-process that would corrupt the JSON-RPC channel; a subprocess
//! keeps the protocol safe, keeps the tools in lock-step with the CLI,
//! and gets crash isolation for free. The pscat binary is found next
//! to this one (same directory); nothing else is needed, no checkout, no
//! `scripts/`, no shell (issue #600), so the server works from a bare
//! install on Windows too.
//!
//! **Protocol.** The legacy `initialize` handshake, for the revisions in
//! [`SUPPORTED_VERSIONS`] only (the ones real clients were tested with:
//! Claude Code sends 2025-11-25, Codex 2025-06-18). A client asking for
//! another well-formed revision is offered the newest supported one, as
//! the spec's negotiation requires; a missing or malformed version is
//! refused with the supported list. The stateless 2026-07-28 revision is
//! not implemented: its `server/discover` probe gets "method not found",
//! which is what tells a dual-era client to fall back to `initialize`.
//!
//! **Bounds** (issue #568, `SECURITY.md`): every child run has a
//! wall-clock timeout and a cap on captured output; `source`/`text` have
//! a size cap; inline images have a byte and page cap; `out_path` is
//! confined to one output root (render extensions only, no dot-directories)
//! and written by rename, never by following a symlink. Nothing but JSON-RPC is ever written to stdout: diagnostics
//! go to stderr, and child output is always captured through pipes.

use std::io::{BufRead, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

/// Protocol revisions this server implements, newest first. Only add one
/// after testing it against a real client (`tests/mcp.rs`, the notes).
const SUPPORTED_VERSIONS: [&str; 2] = ["2025-11-25", "2025-06-18"];

/// Cap on one child stream (stdout or stderr). `--capabilities` prints
/// about 280 KB, so this leaves room; a program printing in a loop is
/// stopped when it passes this.
const MAX_CHILD_OUTPUT: usize = 4 << 20;

/// Pages returned inline by one call (more: write them with `out_path`).
const MAX_INLINE_PAGES: usize = 20;

const USAGE: &str = "usage: pscat-mcp [options]
   or: pscat mcp [options]

An MCP server on stdin/stdout (launched by an MCP client, not by hand).

  --out-root DIR         directory out_path is confined to (default: the
                         working directory; out_path is off when that is /)
  --timeout SECS         wall-clock limit per tool call (default 60)
  --max-input-bytes N    largest source/text accepted (default 1048576)
  --max-image-bytes N    largest total of inline image bytes per call
                         (default 3750000, about 5 MB once base64-encoded)
  --max-steps N          stop a program after N interpreter steps (default
                         25000000000; 0 = no step limit)
  --max-memory MIB       stop a program that grows the heap by more than MIB
                         (default 1024; 0 = no memory limit)
  -h, --help             this text
  -V, --version          print the version";

/// The limits and the output root, fixed for the server's lifetime by
/// whoever registered it. No tool argument can change them.
struct Config {
    /// Canonical output root, or why `out_path` is unavailable.
    out_root: Result<PathBuf, String>,
    timeout: Duration,
    max_input: usize,
    max_image: usize,
    /// Interpreter limits passed to every `pscat` child (issue #604); 0 is
    /// off. The child's time limit is derived from `timeout`, see
    /// `Config::interpreter_args`.
    max_steps: u64,
    max_memory_mib: u64,
}

impl Config {
    /// The flags that put the interpreter's own limits on a child run. The
    /// time limit is `--timeout` less a margin of a quarter of it, at most
    /// 5 s (55 s of the default 60): the interpreter stops first, with a
    /// structured `limit_exceeded` error and the partial page, and the hard
    /// kill at `--timeout` is only the backstop for a child stuck inside one
    /// long operator. The margin pays for starting the child, writing the
    /// canvas and the lint pass after the limit fires.
    fn interpreter_args(&self, cmd: &mut Command) {
        let total = self.timeout.as_secs_f64();
        let secs = total - (total / 4.0).min(5.0);
        cmd.arg("--max-time").arg(format!("{secs:.3}"));
        cmd.arg("--max-steps").arg(self.max_steps.to_string());
        cmd.arg("--max-memory").arg(self.max_memory_mib.to_string());
    }

    /// `Ok(None)`: `--help`/`--version` was handled, exit 0.
    fn from_args(args: impl Iterator<Item = String>) -> Result<Option<Config>, String> {
        let mut out_root: Option<String> = None;
        let mut timeout = 60u64;
        let mut max_input = 1usize << 20;
        let mut max_image = 3_750_000usize;
        let mut max_steps = pscat::limits::UNTRUSTED_MAX_STEPS;
        let mut max_memory_mib = (pscat::limits::UNTRUSTED_MAX_MEMORY >> 20) as u64;
        let mut args = args;
        while let Some(arg) = args.next() {
            let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
            let number = |name: &str, v: String, lo: u64, hi: u64| -> Result<u64, String> {
                match v.parse::<u64>() {
                    Ok(n) if (lo..=hi).contains(&n) => Ok(n),
                    _ => Err(format!(
                        "{name} must be a whole number {lo}..{hi}, got {v:?}"
                    )),
                }
            };
            match arg.as_str() {
                "-h" | "--help" => {
                    println!("{USAGE}");
                    return Ok(None);
                }
                "-V" | "--version" => {
                    println!("pscat-mcp {}", env!("CARGO_PKG_VERSION"));
                    return Ok(None);
                }
                "--out-root" => out_root = Some(value("--out-root")?),
                "--timeout" => timeout = number("--timeout", value("--timeout")?, 1, 86_400)?,
                "--max-input-bytes" => {
                    max_input =
                        number("--max-input-bytes", value("--max-input-bytes")?, 1, 1 << 30)?
                            as usize
                }
                "--max-image-bytes" => {
                    max_image =
                        number("--max-image-bytes", value("--max-image-bytes")?, 1, 1 << 30)?
                            as usize
                }
                "--max-steps" => {
                    max_steps = number("--max-steps", value("--max-steps")?, 0, u64::MAX)?
                }
                "--max-memory" => {
                    max_memory_mib = number("--max-memory", value("--max-memory")?, 0, 1 << 30)?
                }
                other => return Err(format!("unknown option {other:?}")),
            }
        }
        let out_root = match out_root {
            Some(dir) => {
                Ok(std::fs::canonicalize(&dir).map_err(|e| format!("--out-root {dir:?}: {e}"))?)
            }
            None => default_out_root(),
        };
        if let Ok(root) = &out_root
            && !root.is_dir()
        {
            return Err(format!("--out-root {} is not a directory", root.display()));
        }
        Ok(Some(Config {
            out_root,
            timeout: Duration::from_secs(timeout),
            max_input,
            max_image,
            max_steps,
            max_memory_mib,
        }))
    }
}

/// The working directory the client started us in (Claude Code and Codex
/// use the project directory). A client that starts servers in `/`
/// (some desktop apps do) would make that the whole disk, so `out_path`
/// is switched off there until `--out-root` names a directory.
fn default_out_root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir()
        .and_then(std::fs::canonicalize)
        .map_err(|e| format!("out_path is unavailable: working directory: {e}"))?;
    if cwd.parent().is_none() {
        return Err(format!(
            "out_path is unavailable: the server was started in {}; register it with \
             --out-root DIR to allow writing files",
            cwd.display()
        ));
    }
    Ok(cwd)
}

/// Serve MCP on stdin/stdout until stdin closes. `args` are the options
/// after the program name (`pscat-mcp`) or the subcommand (`pscat mcp`).
pub fn run(args: impl Iterator<Item = String>) -> ExitCode {
    let cfg = match Config::from_args(args) {
        Ok(Some(cfg)) => cfg,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pscat-mcp: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    // A JSON-escaped source can be up to 6x its size (`\u0001`), plus the
    // envelope. Past this the line is dropped unread; under it, an
    // oversized `source` still gets a proper tool error on its own id.
    let max_line = cfg.max_input.saturating_mul(6).saturating_add(64 << 10);
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    loop {
        let line = match read_line_bounded(&mut input, max_line) {
            Ok(Line::Eof) | Err(_) => break,
            Ok(Line::TooLong) => {
                send(&error(
                    Value::Null,
                    -32600,
                    &format!("request line longer than {max_line} bytes; dropped"),
                    None,
                ));
                continue;
            }
            Ok(Line::Data(bytes)) => bytes,
        };
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if let Some(reply) = handle_message(&cfg, &line) {
            send(&reply);
        }
    }
    ExitCode::SUCCESS
}

fn send(reply: &Value) {
    // serde_json escapes every control character, so one reply is one line.
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{reply}");
    let _ = out.flush();
}

enum Line {
    Eof,
    TooLong,
    Data(Vec<u8>),
}

/// One `\n`-terminated line as bytes (UTF-8 is checked by the JSON parse,
/// so a bad byte is a parse error, not the end of the session). A line
/// longer than `max` is consumed without being buffered.
fn read_line_bounded(r: &mut impl BufRead, max: usize) -> std::io::Result<Line> {
    let mut buf = Vec::new();
    let mut too_long = false;
    let mut any = false;
    loop {
        let avail = r.fill_buf()?;
        if avail.is_empty() {
            return Ok(match (any, too_long) {
                (false, _) => Line::Eof,
                (true, true) => Line::TooLong,
                (true, false) => Line::Data(buf),
            });
        }
        any = true;
        let newline = avail.iter().position(|&b| b == b'\n');
        let chunk_len = newline.unwrap_or(avail.len());
        if !too_long {
            if buf.len() + chunk_len > max {
                too_long = true;
                buf = Vec::new();
            } else {
                buf.extend_from_slice(&avail[..chunk_len]);
            }
        }
        let consumed = newline.map_or(avail.len(), |i| i + 1);
        r.consume(consumed);
        if newline.is_some() {
            if too_long {
                return Ok(Line::TooLong);
            }
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
            return Ok(Line::Data(buf));
        }
    }
}

/// The reply to one line, or `None` when JSON-RPC says to stay silent
/// (notifications, and responses to requests we never send).
fn handle_message(cfg: &Config, line: &[u8]) -> Option<Value> {
    let msg: Value = match serde_json::from_slice(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(error(
                Value::Null,
                -32700,
                &format!("parse error: {e}"),
                None,
            ));
        }
    };
    let Some(obj) = msg.as_object() else {
        let what = if msg.is_array() {
            "batched requests are not supported"
        } else {
            "a request must be a JSON object"
        };
        return Some(error(Value::Null, -32600, what, None));
    };
    let Some(method) = obj.get("method") else {
        // A response (`result`/`error`) to the client's side of the
        // conversation: this server sends no requests, so nothing to do.
        if obj.contains_key("result") || obj.contains_key("error") {
            return None;
        }
        let id = valid_id(obj).unwrap_or(Value::Null);
        return Some(error(id, -32600, "missing method", None));
    };
    let id = match obj.get("id") {
        None => return None, // a notification
        Some(_) => match valid_id(obj) {
            Some(id) => id,
            None => {
                return Some(error(
                    Value::Null,
                    -32600,
                    "id must be a string or a number",
                    None,
                ));
            }
        },
    };
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(error(id, -32600, "jsonrpc must be \"2.0\"", None));
    }
    let Some(method) = method.as_str() else {
        return Some(error(id, -32600, "method must be a string", None));
    };
    let empty = Value::Object(Map::new());
    let params = match obj.get("params") {
        None => &empty,
        Some(p) if p.is_object() => p,
        Some(_) => return Some(error(id, -32602, "params must be an object", None)),
    };
    Some(match method {
        "initialize" => match initialize_result(cfg, params) {
            Ok(result) => ok(id, result),
            Err((message, data)) => error(id, -32602, &message, Some(data)),
        },
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, json!({ "tools": tool_list(cfg) })),
        "tools/call" => tools_call(cfg, id, params),
        // `server/discover` (2026-07-28) lands here on purpose: a plain
        // "method not found" marks this as a legacy server.
        _ => error(id, -32601, &format!("method not found: {method}"), None),
    })
}

fn valid_id(obj: &Map<String, Value>) -> Option<Value> {
    obj.get("id")
        .filter(|id| id.is_string() || id.is_number())
        .cloned()
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut err = json!({ "code": code, "message": message });
    if let Some(data) = data {
        err["data"] = data;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": err })
}

/// The `initialize` result, or the `-32602` message and data. Supported
/// revision: answered as asked. Another well-formed revision (`YYYY-MM-DD`):
/// our newest, and the client decides whether it can speak it. Anything
/// else: refused with the list.
fn initialize_result(cfg: &Config, params: &Value) -> Result<Value, (String, Value)> {
    let requested = params.get("protocolVersion");
    let version = match requested.and_then(Value::as_str) {
        Some(v) if SUPPORTED_VERSIONS.contains(&v) => v,
        Some(v) if is_revision(v) => SUPPORTED_VERSIONS[0],
        _ => {
            return Err((
                "Unsupported protocol version".into(),
                json!({
                    "supported": SUPPORTED_VERSIONS,
                    "requested": requested.cloned().unwrap_or(Value::Null),
                }),
            ));
        }
    };
    let out_path = match &cfg.out_root {
        Ok(root) => format!(
            "out_path writes only inside {} (relative paths start there), only to a name \
             ending in the output format's extension (.png, .svg, .pdf), and never into \
             a dot-directory.",
            root.display()
        ),
        Err(why) => why.clone(),
    };
    Ok(json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "pscat",
            "title": "pscat PostScript interpreter",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": format!(
            "Render and debug PostScript. render_postscript returns pages as PNG; \
             eval_postscript returns what a program printed or its PostScript error; \
             chalkboard_math writes a LaTeX formula on a chalkboard. \
             Each call is limited to {}s. {out_path}",
            cfg.timeout.as_secs()
        ),
    }))
}

/// `YYYY-MM-DD`, the shape of every MCP revision identifier.
fn is_revision(v: &str) -> bool {
    let b = v.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

fn tools_call(cfg: &Config, id: Value, params: &Value) -> Value {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return error(id, -32602, "tools/call needs a tool name", None);
    };
    let tool: fn(&Config, &Value) -> Result<Value, ToolError> = match name {
        "render_postscript" => render_postscript,
        "handwrite" => handwrite,
        "chalkboard_math" => chalkboard_math,
        "describe_art_capabilities" => |cfg, _| describe_art_capabilities(cfg),
        "eval_postscript" => eval_postscript,
        other => return error(id, -32602, &format!("unknown tool: {other}"), None),
    };
    let empty = Value::Object(Map::new());
    let args = params.get("arguments").unwrap_or(&empty);
    let result = if args.is_object() {
        tool(cfg, args)
    } else {
        Err(ToolError::invalid("arguments must be an object"))
    };
    match result {
        Ok(content) => ok(id, json!({ "content": content, "isError": false })),
        Err(e) => ok(id, e.into_result()),
    }
}

/// A tool failure: `isError: true`, the message as text (what a model
/// reads), and the same facts as `structuredContent.error` for a client
/// that wants fields. Only error results carry `structuredContent`, so
/// it never stands in for an image.
struct ToolError {
    kind: &'static str,
    message: String,
    fields: Map<String, Value>,
}

impl ToolError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        ToolError {
            kind,
            message: message.into(),
            fields: Map::new(),
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_argument", message)
    }

    fn with(mut self, key: &str, value: Value) -> Self {
        self.fields.insert(key.into(), value);
        self
    }

    fn into_result(self) -> Value {
        let mut error = self.fields;
        error.insert("kind".into(), json!(self.kind));
        error.insert("message".into(), json!(self.message));
        json!({
            "content": [text_content(&self.message)],
            "structuredContent": { "error": error },
            "isError": true,
        })
    }
}

impl From<String> for ToolError {
    fn from(message: String) -> Self {
        ToolError::new("internal", message)
    }
}

fn text_content(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

fn image_content(png: &[u8]) -> Value {
    json!({ "type": "image", "data": base64(png), "mimeType": "image/png" })
}

/// MCP tool annotations (#687). All five fields are set explicitly: the spec's
/// defaults (`readOnlyHint` false, `destructiveHint` true, `openWorldHint`
/// true) are the pessimistic ones, so leaving a field out is not neutral.
/// `destructiveHint` is false even for the writers: `out_path` is confined to
/// the output root and replaces only an existing render file of the same
/// extension. `idempotentHint` is true everywhere: the same arguments produce
/// the same output, and a repeated write replaces the same file. `openWorldHint`
/// is false: no tool touches the network. The hints are static per tool, so a
/// tool whose behaviour depends on an argument (`out_path`) gets the
/// conservative `readOnlyHint: false`. A new tool must call this and add its
/// row to the table in `tests/mcp.rs`.
fn annotations(title: &str, read_only: bool) -> Value {
    json!({
        "title": title,
        "readOnlyHint": read_only,
        "destructiveHint": false,
        "idempotentHint": true,
        "openWorldHint": false
    })
}

fn tool_list(cfg: &Config) -> Value {
    let out_path = match &cfg.out_root {
        Ok(root) => format!(
            "Also write the file here: a path inside {} (relative paths start there; \
             the directory must exist and not start with a dot; the name must end in the \
             output format's extension, .png for handwrite; an existing file with that \
             extension is replaced). ",
            root.display()
        ),
        Err(why) => format!("{why}. "),
    };
    let limits = format!(
        "Limits: {}s per call, {} bytes of input. The interpreter stops a runaway program \
         (a limit_exceeded error naming steps, time or memory); stopped cannot catch it.",
        cfg.timeout.as_secs(),
        cfg.max_input
    );
    json!([
        {
            "name": "render_postscript",
            "title": "Render PostScript",
            "annotations": annotations("Render PostScript", false),
            "description": format!("Render a PostScript program. Returns the page(s) as PNG images \
                (default), or SVG text, or writes a PDF to out_path. Partial renders are \
                returned even when the program errors, with the error text alongside, plus \
                a Lint: block when the page looks wrong (blank page, unbalanced gsave, \
                leftover stack). {limits} Inline images are capped at {} bytes in total \
                and {MAX_INLINE_PAGES} pages; past that, lower dpi/size or use out_path. \
                Writes a file only when out_path is given (then it replaces an existing render \
                file of that extension), which is why this tool is not marked read-only. The \
                program can read any local file you can.",
                cfg.max_image),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "The PostScript program" },
                    "width": { "type": "integer", "minimum": 1, "maximum": 8000, "description": "Page width in points (default 612)" },
                    "height": { "type": "integer", "minimum": 1, "maximum": 8000, "description": "Page height in points (default 792)" },
                    "dpi": { "type": "number", "minimum": 9, "maximum": 1200, "description": "Device resolution (default 72; 144 = 2x pixels)" },
                    "format": { "type": "string", "enum": ["png", "svg", "pdf"], "description": "Output format (default png)" },
                    "halftone": { "type": "boolean", "description": "Screen the raster like a mono laser printer (png only)" },
                    "out_path": { "type": "string", "description": format!("{out_path}Required for pdf. A multi-page render writes name-001.ext, name-002.ext, ...") }
                },
                "required": ["source"]
            }
        },
        {
            "name": "handwrite",
            "title": "Handwritten note",
            "annotations": annotations("Handwritten note", false),
            "description": format!("Render text as a handwritten note (PNG): a dynamic pen-written \
                font with per-letter variation, word-wrapped, auto-sized page. \
                Lowercase letters, digits and .,-'!? only; anything else is invisible. \
                Writes a file only when out_path is given (then it replaces an existing .png), \
                which is why this tool is not marked read-only. {limits}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "The note to write (newlines force line breaks)" },
                    "size": { "type": "number", "description": "Font size in points (default 36)" },
                    "width": { "type": "integer", "minimum": 1, "maximum": 8000, "description": "Page width in points (default 612)" },
                    "paper": { "type": "string", "enum": ["plain", "ruled"], "description": "Paper style (default plain)" },
                    "ink": { "type": "string", "description": "Ink color as R,G,B in 0..1 (default 0.13,0.14,0.34)" },
                    "jitter": { "type": "number", "description": "Ink wobble; 0 = calm (default 16)" },
                    "seed": { "type": "integer", "description": "Random seed; same seed, same page (default 1985)" },
                    "dpi": { "type": "number", "minimum": 9, "maximum": 1200, "description": "Render resolution (default 72)" },
                    "out_path": { "type": "string", "description": out_path.trim_end() }
                },
                "required": ["text"]
            }
        },
        {
            "name": "chalkboard_math",
            "title": "Chalkboard math",
            "annotations": annotations("Chalkboard math", false),
            "description": format!("Write a LaTeX formula on a chalkboard: hand-drawn chalk \
                strokes on a slate with eraser marks and the ghost of an earlier lecture. \
                Returns the board as a PNG image. Type the LaTeX as in a document, single \
                backslashes: \\\\ breaks rows, & aligns them, and an align*/equation \
                wrapper or $...$ is fine. The subset is mathkit's (fractions, roots, sums, \
                integrals, Greek, matrices, cases, accents...); a command outside it is a \
                latex_error naming it. Same input and seed: the same image, byte for byte. \
                The input is LaTeX only, never PostScript. svg and pdf (megabytes: tens of \
                thousands of grain marks) are written to out_path only. Writes a file only \
                when out_path is given (then it replaces an existing file of that \
                extension), which is why this tool is not marked read-only. Limits: \
                latex at most {MAX_LATEX_BYTES} bytes; the board at most {MAX_CHALK_PIXELS} \
                pixels (width and height in points times dpi/72) and 8000 a side; \
                {}s per call; the interpreter's step, time and memory limits \
                (limit_exceeded). Inline images are capped at {} bytes: past that, lower \
                dpi or use out_path.", cfg.timeout.as_secs(), cfg.max_image),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "latex": { "type": "string", "description": format!("The formula, as typed in a LaTeX document (single backslashes); at most {MAX_LATEX_BYTES} bytes") },
                    "size": { "type": "number", "exclusiveMinimum": 0, "maximum": 1000, "description": "Writing size in points, before any shrink to fit the board (default 64)" },
                    "irregular": { "type": "number", "minimum": 0, "maximum": 5, "description": "How far the layout departs from clean TeX: 0 clean and level, 1 a steady hand (default 1)" },
                    "slope": { "type": "number", "minimum": -45, "maximum": 45, "description": "Baseline tilt in degrees (default: a small one picked from the seed)" },
                    "jitter": { "type": "number", "minimum": 0, "maximum": 0.5, "description": "Each glyph's wobble as a fraction of the size (default 0.016)" },
                    "align": { "type": "string", "enum": ["center", "left"], "description": "Where the lines sit across the board (default center)" },
                    "color": { "type": "string", "enum": pscat::chalk::CHALK_COLORS, "description": "Chalk colour (default white)" },
                    "board": {
                        "description": "The board: true (default, a slate), false (chalk alone on a transparent page), \"slate\", \"greenboard\", [r, g, b] in 0..1, or an object of board keys (tone, haze, ghosting, swipes, wipes, smudges, ledge)",
                        "anyOf": [
                            { "type": "boolean" },
                            { "type": "string", "enum": ["slate", "greenboard"] },
                            { "type": "array", "items": { "type": "number", "minimum": 0, "maximum": 1 }, "minItems": 3, "maxItems": 3 },
                            { "type": "object" }
                        ]
                    },
                    "ghosting": { "type": ["number", "boolean"], "minimum": 0, "maximum": 1, "description": "How much of an earlier lecture's erased writing shows, 0..1 (default 0.4; false = none). Needs a board" },
                    "seed": { "type": "integer", "minimum": -1_000_000_000, "maximum": 1_000_000_000, "description": "Seed for the layout, the chalk and the board; same seed, same image (default 1)" },
                    "width": { "type": "integer", "minimum": 1, "maximum": 8000, "description": "Board width in points (default 800)" },
                    "height": { "type": "integer", "minimum": 1, "maximum": 8000, "description": "Board height in points (default 400)" },
                    "dpi": { "type": "number", "minimum": 9, "maximum": 1200, "description": "Resolution (default 144: the default board is 1600x800 pixels)" },
                    "format": { "type": "string", "enum": ["png", "svg", "pdf"], "description": "Output format (default png; svg and pdf need out_path)" },
                    "config": { "type": "object", "description": "The full board config, as pscat chalk --json takes it: groups layout (size, irregular, slope, jitter, align, margin, width, height, dpi), chalk (color, strength, pressure, tilt, dust, width), board (or false), and seed. The keys above are laid over it" },
                    "out_path": { "type": "string", "description": out_path.trim_end() }
                },
                "required": ["latex"]
            }
        },
        {
            "name": "describe_art_capabilities",
            "title": "List art capabilities",
            "annotations": annotations("List art capabilities", true),
            "description": "List the creative capabilities this pscat build actually has installed: fonts, \
                Type 3 program faces, mood palettes, page templates, and artkit/style-pack procedures. \
                Structured JSON, safe to call instead of assuming names from documentation, which can \
                drift from what's really loadable.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        },
        {
            "name": "eval_postscript",
            "title": "Run PostScript",
            "annotations": annotations("Run PostScript", true),
            "description": format!("Run a PostScript program headlessly and return what it printed \
                (=, print, pstack). On error: the standard PostScript error name, the offending \
                command and line, plus a gs-style operand-stack post-mortem. The debugging \
                loop. Writes nothing, but the program can read any local file you can. {limits}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "The PostScript program" }
                },
                "required": ["source"]
            }
        }
    ])
}

// --- argument checks ------------------------------------------------

fn required_text<'a>(cfg: &Config, args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    let s = match args.get(key) {
        None | Some(Value::Null) => return Err(ToolError::invalid(format!("{key} is required"))),
        Some(Value::String(s)) => s.as_str(),
        Some(_) => return Err(ToolError::invalid(format!("{key} must be a string"))),
    };
    if s.len() > cfg.max_input {
        return Err(ToolError::new(
            "input_too_large",
            format!(
                "{key} is {} bytes; this server accepts at most {}",
                s.len(),
                cfg.max_input
            ),
        )
        .with("limit", json!(cfg.max_input))
        .with("size", json!(s.len())));
    }
    Ok(s)
}

fn optional_str<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(ToolError::invalid(format!("{key} must be a string"))),
    }
}

fn optional_number(args: &Value, key: &str, lo: f64, hi: f64) -> Result<Option<f64>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_f64() {
            Some(n) if (lo..=hi).contains(&n) => Ok(Some(n)),
            _ => Err(ToolError::invalid(format!(
                "{key} must be a number from {lo} to {hi}, got {v}"
            ))),
        },
    }
}

fn optional_int(args: &Value, key: &str, lo: i64, hi: i64) -> Result<Option<i64>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_i64() {
            Some(n) if (lo..=hi).contains(&n) => Ok(Some(n)),
            _ => Err(ToolError::invalid(format!(
                "{key} must be a whole number from {lo} to {hi}, got {v}"
            ))),
        },
    }
}

fn optional_bool(args: &Value, key: &str) -> Result<Option<bool>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(ToolError::invalid(format!("{key} must be true or false"))),
    }
}

/// A number as the CLI wants it: integers without a trailing `.0`.
fn number_arg(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

// --- the tools ------------------------------------------------------

fn render_postscript(cfg: &Config, args: &Value) -> Result<Value, ToolError> {
    let source = required_text(cfg, args, "source")?;
    let format = optional_str(args, "format")?.unwrap_or("png");
    let flag = match format {
        "png" => "--png",
        "svg" => "--svg",
        "pdf" => "--pdf",
        other => {
            return Err(ToolError::invalid(format!(
                "format must be png, svg or pdf, got {other:?}"
            )));
        }
    };
    let width = optional_int(args, "width", 1, 8000)?.unwrap_or(612);
    let height = optional_int(args, "height", 1, 8000)?.unwrap_or(792);
    let dpi = optional_number(args, "dpi", 9.0, 1200.0)?;
    let halftone = optional_bool(args, "halftone")?.unwrap_or(false);
    // Checked before rendering, so a refused path costs nothing.
    let dest = optional_str(args, "out_path")?
        .map(|p| confine(cfg, p, format))
        .transpose()?;
    if format == "pdf" && dest.is_none() {
        return Err(ToolError::invalid(
            "pdf output needs out_path (a PDF is a file, not a chat message)",
        ));
    }

    let scratch = ScratchDir::new()?;
    let out = scratch.path().join(format!("page.{format}"));
    let mut cmd = Command::new(pscat_bin()?);
    cfg.interpreter_args(&mut cmd);
    cmd.arg("--page").arg(format!("{width}x{height}"));
    if let Some(dpi) = dpi {
        cmd.arg("--dpi").arg(number_arg(dpi));
    }
    if halftone {
        cmd.arg("--halftone");
    }
    cmd.arg(flag).arg(&out).arg("-").arg("--lint");
    let run = run_child(cfg, &mut cmd, source.as_bytes())?;
    let (lint_findings, other_stderr) = split_lint_findings(&run.stderr);

    let pages = collect_pages(&out);
    if pages.is_empty() {
        return Err(failed_run(
            "no_output",
            format!(
                "no output produced\nstdout: {}\nstderr: {}",
                run.stdout, run.stderr
            ),
            &run.stderr,
        ));
    }
    let written = match &dest {
        Some(dest) => Some(dest.write_pages(&pages)?),
        None => None,
    };

    // Artifacts are written even after a PostScript error (partial
    // canvas); pass the error text along rather than hiding the render.
    let mut content = Vec::new();
    if !run.success {
        content.push(text_content(&format!("PostScript error:\n{other_stderr}")));
    }
    // Silent when clean (every render would otherwise carry a "nothing's
    // wrong" line) — only speak up when the lint pass actually found
    // something (issue #17).
    if !lint_findings.is_empty() {
        content.push(text_content(&format!("Lint:\n{lint_findings}")));
    }
    if format != "pdf" {
        for body in read_inline(cfg, &pages, written.as_deref())? {
            content.push(match format {
                "png" => image_content(&body),
                _ => text_content(&String::from_utf8_lossy(&body)),
            });
        }
    }
    if let Some(written) = written {
        content.push(text_content(&wrote_message(&written)));
    }
    Ok(Value::Array(content))
}

fn handwrite(cfg: &Config, args: &Value) -> Result<Value, ToolError> {
    let text = required_text(cfg, args, "text")?;
    let dest = optional_str(args, "out_path")?
        .map(|p| confine(cfg, p, "png"))
        .transpose()?;
    let scratch = ScratchDir::new()?;
    let out = scratch.path().join("note.png");

    let mut cmd = Command::new(pscat_bin()?);
    cmd.arg("handwrite").arg("-o").arg(&out);
    for (flag, key, lo, hi) in [
        ("--size", "size", 0.0, 2000.0),
        ("--jitter", "jitter", 0.0, 10_000.0),
        ("--dpi", "dpi", 9.0, 1200.0),
    ] {
        if let Some(n) = optional_number(args, key, lo, hi)? {
            cmd.arg(flag).arg(number_arg(n));
        }
    }
    if let Some(w) = optional_int(args, "width", 1, 8000)? {
        cmd.arg("--width").arg(w.to_string());
    }
    // The library's `srand` wants an integer: `3.0` would be a real.
    if let Some(seed) = optional_int(args, "seed", i64::from(i32::MIN), i64::from(i32::MAX))? {
        cmd.arg("--seed").arg(seed.to_string());
    }
    if let Some(paper) = optional_str(args, "paper")? {
        if paper != "plain" && paper != "ruled" {
            return Err(ToolError::invalid(format!(
                "paper must be plain or ruled, got {paper:?}"
            )));
        }
        cmd.arg("--paper").arg(paper);
    }
    if let Some(ink) = optional_str(args, "ink")? {
        cmd.arg("--ink").arg(ink);
    }
    // `--` so a text that starts with `-` is text, not an option.
    cmd.arg("--").arg(text);
    let run = run_child(cfg, &mut cmd, b"")?;
    if !run.success {
        return Err(ToolError::new(
            "handwrite_failed",
            format!("handwrite failed:\n{}", run.stderr),
        ));
    }
    let pages = collect_pages(&out);
    if pages.is_empty() {
        return Err(ToolError::new("no_output", "handwrite produced no image"));
    }
    let written = match &dest {
        Some(dest) => Some(dest.write_pages(&pages)?),
        None => None,
    };
    let bodies = read_inline(cfg, &pages, written.as_deref())?;
    let mut content: Vec<Value> = bodies.iter().map(|b| image_content(b)).collect();
    if let Some(written) = written {
        content.push(text_content(&wrote_message(&written)));
    }
    Ok(Value::Array(content))
}

/// The longest `latex` `chalkboard_math` takes, in bytes. A ten-row
/// derivation is about 1 KB and a board that size is already crowded; four
/// times that (about 40 rows of a Taylor expansion) rendered in 25 s on the
/// machine it was measured on with the default board, inside the default
/// time limit (#688); past that the time limit stops it.
const MAX_LATEX_BYTES: usize = 4096;

/// The largest `chalkboard_math` board, in device pixels (width x height at
/// the chosen dpi). An 8K screen (7680x4320) or an A2 poster at 300 dpi
/// (about 35 million) fits; a 1920x1200 pt board at 300 dpi (40 million)
/// peaked at 0.63 GB resident under the server's limits (#688). The
/// interpreter's own ceiling, 8000 pixels a side (64 million), still applies;
/// this cap is headroom on memory and time below it (the canvas is allocated
/// before `--max-memory` starts counting).
const MAX_CHALK_PIXELS: u64 = 40_000_000;

/// The flat `chalkboard_math` keys that map onto one key of the #555 config:
/// (argument, group, key).
const CHALK_FLAT_KEYS: [(&str, &str, &str); 9] = [
    ("size", "layout", "size"),
    ("irregular", "layout", "irregular"),
    ("slope", "layout", "slope"),
    ("jitter", "layout", "jitter"),
    ("align", "layout", "align"),
    ("width", "layout", "width"),
    ("height", "layout", "height"),
    ("dpi", "layout", "dpi"),
    ("color", "chalk", "color"),
];

fn present<'a>(args: &'a Value, key: &str) -> Option<&'a Value> {
    args.get(key).filter(|v| !v.is_null())
}

/// The board's config: `config` (the `pscat chalk --json` schema) first,
/// then the flat keys over it. A flat key's error names the flat key.
fn chalk_config(args: &Value) -> Result<pscat::chalk::Config, ToolError> {
    use pscat::chalk::{ChalkError, Config};
    let mut cfg = match present(args, "config") {
        None => Config::default(),
        Some(v @ Value::Object(_)) => {
            Config::from_value(v).map_err(|e| ToolError::invalid(e.to_string()))?
        }
        Some(_) => return Err(ToolError::invalid("config must be an object")),
    };
    // `path` is how the config reader names the key; the message says `arg`.
    fn overlay(cfg: &mut Config, arg: &str, path: &str, v: Value) -> Result<(), ToolError> {
        cfg.apply(&v).map_err(|e| match e {
            ChalkError::Config(m) => ToolError::invalid(m.replace(path, arg)),
            e => ToolError::invalid(e.to_string()),
        })
    }
    for (arg, group, key) in CHALK_FLAT_KEYS {
        if let Some(v) = present(args, arg) {
            overlay(
                &mut cfg,
                arg,
                &format!("{group}.{key}"),
                json!({ group: { key: v } }),
            )?;
        }
    }
    if let Some(b) = present(args, "board") {
        match b {
            Value::String(_) | Value::Array(_) => overlay(
                &mut cfg,
                "board",
                "board.tone",
                json!({ "board": { "tone": b } }),
            )?,
            Value::Bool(_) | Value::Object(_) => {
                overlay(&mut cfg, "board", "board", json!({ "board": b }))?
            }
            _ => {
                return Err(ToolError::invalid(format!(
                    "board must be true, false, \"slate\", \"greenboard\", [r, g, b] or an \
                     object, got {b}"
                )));
            }
        }
    }
    if let Some(g) = present(args, "ghosting") {
        // Laid over no board, the config reader would bring a default one
        // back: refuse the contradiction instead.
        if cfg.board.is_none() {
            return Err(ToolError::invalid(
                "ghosting needs a board; drop it or the board: false",
            ));
        }
        overlay(
            &mut cfg,
            "ghosting",
            "board.ghosting",
            json!({ "board": { "ghosting": g } }),
        )?;
    }
    if let Some(s) = present(args, "seed") {
        overlay(&mut cfg, "seed", "seed", json!({ "seed": s }))?;
    }
    Ok(cfg)
}

/// A chalk error as a tool error: the one-line message, and its stable
/// code (`unsupported_command`, ...) and token as fields.
fn chalk_error(e: pscat::chalk::ChalkError) -> ToolError {
    use pscat::chalk::ChalkError;
    let code = e.code();
    let err = match &e {
        ChalkError::Config(_) => ToolError::invalid(e.to_string()),
        ChalkError::Empty => ToolError::new("latex_error", e.to_string()),
        ChalkError::Latex { token, .. } => {
            ToolError::new("latex_error", e.to_string()).with("token", json!(token))
        }
        ChalkError::Render(_) => ToolError::new("render_failed", e.to_string()),
    };
    err.with("code", json!(code))
}

/// The board's size in device pixels, rounded as the interpreter rounds
/// (`Interp::with_page_scaled`).
fn chalk_pixels(cfg: &pscat::chalk::Config) -> (u64, u64) {
    let px = |pt: u32| ((pt as f32 * (cfg.dpi / 72.0)).round() as u64).max(1);
    (px(cfg.width), px(cfg.height))
}

fn chalkboard_math(cfg: &Config, args: &Value) -> Result<Value, ToolError> {
    let latex = required_text(cfg, args, "latex")?;
    if latex.len() > MAX_LATEX_BYTES {
        return Err(ToolError::new(
            "input_too_large",
            format!(
                "latex is {} bytes; this tool takes at most {MAX_LATEX_BYTES} (a board that \
                 full is unreadable: split it over several boards)",
                latex.len()
            ),
        )
        .with("limit", json!(MAX_LATEX_BYTES))
        .with("size", json!(latex.len())));
    }
    let format = optional_str(args, "format")?.unwrap_or("png");
    let flag = match format {
        "png" => "--png",
        "svg" => "--svg",
        "pdf" => "--pdf",
        other => {
            return Err(ToolError::invalid(format!(
                "format must be png, svg or pdf, got {other:?}"
            )));
        }
    };
    let dest = optional_str(args, "out_path")?
        .map(|p| confine(cfg, p, format))
        .transpose()?;
    if format != "png" && dest.is_none() {
        return Err(ToolError::invalid(format!(
            "{format} output needs out_path: a chalk board is tens of thousands of grain \
             marks, megabytes of {format}, too big to return inline"
        )));
    }
    let board = chalk_config(args)?;
    let (w, h) = chalk_pixels(&board);
    if w > 8000 || h > 8000 || w * h > MAX_CHALK_PIXELS {
        return Err(ToolError::new(
            "output_too_large",
            format!(
                "the board would be {w}x{h} pixels; this tool draws at most \
                 {MAX_CHALK_PIXELS} pixels and 8000 a side. Lower dpi, width or height."
            ),
        )
        .with("limit_pixels", json!(MAX_CHALK_PIXELS))
        .with("pixels", json!(w * h))
        .with("width_px", json!(w))
        .with("height_px", json!(h)));
    }
    // The program is generated here, in-process (pure, no files); the
    // interpreter runs it in a child, under the same limits as
    // render_postscript, so a board costs at most one call's limits.
    let driver = pscat::chalk::driver(latex, &board).map_err(chalk_error)?;

    let scratch = ScratchDir::new()?;
    let out = scratch.path().join(format!("board.{format}"));
    let mut cmd = Command::new(pscat_bin()?);
    cfg.interpreter_args(&mut cmd);
    cmd.arg("--page")
        .arg(format!("{}x{}", driver.width, driver.height))
        .arg("--dpi")
        .arg(number_arg(f64::from(driver.dpi)));
    if driver.transparent {
        cmd.arg("--transparent");
    }
    cmd.arg(flag).arg(&out).arg("-");
    let run = run_child(cfg, &mut cmd, driver.source.as_bytes())?;
    if !run.success {
        // A TeX error is mathkit's `undefined` naming `mk-<kind>:<token>`:
        // the one-line message, never the PostScript report. The page drawn
        // so far is blank (the error comes from the measuring pass), so no
        // partial image goes back.
        let fields = ps_error_fields(&run.stderr).unwrap_or_default();
        if !fields.contains_key("limit")
            && fields.get("ps_error").and_then(Value::as_str) == Some("undefined")
            && let Some(cmd) = fields.get("offending_command").and_then(Value::as_str)
            && let Some(e) = pscat::chalk::explain(&pscat::PsError::Undefined(cmd.into()))
        {
            return Err(chalk_error(e));
        }
        return Err(failed_run(
            "render_failed",
            format!("the chalkboard did not render:\n{}", run.stderr.trim_end()),
            &run.stderr,
        ));
    }
    let pages = collect_pages(&out);
    if pages.is_empty() {
        return Err(ToolError::new(
            "no_output",
            "the chalkboard produced no page",
        ));
    }
    let written = match &dest {
        Some(dest) => Some(dest.write_pages(&pages)?),
        None => None,
    };
    let mut content = Vec::new();
    if format == "png" {
        for body in read_inline(cfg, &pages, written.as_deref())? {
            content.push(image_content(&body));
        }
    }
    if let Some(written) = written {
        content.push(text_content(&wrote_message(&written)));
    }
    Ok(Value::Array(content))
}

fn describe_art_capabilities(cfg: &Config) -> Result<Value, ToolError> {
    let mut cmd = Command::new(pscat_bin()?);
    cmd.arg("--capabilities");
    let run = run_child(cfg, &mut cmd, b"")?;
    if !run.success {
        return Err(ToolError::new(
            "capabilities_failed",
            format!("--capabilities failed:\n{}", run.stderr),
        ));
    }
    Ok(Value::Array(vec![text_content(&run.stdout)]))
}

fn eval_postscript(cfg: &Config, args: &Value) -> Result<Value, ToolError> {
    let source = required_text(cfg, args, "source")?;
    let mut cmd = Command::new(pscat_bin()?);
    cfg.interpreter_args(&mut cmd);
    cmd.arg("--headless").arg("--pstack-on-error").arg("-");
    let run = run_child(cfg, &mut cmd, source.as_bytes())?;
    let mut report = String::new();
    if !run.stdout.is_empty() {
        report.push_str(&run.stdout);
    }
    if !run.success {
        if !report.is_empty() && !report.ends_with('\n') {
            report.push('\n');
        }
        report.push_str(&run.stderr);
    }
    if report.is_empty() {
        report.push_str("(ran clean, printed nothing)");
    }
    if run.success {
        return Ok(Value::Array(vec![text_content(&report)]));
    }
    Err(failed_run("postscript_error", report, &run.stderr))
}

/// The pages' bytes for inline content, checked against the page and
/// byte caps from the file sizes *before* anything is read, so a refused
/// render never sits in the server's memory. Over a cap is an error, not a
/// silent drop; the file the caller asked for is already written by then,
/// and the message says where.
fn read_inline(
    cfg: &Config,
    pages: &[PathBuf],
    written: Option<&[PathBuf]>,
) -> Result<Vec<Vec<u8>>, ToolError> {
    let mut total = 0usize;
    if pages.len() <= MAX_INLINE_PAGES {
        for page in pages {
            let len = std::fs::metadata(page)
                .map_err(|e| format!("stat {page:?}: {e}"))?
                .len();
            total = total.saturating_add(usize::try_from(len).unwrap_or(usize::MAX));
        }
    }
    if total <= cfg.max_image && pages.len() <= MAX_INLINE_PAGES {
        let mut bodies = Vec::with_capacity(pages.len());
        for page in pages {
            bodies.push(std::fs::read(page).map_err(|e| format!("read {page:?}: {e}"))?);
        }
        return Ok(bodies);
    }
    let what = if pages.len() > MAX_INLINE_PAGES {
        format!(
            "{} pages; at most {MAX_INLINE_PAGES} are returned inline",
            pages.len()
        )
    } else {
        format!(
            "{total} bytes of output; at most {} are returned inline",
            cfg.max_image
        )
    };
    let next = match written {
        Some(w) => format!(" {}", wrote_message(w)),
        None => " Lower dpi or the page size, or pass out_path to keep the file.".into(),
    };
    Err(
        ToolError::new("output_too_large", format!("the render is {what}.{next}"))
            .with("limit_bytes", json!(cfg.max_image))
            .with("size_bytes", json!(total))
            .with("pages", json!(pages.len())),
    )
}

fn wrote_message(written: &[PathBuf]) -> String {
    match written {
        [one] => format!("wrote {}", one.display()),
        many => format!(
            "wrote {} files: {}",
            many.len(),
            many.iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// A tool error for a child run that failed, with the PostScript error's
/// fields from its stderr. A run the interpreter stopped at one of its limits
/// (steps, time or memory) is `limit_exceeded` with a `limit` field, whatever
/// `kind` the caller would otherwise use: the caller can tell it from a
/// program bug, and from the server's own `timeout` kill.
fn failed_run(kind: &'static str, message: String, stderr: &str) -> ToolError {
    let fields = ps_error_fields(stderr);
    let limited = fields.as_ref().is_some_and(|f| f.contains_key("limit"));
    let mut e = ToolError::new(if limited { "limit_exceeded" } else { kind }, message);
    if let Some(fields) = fields {
        e.fields.extend(fields);
    }
    e
}

/// The fields of pscat's `%%[ Error: NAME; OffendingCommand: CMD; Line: N ]%%`.
fn ps_error_fields(stderr: &str) -> Option<Map<String, Value>> {
    let line = stderr.lines().find(|l| l.contains("%%[ Error: "))?;
    let start = line.find("%%[ ")? + 4;
    let end = line[start..].find(" ]%%").map_or(line.len(), |i| start + i);
    let mut fields = Map::new();
    for part in line[start..end].split("; ") {
        let Some((key, value)) = part.split_once(": ") else {
            continue;
        };
        match key {
            // `limitexceeded (steps)`: the interpreter's own limits (#604).
            "Error" if value.starts_with("limitexceeded (") && value.ends_with(')') => {
                fields.insert("limit".into(), json!(&value[15..value.len() - 1]));
                fields.insert("ps_error".into(), json!("limitexceeded"))
            }
            "Error" => fields.insert("ps_error".into(), json!(value)),
            "OffendingCommand" => fields.insert("offending_command".into(), json!(value)),
            "Line" => fields.insert(
                "line".into(),
                value.parse::<u64>().map_or(json!(value), |n| json!(n)),
            ),
            _ => None,
        };
    }
    (!fields.is_empty()).then_some(fields)
}

/// Split `--lint`'s `pscat: lint: ...` lines out of a child's stderr:
/// `(finding lines joined with '\n', the rest of stderr joined back)`.
/// The `pscat: lint: clean` line (no findings) is dropped from both —
/// callers should stay silent when there's nothing to report. So is the
/// `[run-error]` line (issue #369): `render_postscript` already leads
/// with its own `PostScript error:` block for a failed run, and that
/// block carries the report, so repeating the verdict under `Lint:`
/// would only say the same thing twice.
fn split_lint_findings(stderr: &str) -> (String, String) {
    const PREFIX: &str = "pscat: lint: ";
    const CLEAN: &str = "pscat: lint: clean";
    const RUN_ERROR: &str = "pscat: lint: [run-error]";
    let mut findings = Vec::new();
    let mut rest = Vec::new();
    for line in stderr.lines() {
        if line == CLEAN || line.starts_with(RUN_ERROR) {
            continue;
        } else if line.starts_with(PREFIX) {
            findings.push(line);
        } else {
            rest.push(line);
        }
    }
    (findings.join("\n"), rest.join("\n"))
}

// --- out_path confinement --------------------------------------------

/// A checked destination: a directory inside the output root (canonical,
/// so no symlink in it leads out) and a plain file name that ends in the
/// render's own extension.
struct Dest {
    dir: PathBuf,
    name: String,
}

/// Resolve `raw` against the output root. Refused: any `..`, a path whose
/// directory is missing or resolves (through symlinks) outside the root,
/// a name that is not a plain file name, a name that does not end in `.ext`
/// (the render's format: `png`, `svg` or `pdf`), and a directory below the
/// root with a name starting with a dot (`.git`, `.claude`, ...). Absolute
/// paths are fine when they land inside the root.
///
/// The extension rule is also the overwrite rule: the target of a write is
/// `name`, or `stem-NNN.ext` for several pages, so an existing file can
/// only ever be replaced by a render of its own extension.
fn confine(cfg: &Config, raw: &str, ext: &str) -> Result<Dest, ToolError> {
    let root = cfg
        .out_root
        .as_ref()
        .map_err(|why| ToolError::new("out_path_disabled", why.clone()))?;
    let refuse = |why: &str| {
        ToolError::new(
            "out_path_refused",
            format!("out_path {raw:?} refused: {why}"),
        )
        .with("out_root", json!(root.display().to_string()))
        .with("required_extension", json!(format!(".{ext}")))
    };
    if raw.is_empty() || raw.contains('\0') {
        return Err(refuse("not a usable path"));
    }
    let path = Path::new(raw);
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(refuse("'..' is not allowed"));
    }
    let joined = root.join(path); // an absolute `path` replaces `root`
    let name = match (joined.file_name(), raw.ends_with(['/', '\\'])) {
        (Some(name), false) => name.to_string_lossy().into_owned(),
        _ => return Err(refuse("it must name a file")),
    };
    let has_ext = Path::new(&name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext));
    if !has_ext {
        return Err(refuse(&format!(
            "the name must end in .{ext}, the extension of the output format"
        )));
    }
    let parent = joined
        .parent()
        .ok_or_else(|| refuse("it must name a file"))?;
    let dir = std::fs::canonicalize(parent).map_err(|e| {
        refuse(&format!(
            "its directory {} is not usable ({e})",
            parent.display()
        ))
    })?;
    if !dir.starts_with(root) {
        return Err(refuse(&format!(
            "it is outside the output root {}",
            root.display()
        )));
    }
    // Judged on the resolved directory below the root, so a symlink to a
    // dot-directory is caught too, and a root that is itself under one
    // (`--out-root ~/.cache/renders`) still works.
    let below = dir.strip_prefix(root).unwrap_or(Path::new(""));
    if below
        .components()
        .any(|c| matches!(c, Component::Normal(n) if n.to_string_lossy().starts_with('.')))
    {
        return Err(refuse("writing into a dot-directory is not allowed"));
    }
    Ok(Dest { dir, name })
}

impl Dest {
    /// The checked directory still resolves to itself. `confine` ran
    /// before the render, so a directory on the path swapped for a symlink
    /// meanwhile would otherwise carry the write out of the root. Checked
    /// just before the temp file is created and again before the rename,
    /// which narrows that window to the two calls themselves; std has no
    /// `openat`, so a process racing those exact calls can still win
    /// (SECURITY.md).
    fn still_inside(&self) -> std::io::Result<()> {
        if std::fs::canonicalize(&self.dir)? == self.dir {
            Ok(())
        } else {
            Err(std::io::Error::other(
                "the directory changed during the render (now a symlink?)",
            ))
        }
    }

    /// Copy the rendered pages in. One page keeps the exact name; more
    /// are numbered `name-001.ext`, ... (the CLI's convention). Each file
    /// is written to a fresh temp file in the same directory and renamed
    /// over the target: a rename replaces a symlink rather than following
    /// it, so a link swapped in after the check still cannot redirect the
    /// write. An existing target that is not a regular file (a symlink,
    /// a directory, a device) is refused.
    fn write_pages(&self, pages: &[PathBuf]) -> Result<Vec<PathBuf>, ToolError> {
        let names: Vec<String> = if pages.len() == 1 {
            vec![self.name.clone()]
        } else {
            let p = Path::new(&self.name);
            let stem = p.file_stem().unwrap_or_default().to_string_lossy();
            let ext = p.extension().unwrap_or_default().to_string_lossy();
            (1..=pages.len())
                .map(|n| format!("{stem}-{n:03}.{ext}"))
                .collect()
        };
        for name in &names {
            let target = self.dir.join(name);
            if let Ok(meta) = std::fs::symlink_metadata(&target)
                && !meta.is_file()
            {
                return Err(ToolError::new(
                    "out_path_refused",
                    format!(
                        "out_path refused: {} exists and is not a regular file",
                        target.display()
                    ),
                ));
            }
        }
        let mut written = Vec::new();
        for (page, name) in pages.iter().zip(&names) {
            let target = self.dir.join(name);
            let tmp = self.dir.join(format!(".{name}.{}.tmp", unique_suffix()));
            let copied = (|| -> std::io::Result<()> {
                let mut src = std::fs::File::open(page)?;
                self.still_inside()?;
                let mut dst = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true) // never through an existing file or link
                    .open(&tmp)?;
                std::io::copy(&mut src, &mut dst)?;
                dst.sync_all()?;
                self.still_inside()?;
                // Closed before the rename: Windows will not rename a file
                // that is still open.
                drop((src, dst));
                std::fs::rename(&tmp, &target)
            })();
            if let Err(e) = copied {
                let _ = std::fs::remove_file(&tmp);
                return Err(ToolError::new(
                    "write_failed",
                    format!("could not write {}: {e}", target.display()),
                ));
            }
            written.push(target);
        }
        Ok(written)
    }
}

// --- plumbing -------------------------------------------------------

struct RunOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

/// Run a child with `stdin` fed in, both outputs captured, and the
/// config's wall-clock limit. Past the limit, or past
/// [`MAX_CHILD_OUTPUT`] on either stream, the child is killed and the
/// call fails with a `timeout` / `output_too_large` tool error, so a
/// runaway program costs at most one timeout and bounded memory.
fn run_child(cfg: &Config, cmd: &mut Command, stdin: &[u8]) -> Result<RunOutput, ToolError> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {:?}: {e}", cmd.get_program()))?;
    let overflow = Arc::new(AtomicBool::new(false));
    let feeder = {
        let mut pipe = child.stdin.take().expect("piped stdin");
        let bytes = stdin.to_vec();
        // A thread, so a child that stops reading cannot wedge the server;
        // a write error just means the child exited early.
        std::thread::spawn(move || {
            let _ = pipe.write_all(&bytes);
        })
    };
    let out = capture(child.stdout.take().expect("piped stdout"), &overflow);
    let err = capture(child.stderr.take().expect("piped stderr"), &overflow);

    let started = Instant::now();
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status.success()),
            Ok(None) => {}
            Err(e) => break Err(format!("wait for child: {e}")),
        }
        if overflow.load(Ordering::Relaxed) {
            break Err("output".into());
        }
        if started.elapsed() >= cfg.timeout {
            break Err("timeout".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if outcome.is_err() {
        kill(&mut child);
    }
    let _ = feeder.join();
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    // A program that floods and exits between two polls, or whose last
    // bytes arrive while the readers are joined, has still overflowed.
    let outcome = match outcome {
        Ok(_) if overflow.load(Ordering::Relaxed) => Err("output".to_string()),
        other => other,
    };
    let text = |b: Vec<u8>| String::from_utf8_lossy(&b).into_owned();
    match outcome {
        Ok(success) => Ok(RunOutput {
            success,
            stdout: text(stdout),
            stderr: text(stderr),
        }),
        Err(why) if why == "timeout" => Err(ToolError::new(
            "timeout",
            format!(
                "stopped after {}s, this server's limit per call (a loop that never ends?)",
                cfg.timeout.as_secs()
            ),
        )
        .with("limit_seconds", json!(cfg.timeout.as_secs()))),
        Err(why) if why == "output" => {
            let head: String = text(stdout).chars().take(2000).collect();
            Err(ToolError::new(
                "output_too_large",
                format!(
                    "stopped: the program printed more than {MAX_CHILD_OUTPUT} bytes. \
                     The first part:\n{head}"
                ),
            )
            .with("limit_bytes", json!(MAX_CHILD_OUTPUT)))
        }
        Err(why) => Err(why.into()),
    }
}

/// Read a child stream on a thread, keeping at most [`MAX_CHILD_OUTPUT`]
/// bytes; past that, flag the overflow and keep draining (so the child
/// never blocks on a full pipe) until it is killed.
fn capture(
    mut pipe: impl Read + Send + 'static,
    overflow: &Arc<AtomicBool>,
) -> std::thread::JoinHandle<Vec<u8>> {
    let overflow = Arc::clone(overflow);
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if kept.len() + n <= MAX_CHILD_OUTPUT {
                        kept.extend_from_slice(&chunk[..n]);
                    } else {
                        overflow.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
        kept
    })
}

fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The pscat binary lives next to this one (or is this one, under `pscat mcp`).
fn pscat_bin() -> Result<PathBuf, String> {
    let me = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let bin = me.with_file_name(format!("pscat{}", std::env::consts::EXE_SUFFIX));
    if bin.exists() {
        Ok(bin)
    } else {
        Err(format!("pscat binary not found at {}", bin.display()))
    }
}

fn unique_suffix() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// A fresh private directory for one call's render, deleted when the
/// call ends (renders used to pile up in the temp dir).
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new() -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("pscat-mcp-{}", unique_suffix()));
        // A leftover from a crashed run with the same pid is cleared
        // first; `create_dir` (not `_all`) then fails loudly on a race.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).map_err(|e| format!("scratch dir {}: {e}", dir.display()))?;
        Ok(ScratchDir(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Single-page output keeps the exact path; multi-page renders write
/// out-001.ext, out-002.ext, ... (the CLI's numbered_path convention).
fn collect_pages(out: &Path) -> Vec<PathBuf> {
    if out.exists() {
        return vec![out.to_path_buf()];
    }
    let (stem, ext) = (
        out.file_stem().unwrap_or_default().to_string_lossy(),
        out.extension().unwrap_or_default().to_string_lossy(),
    );
    let mut pages = Vec::new();
    for n in 1.. {
        let numbered = out.with_file_name(format!("{stem}-{n:03}.{ext}"));
        if !numbered.exists() {
            break;
        }
        pages.push(numbered);
    }
    pages
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_shape() {
        assert!(is_revision("2025-11-25"));
        assert!(is_revision("2099-01-01"));
        assert!(!is_revision("1.0.0"));
        assert!(!is_revision("2025-11-2"));
        assert!(!is_revision("2025/11/25"));
    }

    #[test]
    fn bounded_reader_drops_long_lines_and_keeps_going() {
        let data = b"short\n0123456789abcdef\nnext\r\nlast-no-newline";
        let mut r = std::io::BufReader::with_capacity(4, &data[..]);
        let mut got = Vec::new();
        loop {
            match read_line_bounded(&mut r, 8).unwrap() {
                Line::Eof => break,
                Line::TooLong => got.push("<long>".to_string()),
                Line::Data(b) => got.push(String::from_utf8(b).unwrap()),
            }
        }
        assert_eq!(got, ["short", "<long>", "next", "<long>"]);
    }

    #[test]
    fn ps_error_line_parses_into_fields() {
        let f = ps_error_fields(
            "%%[ Error: undefined; OffendingCommand: bogus; Line: 3 ]%%\nOperand stack (0):",
        )
        .unwrap();
        assert_eq!(f["ps_error"], "undefined");
        assert_eq!(f["offending_command"], "bogus");
        assert_eq!(f["line"], 3);
        assert!(ps_error_fields("all fine").is_none());
    }

    #[test]
    fn limit_error_line_names_the_limit() {
        let f = ps_error_fields(
            "%%[ Error: limitexceeded (steps); OffendingCommand: loop; Line: 1 ]%%",
        )
        .unwrap();
        assert_eq!(f["ps_error"], "limitexceeded");
        assert_eq!(f["limit"], "steps");
        assert_eq!(f["offending_command"], "loop");
        let e = failed_run(
            "postscript_error",
            "m".into(),
            "%%[ Error: limitexceeded (time) ]%%",
        );
        assert_eq!(e.kind, "limit_exceeded");
        assert_eq!(e.fields["limit"], "time");
        let e = failed_run("postscript_error", "m".into(), "%%[ Error: undefined ]%%");
        assert_eq!(e.kind, "postscript_error");
    }
}
