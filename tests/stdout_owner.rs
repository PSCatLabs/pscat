//! Several interpreters in one process and the timed stdout buffer (#1457,
//! Codex round 2, R2).
//!
//! The timed path keeps std's line buffering in a buffer of its own. Only one
//! interpreter owns it at a time; another interpreter's writes, or its
//! `Drop`, must never send the owner's partial line early or drop it.
//!
//! The work runs in a child copy of this test binary whose stdout and stderr
//! are one pipe the parent drains (`2>&1`), so the order of the bytes against
//! stderr is visible. The child marks its own output with raw writes, so the
//! test harness's lines around it are left out.
#![cfg(unix)]

mod common;

use common::CommandDeadline as _;
use pscat::{Interp, Limits};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::Duration;

const ENV: &str = "PSCAT_STDOUT_OWNER_CHILD";
const BEGIN: &[u8] = b"<<begin>>";
const END: &[u8] = b"<<end>>";

fn raw(bytes: &[u8]) {
    // SAFETY: a plain write of a live slice to fd 1.
    unsafe { libc::write(1, bytes.as_ptr().cast(), bytes.len()) };
}

fn timed() -> Interp {
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_time(Duration::from_secs(60)))
        .expect("set the limits");
    it.run_str("/f (%stdout) (w) file def /e (%stderr) (w) file def")
        .expect("define f and e");
    it
}

fn child() {
    raw(BEGIN);
    let mut a = timed();
    // A partial line, held in the line buffer as std holds it.
    a.run_str("f (a1) writestring").expect("a1");
    {
        // An unrelated interpreter, with captured stdout, comes and goes.
        let mut b = Interp::new();
        b.capture_stdout();
        b.run_str("(b) print").expect("b");
    }
    {
        // And one without limits, writing nothing.
        let mut b = Interp::new();
        b.run_str("1 pop").expect("b2");
    }
    // Without a limit: `x` joins the buffered `a1`, `E` goes to stderr at
    // once, and the line leaves at its newline: "E" then "a1x\n".
    a.run_str("f (x) writestring e (E) writestring f (\\n) writestring")
        .expect("x");
    a.run_str("f (a3) writestring").expect("a3");
    {
        // Another timed interpreter writes a whole line while `a` owns a
        // partial one, and is dropped: `a3` must stay where it is.
        let mut c = timed();
        c.run_str("(c\\n) print").expect("c");
    }
    a.run_str("e (F) writestring f (\\n) writestring")
        .expect("a3 end");
    drop(a);
    raw(END);
}

#[test]
fn another_interpreter_never_moves_the_owners_line() {
    if std::env::var_os(ENV).is_some() {
        child();
        return;
    }
    let (mut reader, writer) = std::io::pipe().expect("a pipe");
    let err = writer.try_clone().expect("a second write end");
    let mut cmd = Command::new(std::env::current_exe().expect("this test binary"));
    cmd.args([
        "--exact",
        "another_interpreter_never_moves_the_owners_line",
        "--nocapture",
        "--test-threads=1",
    ])
    .env(ENV, "1")
    .stdin(Stdio::null())
    .stdout(writer)
    .stderr(err);
    let drained = std::thread::spawn(move || {
        let mut all = Vec::new();
        let _ = reader.read_to_end(&mut all);
        all
    });
    let status = cmd
        .status_within(Duration::from_secs(60))
        .expect("run the child");
    drop(cmd);
    let all = drained.join().expect("the reader thread");
    assert!(status.success(), "{}", String::from_utf8_lossy(&all));
    let start = all
        .windows(BEGIN.len())
        .position(|w| w == BEGIN)
        .expect("the child's begin mark")
        + BEGIN.len();
    let end = all
        .windows(END.len())
        .position(|w| w == END)
        .expect("the child's end mark");
    let got = &all[start..end];
    assert_eq!(got, b"Ea1x\nc\nFa3\n", "{:?}", String::from_utf8_lossy(got));
}
