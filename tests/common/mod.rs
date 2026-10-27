//! Helpers shared by the integration tests: PDF inspection, scratch
//! paths, CPU-time budgets.
#![allow(dead_code)] // each test crate that mounts this module uses a subset

/// The document as text with every page content stream inflated in
/// place (content streams are Flate-compressed, issue #244), so the
/// assertions below can read operators directly. Image XObjects (dict
/// has `/Subtype /Image`) are left compressed.
pub fn pdf_text(pdf: &[u8]) -> String {
    use flate2::read::ZlibDecoder;
    use std::io::Read as _;

    let find = |hay: &[u8], needle: &[u8], from: usize| {
        hay[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|i| i + from)
    };
    let mut out: Vec<u8> = Vec::new();
    let mut at = 0;
    while let Some(s) = find(pdf, b"stream\n", at) {
        let body = s + b"stream\n".len();
        // Skip "endstream" matches: a real stream keyword follows ">>\n".
        let dict_start = pdf[..s].windows(2).rposition(|w| w == b"<<").unwrap_or(0);
        let dict = String::from_utf8_lossy(&pdf[dict_start..s]).into_owned();
        if !dict.ends_with(">>\n") || dict.contains("/Subtype /Image") {
            out.extend_from_slice(&pdf[at..body]);
            at = body;
            continue;
        }
        let len: usize = dict
            .split("/Length ")
            .nth(1)
            .and_then(|r| r.split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .expect("stream /Length");
        out.extend_from_slice(&pdf[at..body]);
        let raw = &pdf[body..body + len];
        if dict.contains("/Filter /FlateDecode") {
            let mut inflated = Vec::new();
            ZlibDecoder::new(raw)
                .read_to_end(&mut inflated)
                .expect("content stream inflates");
            out.extend_from_slice(&inflated);
        } else {
            out.extend_from_slice(raw);
        }
        at = body + len;
    }
    out.extend_from_slice(&pdf[at..]);
    String::from_utf8_lossy(&out).into_owned()
}

// ---------------------------------------------------------------------
// Scratch paths (issue #487)
//
// Agents run `cargo test` in several worktrees at once, and the tests
// of one binary run on parallel threads. A fixed temp path
// (`pscat_corpus_x.png`) is then clobbered or deleted by a neighbour.
// Every test that writes a file goes through `scratch_dir`, so no test
// names a shared path. `tests/scratch_paths.rs` enforces it.
// ---------------------------------------------------------------------

/// Write `bytes` to a piped child's stdin, then close it. A child that
/// rejects its arguments exits without reading stdin, so the write can
/// hit a closed pipe (`BrokenPipe`); that is the child's business, and the
/// caller still asserts on its exit status and stderr (issue #630). Any
/// other write error is a real failure.
pub fn feed_stdin(child: &mut std::process::Child, bytes: &[u8]) {
    use std::io::Write;
    let mut stdin = child.stdin.take().expect("child stdin is piped");
    if let Err(e) = stdin.write_all(bytes) {
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe, "write stdin: {e}");
    }
}

/// A path as it can sit inside a PostScript string literal: `/` separators.
/// Windows temp paths contain `\`, which a PostScript string reads as an
/// escape (`\U`, `\t`, ...), and Windows accepts `/` everywhere (issue #601).
pub fn ps_path(p: &std::path::Path) -> String {
    p.to_str().expect("utf-8 path").replace('\\', "/")
}

/// A new, already created directory that no other call shares, in this
/// process or any other: `<temp>/pscat-test-<pid>/<tag>-<n>`. `tag` only
/// helps a human reading the temp dir; put a file under it with
/// `.join(name)`. Nothing is cleaned up unless the caller does it.
pub fn scratch_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut base = std::env::temp_dir();
    // Some harnesses point TMPDIR into target/, which is inside the
    // checkout; `identity_check.sh` refuses a scratch dir inside either tree.
    if base.starts_with(env!("CARGO_MANIFEST_DIR")) {
        base = std::path::PathBuf::from("/tmp");
    }
    let dir = base
        .join(format!("pscat-test-{}", std::process::id()))
        .join(format!("{tag}-{n}"));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

// ---------------------------------------------------------------------
// CPU-time budgets (issue #329)
//
// A render-budget or "this guard must not walk the path first" test
// measures CPU time of the test's own thread, never wall time. Wall time
// includes every moment the scheduler gave the core to someone else, so
// with several agents running gates on one machine (or a shared CI
// runner) a 1 s render read 2 s and the test went red for no reason.
//
// Thread CPU time, not process CPU time: cargo runs the tests of one
// file in parallel threads of one process, so `RUSAGE_SELF` would count
// the neighbours. The interpreter is single-threaded and the tests build
// and run it on the test thread, so the thread clock is exactly the work
// under test.
// ---------------------------------------------------------------------

use std::time::Duration;

/// CPU time this thread has consumed so far (user + system).
#[cfg(unix)]
pub fn thread_cpu_time() -> Duration {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid out-pointer; CLOCK_THREAD_CPUTIME_ID is
    // supported on Linux and macOS (10.12+).
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    assert_eq!(rc, 0, "clock_gettime(CLOCK_THREAD_CPUTIME_ID) failed");
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

/// Fallback where there is no per-thread CPU clock: wall time. Only the
/// unix runners (Linux, macOS) gate merges.
#[cfg(not(unix))]
pub fn thread_cpu_time() -> Duration {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

/// Runs `f` once and returns its value with the CPU time it used.
pub fn cpu_time_of<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t0 = thread_cpu_time();
    let value = f();
    (value, thread_cpu_time().saturating_sub(t0))
}

/// `PSCAT_TEST_BUDGET_SCALE` multiplies every CPU budget in the suite
/// (default 1). Set it far below 1 (for example `0.001`) to confirm a
/// budget test still fails when the budget is wrong: the negative check
/// the timing-test guidance asks for.
pub fn scale_budget(budget: Duration) -> Duration {
    match std::env::var("PSCAT_TEST_BUDGET_SCALE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
    {
        Some(k) if k.is_finite() && k >= 0.0 => budget.mul_f64(k),
        _ => budget,
    }
}

/// Best-of-`tries` CPU-time budget check. Runs `f` (which must rebuild
/// any state it mutates, and may assert on its own result) up to `tries`
/// times, stops at the first run under the (scaled) budget, and panics
/// unless the best run was under it. A slow outlier costs a retry; a real
/// regression is slow every time. Prints the per-try times so drift shows
/// before it fails. Returns the value of the fastest run.
pub fn within_cpu_budget<T>(
    what: &str,
    budget: Duration,
    tries: usize,
    mut f: impl FnMut() -> T,
) -> T {
    assert!(tries >= 1);
    let budget = scale_budget(budget);
    let mut best: Option<(T, Duration)> = None;
    let mut seen = Vec::new();
    for _ in 0..tries {
        let (value, cpu) = cpu_time_of(&mut f);
        seen.push(format!("{:.3}s", cpu.as_secs_f64()));
        if best.as_ref().is_none_or(|(_, b)| cpu < *b) {
            best = Some((value, cpu));
        }
        if cpu < budget {
            break;
        }
    }
    let (value, cpu) = best.expect("at least one try");
    eprintln!(
        "{what}: cpu {:.3}s of {:.3}s budget (tries: {})",
        cpu.as_secs_f64(),
        budget.as_secs_f64(),
        seen.join(", ")
    );
    assert!(
        cpu < budget,
        "{what}: best CPU time over {} tries was {cpu:?}, over the {budget:?} budget",
        seen.len()
    );
    value
}

// ---------------------------------------------------------------------
// Child-process deadlines (issue #841)
//
// A test that spawns pscat / pscat-mcp must never wait on it without a
// deadline: a child that does not exit (a regression in the limits, or a
// test written before the feature exists) blocks `wait_with_output()`
// forever, and if the runner is interrupted the orphan keeps spinning
// (one ran for 2.8 days). Every spawn in the suite goes through here.
//
// * `Command` extension methods stand in for `output()` / `status()`:
//   `output_deadline()` / `status_deadline()` use `default_deadline()` (60 s, or
//   `PSCAT_TEST_DEADLINE_SECS`), the `*_within(d)` forms take a per-call
//   deadline. They return `io::Result` like std (a spawn error is still an
//   `Err`); a deadline is a panic, after the child has been killed.
// * `wait_with_deadline(child, d)` is the same for a child the test spawned
//   itself (and fed with `feed_stdin`).
// * `Watchdog` guards a long-lived, interactive child (the MCP server).
//
// On the deadline the child's descendants are killed too (pscat-mcp runs
// every render in a pscat child of its own), found by parent pid with
// `pgrep -P`, never by a name pattern. The child stays in the test run's
// process group on purpose: a Ctrl-C of `cargo test` still reaches it.
// std only, plus `libc` (already a dev-dependency) for the descendant kill.
// Windows: the descendant kill and `Watchdog` are no-ops there (the child
// itself is still killed by `output_*`/`status_*`/`wait_*`); only the unix
// runners (Linux, macOS) gate merges.
// ---------------------------------------------------------------------

use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::Instant;

/// Default deadline for one child. Generous: it exists to stop a hang, not
/// to time a render, and a loaded machine (several gates at once) is slow.
/// `PSCAT_TEST_DEADLINE_SECS` overrides it for every call that does not
/// pass its own.
pub fn default_deadline() -> Duration {
    std::env::var("PSCAT_TEST_DEADLINE_SECS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s > 0.0)
        .map(Duration::from_secs_f64)
        .unwrap_or(Duration::from_secs(60))
}

/// `pid` and every process below it, found with `pgrep -P` (a no-op where
/// there is no pgrep). Parent pids only, so no other run's process matches.
#[cfg(unix)]
fn descendants(pid: u32) -> Vec<u32> {
    let mut found = Vec::new();
    let mut todo = vec![pid];
    while let Some(p) = todo.pop() {
        let Ok(out) = Command::new("pgrep").args(["-P", &p.to_string()]).output() else {
            break;
        };
        for kid in String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .filter_map(|s| s.parse::<u32>().ok())
        {
            found.push(kid);
            todo.push(kid);
        }
    }
    found
}

/// SIGKILL `pid` and its descendants (the descendants are listed first:
/// once the parent dies they are re-parented and can no longer be found).
#[cfg(unix)]
fn kill_tree(pid: u32) {
    let kids = descendants(pid);
    for p in std::iter::once(pid).chain(kids) {
        // SAFETY: plain signal send to a pid we spawned or found below it.
        unsafe { libc::kill(p as libc::pid_t, libc::SIGKILL) };
    }
}

#[cfg(not(unix))]
fn kill_tree(_pid: u32) {}

#[cfg(not(unix))]
fn descendants(_pid: u32) -> Vec<u32> {
    Vec::new()
}

/// SIGKILL these pids (a snapshot of a child's descendants).
#[cfg(unix)]
fn kill_pids(pids: &[u32]) {
    for &p in pids {
        // SAFETY: plain signal send; the pids were descendants moments ago.
        unsafe { libc::kill(p as libc::pid_t, libc::SIGKILL) };
    }
}

#[cfg(not(unix))]
fn kill_pids(_pids: &[u32]) {}

fn describe(cmd: &str) -> String {
    let one_line = cmd.replace('\n', "\\n");
    if one_line.chars().count() > 300 {
        let head: String = one_line.chars().take(300).collect();
        format!("{head}...")
    } else {
        one_line
    }
}

/// Kill `child` (and what it spawned), reap it, and fail the test.
fn kill_and_panic(child: &mut Child, what: &str, deadline: Duration) -> ! {
    kill_tree(child.id());
    let _ = child.kill();
    let _ = child.wait();
    panic!(
        "child still running after its {deadline:?} deadline, killed: {}",
        describe(what)
    );
}

/// Poll `try_wait` until the child exits or `deadline` passes. The sleep
/// grows with the elapsed time (1 ms up to 25 ms), so a short run is
/// noticed within about a millisecond and a long one costs little.
///
/// Returns the status and the descendants last seen alive (a snapshot taken
/// from 200 ms on, then once a second, so a quick child pays nothing): once
/// the child has exited its own children are re-parented and can no longer
/// be found, but a descendant that outlives it can still hold its pipes.
fn poll_exit(child: &mut Child, what: &str, deadline: Duration) -> (ExitStatus, Vec<u32>) {
    let start = Instant::now();
    let mut seen = Vec::new();
    let mut next_snapshot = Duration::from_millis(200);
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return (status, seen);
        }
        let elapsed = start.elapsed();
        if elapsed >= deadline {
            kill_and_panic(child, what, deadline);
        }
        if elapsed >= next_snapshot {
            seen = descendants(child.id());
            next_snapshot = elapsed + Duration::from_secs(1);
        }
        let nap = (elapsed / 10).clamp(Duration::from_millis(1), Duration::from_millis(25));
        std::thread::sleep(nap.min(deadline - elapsed));
    }
}

/// Drain a pipe on its own thread so a full pipe can never stall the child.
fn drain<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut buf);
        }
        let _ = tx.send(buf);
    });
    rx
}

/// How long a pipe may stay open after the child has exited. At EOF it
/// returns at once; a pipe still open this long is held by a descendant that
/// outlived the child.
const PIPE_EOF_GRACE: Duration = Duration::from_secs(2);

/// Collect a drained pipe, or `None` if it is still open after the grace.
fn collect(rx: &std::sync::mpsc::Receiver<Vec<u8>>) -> Option<Vec<u8>> {
    rx.recv_timeout(PIPE_EOF_GRACE).ok()
}

/// `Child::wait_with_output` with a deadline. `child` must have been spawned
/// with whatever stdout/stderr it should have; piped ones are read on
/// threads while the child runs. A piped stdin still in `child.stdin` is
/// closed first (feed it with `feed_stdin` beforehand, which takes it).
/// Panics, after killing the child, when it has not exited by `deadline`.
pub fn wait_with_deadline(mut child: Child, deadline: Duration) -> Output {
    wait_with_deadline_named(&mut child, "(spawned by the test)", deadline)
}

/// `wait_with_deadline(child, default_deadline())`.
pub fn wait_output(child: Child) -> Output {
    wait_with_deadline(child, default_deadline())
}

fn wait_with_deadline_named(child: &mut Child, what: &str, deadline: Duration) -> Output {
    drop(child.stdin.take());
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let (status, descendants) = poll_exit(child, what, deadline);
    match (collect(&out), collect(&err)) {
        (Some(stdout), Some(stderr)) => Output {
            status,
            stdout,
            stderr,
        },
        _ => {
            // A descendant of the child holds its stdout/stderr open: the
            // child exited, but the run is not over. Do not report success
            // with lost output and a runaway left behind.
            kill_pids(&descendants);
            panic!(
                "a descendant of the child still held its output pipes {PIPE_EOF_GRACE:?} \
                 after the child exited (killed the ones seen: {descendants:?}): {}",
                describe(what)
            );
        }
    }
}

/// Drop-in replacements for `Command::output` / `status` that cannot hang.
pub trait CommandDeadline {
    /// Like `output()` (stdin null, stdout and stderr captured), killed after
    /// `deadline`.
    fn output_within(&mut self, deadline: Duration) -> std::io::Result<Output>;
    /// `output_within(default_deadline())`.
    fn output_deadline(&mut self) -> std::io::Result<Output> {
        self.output_within(default_deadline())
    }
    /// Like `output_within`, with `input` written to the child's stdin (on a
    /// thread, then closed). A child that exits without reading it is fine.
    fn output_with_input_within(
        &mut self,
        input: &[u8],
        deadline: Duration,
    ) -> std::io::Result<Output>;
    /// `output_with_input_within(input, default_deadline())`.
    fn output_with_input(&mut self, input: &[u8]) -> std::io::Result<Output> {
        self.output_with_input_within(input, default_deadline())
    }
    /// Like `status()` (stdio as configured, inherited by default), killed
    /// after `deadline`.
    fn status_within(&mut self, deadline: Duration) -> std::io::Result<ExitStatus>;
    /// `status_within(default_deadline())`.
    fn status_deadline(&mut self) -> std::io::Result<ExitStatus> {
        self.status_within(default_deadline())
    }
}

impl CommandDeadline for Command {
    fn output_within(&mut self, deadline: Duration) -> std::io::Result<Output> {
        let what = format!("{self:?}");
        self.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = self.spawn()?;
        Ok(wait_with_deadline_named(&mut child, &what, deadline))
    }

    fn output_with_input_within(
        &mut self,
        input: &[u8],
        deadline: Duration,
    ) -> std::io::Result<Output> {
        let what = format!("{self:?}");
        self.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = self.spawn()?;
        let mut stdin = child.stdin.take().expect("stdin is piped");
        let input = input.to_vec();
        // On a thread: a child that fills its stdout pipe before it has read
        // all of a large input must not deadlock against this write. A child
        // that exits early closes the pipe (BrokenPipe), which is its
        // business: the caller asserts on status and stderr (issue #630).
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(&input);
        });
        Ok(wait_with_deadline_named(&mut child, &what, deadline))
    }

    fn status_within(&mut self, deadline: Duration) -> std::io::Result<ExitStatus> {
        let what = format!("{self:?}");
        let mut child = self.spawn()?;
        Ok(poll_exit(&mut child, &what, deadline).0)
    }
}

/// A deadline for a child the test talks to while it runs (the MCP server,
/// whose replies are read with a blocking `read_line`). When `deadline`
/// passes before the guard is dropped, the child and its descendants are
/// killed: the test's pipe reads then see EOF and it fails instead of
/// hanging. Keep the guard in the struct that owns the child and drop it
/// before reaping the child (so a reaped pid is never signalled).
pub struct Watchdog {
    cancel: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Watchdog {
    pub fn new(child: &Child, deadline: Duration) -> Watchdog {
        let (cancel, rx) = std::sync::mpsc::channel::<()>();
        let pid = child.id();
        let thread = std::thread::spawn(move || {
            if rx.recv_timeout(deadline) == Err(std::sync::mpsc::RecvTimeoutError::Timeout) {
                eprintln!("watchdog: child {pid} still running after {deadline:?}, killing it");
                kill_tree(pid);
            }
        });
        Watchdog {
            cancel: Some(cancel),
            thread: Some(thread),
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        drop(self.cancel.take()); // wakes the thread at once
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
