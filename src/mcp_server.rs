//! pscat-mcp — pscat as MCP tools (Stage 14; hardened in #568).
//!
//! One server, two ways in: the `pscat-mcp` binary (`src/bin/pscat_mcp.rs`)
//! and `pscat mcp` (`src/main.rs`), which the MCP Registry entry runs
//! because a cargo package is launched by its crate name (#569). Both
//! binaries include this file with `#[path]` and call [`run`]; it is not
//! part of the library.
//!
//! An MCP server over stdio (newline-delimited JSON-RPC 2.0) exposing
//! ten tools: `render_postscript`, `handwrite`, `chalkboard_math`,
//! `review_sheet`, `describe_art_capabilities`, `eval_postscript`, and the
//! render jobs `start_render`, `render_status`, `render_result`,
//! `cancel_render`. Works with any MCP
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
//! **Concurrency** (issues #622, #741). The main thread reads stdin and
//! answers what never blocks itself: `initialize`, `ping`, `tools/list`,
//! `notifications/cancelled` and the four job tools. Every other
//! `tools/call` runs on its own thread, registered under its request id
//! before the thread starts, so `notifications/cancelled` finds it and
//! kills its child, and `ping` is answered during a long render. Replies
//! go through one writer ([`Server::send`]): the whole line is written
//! under the stdout lock, so two threads never interleave. Render jobs
//! (`start_render` and friends) run on threads of their own, in a job table
//! with a cap on running and kept jobs. When stdin closes, every job is
//! cancelled, the calls already asked for finish and answer, and every
//! thread is joined before the server exits, so no child outlives a clean
//! shutdown; each child also gets
//! `--exit-with-parent`, which stops it if the server is killed outright
//! (Unix).
//!
//! **Bounds** (issue #568, `SECURITY.md`): every child run has a
//! wall-clock timeout and a cap on captured output; `source`/`text` have
//! a size cap; inline images have a byte and page cap; `out_path` is
//! confined to one output root (render extensions only, no dot-directories)
//! and written by rename, never by following a symlink; programs read files only
//! inside the read roots (issue #763: the working directory, the pscat data
//! directories, `--allow-read`), passed to every child as `--confine-reads`
//! and `--read-root`. Nothing but JSON-RPC is ever written to stdout: diagnostics
//! go to stderr, and child output is always captured through pipes.

use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
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

/// Tool calls (not jobs) running at once. Each is a child of up to
/// `--max-memory`; a call past this is refused with `busy`, not queued,
/// since a queued call would only eat the client's own timeout.
const MAX_CALLS: usize = 4;

/// `--max-bands`' default: with the default `--max-jobs 2`, two banded jobs
/// take 8 threads, which leaves cores free on a laptop.
const DEFAULT_MAX_BANDS: u64 = 4;

/// Render jobs kept at once: queued, running and finished ones whose result
/// has not expired. A new job evicts the oldest finished one past this, and
/// is refused with `busy` when none has finished.
const MAX_HELD_JOBS: usize = 16;

const USAGE: &str = "usage: pscat-mcp [options]
   or: pscat mcp [options]

An MCP server on stdin/stdout (launched by an MCP client, not by hand).

  --out-root DIR         directory out_path is confined to (default: the
                         working directory; out_path is off when that is /)
  --allow-read DIR       also let programs read files inside DIR
                         (repeatable); they always may in the working
                         directory (unless that is /) and the pscat data
                         directories ($PSCAT_HOME, the per-user one)
  --timeout SECS         wall-clock limit per tool call (default 60)
  --job-timeout SECS     wall-clock limit per render job, start_render
                         (default 3600)
  --max-jobs N           render jobs running at once; more wait, queued
                         (default 2)
  --job-ttl SECS         how long a finished job's result is kept (default
                         3600)
  --max-bands N          most threads one render may ask for with bands
                         (render_postscript, start_render; 1..64, default 4;
                         1 turns banded rendering off)
  --max-input-bytes N    largest source/text accepted (default 1048576)
  --max-image-bytes N    largest total of inline image bytes per call
                         (default 3750000, about 5 MB once base64-encoded)
  --max-pixels N         largest page or board in device pixels, width x
                         height at the dpi, for render_postscript, handwrite,
                         chalkboard_math and review_sheet (default 40000000;
                         at most 64000000, and 8000 a side always)
  --max-latex-bytes N    largest LaTeX chalkboard_math takes (default 4096)
  --max-steps N          stop a program after N interpreter steps (default
                         25000000000; 0 = no step limit)
  --max-memory MIB       stop a program that grows the heap by more than MIB
                         (default 1024; 0 = no memory limit)
  -h, --help             this text
  -V, --version          print the version

Each numeric limit above also reads an environment variable, PSCAT_MCP_ plus
the flag in capitals with underscores (PSCAT_MCP_TIMEOUT, PSCAT_MCP_MAX_STEPS,
PSCAT_MCP_MAX_PIXELS, ...); a flag overrides it, and a bad value stops the
server. --allow-read and --out-root have no variable.";

/// The limits and the output root, fixed for the server's lifetime by
/// whoever registered it. No tool argument can change them. Each call and
/// job runs on its own copy, which adds that run's cancel flag and, for a
/// job, its page counter and the job timeout.
#[derive(Clone)]
struct Config {
    /// Canonical output root, or why `out_path` is unavailable.
    out_root: Result<PathBuf, String>,
    /// Canonical directories every child's program may read from (issue
    /// #763): the working directory (not `/`), `$PSCAT_HOME` and the
    /// per-user data directory when they exist, and each `--allow-read`.
    /// Always passed, so an empty list still confines reads (to the
    /// embedded `lib/`).
    read_roots: Vec<PathBuf>,
    timeout: Duration,
    /// `--job-timeout`: the wall-clock limit of one render job.
    job_timeout: Duration,
    /// `--max-jobs`: render jobs running at once.
    max_jobs: usize,
    /// `--job-ttl`: how long a finished job is kept.
    job_ttl: Duration,
    /// `--max-bands` (issue #843): the most threads one render's `bands`
    /// may ask for, so a single call can't take every core.
    max_bands: u32,
    /// This run's cancel flag: set by `notifications/cancelled`,
    /// `cancel_render` or shutdown; `run_child` kills its child when it is.
    /// The server's own copy holds a flag nobody sets.
    cancel: Arc<AtomicBool>,
    /// A job's finished pages, from the child's `--progress` lines; `None`
    /// for a plain call.
    progress: Option<Arc<AtomicU64>>,
    max_input: usize,
    max_image: usize,
    /// Largest page, board or sheet in device pixels (issue #711): one cap
    /// for every tool that allocates a canvas. See [`DEFAULT_MAX_PIXELS`].
    max_pixels: u64,
    /// Longest `latex` for `chalkboard_math`, in bytes.
    max_latex: usize,
    /// Interpreter limits passed to every `pscat` child (issue #604); 0 is
    /// off. The child's time limit is derived from `timeout`, see
    /// `Config::interpreter_args`.
    max_steps: u64,
    max_memory_mib: u64,
}

impl Config {
    /// The copy one tool call runs on.
    fn for_call(&self, cancel: Arc<AtomicBool>) -> Config {
        Config {
            cancel,
            ..self.clone()
        }
    }

    /// The copy one render job runs on: the job timeout, its own flag and
    /// page counter.
    fn for_job(&self, cancel: Arc<AtomicBool>, progress: Arc<AtomicU64>) -> Config {
        Config {
            timeout: self.job_timeout,
            cancel,
            progress: Some(progress),
            ..self.clone()
        }
    }

    /// The flags that put the interpreter's own limits on a child run. The
    /// time limit is `--timeout` less a margin of a quarter of it, at most
    /// 5 s (55 s of the default 60): the interpreter stops first, with a
    /// structured `limit_exceeded` error and the partial page, and the hard
    /// kill at `--timeout` is only the backstop for a child stuck inside one
    /// long operator. The margin pays for starting the child, writing the
    /// canvas and the lint pass after the limit fires.
    /// The exception is `handwrite`: its child runs two programs (a measuring
    /// pass, then the render) and each gets this limit, so the two together
    /// can reach the kill and come back as `timeout`.
    fn interpreter_args(&self, cmd: &mut Command) {
        let total = self.timeout.as_secs_f64();
        let secs = total - (total / 4.0).min(5.0);
        cmd.arg("--max-time").arg(format!("{secs:.3}"));
        cmd.arg("--max-steps").arg(self.max_steps.to_string());
        cmd.arg("--max-memory").arg(self.max_memory_mib.to_string());
        self.read_root_args(cmd);
        // A server that is killed (no chance to cancel) takes its renders
        // with it.
        cmd.arg("--exit-with-parent")
            .arg(std::process::id().to_string());
    }

    /// The flags that confine a child's reads to the read roots (issue
    /// #763). `--confine-reads` comes first so that no roots at all still
    /// means confined, never open. `pscat` and `pscat handwrite` take both.
    fn read_root_args(&self, cmd: &mut Command) {
        cmd.arg("--confine-reads");
        for dir in &self.read_roots {
            cmd.arg("--read-root").arg(dir);
        }
    }

    /// `Ok(None)`: `--help`/`--version` was handled, exit 0. Numeric limits
    /// also come from the `PSCAT_MCP_*` environment variables
    /// ([`NUMERIC_OPTIONS`]); a flag overrides its variable.
    fn from_args(args: impl Iterator<Item = String>) -> Result<Option<Config>, String> {
        Config::from_args_env(args, |name| std::env::var_os(name))
    }

    /// [`Config::from_args`] with the environment passed in, so a test never
    /// reads (or sets) the process's own.
    fn from_args_env(
        args: impl Iterator<Item = String>,
        env: impl Fn(&str) -> Option<std::ffi::OsString>,
    ) -> Result<Option<Config>, String> {
        let mut out_root: Option<String> = None;
        let mut allow_read: Vec<String> = Vec::new();
        // Numeric limits given as flags, by flag name.
        let mut flags: HashMap<&'static str, u64> = HashMap::new();
        let mut args = args;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => {
                    println!("{USAGE}");
                    return Ok(None);
                }
                "-V" | "--version" => {
                    println!("pscat-mcp {}", env!("CARGO_PKG_VERSION"));
                    return Ok(None);
                }
                "--out-root" => {
                    out_root = Some(args.next().ok_or("--out-root needs a value")?);
                }
                "--allow-read" => {
                    allow_read.push(args.next().ok_or("--allow-read needs a value")?);
                }
                other => match NUMERIC_OPTIONS.iter().find(|o| o.flag == other) {
                    Some(o) => {
                        let v = args
                            .next()
                            .ok_or_else(|| format!("{other} needs a value"))?;
                        flags.insert(o.flag, parse_limit(other, &v, o)?);
                    }
                    None => return Err(format!("unknown option {other:?}")),
                },
            }
        }
        // Only after the flags, so `--help` works whatever the environment
        // holds. Every variable is checked, even one a flag overrides: a
        // typo in a settings file should not hide behind a flag. An empty
        // value is unset. There is no variable for `--allow-read` or
        // `--out-root`: what the programs may read, and where `out_path`
        // writes, is chosen only by whoever starts the server (#740, #756).
        let mut from_env: HashMap<&'static str, u64> = HashMap::new();
        for o in NUMERIC_OPTIONS {
            let Some(raw) = env(o.env) else { continue };
            let raw = raw
                .into_string()
                .map_err(|_| format!("{} is not valid UTF-8", o.env))?;
            if raw.trim().is_empty() {
                continue;
            }
            from_env.insert(o.flag, parse_limit(o.env, raw.trim(), o)?);
        }
        let limit = |flag: &str| -> u64 {
            flags
                .get(flag)
                .or_else(|| from_env.get(flag))
                .copied()
                .unwrap_or_else(|| {
                    NUMERIC_OPTIONS
                        .iter()
                        .find(|o| o.flag == flag)
                        .map_or(0, |o| o.default)
                })
        };
        let timeout = limit("--timeout");
        let job_timeout = limit("--job-timeout");
        let max_jobs = limit("--max-jobs");
        let job_ttl = limit("--job-ttl");
        let max_bands = limit("--max-bands") as u32;
        let max_input = limit("--max-input-bytes") as usize;
        let max_image = limit("--max-image-bytes") as usize;
        let max_pixels = limit("--max-pixels");
        let max_latex = limit("--max-latex-bytes") as usize;
        let max_steps = limit("--max-steps");
        let max_memory_mib = limit("--max-memory");
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
        let read_roots = read_roots(&allow_read)?;
        Ok(Some(Config {
            out_root,
            read_roots,
            timeout: Duration::from_secs(timeout),
            job_timeout: Duration::from_secs(job_timeout),
            max_jobs: max_jobs as usize,
            job_ttl: Duration::from_secs(job_ttl),
            max_bands,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: None,
            max_input,
            max_image,
            max_pixels,
            max_latex,
            max_steps,
            max_memory_mib,
        }))
    }
}

/// A numeric option of the server: its flag, the environment variable that
/// sets the same thing (issue #756, so a plugin user can raise a limit from
/// the client's settings without registering the server by hand), the
/// accepted range and the default. One table, so the flag and the variable
/// cannot disagree on a range.
struct NumericOption {
    flag: &'static str,
    env: &'static str,
    lo: u64,
    hi: u64,
    default: u64,
}

const fn opt(
    flag: &'static str,
    env: &'static str,
    lo: u64,
    hi: u64,
    default: u64,
) -> NumericOption {
    NumericOption {
        flag,
        env,
        lo,
        hi,
        default,
    }
}

/// Every numeric flag, with its `PSCAT_MCP_*` variable. Deliberately absent:
/// `--allow-read` and `--out-root`, which widen what a program can read or
/// where files are written, and so stay flags only.
const NUMERIC_OPTIONS: &[NumericOption] = &[
    opt("--timeout", "PSCAT_MCP_TIMEOUT", 1, 86_400, 60),
    opt("--job-timeout", "PSCAT_MCP_JOB_TIMEOUT", 1, 86_400, 3600),
    opt("--max-jobs", "PSCAT_MCP_MAX_JOBS", 1, 16, 2),
    opt("--job-ttl", "PSCAT_MCP_JOB_TTL", 1, 86_400, 3600),
    // 64 is `pscat --bands`' own ceiling.
    opt(
        "--max-bands",
        "PSCAT_MCP_MAX_BANDS",
        1,
        64,
        DEFAULT_MAX_BANDS,
    ),
    opt(
        "--max-input-bytes",
        "PSCAT_MCP_MAX_INPUT_BYTES",
        1,
        1 << 30,
        1 << 20,
    ),
    opt(
        "--max-image-bytes",
        "PSCAT_MCP_MAX_IMAGE_BYTES",
        1,
        1 << 30,
        3_750_000,
    ),
    opt(
        "--max-pixels",
        "PSCAT_MCP_MAX_PIXELS",
        1,
        64_000_000,
        DEFAULT_MAX_PIXELS,
    ),
    opt(
        "--max-latex-bytes",
        "PSCAT_MCP_MAX_LATEX_BYTES",
        1,
        1 << 30,
        DEFAULT_MAX_LATEX_BYTES as u64,
    ),
    opt(
        "--max-steps",
        "PSCAT_MCP_MAX_STEPS",
        0,
        u64::MAX,
        pscat::limits::UNTRUSTED_MAX_STEPS,
    ),
    opt(
        "--max-memory",
        "PSCAT_MCP_MAX_MEMORY",
        0,
        1 << 30,
        (pscat::limits::UNTRUSTED_MAX_MEMORY >> 20) as u64,
    ),
];

/// `name` is the flag or the variable, whichever the value came from.
fn parse_limit(name: &str, v: &str, o: &NumericOption) -> Result<u64, String> {
    match v.parse::<u64>() {
        Ok(n) if (o.lo..=o.hi).contains(&n) => Ok(n),
        _ => Err(format!(
            "{name} must be a whole number {}..{}, got {v:?}",
            o.lo, o.hi
        )),
    }
}

/// The read roots (issue #763; see [`Config::read_roots`]). A bad
/// `--allow-read` is a startup error; a default directory that doesn't
/// exist (no per-user data directory yet) is left out.
fn read_roots(allow_read: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut add = |p: PathBuf| {
        if !roots.contains(&p) {
            roots.push(p);
        }
    };
    // As for out_path: a client that starts servers in `/` (or a
    // `PSCAT_HOME=/`) must not make the whole disk readable, so no default
    // root is the filesystem root.
    let mut add_default = |d: &Path| {
        if let Ok(canon) = std::fs::canonicalize(d)
            && canon.is_dir()
            && canon.parent().is_some()
        {
            add(canon);
        }
    };
    if let Ok(cwd) = std::env::current_dir() {
        add_default(&cwd);
    }
    let dirs = pscat::paths::process_roots();
    for d in [&dirs.home, &dirs.user_data].into_iter().flatten() {
        add_default(d);
    }
    for d in allow_read {
        let canon = std::fs::canonicalize(d).map_err(|e| format!("--allow-read {d:?}: {e}"))?;
        if !canon.is_dir() {
            return Err(format!("--allow-read {d:?} is not a directory"));
        }
        add(canon);
    }
    Ok(roots)
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

/// What the tool descriptions say about reads (issue #763); the server
/// instructions list the actual directories ([`read_roots_note`]).
const READS_BRIEF: &str = "Programs read files ((path) (r) file, run, filenameforall) only inside \
    this server's read roots: its working directory, the pscat data directories and any \
    --allow-read directory; elsewhere is invalidfileaccess. The bundled lib/ is always available.";

/// The server instructions' sentence on reads, naming the read roots.
fn read_roots_note(cfg: &Config) -> String {
    let dirs: Vec<String> = cfg
        .read_roots
        .iter()
        .map(|d| d.display().to_string())
        .collect();
    let dirs = if dirs.is_empty() {
        "none (only the bundled lib/)".to_string()
    } else {
        dirs.join(", ")
    };
    format!(
        "Programs read files ((path) (r) file, run, readreals, filenameforall) only inside \
         these read roots: {dirs}. Anything else, `..` and symlinks out included, is \
         invalidfileaccess; the bundled lib/ is always available. Whoever registered the \
         server can add a directory with --allow-read DIR."
    )
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
    let server = Arc::new(Server::new(cfg));
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    loop {
        let line = match read_line_bounded(&mut input, max_line) {
            Ok(Line::Eof) | Err(_) => break,
            Ok(Line::TooLong) => {
                server.send(&error(
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
        if let Some(reply) = handle_message(&server, &line) {
            server.send(&reply);
        }
    }
    server.shut_down();
    ExitCode::SUCCESS
}

/// A mutex that a panicking thread poisoned still holds consistent data
/// here (every critical section is a few field writes), so take it anyway.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The server's shared state. Locks are never nested: `inflight`, `jobs`
/// and `threads` are each taken alone, and the stdout lock only inside
/// [`Server::send`], with nothing else held.
struct Server {
    cfg: Config,
    /// Tool calls running on their own thread, by request id (its JSON
    /// text, so `1` and `"1"` differ), with each call's cancel flag.
    inflight: Mutex<HashMap<String, Arc<AtomicBool>>>,
    jobs: Mutex<JobTable>,
    /// Signalled whenever a job slot frees up or a job is cancelled, so a
    /// queued job looks again.
    job_wake: Condvar,
    /// Every call and job thread not yet joined.
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Server {
    fn new(cfg: Config) -> Self {
        Server {
            cfg,
            inflight: Mutex::new(HashMap::new()),
            jobs: Mutex::new(JobTable::default()),
            job_wake: Condvar::new(),
            threads: Mutex::new(Vec::new()),
        }
    }

    /// Write one message. The single writer: the line is built first and
    /// written whole under the stdout lock, so replies from several threads
    /// never interleave. serde_json escapes every control character, so one
    /// reply is one line.
    fn send(&self, reply: &Value) {
        let line = format!("{reply}\n");
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(line.as_bytes());
        let _ = out.flush();
    }

    /// Keep a thread's handle for the join at shutdown, dropping the ones
    /// that have finished (their threads are gone; nothing to reap).
    fn track(&self, handle: JoinHandle<()>) {
        let mut threads = lock(&self.threads);
        threads.retain(|h| !h.is_finished());
        threads.push(handle);
    }

    /// `notifications/cancelled`: flag the call, whose `run_child` then
    /// kills the child. An id that is not running (finished, unknown, or a
    /// fast request answered inline) is ignored, as the spec allows.
    fn cancel_request(&self, request_id: &Value) {
        if let Some(flag) = lock(&self.inflight).get(&request_id.to_string()) {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// Run a tool call on its own thread. `Some` is an immediate reply: the
    /// id is already running, or too many calls are.
    fn spawn_call(self: &Arc<Self>, id: Value, tool: ToolFn, args: Value) -> Option<Value> {
        let key = id.to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut inflight = lock(&self.inflight);
            if inflight.contains_key(&key) {
                return Some(error(
                    id,
                    -32600,
                    "a request with this id is still running",
                    None,
                ));
            }
            if inflight.len() >= MAX_CALLS {
                return Some(ok(
                    id,
                    ToolError::new(
                        "busy",
                        format!(
                            "{MAX_CALLS} tool calls are already running, this server's limit; \
                             wait for one to finish, or use start_render for a long render"
                        ),
                    )
                    .with("limit_calls", json!(MAX_CALLS))
                    .into_result(),
                ));
            }
            // Registered before the thread starts, so a cancel sent right
            // after the request always finds it.
            inflight.insert(key.clone(), Arc::clone(&cancel));
        }
        let server = Arc::clone(self);
        let reply_id = id.clone();
        let spawned = std::thread::Builder::new()
            .name("pscat-mcp-call".into())
            .spawn(move || {
                let cfg = server.cfg.for_call(Arc::clone(&cancel));
                let outcome =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tool(&cfg, &args)));
                let reply = match outcome {
                    Ok(Ok(content)) => {
                        ok(reply_id, json!({ "content": content, "isError": false }))
                    }
                    Ok(Err(e)) => ok(reply_id, e.into_result()),
                    Err(_) => error(reply_id, -32603, "internal error: the tool panicked", None),
                };
                // Out of the table before the reply is written: a client
                // may reuse an id once it has its answer (the tests do).
                // The removal and the flag read share the lock with
                // `cancel_request`, so a cancel either lands first (then no
                // reply, as the spec asks) or finds nothing to cancel.
                let cancelled = {
                    let mut inflight = lock(&server.inflight);
                    inflight.remove(&key);
                    cancel.load(Ordering::SeqCst)
                };
                if !cancelled {
                    server.send(&reply);
                }
            });
        match spawned {
            Ok(handle) => {
                self.track(handle);
                None
            }
            Err(e) => {
                lock(&self.inflight).remove(&id.to_string());
                Some(error(
                    id,
                    -32603,
                    &format!("cannot start a thread: {e}"),
                    None,
                ))
            }
        }
    }

    /// Stdin closed: cancel every job, let the calls already asked for
    /// finish and answer (each is bounded by `--timeout`; a client that
    /// pipes one request and closes stdin still gets its reply, as it did
    /// when requests were handled one at a time), then join every thread, so
    /// each child is reaped and each scratch directory removed before the
    /// process exits. A client that wants a call stopped sends
    /// `notifications/cancelled` first.
    fn shut_down(&self) {
        {
            let jobs = lock(&self.jobs);
            for job in jobs.jobs.values() {
                job.cancel.store(true, Ordering::SeqCst);
            }
            // Under the lock, so a queued job between its flag check and
            // its wait cannot miss this.
            self.job_wake.notify_all();
        }
        let threads = std::mem::take(&mut *lock(&self.threads));
        for handle in threads {
            let _ = handle.join();
        }
    }
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
/// (notifications, and responses to requests we never send) or when a
/// thread will answer (a tool call).
fn handle_message(server: &Arc<Server>, line: &[u8]) -> Option<Value> {
    let cfg = &server.cfg;
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
        None => {
            // A notification. `notifications/cancelled` stops a running
            // call; every other one needs nothing from us.
            if method.as_str() == Some("notifications/cancelled")
                && let Some(request_id) = obj.get("params").and_then(|p| p.get("requestId"))
            {
                server.cancel_request(request_id);
            }
            return None;
        }
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
        "tools/call" => return tools_call(server, id, params),
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
             Each call is limited to {}s: this server's --timeout, 60 by default, which \
             whoever registered it can raise (up to 86400); the MCP client's own tool-call \
             timeout can also cut a call off (Codex's default is 60 s). For a render that may \
             take longer, use start_render: it returns a job_id at once, render_status polls \
             it, render_result fetches the pages, cancel_render stops it; a job may run {}s. \
             {bands}A page, board or sheet is limited to {} pixels. \
             {out_path} {reads}",
            cfg.timeout.as_secs(),
            cfg.job_timeout.as_secs(),
            cfg.max_pixels,
            bands = bands_note(cfg),
            reads = read_roots_note(cfg),
        ),
    }))
}

/// The instructions' line on `bands` (#843), empty when `--max-bands 1`
/// turns banded renders off.
fn bands_note(cfg: &Config) -> String {
    if cfg.max_bands < 2 {
        return String::new();
    }
    format!(
        "Both take bands (up to {}) to render a PNG on several threads, which pays off \
         only for a program that reads /BandBBox and skips work outside its band. ",
        cfg.max_bands
    )
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

/// A tool that runs a child: on its own thread, cancellable.
type ToolFn = fn(&Config, &Value) -> Result<Value, ToolError>;

/// A job tool: answered inline (it only reads or writes the job table),
/// returning the whole `tools/call` result.
type JobToolFn = fn(&Arc<Server>, &Value) -> Result<Value, ToolError>;

/// A `tools/call`: the name and arguments are checked here, so a malformed
/// call is answered in order; a job tool is answered inline; any other tool
/// runs on its own thread and `None` is returned.
fn tools_call(server: &Arc<Server>, id: Value, params: &Value) -> Option<Value> {
    enum Tool {
        Child(ToolFn),
        Job(JobToolFn),
    }
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return Some(error(id, -32602, "tools/call needs a tool name", None));
    };
    let tool = match name {
        "render_postscript" => Tool::Child(render_postscript),
        "handwrite" => Tool::Child(handwrite),
        "chalkboard_math" => Tool::Child(chalkboard_math),
        "review_sheet" => Tool::Child(review_sheet),
        "describe_art_capabilities" => Tool::Child(|cfg, _| describe_art_capabilities(cfg)),
        "eval_postscript" => Tool::Child(eval_postscript),
        "start_render" => Tool::Job(start_render),
        "render_status" => Tool::Job(render_status),
        "render_result" => Tool::Job(render_result),
        "cancel_render" => Tool::Job(cancel_render),
        other => return Some(error(id, -32602, &format!("unknown tool: {other}"), None)),
    };
    let empty = Value::Object(Map::new());
    let args = params.get("arguments").unwrap_or(&empty);
    if !args.is_object() {
        return Some(ok(
            id,
            ToolError::invalid("arguments must be an object").into_result(),
        ));
    }
    match tool {
        Tool::Job(f) => Some(match f(server, args) {
            Ok(result) => ok(id, result),
            Err(e) => ok(id, e.into_result()),
        }),
        Tool::Child(f) => server.spawn_call(id, f, args.clone()),
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
/// extension. `idempotentHint` is true for every tool but `start_render`
/// (each call starts another job): the same arguments produce the same
/// output, and a repeated write replaces the same file. `openWorldHint`
/// is false: no tool touches the network. The hints are static per tool, so a
/// tool whose behaviour depends on an argument (`out_path`) gets the
/// conservative `readOnlyHint: false`. A new tool must call this and add its
/// row to the table in `tests/mcp.rs`.
fn annotations(title: &str, read_only: bool) -> Value {
    annotations_with(title, read_only, true)
}

fn annotations_with(title: &str, read_only: bool, idempotent: bool) -> Value {
    json!({
        "title": title,
        "readOnlyHint": read_only,
        "destructiveHint": false,
        "idempotentHint": idempotent,
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
    let render_schema = json!({
        "type": "object",
        "properties": {
            "source": { "type": "string", "description": "The PostScript program" },
            "width": { "type": "integer", "minimum": 1, "maximum": 8000, "description": "Page width in points (default 612)" },
            "height": { "type": "integer", "minimum": 1, "maximum": 8000, "description": "Page height in points (default 792)" },
            "dpi": { "type": "number", "minimum": 9, "maximum": 1200, "description": "Device resolution (default 72; 144 = 2x pixels)" },
            "format": { "type": "string", "enum": ["png", "svg", "pdf"], "description": "Output format (default png)" },
            "halftone": { "type": "boolean", "description": "Screen the raster like a mono laser printer (png only)" },
            "bands": { "type": "integer", "minimum": 1, "maximum": cfg.max_bands, "description": bands_description(cfg) },
            "out_path": { "type": "string", "description": format!("{out_path}Required for pdf. A multi-page render writes name-001.ext, name-002.ext, ...") }
        },
        "required": ["source"]
    });
    let max_pixels = cfg.max_pixels;
    let max_latex = cfg.max_latex;
    let canvas = format!(
        "A page is at most {max_pixels} pixels (width and height in points times dpi/72) and \
         8000 a side (output_too_large)."
    );
    let limits = format!(
        "Limits: {}s per call (the server's --timeout; its default is 60), {} bytes of input, \
         The interpreter stops a runaway program (a limit_exceeded error naming \
         steps, time or memory); stopped cannot catch it.",
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
                leftover stack). {limits} {canvas} Inline images are capped at {} bytes in total \
                and {MAX_INLINE_PAGES} pages; past that, lower dpi/size or use out_path. \
                Writes a file only when out_path is given (then it replaces an existing render \
                file of that extension), which is why this tool is not marked read-only. \
                {READS_BRIEF}",
                cfg.max_image),
            "inputSchema": render_schema.clone()
        },
        {
            "name": "handwrite",
            "title": "Handwriting-style note",
            "annotations": annotations("Handwriting-style note", false),
            "description": format!("Render text as a handwriting-style note (PNG): a natural, hand-drawn \
                script with per-letter variation, word-wrapped, auto-sized page. \
                A set of hand-drawn styles for notes, cards and art; not for imitating a \
                specific person's handwriting or presenting writing as someone else's. \
                Lowercase letters, digits and .,-'!? only; anything else is invisible. \
                Writes a file only when out_path is given (then it replaces an existing .png), \
                which is why this tool is not marked read-only. {limits} {canvas}"),
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
                backslashes: \\\\ breaks rows, & marks where align, aligned and split rows line up, \
                gather, gather* and gathered centre each row instead, and an align*/equation \
                wrapper or $...$ is fine. The subset is mathkit's (fractions, roots, sums, \
                integrals, Greek, matrices, cases, accents...); a command outside it is a \
                latex_error naming it. Same input and seed: the same image, byte for byte. \
                The input is LaTeX only, never PostScript. svg and pdf (megabytes: tens of \
                thousands of grain marks) are written to out_path only. Writes a file only \
                when out_path is given (then it replaces an existing file of that \
                extension), which is why this tool is not marked read-only. Limits: \
                latex at most {max_latex} bytes; the board at most {max_pixels} \
                pixels (width and height in points times dpi/72) and 8000 a side; \
                {}s per call (the server's --timeout; its default is 60); the interpreter's step, time and memory limits \
                (limit_exceeded). Inline images are capped at {} bytes: past that, lower \
                dpi or use out_path.", cfg.timeout.as_secs(), cfg.max_image),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "latex": { "type": "string", "description": format!("The formula, as typed in a LaTeX document (single backslashes); at most {max_latex} bytes") },
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
                    "ghosting": {
                        "description": "How much of an earlier lecture's erased writing shows, 0..1 (default 0.4; false = none). Needs a board",
                        "anyOf": [
                            { "type": "number", "minimum": 0, "maximum": 1 },
                            { "type": "boolean" }
                        ]
                    },
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
            "name": "review_sheet",
            "title": "Review sheet",
            "annotations": annotations("Review sheet", false),
            "description": format!("Compose a labelled review sheet: each PostScript program is a row \
                and each entry of its dpi list a cell, drawn at its own true pixel size (a 48 px \
                render stays 48 px beside a 512 px one), centred in its column with a hairline \
                around it, the row label above and a pixel-size caption (`48x48 px`) under each \
                cell. For comparing candidates (logos, icons, a seed's variants) at several \
                sizes in one image. Returns one PNG: for the same programs, pages, dpis and labels, byte for byte what \
                `pscat sheet` writes. A dpi entry {{\"dpi\": 9, \"zoom\": 4}} draws \
                the 9 dpi render again enlarged 4x with no smoothing (whole factors 2..16), so a \
                small icon can be read pixel by pixel. Programs are given inline as `source`, \
                never as file names. Each cell runs in its own child under the interpreter's \
                limits; a cell whose program fails is still drawn as far as it got, with the \
                error text alongside. Labels in Latin (accents included), Korean, Japanese, \
                Chinese and Thai are drawn in a face for the script (the last three need the \
                cjk-thai font pack). Writes a file only when out_path is given (then it replaces \
                an existing .png), which is why this tool is not marked read-only. Limits: \
                {MAX_SHEET_CELLS} cells; the sheet at most {max_pixels} pixels and 8000 a \
                side, and the cells rendered at most {max_pixels} pixels in all; labels at \
                most {MAX_SHEET_LABEL} characters; {}s for the whole sheet (the server's --timeout; its default is 60), {} bytes of source in \
                all; the interpreter's step, time and memory limits per cell (limit_exceeded). \
                The inline image is capped at {} bytes: past that, lower the dpi list or use \
                out_path. {READS_BRIEF}",
                cfg.timeout.as_secs(), cfg.max_input, cfg.max_image),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entries": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_SHEET_CELLS,
                        "description": "The rows, top to bottom, one program each",
                        "items": {
                            "type": "object",
                            "properties": {
                                "source": { "type": "string", "description": "The PostScript program" },
                                "label": { "type": "string", "maxLength": MAX_SHEET_LABEL, "description": "The row label (default: \"program N\", N counting from 1)" },
                                "width": { "type": "integer", "minimum": 1, "maximum": 8000, "description": "Page width in points (default: the top-level width)" },
                                "height": { "type": "integer", "minimum": 1, "maximum": 8000, "description": "Page height in points (default: the top-level height)" },
                                "dpi": sheet_dpi_schema("The cells of this row: one per item, left to right (default: the top-level dpi)")
                            },
                            "required": ["source"]
                        }
                    },
                    "width": { "type": "integer", "minimum": 1, "maximum": 8000, "description": "Page width in points for entries that give none (default 612)" },
                    "height": { "type": "integer", "minimum": 1, "maximum": 8000, "description": "Page height in points for entries that give none (default 792)" },
                    "dpi": sheet_dpi_schema("One cell per item, left to right: a dpi (9..1200), or {\"dpi\": 9, \"zoom\": 4} to also draw it enlarged 4x. The default for entries that give no list (default [72])"),
                    "labels": { "type": "boolean", "description": "Draw the row labels (default true)" },
                    "sizes": { "type": "boolean", "description": "Draw the pixel-size caption under each cell (default true)" },
                    "out_path": { "type": "string", "description": out_path.trim_end() }
                },
                "required": ["entries"]
            }
        },
        {
            "name": "describe_art_capabilities",
            "title": "List art capabilities",
            "annotations": annotations("List art capabilities", true),
            "description": "List the creative capabilities this pscat build actually has installed: fonts, \
                Type 3 program faces, mood palettes, page templates, artkit/style-pack procedures, and \
                native pscat operators. \
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
                (=, print, pstack), then anything the program wrote to stderr as a Warnings item. On error: the standard PostScript error name, the offending \
                command and line, plus a gs-style operand-stack post-mortem. The debugging \
                loop. Writes nothing. {READS_BRIEF} {limits}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "The PostScript program" }
                },
                "required": ["source"]
            }
        },
        {
            "name": "start_render",
            "title": "Start a render job",
            "annotations": annotations_with("Start a render job", false, false),
            "description": format!("Start render_postscript as a background job, for a render that \
                may take longer than one call: same arguments, returns a job_id at once. Poll \
                render_status(job_id) until the state is done, failed or cancelled, then \
                render_result(job_id) returns exactly what render_postscript would (images, \
                error text, Lint, Warnings, the file written). A job may run {}s (the server's \
                --job-timeout) under the same step, memory and pixel limits; {} run at once \
                and more wait queued; a finished job is kept {}s. Bad arguments are refused \
                here, before any job starts. A long render of a program that reads /BandBBox \
                can also pass bands to use several threads. Writes a file only when out_path \
                is given.",
                cfg.job_timeout.as_secs(), cfg.max_jobs, cfg.job_ttl.as_secs()),
            "inputSchema": render_schema
        },
        {
            "name": "render_status",
            "title": "Render job status",
            "annotations": annotations("Render job status", true),
            "description": "The state of a start_render job: queued, running, done, failed or \
                cancelled, with elapsed_seconds and pages_done (pages ended by showpage so \
                far; a single page shows 0 while it is drawn). Answers at once, even while renders run. \
                A failed job's error is included.",
            "inputSchema": job_id_schema("The job_id start_render returned")
        },
        {
            "name": "render_result",
            "title": "Render job result",
            "annotations": annotations("Render job result", true),
            "description": "The result of a finished start_render job: exactly what \
                render_postscript returns (a failed render is an error result with its partial \
                page, as there). A job still queued or running is a job_not_finished error; a \
                cancelled one, cancelled. The result can be fetched again until the job expires.",
            "inputSchema": job_id_schema("The job_id start_render returned")
        },
        {
            "name": "cancel_render",
            "title": "Cancel a render job",
            "annotations": annotations("Cancel a render job", false),
            "description": "Cancel a start_render job: a queued job never starts, a running one's \
                render is stopped. Returns at once; render_status says cancelled once the render \
                has stopped. A finished job is left as it is.",
            "inputSchema": job_id_schema("The job_id to cancel")
        }
    ])
}

/// The `bands` argument's description (issue #843): what it is for, the
/// cap, and what it gives up.
fn bands_description(cfg: &Config) -> String {
    format!(
        "Render the PNG on this many threads, as pscat --bands N (default 1; at most {}, \
         this server's --max-bands). The page is cut into horizontal bands, about 2 per \
         thread, and each band runs the whole program with /BandIndex, /BandCount and \
         /BandBBox (its area in default user space) defined in userdict, so only a program that reads \
         them and skips work outside its band is faster; any other costs about 2x bands \
         times the work. The image is identical to bands 1 unless the program reads the \
         clock. png only; no Lint block, and a job's pages_done stays 0 until it ends. \
         Step and time limits apply to each band, the memory limit to all of them together, \
         and the call's (or job's) wall-clock limit to the whole render.",
        cfg.max_bands
    )
}

fn job_id_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "properties": {
            "job_id": { "type": "string", "description": description }
        },
        "required": ["job_id"]
    })
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
    render_run(cfg, &render_plan(cfg, args)?)
}

/// A checked `render_postscript` request: everything that can be refused
/// without running the program has been, so `start_render` can refuse it at
/// once instead of in the job.
struct RenderPlan {
    source: String,
    format: &'static str,
    flag: &'static str,
    width: i64,
    height: i64,
    dpi: Option<f64>,
    halftone: bool,
    /// Threads for `pscat --bands` (issue #843); 1 is a plain render.
    bands: u32,
    dest: Option<Dest>,
}

fn render_plan(cfg: &Config, args: &Value) -> Result<RenderPlan, ToolError> {
    let source = required_text(cfg, args, "source")?;
    let (format, flag) = match optional_str(args, "format")?.unwrap_or("png") {
        "png" => ("png", "--png"),
        "svg" => ("svg", "--svg"),
        "pdf" => ("pdf", "--pdf"),
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
    // The canvas is allocated before `--max-memory` counts, so a page over
    // the pixel cap is refused before a child starts (#711). Every format
    // draws on the same raster.
    let (w_px, h_px) = device_pixels((width as u32, height as u32), dpi.unwrap_or(72.0) as f32);
    if !pixels_allowed(cfg, w_px, h_px) {
        return Err(too_many_pixels(cfg, "the page", w_px, h_px));
    }
    let bands = render_bands(cfg, args, format, h_px)?;
    // Checked before rendering, so a refused path costs nothing.
    let dest = optional_str(args, "out_path")?
        .map(|p| confine(cfg, p, format))
        .transpose()?;
    if format == "pdf" && dest.is_none() {
        return Err(ToolError::invalid(
            "pdf output needs out_path (a PDF is a file, not a chat message)",
        ));
    }
    Ok(RenderPlan {
        source: source.to_string(),
        format,
        flag,
        width,
        height,
        dpi,
        halftone,
        bands,
        dest,
    })
}

/// The `bands` argument, checked against everything `pscat --bands` would
/// refuse (`bands.rs` `check_options`) so the refusal names the argument
/// and comes before any child or job. Of the flags it refuses, only the
/// output format can come from a tool argument; `--lint` and `--progress`
/// are the server's own and are left off a banded run, and no tool passes
/// `--allow-write` or `--profile`.
fn render_bands(cfg: &Config, args: &Value, format: &str, h_px: u64) -> Result<u32, ToolError> {
    let bands = match args.get("bands") {
        None | Some(Value::Null) => return Ok(1),
        Some(v) => match v.as_u64() {
            Some(n) if n >= 1 => n,
            _ => {
                return Err(ToolError::invalid(format!(
                    "bands must be a whole number from 1 to {}, got {v}",
                    cfg.max_bands
                )));
            }
        },
    };
    if bands > u64::from(cfg.max_bands) {
        return Err(ToolError::invalid(format!(
            "bands {bands} is over this server's limit of {} (its --max-bands, which \
             whoever registered the server can raise, up to 64)",
            cfg.max_bands
        )));
    }
    if bands > 1 && format != "png" {
        return Err(ToolError::invalid(format!(
            "bands renders a PNG only; leave it out (or 1) for {format}"
        )));
    }
    if bands > h_px {
        return Err(ToolError::invalid(format!(
            "bands {bands} is more than the page's {h_px} device rows"
        )));
    }
    Ok(bands as u32)
}

/// Run a checked render: the child, the pages, the file and the content.
fn render_run(cfg: &Config, plan: &RenderPlan) -> Result<Value, ToolError> {
    let RenderPlan {
        source,
        format,
        flag,
        width,
        height,
        dpi,
        halftone,
        bands,
        dest,
    } = plan;
    let (format, flag, dpi) = (*format, *flag, *dpi);
    let scratch = ScratchDir::new()?;
    let out = scratch.path().join(format!("page.{format}"));
    let mut cmd = Command::new(pscat_bin()?);
    cfg.interpreter_args(&mut cmd);
    // `pscat --bands` refuses `--progress` and `--lint` (each band re-runs
    // the whole program), so a banded run goes without both.
    let banded = *bands > 1;
    if cfg.progress.is_some() && !banded {
        cmd.arg("--progress");
    }
    if banded {
        cmd.arg("--bands").arg(bands.to_string());
    }
    cmd.arg("--page").arg(format!("{width}x{height}"));
    if let Some(dpi) = dpi {
        cmd.arg("--dpi").arg(number_arg(dpi));
    }
    if *halftone {
        cmd.arg("--halftone");
    }
    cmd.arg(flag).arg(&out).arg("-");
    if !banded {
        cmd.arg("--lint");
    }
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
    // A failed run's stderr is the `PostScript error:` block above.
    if run.success {
        content.extend(warnings_content(&other_stderr));
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

// --- render jobs (#741) ----------------------------------------------

/// The render jobs: queued, running and finished (until they expire).
#[derive(Default)]
struct JobTable {
    jobs: HashMap<String, Job>,
    /// Jobs holding a slot (at most `--max-jobs`).
    running: usize,
    /// The last job number handed out; ids are `job-N`.
    next: u64,
}

struct Job {
    /// Its number: the order jobs start in, and the oldest is evicted first.
    seq: u64,
    created: Instant,
    finished: Option<Instant>,
    state: JobState,
    cancel: Arc<AtomicBool>,
    /// Pages finished so far (the child's `--progress` lines).
    pages: Arc<AtomicU64>,
}

enum JobState {
    Queued,
    Running,
    /// The `tools/call` result `render_postscript` would have returned.
    Done(Value),
    /// The same, an error result (`isError: true`).
    Failed(Value),
    Cancelled,
}

impl JobState {
    fn name(&self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Running => "running",
            JobState::Done(_) => "done",
            JobState::Failed(_) => "failed",
            JobState::Cancelled => "cancelled",
        }
    }
}

impl JobTable {
    /// Drop the finished jobs older than `ttl`.
    fn purge(&mut self, ttl: Duration) {
        self.jobs
            .retain(|_, j| j.finished.is_none_or(|f| f.elapsed() < ttl));
    }

    /// Record a job's end. Its slot, if it held one, is released by the
    /// caller.
    fn finish(&mut self, id: &str, state: JobState) {
        if let Some(job) = self.jobs.get_mut(id) {
            job.state = state;
            job.finished = Some(Instant::now());
        }
    }

    /// Whether `seq` is the oldest queued job not cancelled: jobs start in
    /// the order they were asked for.
    fn first_in_line(&self, seq: u64) -> bool {
        self.jobs
            .values()
            .filter(|j| matches!(j.state, JobState::Queued) && !j.cancel.load(Ordering::SeqCst))
            .all(|j| j.seq >= seq)
    }
}

/// A job's slot, released when this is dropped: by [`JobSlot::finish`], or
/// by unwinding if the render panicked, which records the job as failed so
/// it never shows `running` forever.
struct JobSlot<'a> {
    server: &'a Server,
    id: &'a str,
    state: Option<JobState>,
}

impl JobSlot<'_> {
    fn finish(mut self, state: JobState) {
        self.state = Some(state);
    }
}

impl Drop for JobSlot<'_> {
    fn drop(&mut self) {
        let state = self.state.take().unwrap_or_else(|| {
            JobState::Failed(
                ToolError::new("internal", "internal error: the render panicked").into_result(),
            )
        });
        let mut jobs = lock(&self.server.jobs);
        jobs.running -= 1;
        jobs.finish(self.id, state);
        self.server.job_wake.notify_all();
    }
}

/// A job's thread: wait for a slot, render, record the outcome.
fn run_job(server: Arc<Server>, id: String, plan: RenderPlan) {
    let (cancel, pages) = {
        let mut jobs = lock(&server.jobs);
        let Some(job) = jobs.jobs.get(&id) else {
            return;
        };
        let (seq, cancel, pages) = (job.seq, Arc::clone(&job.cancel), Arc::clone(&job.pages));
        loop {
            // Both looked at under the lock that `cancel_render` and
            // shutdown notify under, so no wake-up is lost.
            if cancel.load(Ordering::SeqCst) {
                jobs.finish(&id, JobState::Cancelled);
                // The next in line may be waiting on this one.
                server.job_wake.notify_all();
                return;
            }
            if jobs.running < server.cfg.max_jobs && jobs.first_in_line(seq) {
                break;
            }
            jobs = server
                .job_wake
                .wait(jobs)
                .unwrap_or_else(PoisonError::into_inner);
        }
        jobs.running += 1;
        if let Some(job) = jobs.jobs.get_mut(&id) {
            job.state = JobState::Running;
        }
        // Leaving the queue makes the next job first in line, and a slot may
        // still be free for it: wake it, or it would wait for a job to end.
        server.job_wake.notify_all();
        (cancel, pages)
    };
    let slot = JobSlot {
        server: &server,
        id: &id,
        state: None,
    };
    let cfg = server.cfg.for_job(cancel, pages);
    let state = match render_run(&cfg, &plan) {
        Ok(content) => JobState::Done(json!({ "content": content, "isError": false })),
        Err(e) if e.kind == "cancelled" => JobState::Cancelled,
        Err(e) => JobState::Failed(e.into_result()),
    };
    slot.finish(state);
}

/// A successful job-tool result: the facts as JSON text (what a model
/// reads) and as `structuredContent` (for a client that wants fields).
fn job_result(facts: Value) -> Value {
    json!({
        "content": [text_content(&facts.to_string())],
        "structuredContent": facts,
        "isError": false,
    })
}

fn unknown_job(cfg: &Config, id: &str) -> ToolError {
    ToolError::new(
        "unknown_job",
        format!(
            "no job {id:?}: never started, or finished and dropped (a finished job is kept \
             {}s, and at most {MAX_HELD_JOBS} jobs are kept)",
            cfg.job_ttl.as_secs()
        ),
    )
    .with("job_id", json!(id))
}

fn job_id(args: &Value) -> Result<&str, ToolError> {
    optional_str(args, "job_id")?.ok_or_else(|| ToolError::invalid("job_id is required"))
}

/// What `render_status` says about a job.
fn job_facts(id: &str, job: &Job) -> Value {
    let end = job.finished.unwrap_or_else(Instant::now);
    let elapsed = end.saturating_duration_since(job.created).as_secs_f64();
    let mut facts = json!({
        "job_id": id,
        "state": job.state.name(),
        "elapsed_seconds": (elapsed * 10.0).round() / 10.0,
        "pages_done": job.pages.load(Ordering::SeqCst),
    });
    if job.finished.is_none() && job.cancel.load(Ordering::SeqCst) {
        facts["cancel_requested"] = json!(true);
    }
    if let JobState::Failed(result) = &job.state {
        facts["error"] = result["structuredContent"]["error"].clone();
    }
    facts
}

fn start_render(server: &Arc<Server>, args: &Value) -> Result<Value, ToolError> {
    let cfg = &server.cfg;
    // Every argument check `render_postscript` makes happens now, so a bad
    // request is refused here, not in the job.
    let plan = render_plan(cfg, args)?;
    let id = {
        let mut jobs = lock(&server.jobs);
        jobs.purge(cfg.job_ttl);
        if jobs.jobs.len() >= MAX_HELD_JOBS {
            let oldest = jobs
                .jobs
                .iter()
                .filter(|(_, j)| j.finished.is_some())
                .min_by_key(|(_, j)| j.seq)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    jobs.jobs.remove(&k);
                }
                None => {
                    return Err(ToolError::new(
                        "busy",
                        format!(
                            "{MAX_HELD_JOBS} jobs are queued or running, this server's limit; \
                             wait for one to finish or cancel one"
                        ),
                    )
                    .with("limit_jobs", json!(MAX_HELD_JOBS)));
                }
            }
        }
        jobs.next += 1;
        let seq = jobs.next;
        let id = format!("job-{seq}");
        jobs.jobs.insert(
            id.clone(),
            Job {
                seq,
                created: Instant::now(),
                finished: None,
                state: JobState::Queued,
                cancel: Arc::new(AtomicBool::new(false)),
                pages: Arc::new(AtomicU64::new(0)),
            },
        );
        id
    };
    let job_server = Arc::clone(server);
    let job = id.clone();
    let spawned = std::thread::Builder::new()
        .name("pscat-mcp-job".into())
        .spawn(move || run_job(job_server, job, plan));
    match spawned {
        Ok(handle) => server.track(handle),
        Err(e) => {
            lock(&server.jobs).jobs.remove(&id);
            return Err(ToolError::new(
                "internal",
                format!("cannot start a thread: {e}"),
            ));
        }
    }
    Ok(job_result(json!({
        "job_id": id,
        "state": "queued",
        "job_timeout_seconds": cfg.job_timeout.as_secs(),
        "next": "poll render_status with this job_id until its state is done, failed or \
                 cancelled, then call render_result",
    })))
}

fn render_status(server: &Arc<Server>, args: &Value) -> Result<Value, ToolError> {
    let id = job_id(args)?;
    let mut jobs = lock(&server.jobs);
    jobs.purge(server.cfg.job_ttl);
    let job = jobs
        .jobs
        .get(id)
        .ok_or_else(|| unknown_job(&server.cfg, id))?;
    Ok(job_result(job_facts(id, job)))
}

fn render_result(server: &Arc<Server>, args: &Value) -> Result<Value, ToolError> {
    let id = job_id(args)?;
    let mut jobs = lock(&server.jobs);
    jobs.purge(server.cfg.job_ttl);
    let job = jobs
        .jobs
        .get(id)
        .ok_or_else(|| unknown_job(&server.cfg, id))?;
    match &job.state {
        JobState::Done(result) | JobState::Failed(result) => Ok(result.clone()),
        JobState::Cancelled => Err(ToolError::new(
            "cancelled",
            format!("job {id} was cancelled; it has no result"),
        )
        .with("job_id", json!(id))),
        JobState::Queued | JobState::Running => Err(ToolError::new(
            "job_not_finished",
            format!(
                "job {id} is {}; poll render_status until it is done",
                job.state.name()
            ),
        )
        .with("job_id", json!(id))
        .with("state", json!(job.state.name()))),
    }
}

fn cancel_render(server: &Arc<Server>, args: &Value) -> Result<Value, ToolError> {
    let id = job_id(args)?;
    let mut jobs = lock(&server.jobs);
    jobs.purge(server.cfg.job_ttl);
    let job = jobs
        .jobs
        .get(id)
        .ok_or_else(|| unknown_job(&server.cfg, id))?;
    if job.finished.is_none() {
        job.cancel.store(true, Ordering::SeqCst);
        // Under the lock: a queued job wakes and finds its flag set.
        server.job_wake.notify_all();
    }
    Ok(job_result(job_facts(id, job)))
}

fn handwrite(cfg: &Config, args: &Value) -> Result<Value, ToolError> {
    let text = required_text(cfg, args, "text")?;
    let dest = optional_str(args, "out_path")?
        .map(|p| confine(cfg, p, "png"))
        .transpose()?;
    // The note's height is auto-sized inside the child, so the width is
    // checked here and the area there (`--max-pixels`, #711); an area over
    // the cap comes back as a recognisable refusal, mapped below.
    let dpi = optional_number(args, "dpi", 9.0, 1200.0)?.unwrap_or(72.0);
    let width = optional_int(args, "width", 1, 8000)?.unwrap_or(612);
    let (w_px, _) = device_pixels((width as u32, 1), dpi as f32);
    if w_px > 8000 || w_px > cfg.max_pixels {
        return Err(ToolError::new(
            "output_too_large",
            format!(
                "the note would be {w_px} pixels wide; this server draws at most 8000 a side \
                 and {} pixels in all (--max-pixels). Lower dpi or width.",
                cfg.max_pixels
            ),
        )
        .with("limit_pixels", json!(cfg.max_pixels))
        .with("width_px", json!(w_px)));
    }

    let scratch = ScratchDir::new()?;
    let out = scratch.path().join("note.png");
    let mut cmd = Command::new(pscat_bin()?);
    cmd.arg("handwrite").arg("-o").arg(&out);
    cmd.arg("--max-pixels").arg(cfg.max_pixels.to_string());
    // The same flags every other child gets (#701): the step, time and
    // memory limits, the read confinement, and `--exit-with-parent`. A
    // note's two programs (the measuring pass and the render) each run under
    // them, so `--timeout`'s hard kill stays the backstop, not the only bound.
    cfg.interpreter_args(&mut cmd);
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
        if let Some((w, h, _)) = pscat::limits::parse_pixel_cap_message(&run.stderr) {
            return Err(too_many_pixels(cfg, "the note", w, h));
        }
        return Err(failed_run(
            "handwrite_failed",
            format!("handwrite failed:\n{}", run.stderr),
            &run.stderr,
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
    content.extend(warnings_content(&run.stderr));
    if let Some(written) = written {
        content.push(text_content(&wrote_message(&written)));
    }
    Ok(Value::Array(content))
}

/// The default for `--max-latex-bytes`: the longest `latex` `chalkboard_math`
/// takes, in bytes. A ten-row derivation is about 1 KB and a board that size
/// is already crowded; four times that (about 40 rows of a Taylor expansion)
/// rendered in 25 s on the machine it was measured on with the default
/// board, inside the default time limit (#688); past that the time limit
/// stops it.
const DEFAULT_MAX_LATEX_BYTES: usize = 4096;

/// The default for `--max-pixels` (#688, #711): the largest page, board or
/// sheet in device pixels (width x height at the chosen dpi), for every tool
/// that allocates a canvas. An 8K screen (7680x4320) or an A2 poster at 300
/// dpi (about 35 million) fits; a 1920x1200 pt board at 300 dpi (40
/// million) peaked at 0.63 GB resident under the server's limits (#688). The
/// interpreter's own ceiling, 8000 pixels a side (64 million), still applies
/// and is the flag's maximum; this cap is headroom on memory and time below
/// it (the canvas is allocated before `--max-memory` starts counting).
const DEFAULT_MAX_PIXELS: u64 = 40_000_000;

/// The pixel size a page of `page` points renders at `dpi`: the interpreter's
/// own rounding (`Interp::with_page_scaled`), repeated here so that a page
/// over the cap is refused before any program runs. A child's PNG that comes
/// back at another size is an internal error, so the two cannot drift apart
/// unnoticed.
fn device_pixels(page: (u32, u32), dpi: f32) -> (u64, u64) {
    let px = |pt: u32| ((pt as f32 * (dpi / 72.0)).round() as u64).max(1);
    (px(page.0), px(page.1))
}

/// `output_too_large` for a `w`x`h`-pixel canvas the server will not draw:
/// over `--max-pixels` or 8000 pixels a side. `what` is "the page", "the
/// board"...; the fields are the same for every tool.
fn too_many_pixels(cfg: &Config, what: &str, w: u64, h: u64) -> ToolError {
    ToolError::new(
        "output_too_large",
        format!(
            "{what} would be {w}x{h} pixels; this server draws at most {} pixels and 8000 a \
             side (--max-pixels). Lower dpi, width or height.",
            cfg.max_pixels
        ),
    )
    .with("limit_pixels", json!(cfg.max_pixels))
    .with("pixels", json!(w * h))
    .with("width_px", json!(w))
    .with("height_px", json!(h))
}

/// Whether a `w`x`h`-pixel canvas is within the caps.
fn pixels_allowed(cfg: &Config, w: u64, h: u64) -> bool {
    w <= 8000 && h <= 8000 && w * h <= cfg.max_pixels
}

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

fn chalkboard_math(cfg: &Config, args: &Value) -> Result<Value, ToolError> {
    let latex = required_text(cfg, args, "latex")?;
    let max_latex = cfg.max_latex;
    if latex.len() > max_latex {
        return Err(ToolError::new(
            "input_too_large",
            format!(
                "latex is {} bytes; this server takes at most {max_latex} (a board that \
                 full is unreadable: split it over several boards, or raise \
                 --max-latex-bytes)",
                latex.len()
            ),
        )
        .with("limit", json!(max_latex))
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
    let (w, h) = device_pixels((board.width, board.height), board.dpi);
    if !pixels_allowed(cfg, w, h) {
        return Err(too_many_pixels(cfg, "the board", w, h));
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
    content.extend(warnings_content(&run.stderr));
    if let Some(written) = written {
        content.push(text_content(&wrote_message(&written)));
    }
    Ok(Value::Array(content))
}

// --- review_sheet (#708) ----------------------------------------------

/// Most cells (dpi entries, over all rows) one sheet may ask for. Each is a
/// child process under the call's one time budget; the logo sheet is 12.
const MAX_SHEET_CELLS: usize = 32;

/// Longest row label, in characters.
const MAX_SHEET_LABEL: usize = 200;

/// Largest `zoom`, as `pscat sheet --dpi 9x16`.
const MAX_SHEET_ZOOM: u32 = 16;

/// Row label and caption type sizes in pixels: `pscat sheet`'s own
/// (`src/subcmd/sheet.rs`), so the two tools draw the same sheet.
const SHEET_LABEL_PX: u32 = 16;
const SHEET_CAPTION_PX: u32 = 12;

/// The least time worth giving a cell. Anything positive keeps the child's
/// own `--max-time` from rounding to zero (which means no limit), and a
/// `--timeout 1` server still draws a small sheet; under this the call stops
/// with `timeout` instead of starting a cell it cannot finish.
const MIN_CELL_BUDGET: Duration = Duration::from_millis(100);

/// The longest error text kept per failed cell.
const MAX_CELL_NOTE: usize = 2000;

struct SheetEntry<'a> {
    source: &'a str,
    label: String,
    page: (u32, u32),
    /// `(dpi, zoom)` per cell; zoom 1 is no enlargement.
    cells: Vec<(f32, u32)>,
}

/// The schema of a dpi list, spelled out where it is used rather than as a
/// `$ref`: some clients do not resolve references in a tool's input schema.
fn sheet_dpi_schema(description: &str) -> Value {
    json!({
        "type": "array",
        "minItems": 1,
        "description": description,
        "items": {
            "anyOf": [
                { "type": "number", "minimum": 9, "maximum": 1200 },
                {
                    "type": "object",
                    "properties": {
                        "dpi": { "type": "number", "minimum": 9, "maximum": 1200 },
                        "zoom": { "type": "integer", "minimum": 2, "maximum": MAX_SHEET_ZOOM }
                    },
                    "required": ["dpi"]
                }
            ]
        }
    })
}

/// A dpi list: `[96, {"dpi": 9, "zoom": 4}]`.
fn sheet_cells(key: &str, v: &Value) -> Result<Vec<(f32, u32)>, ToolError> {
    let items = v
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| ToolError::invalid(format!("{key} must be a non-empty list of cells")))?;
    let dpi = |v: Option<&Value>| match v.and_then(Value::as_f64) {
        Some(d) if (9.0..=1200.0).contains(&d) => Ok(d as f32),
        _ => Err(ToolError::invalid(format!(
            "{key}: a dpi must be a number from 9 to 1200, got {}",
            v.map_or("nothing".to_string(), Value::to_string)
        ))),
    };
    items
        .iter()
        .map(|item| match item {
            Value::Number(_) => Ok((dpi(Some(item))?, 1)),
            Value::Object(o) => {
                let zoom = match o.get("zoom") {
                    None | Some(Value::Null) => 1,
                    Some(z) => z
                        .as_u64()
                        .filter(|z| (2..=u64::from(MAX_SHEET_ZOOM)).contains(z))
                        .ok_or_else(|| {
                            ToolError::invalid(format!(
                                "{key}: zoom must be a whole number from 2 to {MAX_SHEET_ZOOM}, got {z}"
                            ))
                        })? as u32,
                };
                Ok((dpi(o.get("dpi"))?, zoom))
            }
            other => Err(ToolError::invalid(format!(
                "{key}: a cell is a dpi number or {{\"dpi\", \"zoom\"}}, got {other}"
            ))),
        })
        .collect()
}

fn review_sheet(cfg: &Config, args: &Value) -> Result<Value, ToolError> {
    use pscat::contact_sheet::{self as sheet, CellSize, Layout, RowSize};

    let started = Instant::now();
    let max_pixels = cfg.max_pixels;
    let items = match args.get("entries") {
        Some(Value::Array(a)) if !a.is_empty() => a,
        _ => return Err(ToolError::invalid("entries must be a non-empty list")),
    };
    let default_page = (
        optional_int(args, "width", 1, 8000)?.unwrap_or(612) as u32,
        optional_int(args, "height", 1, 8000)?.unwrap_or(792) as u32,
    );
    let default_cells = match args.get("dpi") {
        None | Some(Value::Null) => vec![(72.0, 1)],
        Some(v) => sheet_cells("dpi", v)?,
    };
    let labels = optional_bool(args, "labels")?.unwrap_or(true);
    let sizes = optional_bool(args, "sizes")?.unwrap_or(true);
    let dest = optional_str(args, "out_path")?
        .map(|p| confine(cfg, p, "png"))
        .transpose()?;

    let mut entries = Vec::with_capacity(items.len());
    let mut source_bytes = 0usize;
    for (i, item) in items.iter().enumerate() {
        let which = format!("entries[{i}]");
        if !item.is_object() {
            return Err(ToolError::invalid(format!("{which} must be an object")));
        }
        let source = match item.get("source") {
            Some(Value::String(s)) => s.as_str(),
            None | Some(Value::Null) => {
                return Err(ToolError::invalid(format!("{which}.source is required")));
            }
            Some(_) => {
                return Err(ToolError::invalid(format!(
                    "{which}.source must be a string"
                )));
            }
        };
        source_bytes = source_bytes.saturating_add(source.len());
        let label = match optional_str(item, "label")? {
            Some(l) if l.chars().count() > MAX_SHEET_LABEL => {
                return Err(ToolError::invalid(format!(
                    "{which}.label is over {MAX_SHEET_LABEL} characters"
                )));
            }
            Some(l) => l.to_string(),
            None => format!("program {}", i + 1),
        };
        let page = (
            optional_int(item, "width", 1, 8000)?.map_or(default_page.0, |w| w as u32),
            optional_int(item, "height", 1, 8000)?.map_or(default_page.1, |h| h as u32),
        );
        let cells = match item.get("dpi") {
            None | Some(Value::Null) => default_cells.clone(),
            Some(v) => sheet_cells(&format!("{which}.dpi"), v)?,
        };
        entries.push(SheetEntry {
            source,
            label,
            page,
            cells,
        });
    }
    if source_bytes > cfg.max_input {
        return Err(ToolError::new(
            "input_too_large",
            format!(
                "the sources are {source_bytes} bytes in all; this server accepts at most {}",
                cfg.max_input
            ),
        )
        .with("limit", json!(cfg.max_input))
        .with("size", json!(source_bytes)));
    }
    let n_cells: usize = entries.iter().map(|e| e.cells.len()).sum();
    if n_cells > MAX_SHEET_CELLS {
        return Err(ToolError::new(
            "output_too_large",
            format!("{n_cells} cells is over this tool's limit of {MAX_SHEET_CELLS}"),
        )
        .with("limit_cells", json!(MAX_SHEET_CELLS))
        .with("cells", json!(n_cells)));
    }

    // Every size is known before a program runs, so a sheet over a cap
    // fails at once and costs nothing: the cells' own sizes by arithmetic,
    // the text by the label renderer, the sheet by `Layout`.
    let too_large = |message: String, pixels: u64| {
        ToolError::new("output_too_large", message)
            .with("limit_pixels", json!(max_pixels))
            .with("pixels", json!(pixels))
    };
    let text = |t: &str, px: u32, bold: bool| {
        sheet::render_text(t, px, bold).map_err(|e| ToolError::new("internal", e))
    };
    let mut rendered_pixels = 0u64;
    let mut headers = Vec::new();
    let mut captions = Vec::new();
    let mut row_sizes = Vec::new();
    for e in &entries {
        let header = if labels {
            text(&e.label, SHEET_LABEL_PX, true)?
        } else {
            None
        };
        let dims = |p: Option<&tiny_skia::Pixmap>| p.map_or((0, 0), |p| (p.width(), p.height()));
        let mut row = RowSize {
            header: dims(header.as_ref()),
            cells: Vec::new(),
        };
        let mut row_captions = Vec::new();
        for &(dpi, zoom) in &e.cells {
            let (w, h) = device_pixels(e.page, dpi);
            let side = u64::from(sheet::MAX_SIDE_PX);
            if w * u64::from(zoom) > side || h * u64::from(zoom) > side {
                return Err(too_large(
                    format!(
                        "{}: a {}x{} pt page at {dpi} dpi is {w}x{h} pixels{}; a cell is at most \
                         {side} pixels on a side",
                        e.label,
                        e.page.0,
                        e.page.1,
                        if zoom > 1 {
                            format!(
                                ", {}x{} shown at {zoom}x",
                                w * u64::from(zoom),
                                h * u64::from(zoom)
                            )
                        } else {
                            String::new()
                        }
                    ),
                    w * h,
                ));
            }
            rendered_pixels += w * h;
            if rendered_pixels > max_pixels {
                return Err(too_large(
                    format!(
                        "the cells are over {max_pixels} pixels in all, this tool's limit. \
                         Lower a dpi, a page size or the number of cells."
                    ),
                    rendered_pixels,
                ));
            }
            let caption = if sizes {
                let note = if zoom > 1 {
                    format!(" (shown at {zoom}x)")
                } else {
                    String::new()
                };
                text(&format!("{w}x{h} px{note}"), SHEET_CAPTION_PX, false)?
            } else {
                None
            };
            row.cells.push(CellSize {
                image: ((w as u32) * zoom, (h as u32) * zoom),
                caption: dims(caption.as_ref()),
            });
            row_captions.push(caption);
        }
        headers.push(header);
        captions.push(row_captions);
        row_sizes.push(row);
    }
    let layout = Layout::new(&row_sizes)
        .map_err(|m| too_large(m, 0).with("limit_side_px", json!(sheet::MAX_SIDE_PX)))?;
    let sheet_pixels_total = u64::from(layout.width) * u64::from(layout.height);
    if sheet_pixels_total > max_pixels {
        return Err(too_large(
            format!(
                "the sheet would be {}x{} pixels; this tool draws at most {max_pixels} \
                 pixels. Lower a dpi, a page size or the number of cells.",
                layout.width, layout.height
            ),
            sheet_pixels_total,
        )
        .with("width_px", json!(layout.width))
        .with("height_px", json!(layout.height)));
    }
    let mut out = layout
        .new_sheet()
        .map_err(|m| ToolError::new("internal", m))?;

    // Cells stream into the sheet, one frame in memory at a time. Each is
    // rendered by its own child under the interpreter's limits, and every
    // child gets what is left of the one call's time budget.
    let scratch = ScratchDir::new()?;
    let bin = pscat_bin()?;
    let mut notes = Vec::new();
    // Stderr of the cells that succeeded (see `warnings_content`).
    let mut cell_stderr = String::new();
    let mut n = 0usize;
    for (r, e) in entries.iter().enumerate() {
        if let Some(h) = &headers[r] {
            let (x, y) = layout.header_xy[r];
            sheet::blit_at(&mut out, x, y, h);
        }
        for (c, &(dpi, zoom)) in e.cells.iter().enumerate() {
            n += 1;
            let left = cfg.timeout.saturating_sub(started.elapsed());
            if left < MIN_CELL_BUDGET {
                return Err(sheet_timeout(cfg));
            }
            let budget = Config {
                timeout: left,
                ..cfg.clone()
            };
            let png = scratch.path().join(format!("cell-{n}.png"));
            let mut cmd = Command::new(&bin);
            budget.interpreter_args(&mut cmd);
            cmd.arg("--page")
                .arg(format!("{}x{}", e.page.0, e.page.1))
                .arg("--dpi")
                .arg(number_arg(f64::from(dpi)))
                .arg("--png")
                .arg(&png)
                .arg("-");
            let run = match run_child(&budget, &mut cmd, e.source.as_bytes()) {
                Ok(run) => run,
                Err(t) if t.kind == "timeout" => return Err(sheet_timeout(cfg)),
                Err(other) => return Err(other),
            };
            // The last page, as `pscat sheet` takes it: a program that shows
            // several pages contributes its final one.
            let Some(page) = collect_pages(&png).pop() else {
                return Err(failed_run(
                    "render_failed",
                    format!(
                        "{} at {dpi} dpi produced no page:\n{}",
                        e.label,
                        run.stderr.trim_end()
                    ),
                    &run.stderr,
                ));
            };
            if !run.success {
                let mut why = run.stderr.trim_end().to_string();
                if why.len() > MAX_CELL_NOTE {
                    let mut end = MAX_CELL_NOTE;
                    while !why.is_char_boundary(end) {
                        end -= 1;
                    }
                    why.truncate(end);
                    why.push_str("...");
                }
                notes.push(format!("{} at {dpi} dpi failed:\n{why}", e.label));
            } else {
                cell_stderr.push_str(&run.stderr);
                cell_stderr.push('\n');
            }
            let bytes = std::fs::read(&page).map_err(|er| format!("read {page:?}: {er}"))?;
            let _ = std::fs::remove_file(&page);
            let frame = tiny_skia::Pixmap::decode_png(&bytes)
                .map_err(|er| format!("decode the cell for {}: {er}", e.label))?;
            let want = device_pixels(e.page, dpi);
            if (u64::from(frame.width()), u64::from(frame.height())) != want {
                return Err(ToolError::new(
                    "internal",
                    format!(
                        "{} at {dpi} dpi came back {}x{} pixels, not the {}x{} planned",
                        e.label,
                        frame.width(),
                        frame.height(),
                        want.0,
                        want.1
                    ),
                ));
            }
            let frame = if zoom > 1 {
                sheet::zoom_nearest(&frame, zoom)
                    .ok_or_else(|| ToolError::new("internal", "cell too large to zoom"))?
            } else {
                frame
            };
            let (x, y) = layout.image_xy[r][c];
            sheet::frame(&mut out, x, y, frame.width(), frame.height());
            sheet::blit_at(&mut out, x, y, &frame);
            if let Some(cap) = &captions[r][c] {
                let (cx, cy) = layout.caption_xy[r][c];
                sheet::blit_at(&mut out, cx, cy, cap);
            }
        }
    }
    let sheet_png = scratch.path().join("sheet.png");
    out.save_png(&sheet_png)
        .map_err(|e| format!("write the sheet: {e}"))?;
    drop(out);
    let pages = [sheet_png];
    let written = match &dest {
        Some(dest) => Some(dest.write_pages(&pages)?),
        None => None,
    };
    let mut content = Vec::new();
    for note in notes {
        content.push(text_content(&format!("PostScript error in {note}")));
    }
    content.extend(warnings_content(&cell_stderr));
    for body in read_inline(cfg, &pages, written.as_deref())? {
        content.push(image_content(&body));
    }
    if let Some(written) = written {
        content.push(text_content(&wrote_message(&written)));
    }
    Ok(Value::Array(content))
}

fn sheet_timeout(cfg: &Config) -> ToolError {
    ToolError::new(
        "timeout",
        format!(
            "the sheet was not finished in {}s, this server's limit per call. Use fewer or \
             cheaper cells.",
            cfg.timeout.as_secs()
        ),
    )
    .with("limit_seconds", json!(cfg.timeout.as_secs()))
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
        // What the program wrote to stderr (`%stderr`, a font stand-in note)
        // follows the output as `Warnings:`, as it does for a render.
        let mut content = vec![text_content(&report)];
        content.extend(warnings_content(&run.stderr));
        return Ok(Value::Array(content));
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

/// The most warning lines one call returns, and the longest one.
const MAX_WARNING_LINES: usize = 8;
const MAX_WARNING_CHARS: usize = 300;

/// What a child that *succeeded* wrote to stderr, as one text content item
/// (`Warnings:` and the lines), or `None` when there is nothing to say: a
/// note such as a font falling back to a built-in face (#728) exits 0, and
/// would otherwise never reach the caller. Blank lines, `pscat: wrote ...`
/// (the child's own receipt, which names a scratch path), `pscat: progress`
/// lines (for the server) and lint lines (the tools that run `--lint` report
/// those under `Lint:`) are dropped; repeats
/// are shown once, in first-seen order; at most [`MAX_WARNING_LINES`] lines
/// of [`MAX_WARNING_CHARS`] characters each, so the reply stays bounded
/// however chatty a program is. A failed run reports its whole stderr in the
/// error instead.
fn warnings_content(stderr: &str) -> Option<Value> {
    let mut seen: Vec<String> = Vec::new();
    // Every distinct line, shown or not, so `more` counts distinct ones.
    let mut distinct = std::collections::HashSet::new();
    let mut more = 0usize;
    for line in stderr.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("pscat: wrote ")
            || line.starts_with("pscat: lint: ")
            || line.starts_with(PROGRESS_PREFIX)
        {
            continue;
        }
        let line = match line.char_indices().nth(MAX_WARNING_CHARS) {
            Some((end, _)) => format!("{}...", &line[..end]),
            None => line.to_string(),
        };
        if !distinct.insert(line.clone()) {
            continue;
        }
        if seen.len() < MAX_WARNING_LINES {
            seen.push(line);
        } else {
            more += 1;
        }
    }
    if seen.is_empty() {
        return None;
    }
    let mut text = format!("Warnings:\n{}", seen.join("\n"));
    if more > 0 {
        text.push_str(&format!("\n... and {more} more"));
    }
    Some(text_content(&text))
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
    if cfg.cancel.load(Ordering::SeqCst) {
        return Err(cancelled());
    }
    // One spawn at a time. Where std makes a child's pipes with `pipe()`
    // and a separate close-on-exec step (macOS), a fork on another thread
    // in between would inherit this child's stdin write end, and the child
    // would never see the end of its program.
    static SPAWN_LOCK: Mutex<()> = Mutex::new(());
    let spawned = {
        let _one = lock(&SPAWN_LOCK);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    };
    let mut child = spawned.map_err(|e| format!("cannot run {:?}: {e}", cmd.get_program()))?;
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
    let out = capture(child.stdout.take().expect("piped stdout"), &overflow, None);
    let err = capture(
        child.stderr.take().expect("piped stderr"),
        &overflow,
        cfg.progress.clone(),
    );

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
        if cfg.cancel.load(Ordering::SeqCst) {
            break Err("cancelled".into());
        }
        if started.elapsed() >= cfg.timeout {
            break Err("timeout".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // Every way out of the loop but a reaped exit kills and reaps here, so
    // no child is left running or as a zombie.
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
            stderr: without_progress(text(stderr)),
        }),
        Err(why) if why == "cancelled" => Err(cancelled()),
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

fn cancelled() -> ToolError {
    ToolError::new("cancelled", "cancelled by the client")
}

/// The line `pscat --progress` prints as each page is finished.
const PROGRESS_PREFIX: &str = "pscat: progress pages=";

/// A child's stderr without its `--progress` lines, which are for the
/// server, not the caller.
fn without_progress(stderr: String) -> String {
    if !stderr.contains(PROGRESS_PREFIX) {
        return stderr;
    }
    stderr
        .split_inclusive('\n')
        .filter(|l| !l.starts_with(PROGRESS_PREFIX))
        .collect()
}

/// Read a child stream on a thread, keeping at most [`MAX_CHILD_OUTPUT`]
/// bytes; past that, flag the overflow and keep draining (so the child
/// never blocks on a full pipe) until it is killed. With `progress`, each
/// `pscat: progress pages=N` line stores N there as it arrives.
fn capture(
    mut pipe: impl Read + Send + 'static,
    overflow: &Arc<AtomicBool>,
    progress: Option<Arc<AtomicU64>>,
) -> std::thread::JoinHandle<Vec<u8>> {
    let overflow = Arc::clone(overflow);
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut chunk = [0u8; 8192];
        // The start of the line being read, enough to hold a progress line.
        let mut line: Vec<u8> = Vec::new();
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Some(progress) = &progress {
                        for &b in &chunk[..n] {
                            if b == b'\n' {
                                if let Some(pages) = line
                                    .strip_prefix(PROGRESS_PREFIX.as_bytes())
                                    .and_then(|d| std::str::from_utf8(d).ok())
                                    .and_then(|d| d.trim_end_matches('\r').parse::<u64>().ok())
                                {
                                    progress.store(pages, Ordering::SeqCst);
                                }
                                line.clear();
                            } else if line.len() < 64 {
                                line.push(b);
                            }
                        }
                    }
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

    #[cfg(unix)]
    fn test_config(args: &[&str]) -> Config {
        Config::from_args_env(args.iter().map(|a| a.to_string()), |_| None)
            .expect("valid flags")
            .expect("not --help")
    }

    /// `from_args_env` over a fixed environment (issue #756); never the
    /// process's own, which a developer's shell may have set.
    fn config_env(args: &[&str], env: &[(&str, &str)]) -> Result<Option<Config>, String> {
        Config::from_args_env(args.iter().map(|a| a.to_string()), |name| {
            env.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| std::ffi::OsString::from(*v))
        })
    }

    #[test]
    fn env_vars_set_limits() {
        let cfg = config_env(
            &[],
            &[
                ("PSCAT_MCP_TIMEOUT", "1800"),
                ("PSCAT_MCP_JOB_TIMEOUT", "7200"),
                ("PSCAT_MCP_MAX_STEPS", "0"),
                ("PSCAT_MCP_MAX_PIXELS", "64000000"),
                ("PSCAT_MCP_MAX_MEMORY", " 4096 "),
                ("PSCAT_MCP_MAX_JOBS", "4"),
                ("PSCAT_MCP_MAX_BANDS", "16"),
            ],
        )
        .unwrap()
        .unwrap();
        assert_eq!(cfg.timeout, Duration::from_secs(1800));
        assert_eq!(cfg.job_timeout, Duration::from_secs(7200));
        assert_eq!(cfg.max_steps, 0);
        assert_eq!(cfg.max_pixels, 64_000_000);
        assert_eq!(cfg.max_memory_mib, 4096);
        assert_eq!(cfg.max_jobs, 4);
        assert_eq!(cfg.max_bands, 16);
        // Untouched options keep their defaults.
        assert_eq!(cfg.job_ttl, Duration::from_secs(3600));
        assert_eq!(cfg.max_input, 1 << 20);
        assert_eq!(cfg.max_latex, DEFAULT_MAX_LATEX_BYTES);
    }

    /// The naming rule `USAGE` and the guide state: `PSCAT_MCP_` plus the flag
    /// in capitals with underscores.
    #[test]
    fn env_names_follow_the_flag_names() {
        for o in NUMERIC_OPTIONS {
            let expect = format!(
                "PSCAT_MCP_{}",
                o.flag
                    .trim_start_matches("--")
                    .to_uppercase()
                    .replace('-', "_")
            );
            assert_eq!(o.env, expect);
        }
    }

    #[test]
    fn defaults_without_env_match_the_documented_ones() {
        let cfg = config_env(&[], &[]).unwrap().unwrap();
        assert_eq!(cfg.timeout, Duration::from_secs(60));
        assert_eq!(cfg.max_steps, pscat::limits::UNTRUSTED_MAX_STEPS);
        assert_eq!(cfg.max_pixels, DEFAULT_MAX_PIXELS);
        assert_eq!(cfg.max_memory_mib, 1024);
        assert_eq!(cfg.max_image, 3_750_000);
        assert_eq!(cfg.max_jobs, 2);
        assert_eq!(cfg.max_bands, 4);
    }

    #[test]
    fn flag_overrides_env() {
        let cfg = config_env(
            &["--timeout", "90", "--max-steps", "5"],
            &[
                ("PSCAT_MCP_TIMEOUT", "1800"),
                ("PSCAT_MCP_MAX_STEPS", "0"),
                ("PSCAT_MCP_MAX_PIXELS", "1000"),
            ],
        )
        .unwrap()
        .unwrap();
        assert_eq!(cfg.timeout, Duration::from_secs(90));
        assert_eq!(cfg.max_steps, 5);
        // No flag for this one: the variable applies.
        assert_eq!(cfg.max_pixels, 1000);
    }

    #[test]
    fn bad_env_is_refused_with_the_variable_named() {
        for (var, bad) in [
            ("PSCAT_MCP_TIMEOUT", "abc"),
            ("PSCAT_MCP_TIMEOUT", "0"),
            ("PSCAT_MCP_TIMEOUT", "86401"),
            ("PSCAT_MCP_TIMEOUT", "-5"),
            ("PSCAT_MCP_TIMEOUT", "1.5"),
            ("PSCAT_MCP_MAX_PIXELS", "64000001"),
            ("PSCAT_MCP_MAX_PIXELS", "0"),
            ("PSCAT_MCP_MAX_JOBS", "17"),
            ("PSCAT_MCP_MAX_STEPS", "lots"),
            ("PSCAT_MCP_MAX_MEMORY", "1gb"),
        ] {
            let err = config_env(&[], &[(var, bad)]).err().unwrap_or_else(|| {
                panic!("{var}={bad} should be refused");
            });
            assert!(
                err.starts_with(var) && err.contains("whole number") && err.contains(bad),
                "{err}"
            );
        }
        // Checked even when a flag would override it.
        let err = config_env(&["--timeout", "90"], &[("PSCAT_MCP_TIMEOUT", "abc")])
            .err()
            .expect("refused");
        assert!(err.starts_with("PSCAT_MCP_TIMEOUT"), "{err}");
    }

    #[test]
    fn empty_env_is_unset_and_help_ignores_env() {
        let cfg = config_env(
            &[],
            &[("PSCAT_MCP_TIMEOUT", ""), ("PSCAT_MCP_MAX_STEPS", "  ")],
        )
        .unwrap()
        .unwrap();
        assert_eq!(cfg.timeout, Duration::from_secs(60));
        // `--help` and `--version` answer even with a bad variable set.
        assert!(
            config_env(&["--help"], &[("PSCAT_MCP_TIMEOUT", "abc")])
                .unwrap()
                .is_none()
        );
        assert!(
            config_env(&["-V"], &[("PSCAT_MCP_TIMEOUT", "abc")])
                .unwrap()
                .is_none()
        );
    }

    /// Reads and writes stay flag-only: no variable adds a read root or
    /// moves the output root (#740, #756).
    #[test]
    fn no_env_form_for_reads_or_writes() {
        let dir = std::env::temp_dir();
        let d = dir.to_str().unwrap();
        let with = config_env(
            &[],
            &[
                ("PSCAT_MCP_ALLOW_READ", d),
                ("PSCAT_MCP_READ_ROOT", d),
                ("PSCAT_MCP_OUT_ROOT", d),
            ],
        )
        .unwrap()
        .unwrap();
        let without = config_env(&[], &[]).unwrap().unwrap();
        assert_eq!(with.read_roots, without.read_roots);
        assert_eq!(with.out_root, without.out_root);
        assert!(
            NUMERIC_OPTIONS
                .iter()
                .all(|o| o.flag != "--allow-read" && o.flag != "--out-root")
        );
    }

    /// Issue #725: the last-resort kill at `--timeout`, for a child stuck
    /// where the interpreter's own limits never look (here a child that is
    /// not pscat at all). The bound is the counterfactual: unkilled, the
    /// child would run its full 60 s.
    #[cfg(unix)]
    #[test]
    fn the_timeout_backstop_kills_a_child_that_ignores_every_limit() {
        let cfg = test_config(&["--timeout", "1"]);
        let started = Instant::now();
        let mut cmd = Command::new("sleep");
        cmd.arg("60");
        let err = run_child(&cfg, &mut cmd, b"").err().expect("killed");
        assert_eq!(err.kind, "timeout", "{}", err.message);
        assert_eq!(err.fields["limit_seconds"], 1);
        assert!(err.message.contains("limit per call"), "{}", err.message);
        assert!(
            started.elapsed() < Duration::from_secs(50),
            "killed, not waited out: {:?}",
            started.elapsed()
        );
    }

    /// Issue #622: a cancel flag set while the child runs kills it.
    #[cfg(unix)]
    #[test]
    fn a_cancelled_run_kills_its_child() {
        let cfg = test_config(&["--timeout", "600"]).for_call(Arc::new(AtomicBool::new(false)));
        let flag = Arc::clone(&cfg.cancel);
        let setter = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            flag.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let mut cmd = Command::new("sleep");
        cmd.arg("60");
        let err = run_child(&cfg, &mut cmd, b"").err().expect("cancelled");
        setter.join().unwrap();
        assert_eq!(err.kind, "cancelled", "{}", err.message);
        assert!(
            started.elapsed() < Duration::from_secs(50),
            "killed, not waited out: {:?}",
            started.elapsed()
        );
        // Already cancelled: nothing is started at all.
        let mut cmd = Command::new("sleep");
        cmd.arg("60");
        assert_eq!(
            run_child(&cfg, &mut cmd, b"").err().unwrap().kind,
            "cancelled"
        );
    }

    #[test]
    fn progress_lines_are_counted_and_kept_out_of_the_stderr_returned() {
        let progress = Arc::new(AtomicU64::new(0));
        let overflow = Arc::new(AtomicBool::new(false));
        let stderr = b"note: one\npscat: progress pages=1\nnote: two\npscat: progress pages=12\n";
        // A reader that hands the bytes over a few at a time, so a line is
        // split across reads.
        struct Trickle(&'static [u8]);
        impl Read for Trickle {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let n = self.0.len().min(5).min(buf.len());
                buf[..n].copy_from_slice(&self.0[..n]);
                self.0 = &self.0[n..];
                Ok(n)
            }
        }
        let kept = capture(Trickle(stderr), &overflow, Some(Arc::clone(&progress)))
            .join()
            .unwrap();
        assert_eq!(progress.load(Ordering::SeqCst), 12);
        assert_eq!(
            without_progress(String::from_utf8(kept).unwrap()),
            "note: one\nnote: two\n"
        );
    }

    #[test]
    fn queued_jobs_start_in_order_and_finished_ones_expire() {
        let job = |seq: u64, state: JobState, finished: Option<Instant>| Job {
            seq,
            created: Instant::now(),
            finished,
            state,
            cancel: Arc::new(AtomicBool::new(false)),
            pages: Arc::new(AtomicU64::new(0)),
        };
        let mut t = JobTable::default();
        t.jobs
            .insert("job-1".into(), job(1, JobState::Running, None));
        t.jobs
            .insert("job-2".into(), job(2, JobState::Queued, None));
        t.jobs
            .insert("job-3".into(), job(3, JobState::Queued, None));
        assert!(t.first_in_line(2));
        assert!(!t.first_in_line(3));
        // A cancelled job does not hold the line up.
        t.jobs["job-2"].cancel.store(true, Ordering::SeqCst);
        assert!(t.first_in_line(3));
        let long_ago = Instant::now()
            .checked_sub(Duration::from_secs(120))
            .expect("the clock is past two minutes");
        t.jobs
            .insert("job-4".into(), job(4, JobState::Cancelled, Some(long_ago)));
        t.jobs.insert(
            "job-5".into(),
            job(5, JobState::Cancelled, Some(Instant::now())),
        );
        t.purge(Duration::from_secs(60));
        let mut left: Vec<&str> = t.jobs.keys().map(String::as_str).collect();
        left.sort();
        assert_eq!(left, ["job-1", "job-2", "job-3", "job-5"]);
    }

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
    fn capped_offending_name_still_parses() {
        // The interpreter cuts a pathological name to 512 bytes plus a
        // marker with no `; ` or ` ]%%`, so the fields stay separable (#745).
        let mut it = pscat::Interp::new();
        let e = it.run_str(&"n".repeat(600)).unwrap_err();
        let line = it.error_report(&e);
        let f = ps_error_fields(&line).unwrap();
        assert_eq!(f["ps_error"], "undefined");
        assert_eq!(
            f["offending_command"],
            format!("{} ... truncated", "n".repeat(512))
        );
        assert_eq!(f["line"], 1);
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

    #[test]
    fn warnings_are_deduplicated_bounded_and_skip_the_childs_own_lines() {
        assert!(warnings_content("").is_none());
        assert!(
            warnings_content("pscat: wrote /tmp/x/page.png\npscat: lint: [blank-page] x\n\n")
                .is_none()
        );
        assert!(warnings_content("pscat: progress pages=3\n").is_none());
        let w = warnings_content("note: a\nnote: b\nnote: a\npscat: wrote /tmp/p.png\n").unwrap();
        assert_eq!(w["text"], "Warnings:\nnote: a\nnote: b");
        let many: String = (0..30).map(|i| format!("w{i}\n")).collect();
        let text = warnings_content(&many).unwrap()["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(text.lines().count(), 1 + MAX_WARNING_LINES + 1);
        assert!(text.ends_with("... and 22 more"), "{text}");
        let long = "\u{2603}".repeat(1000);
        let text = warnings_content(&long).unwrap()["text"]
            .as_str()
            .unwrap()
            .to_string();
        // Repeats of an omitted line are one omitted warning.
        let tail = format!("{many}{}", "w99\n".repeat(50));
        assert!(
            warnings_content(&tail).unwrap()["text"]
                .as_str()
                .unwrap()
                .ends_with("... and 23 more")
        );
        assert!(text.chars().count() < 330, "{}", text.chars().count());
    }
}
