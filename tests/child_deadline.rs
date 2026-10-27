//! Issue #841: a test must never wait on a child pscat without a deadline.
//! These tests prove the shared helpers in `tests/common`: a deliberately
//! non-terminating pscat (`{ } loop`, no limits) is killed at the deadline,
//! the test fails with a clear message, and no process is left behind. Each
//! case gives its pscat a unique marker in its arguments and asks
//! `pgrep -f <marker>` (never a broad pattern) whether anything survived.
//! A control proves the marker really is visible to pgrep while the child
//! runs, so an empty answer afterwards means something.
#![cfg(unix)]

use std::io::BufRead;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod common;
use common::CommandDeadline as _;

const PSCAT: &str = env!("CARGO_BIN_EXE_pscat");

/// A runaway program whose text (so its `-e` argument) carries a marker no
/// other process has.
fn runaway(tag: &str) -> (String, String) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let marker = format!(
        "pscat-deadline-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let program = format!("% {marker}\n{{ }} loop\n");
    (marker, program)
}

/// Pids whose full command line contains `marker`.
fn alive(marker: &str) -> Vec<String> {
    let out = Command::new("pgrep")
        .args(["-f", marker])
        .output()
        .expect("run pgrep");
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Nothing carrying `marker` is left. SIGKILL'd processes can show for a
/// moment until init reaps them, so give that a couple of seconds.
fn assert_none_left(marker: &str) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let left = alive(marker);
        if left.is_empty() {
            return;
        }
        assert!(
            Instant::now() < until,
            "processes left behind for {marker}: {left:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn panic_message(r: Result<(), Box<dyn std::any::Any + Send>>) -> String {
    let payload = r.expect_err("the helper should have panicked at the deadline");
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .expect("panic message")
}

/// The control: without the helper the same command keeps running, and
/// pgrep sees it by its marker. (Then it is killed by its own pid.)
#[test]
fn control_pgrep_sees_a_running_runaway() {
    let (marker, program) = runaway("control");
    let mut child = Command::new(PSCAT)
        .args(["--headless", "-e", &program])
        .stdin(Stdio::null())
        .spawn()
        .expect("spawn pscat");
    std::thread::sleep(Duration::from_millis(300));
    assert!(child.try_wait().unwrap().is_none(), "the runaway ended");
    assert!(!alive(&marker).is_empty(), "pgrep cannot see the marker");
    child.kill().unwrap();
    child.wait().unwrap();
    assert_none_left(&marker);
}

#[test]
fn output_deadline_kills_a_runaway_and_fails_the_test() {
    let (marker, program) = runaway("output");
    let started = Instant::now();
    let msg = panic_message(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = Command::new(PSCAT)
                .args(["--headless", "-e", &program])
                .output_within(Duration::from_secs(2));
        }))
        .map(|_| ()),
    );
    assert!(started.elapsed() < Duration::from_secs(10), "too slow");
    assert!(msg.contains("deadline"), "{msg}");
    assert!(msg.contains(&marker), "message names the command: {msg}");
    assert_none_left(&marker);
}

#[test]
fn output_with_input_kills_a_runaway_read_from_stdin() {
    let (marker, program) = runaway("stdin");
    let msg = panic_message(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = Command::new(PSCAT)
                .args([
                    "--headless",
                    "--png",
                    &format!("/nonexistent/{marker}.png"),
                    "-",
                ])
                .output_with_input_within(program.as_bytes(), Duration::from_secs(2));
        }))
        .map(|_| ()),
    );
    assert!(msg.contains("deadline"), "{msg}");
    assert_none_left(&marker);
}

#[test]
fn status_deadline_kills_a_runaway() {
    let (marker, program) = runaway("status");
    let msg = panic_message(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = Command::new(PSCAT)
                .args(["--headless", "-e", &program])
                .stdout(Stdio::null())
                .status_within(Duration::from_secs(2));
        }))
        .map(|_| ()),
    );
    assert!(msg.contains("deadline"), "{msg}");
    assert_none_left(&marker);
}

#[test]
fn wait_with_deadline_kills_a_child_the_test_spawned() {
    let (marker, program) = runaway("wait");
    let mut child = Command::new(PSCAT)
        .args(["--headless", "-e", &program])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pscat");
    common::feed_stdin(&mut child, b"");
    let msg = panic_message(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = common::wait_with_deadline(child, Duration::from_secs(2));
        }))
        .map(|_| ()),
    );
    assert!(msg.contains("deadline"), "{msg}");
    assert_none_left(&marker);
}

/// A server pscat-mcp-style grandchild: the shell backgrounds the runaway
/// and waits, so killing only the shell would leave pscat spinning.
#[test]
fn the_deadline_kills_grandchildren_too() {
    let (marker, program) = runaway("tree");
    let msg = panic_message(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = Command::new("sh")
                .args(["-c", "\"$0\" --headless -e \"$1\" & wait", PSCAT, &program])
                .output_within(Duration::from_secs(2));
        }))
        .map(|_| ()),
    );
    assert!(msg.contains("deadline"), "{msg}");
    assert_none_left(&marker);
}

/// The watchdog for a long-lived, interactive child: the blocking read of
/// its stdout sees EOF once the deadline kills it.
#[test]
fn watchdog_kills_a_hung_interactive_child() {
    let (marker, program) = runaway("watchdog");
    let mut child = Command::new(PSCAT)
        .args(["--headless", "-e", &program])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn pscat");
    let watchdog = common::Watchdog::new(&child, Duration::from_secs(1));
    let mut reader = std::io::BufReader::new(child.stdout.take().unwrap());
    let started = Instant::now();
    let mut line = String::new();
    let n = reader.read_line(&mut line).expect("read");
    assert_eq!(n, 0, "the runaway printed {line:?}");
    assert!(started.elapsed() < Duration::from_secs(10), "too slow");
    drop(watchdog);
    child.wait().unwrap();
    assert_none_left(&marker);
}

/// A guard that is dropped in time never signals anything.
#[test]
fn watchdog_dropped_in_time_leaves_the_child_alone() {
    let mut child = Command::new(PSCAT)
        .args(["--headless", "-e", "1 pop"])
        .stdin(Stdio::null())
        .spawn()
        .expect("spawn pscat");
    let started = Instant::now();
    drop(common::Watchdog::new(&child, Duration::from_secs(30)));
    assert!(started.elapsed() < Duration::from_secs(5), "drop blocked");
    assert!(child.wait().unwrap().success());
}

/// Piping still works: a program (and its output) far larger than a pipe
/// buffer goes through stdin and stdout and stderr with no deadlock.
#[test]
fn large_stdin_stdout_and_stderr_do_not_deadlock() {
    // ~1.2 MB of source on stdin; the program prints ~1.1 MB to stdout and
    // ~0.6 MB to stderr. 64 KiB pipe buffers fill long before it ends.
    let mut source = String::from("% ");
    source.push_str(&"x".repeat(1_200_000));
    source.push_str(
        "\n100000 { (0123456789) = } repeat\n\
         (%stderr) (w) file /e exch def 50000 { e (0123456789\\n) writestring } repeat e flushfile\n",
    );
    let out = Command::new(PSCAT)
        .args(["--headless", "-"])
        .output_with_input_within(source.as_bytes(), Duration::from_secs(60))
        .expect("run pscat");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout.len(), 100_000 * 11);
    assert_eq!(out.stderr.len(), 50_000 * 11);
}

/// A child that exits without reading its stdin is not an error.
#[test]
fn a_child_that_ignores_stdin_is_fine() {
    let big = vec![b'x'; 1_000_000];
    let out = Command::new(PSCAT)
        .args(["--headless", "-e", "1 pop"])
        .output_with_input_within(&big, Duration::from_secs(60))
        .expect("run pscat");
    assert!(out.status.success());
}
