//! A program's write to stdout or stderr is held to `--max-time` (#1457).
//!
//! A write to a pipe nobody drains used to block in `write_all` with no
//! deadline, so `--max-time` could not end a program stuck on a slow
//! consumer. Each test here bounds its own run: the child is waited on with
//! the deadline helpers in `tests/common`, which kill it and fail the test
//! instead of hanging the suite. Unix only: the timed write is built on
//! `poll(2)`/`fstat(2)`, and on Windows a write still blocks as before.
//!
//! Under a limit that is not reached, nothing may change: the bytes, their
//! order against stderr, where a write error surfaces. The differential
//! tests run each program with and without `--max-time` and compare.
#![cfg(unix)]

mod common;

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::{UnixDatagram, UnixStream};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// The hang guard for one child. Generous: it stops a hang, it does not time
/// the run (a stopped write ends well inside it).
const GUARD: Duration = Duration::from_secs(30);

/// Four MiB: far more than any pipe or socket buffer holds by default, so a
/// write of it blocks once the buffer is full.
const BIG: usize = 4 << 20;

/// The time limit for the runs that must stop: long enough for any of the
/// programs below to fill a pipe first, so the stop is a blocked write.
const STOP_AFTER: &str = "0.5";

/// The two files every differential program writes through.
const FILES: &str = "/f (%stdout) (w) file def /e (%stderr) (w) file def ";

fn spawn(args: &[&str], stdout: Stdio, stderr: Stdio) -> Child {
    Command::new(BIN)
        .args(args)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .expect("spawn pscat")
}

fn set_nonblocking(fd: &impl AsRawFd, on: bool) {
    let fd = fd.as_raw_fd();
    // SAFETY: plain fcntl flag changes on a descriptor the test owns.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        assert!(flags >= 0);
        let flags = if on {
            flags | libc::O_NONBLOCK
        } else {
            flags & !libc::O_NONBLOCK
        };
        assert!(libc::fcntl(fd, libc::F_SETFL, flags) == 0);
    }
}

/// Fill `writer`'s pipe until it takes no more (big pieces, then single
/// bytes, so a macOS pipe also grows to its largest), and return how many
/// bytes it holds. The descriptor is blocking again afterwards, as a child
/// expects it.
fn fill(writer: &mut std::io::PipeWriter) -> usize {
    set_nonblocking(writer, true);
    let mut n = 0;
    for piece in [4096usize, 1] {
        let chunk = vec![b'#'; piece];
        while let Ok(k) = writer.write(&chunk) {
            n += k;
        }
    }
    set_nonblocking(writer, false);
    n
}

/// How much a fresh pipe holds on this machine.
fn pipe_capacity() -> usize {
    let (_reader, mut writer) = std::io::pipe().expect("a pipe");
    fill(&mut writer)
}

/// Run `program` with a short `--max-time` and stdout a pipe the test never
/// reads while the child runs. Returns the child's stderr, its exit code and
/// what the pipe held when it exited.
fn undrained_pipe(program: &str) -> (String, Option<i32>, Vec<u8>) {
    let (mut reader, writer) = std::io::pipe().expect("a pipe for the child's stdout");
    let child = spawn(
        &["--max-time", STOP_AFTER, "-e", program],
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
/// limit when the reader never drains the pipe. The pipe is full when the
/// child exits (the stop was a write waiting on it), and holds the start of
/// the output: what was written stays written.
#[test]
fn every_stdout_writer_stops_at_max_time_on_an_undrained_pipe() {
    let capacity = pipe_capacity();
    let big = BIG.to_string();
    let cases: Vec<(&str, String, &[u8])> = vec![
        ("=", format!("{big} string ="), b"\0"),
        // `==` bounds one string (#703): a loop of them.
        ("==", "{ 1000 string == } loop".into(), b""),
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
        ("flush", "{ (AAAA) print flush } loop".into(), b"A"),
        // `stack` and `pstack` bound what one call prints (#703), so a
        // loop of them, not one big operand, passes the pipe's size.
        (
            "stack",
            "{ 1 1 1000 { } for stack clear } loop".into(),
            b"0123456789\n",
        ),
        (
            "pstack",
            "{ 1 1 1000 { } for pstack clear } loop".into(),
            b"0123456789\n",
        ),
        (
            "printobject",
            "{ 60000 string 0 printobject } loop".into(),
            b"",
        ),
    ];
    for (what, program, alphabet) in cases {
        let (stderr, code, held) = undrained_pipe(&program);
        assert_stopped_by_time(what, &stderr, code);
        // Full, give or take one write's worth: the writer was waiting.
        assert!(
            held.len() + 4096 >= capacity,
            "{what}: the pipe holds {} of {capacity} bytes: the stop was not a blocked write",
            held.len()
        );
        assert!(held.len() < BIG, "{what}: the whole output was written");
        if !alphabet.is_empty() {
            assert!(
                held.iter().all(|b| alphabet.contains(b)),
                "{what}: the pipe holds bytes that are not the output's start"
            );
        }
    }
}

/// The same for a stdout that is a stream socket (libuv gives children
/// socketpairs for stdio).
#[test]
fn a_stdout_socket_nobody_reads_stops_at_max_time() {
    let (mut ours, theirs) = UnixStream::pair().expect("a socket pair");
    let child = spawn(
        &["--max-time", STOP_AFTER, "-e", &format!("{BIG} string =")],
        Stdio::from(OwnedFd::from(theirs)),
        Stdio::piped(),
    );
    let out = common::wait_with_deadline(child, GUARD);
    let mut held = Vec::new();
    ours.read_to_end(&mut held).expect("read the socket");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stopped_by_time("= to a socket", &stderr, out.status.code());
    assert!(!held.is_empty(), "nothing reached the socket");
    assert!(held.len() < BIG && held.iter().all(|&b| b == 0));
}

/// A datagram socket keeps its message boundaries under a limit: one write
/// of 2048 bytes is one datagram of 2048 bytes, as without one.
#[test]
fn a_datagram_stdout_gets_the_same_datagrams_under_a_time_limit() {
    for flags in [&[][..], &["--max-time", "30"][..]] {
        let (ours, theirs) = UnixDatagram::pair().expect("a datagram socket pair");
        let mut args = flags.to_vec();
        args.extend(["-e", "(%stdout) (w) file 2048 string writestring"]);
        let child = spawn(&args, Stdio::from(OwnedFd::from(theirs)), Stdio::piped());
        let out = common::wait_with_deadline(child, GUARD);
        assert!(out.status.success(), "{flags:?}");
        ours.set_nonblocking(true).expect("non-blocking");
        let mut sizes = Vec::new();
        let mut buf = vec![0u8; 1 << 16];
        while let Ok(n) = ours.recv(&mut buf) {
            sizes.push(n);
        }
        assert_eq!(sizes, [2048], "{flags:?}: the datagrams received");
    }
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
    assert!(total > 0, "nothing reached stderr");
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

/// Run `pscat <flags> <args>` with stdout and stderr the same drained pipe
/// (`2>&1`), `stdin` fed to it, and return the merged bytes and the status.
fn merged(flags: &[&str], args: &[&str], stdin: &[u8]) -> (Vec<u8>, Option<i32>) {
    let (mut reader, writer) = std::io::pipe().expect("a pipe for the child's output");
    let err = writer.try_clone().expect("a second write end");
    let mut all_args = flags.to_vec();
    all_args.extend_from_slice(args);
    let mut child = Command::new(BIN)
        .args(&all_args)
        .stdin(Stdio::piped())
        .stdout(writer)
        .stderr(err)
        .spawn()
        .expect("spawn pscat");
    let drained = std::thread::spawn(move || {
        let mut all = Vec::new();
        let _ = reader.read_to_end(&mut all);
        all
    });
    common::feed_stdin(&mut child, stdin);
    let out = common::wait_with_deadline(child, GUARD);
    (
        drained.join().expect("the reader thread"),
        out.status.code(),
    )
}

/// Programs whose stdout and stderr bytes interleave in a way only std's
/// line buffering decides: the capacity (1024 bytes), a write that fills it
/// exactly, one that overflows it, a newline in the middle, every writer, an
/// error report, `quit`, `stop`, a caught error.
fn order_programs() -> Vec<String> {
    let mut p: Vec<String> = [
        // The cases Codex reported (#1465).
        "f 2000 string writestring e (B) writestring",
        "f (A) writestring 1 1 10000 { pop } for e (B) writestring",
        "f (A) writestring nosuchoperator",
        // std's LineWriter edges, pinned on the base binary.
        "f 1023 string writestring e (E) writestring",
        "f 1024 string writestring e (E) writestring",
        "f 1025 string writestring e (E) writestring",
        "f 1023 string writestring f 0 write e (E) writestring",
        "f 1023 string writestring f 0 write f 0 write e (E) writestring",
        "f 1022 string writestring f 0 write f 0 write e (E) writestring",
        "f 10 string writestring f (\\n) writestring f 5 string writestring e (E) writestring",
        "f 1000 string writestring f 100 string writestring e (E) writestring",
        "f (ab\\ncd) writestring e (E) writestring f (ef\\n) writestring e (F) writestring",
        // Every writer, each followed by a stderr byte.
        "(x) print e (E) writestring",
        "42 = e (E) writestring",
        "(abc) == e (E) writestring",
        "1 2 stack e (E) writestring",
        "1 2 pstack e (E) writestring",
        "f 65 write e (E) writestring",
        "f (ab) writehexstring e (E) writestring",
        "f (q) writestring f flushfile e (E) writestring",
        "f (q) writestring flush e (E) writestring",
        "(q) 0 printobject e (E) writestring",
        "f (q) writestring f (r) 0 writeobject e (E) writestring",
        // Ends: quit, stop, a caught and an uncaught error, handleerror.
        "f (A) writestring e (B) writestring quit",
        "f (A) writestring stop",
        "{ f (A) writestring nosuch } stopped pop e (B) writestring f (C) writestring",
        "{ nosuch } stopped pop f (A) writestring handleerror f (B) writestring",
    ]
    .iter()
    .map(|s| format!("{FILES}{s}"))
    .collect();
    // Long output with partial lines, across many slices.
    p.push(format!(
        "{FILES}1 1 3000 {{ dup = (x) print f 65 write e (.) writestring }} for (tail) print f 66 write"
    ));
    p
}

/// Under a limit that is not reached, stdout and stderr merged into one pipe
/// (`2>&1`) carry the same bytes in the same order as without a limit, for
/// `-e`, for a program file, and at the REPL prompt.
#[test]
fn a_timed_run_merges_stdout_and_stderr_as_an_unlimited_one_does() {
    let dir = common::scratch_dir("stdout-deadline-order");
    for (i, program) in order_programs().iter().enumerate() {
        let file = dir.join(format!("p{i}.ps"));
        std::fs::write(&file, program).expect("write the program");
        let file = file.to_str().expect("a UTF-8 path");
        let runs: [(&str, Vec<&str>, Vec<u8>); 3] = [
            ("-e", vec!["-e", program.as_str()], Vec::new()),
            ("file", vec!["--headless", file], Vec::new()),
            (
                "REPL",
                vec![],
                program.replace("} ", "}\n").replace(' ', "\n").into_bytes(),
            ),
        ];
        for (how, args, stdin) in runs {
            let plain = merged(&[], &args, &stdin);
            let timed = merged(&["--max-time", "60"], &args, &stdin);
            assert_eq!(
                String::from_utf8_lossy(&timed.0),
                String::from_utf8_lossy(&plain.0),
                "{how}: {program}"
            );
            assert_eq!(timed.1, plain.1, "{how}: exit status of {program}");
        }
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
/// an unlimited run writes, the unfinished last line included; to a stream
/// socket too.
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

/// A write that fails because the reader went away fails inside the
/// operator that wrote, so `stopped` catches it as `ioerror`, under a limit
/// as without one.
#[test]
fn stopped_catches_a_write_error_under_a_time_limit() {
    let program = format!(
        "{FILES}{{ f 2000 string writestring }} stopped \
         {{ e (caught) writestring }} {{ e (missed) writestring }} ifelse"
    );
    for flags in [&[][..], &["--max-time", "30"][..]] {
        let (reader, writer) = std::io::pipe().expect("a pipe for the child's stdout");
        drop(reader);
        let mut args = flags.to_vec();
        args.extend(["-e", program.as_str()]);
        let out = common::wait_with_deadline(spawn(&args, writer.into(), Stdio::piped()), GUARD);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(stderr, "caught", "{flags:?}");
    }
}

/// Output a pipe can hold is still written in full when nobody reads until
/// the child exits: under the time limit as without it. (macOS starts a pipe
/// at 16 KiB and grows it to 64 KiB on a write that does not fit; the timed
/// path must let it grow, not stop at the first 16 KiB.)
#[test]
fn output_that_fits_the_pipe_is_written_in_full_under_a_time_limit() {
    let len = pipe_capacity() * 3 / 4;
    let lines = len / 6;
    for (program, expect) in [
        (format!("{len} string print"), len),
        (format!("1 1 {lines} {{ 10000 add = }} for"), lines * 6),
    ] {
        for flags in [&[][..], &["--max-time", "30"][..]] {
            let (mut reader, writer) = std::io::pipe().expect("a pipe for the child's stdout");
            let mut args = flags.to_vec();
            args.extend(["-e", program.as_str()]);
            let child = spawn(&args, writer.into(), Stdio::piped());
            let out = common::wait_with_deadline(child, GUARD);
            let mut held = Vec::new();
            reader.read_to_end(&mut held).expect("read the pipe");
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(out.status.success(), "{program} {flags:?}: {stderr}");
            assert_eq!(held.len(), expect, "{program} {flags:?}");
        }
    }
}

/// A write stopped while it waits on a full pipe is an operator error like
/// any other: the operand stack is restored. The pipe is full before the
/// child starts; 1024 bytes fill std's line buffer (no output yet, as without
/// a limit), and the next byte must flush it, so that `write` is the one
/// that waits and is stopped.
#[test]
fn a_stopped_write_restores_its_operands() {
    let (mut reader, mut writer) = std::io::pipe().expect("a pipe for the child's stdout");
    let prefilled = fill(&mut writer);
    let program = format!("{FILES}f 1023 string writestring f 65 write 1 2 f 66 write");
    let child = spawn(
        &["--max-time", "0.3", "--pstack-on-error", "-e", &program],
        writer.into(),
        Stdio::piped(),
    );
    let out = common::wait_with_deadline(child, GUARD);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("limitexceeded (time); OffendingCommand: write"),
        "{stderr}"
    );
    assert!(
        stderr.contains("Operand stack (4): 1 2 -file- 66"),
        "{stderr}"
    );
    let mut held = Vec::new();
    reader.read_to_end(&mut held).expect("read the pipe");
    assert!(held.len() >= prefilled, "the pipe lost its prefill");
}

/// A run that ends with a partial line in std's buffer and a full pipe:
/// without a limit the process would wait at exit for a reader; under one the
/// run ends at its limit (the bytes that cannot be written are dropped), it
/// does not hang. Whether the program finished, or was stopped by the limit
/// at a slice's start or end with the line still buffered.
#[test]
fn a_full_pipe_at_the_end_of_a_run_does_not_outlast_the_limit() {
    for program in [
        "f (A) writestring",
        "f (A) writestring { } loop",
        "f (A) writestring { 1 pop } loop",
    ] {
        let (_reader, mut writer) = std::io::pipe().expect("a pipe for the child's stdout");
        fill(&mut writer);
        let child = spawn(
            &["--max-time", "0.3", "-e", &format!("{FILES}{program}")],
            writer.into(),
            Stdio::piped(),
        );
        let out = common::wait_with_deadline(child, GUARD);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("limitexceeded (time)"),
            "{program}: {stderr}"
        );
        assert_eq!(out.status.code(), Some(1), "{program}: {stderr}");
    }
}
