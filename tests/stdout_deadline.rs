//! A program's write to stdout or stderr is held to `--max-time` (#1457).
//!
//! A write to a pipe nobody drains used to block in `write_all` with no
//! deadline, so `--max-time` could not end a program stuck on a slow
//! consumer. Each test here bounds its own run: the child is waited on with
//! the deadline helpers in `tests/common`, which kill it and fail the test
//! instead of hanging the suite. Unix only: the timed write is built on
//! `poll(2)`, and on Windows a write still blocks as before.
#![cfg(unix)]

mod common;

use std::io::Read;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// The hang guard for one child. Generous: it stops a hang, it does not time
/// the run (a stopped write ends well inside it).
const GUARD: Duration = Duration::from_secs(30);

/// Four MiB: far more than any pipe or socket buffer holds by default, so a
/// write of it blocks once the buffer is full.
const BIG: usize = 4 << 20;

fn spawn(args: &[&str], stdout: Stdio, stderr: Stdio) -> Child {
    Command::new(BIN)
        .args(args)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .expect("spawn pscat")
}

/// Run `program` with `--max-time 0.1` and stdout a pipe the test never reads
/// while the child runs. Returns the child's stderr, its exit code and what
/// the pipe held when it exited.
fn undrained_pipe(program: &str) -> (String, Option<i32>, Vec<u8>) {
    let (mut reader, writer) = std::io::pipe().expect("a pipe for the child's stdout");
    let child = spawn(
        &["--max-time", "0.1", "-e", program],
        writer.into(),
        Stdio::piped(),
    );
    let out = common::wait_with_deadline(child, GUARD);
    // The child has exited and the test's copy of the write end went with
    // the `Command`, so this reads what the pipe held, then EOF.
    let mut held = Vec::new();
    reader.read_to_end(&mut held).expect("read the pipe");
    (
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
        held,
    )
}

fn assert_stopped_by_time(what: &str, stderr: &str, code: Option<i32>) {
    assert!(
        stderr.contains("limitexceeded (time)"),
        "{what}: expected the time limit, got status {code:?}, stderr {stderr}"
    );
    assert_eq!(code, Some(1), "{what}: {stderr}");
}

/// Every writer that sends a program's output to stdout stops at the time
/// limit when the reader never drains the pipe. The bytes the pipe holds are
/// the start of the output: what was written stays written.
#[test]
fn every_stdout_writer_stops_at_max_time_on_an_undrained_pipe() {
    let big = BIG.to_string();
    let cases: Vec<(&str, String, &[u8])> = vec![
        ("=", format!("{big} string ="), b"\0"),
        ("==", format!("{big} string =="), b"(\\0"),
        ("print", format!("{big} string print"), b"\0"),
        (
            "write",
            "/f (%stdout) (w) file def { f 65 write } loop".into(),
            b"A",
        ),
        (
            "writestring",
            format!("(%stdout) (w) file {big} string writestring"),
            b"\0",
        ),
        (
            "writehexstring",
            format!("(%stdout) (w) file {big} string writehexstring"),
            b"0",
        ),
        (
            "flushfile",
            "/f (%stdout) (w) file def { f (AAAA) writestring f flushfile } loop".into(),
            b"A",
        ),
        ("stack", format!("{big} string stack"), b"\0"),
        ("pstack", format!("{big} string pstack"), b"(\\0"),
        ("printobject", format!("{big} string 0 printobject"), b""),
    ];
    for (what, program, alphabet) in cases {
        let (stderr, code, held) = undrained_pipe(&program);
        assert_stopped_by_time(what, &stderr, code);
        assert!(!held.is_empty(), "{what}: nothing reached the pipe");
        assert!(held.len() < BIG, "{what}: the whole output was written");
        if !alphabet.is_empty() {
            assert!(
                held.iter().all(|b| alphabet.contains(b)),
                "{what}: the pipe holds bytes that are not the output's start"
            );
        }
    }
}

/// The same for a stdout that is a socket (libuv gives children socketpairs
/// for stdio).
#[test]
fn a_stdout_socket_nobody_reads_stops_at_max_time() {
    let (mut ours, theirs) = UnixStream::pair().expect("a socket pair");
    let child = spawn(
        &["--max-time", "0.1", "-e", &format!("{BIG} string =")],
        Stdio::from(OwnedFd::from(theirs)),
        Stdio::piped(),
    );
    let out = common::wait_with_deadline(child, GUARD);
    let mut held = Vec::new();
    ours.read_to_end(&mut held).expect("read the socket");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stopped_by_time("= to a socket", &stderr, out.status.code());
    assert!(held.len() < BIG && held.iter().all(|&b| b == 0));
}

/// `(%stderr)` is held to the limit too. The reader here is slow, not absent
/// (64 KiB per 10 ms): the CLI's own error report after the stop is written
/// to the same stderr and needs a reader to finish. Unbounded, the 16 MiB
/// write would go through in full at that pace; stopped at 0.1 s, a small
/// part of it does.
#[test]
fn a_stderr_write_to_a_slow_reader_stops_at_max_time() {
    let (mut reader, writer) = std::io::pipe().expect("a pipe for the child's stderr");
    let child = spawn(
        &[
            "--max-time",
            "0.1",
            "-e",
            "(%stderr) (w) file 16777216 string writestring",
        ],
        Stdio::null(),
        writer.into(),
    );
    let slow = std::thread::spawn(move || {
        let mut total = 0usize;
        let mut tail = Vec::new();
        let mut buf = vec![0u8; 64 << 10];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    total += n;
                    tail.extend_from_slice(&buf[..n]);
                    let keep = tail.len().saturating_sub(4096);
                    tail.drain(..keep);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        (total, tail)
    });
    let out = common::wait_with_deadline(child, GUARD);
    let (total, tail) = slow.join().expect("the reader thread");
    let tail = String::from_utf8_lossy(&tail);
    assert!(tail.contains("limitexceeded (time)"), "{tail}");
    assert_eq!(out.status.code(), Some(1));
    assert!(total < 16 << 20, "the whole stderr write went through");
}

/// With no time limit (no flag, or `--max-time 0`) a write still blocks until
/// the reader drains the pipe, and then delivers every byte.
#[test]
fn with_no_time_limit_a_write_waits_for_the_reader() {
    for flags in [&[][..], &["--max-time", "0"][..]] {
        let (mut reader, writer) = std::io::pipe().expect("a pipe for the child's stdout");
        let mut args = flags.to_vec();
        let program = format!("{BIG} string =");
        args.extend(["-e", program.as_str()]);
        let mut child = spawn(&args, writer.into(), Stdio::null());
        let guard = common::Watchdog::new(&child, GUARD);
        std::thread::sleep(Duration::from_millis(300));
        let early = child.try_wait().expect("try_wait");
        let drained = std::thread::spawn(move || {
            let mut all = Vec::new();
            let _ = reader.read_to_end(&mut all);
            all
        });
        drop(guard);
        let out = common::wait_with_deadline(child, GUARD);
        let all = drained.join().expect("the reader thread");
        assert!(early.is_none(), "{flags:?}: exited before the reader read");
        assert!(out.status.success(), "{flags:?}");
        assert_eq!(all.len(), BIG + 1, "{flags:?}");
    }
}

/// A program that mixes the writers: `=` lines, `print` with no newline,
/// single bytes through `%stdout`, `stack`, `flush`, and a last partial line
/// before an error.
const MIXED: &str = "/f (%stdout) (w) file def \
    1 1 20000 { dup = (x) print f 65 write ( ) print } for \
    1 1 3000 { f exch 256 mod write } for flush \
    1 2 3 stack pop pop pop \
    (tail) print f 66 write nosuchoperator";

/// What a run of [`MIXED`] writes to stdout as a pipe the test drains.
fn mixed_output(flags: &[&str]) -> Vec<u8> {
    let mut args = flags.to_vec();
    args.extend(["-e", MIXED]);
    let out = common::wait_with_deadline(spawn(&args, Stdio::piped(), Stdio::piped()), GUARD);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("undefined"), "{flags:?}: {stderr}");
    out.stdout
}

/// Under a time limit that is not reached, a drained stdout gets exactly what
/// an unlimited run writes, in the same order, the unfinished last line
/// included; to a socket too.
#[test]
fn a_timed_run_writes_what_an_unlimited_one_does() {
    let plain = mixed_output(&[]);
    assert!(plain.len() > 200_000, "the output passes any pipe buffer");
    assert!(plain.ends_with(b"tailB"));
    assert_eq!(mixed_output(&["--max-time", "60"]), plain);

    let (mut ours, theirs) = UnixStream::pair().expect("a socket pair");
    let child = spawn(
        &["--max-time", "60", "-e", MIXED],
        Stdio::from(OwnedFd::from(theirs)),
        Stdio::piped(),
    );
    let drained = std::thread::spawn(move || {
        let mut all = Vec::new();
        let _ = ours.read_to_end(&mut all);
        all
    });
    let out = common::wait_with_deadline(child, GUARD);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(drained.join().expect("the reader thread"), plain);
}

/// A reader that went away is still the quiet SIGPIPE exit (#1059) under a
/// time limit: the timed write reports `EPIPE` as the plain one does. The
/// reader is closed before the spawn (`notes/1294-closed-stdout-tests.md`).
#[test]
fn a_closed_stdout_under_a_time_limit_ends_quietly() {
    for program in ["(x) print", "1048576 string ="] {
        let (reader, writer) = std::io::pipe().expect("a pipe for the child's stdout");
        drop(reader);
        let child = spawn(
            &["--max-time", "30", "-e", program],
            writer.into(),
            Stdio::piped(),
        );
        let out = common::wait_with_deadline(child, GUARD);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.is_empty(), "{program}: {stderr}");
        assert_eq!(
            out.status.code(),
            Some(141),
            "{program}: the SIGPIPE status"
        );
    }
}
