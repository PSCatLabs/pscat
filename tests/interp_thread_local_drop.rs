//! #1404: an `Interp` kept in a `thread_local!` is dropped while the thread
//! exits, when the library's own thread-locals may already be destroyed.
//! Destructors run in an unspecified order, so `Drop for Interp` (and the
//! array and dictionary teardown under it) must not assume any of them is
//! still alive. The failure is a double panic ("cannot access a Thread Local
//! Storage value during or after destruction"), which aborts the whole
//! process, so each case runs in a child process: the parent test sees a
//! failed exit status instead of the test binary dying.

use std::cell::RefCell;
use std::process::Command;

const ENV: &str = "PSCAT_TLS_DROP_CASE";

thread_local! { static A: RefCell<Option<pscat::Interp>> = const { RefCell::new(None) }; }
thread_local! { static B: RefCell<Option<pscat::Interp>> = const { RefCell::new(None) }; }

fn run(src: &str) -> pscat::Interp {
    let mut interp = pscat::Interp::new();
    interp.run_str(src).expect("program runs");
    interp
}

/// A program that leaves arrays, nested dicts and a name behind.
///
/// The freed temporary matters: it is the first array teardown on the thread,
/// which is what creates the library's own thread-local that the teardown
/// uses. Without one, that thread-local does not exist yet when the thread
/// exits, and a single interpreter does not show the bug.
const WORK: &str = "/foo 1 def /arr [1 [2 3] << /k [4 5] >>] def \
                    /d << /in << /deep [6] >> >> def [1 2 3] pop \
                    /Helvetica findfont 12 scalefont setfont gsave 10 10 moveto (hi) show \
                    0 0 5 0 360 arc fill grestore";

fn child(case: &str) {
    std::thread::spawn({
        let case = case.to_string();
        move || match case.as_str() {
            // The issue's sketch: one interpreter in one thread-local.
            "empty" => A.with(|a| *a.borrow_mut() = Some(pscat::Interp::new())),
            "one" => A.with(|a| *a.borrow_mut() = Some(run(WORK))),
            // Two thread-locals, each holding an interpreter that ran a program.
            "two" => {
                B.with(|b| *b.borrow_mut() = Some(run(WORK)));
                A.with(|a| *a.borrow_mut() = Some(run(WORK)));
            }
            // The other order of first use.
            "two-reversed" => {
                A.with(|a| *a.borrow_mut() = Some(run(WORK)));
                B.with(|b| *b.borrow_mut() = Some(run(WORK)));
            }
            // Only the second one runs a program: the first is empty.
            "two-mixed" => {
                B.with(|b| *b.borrow_mut() = Some(pscat::Interp::new()));
                A.with(|a| *a.borrow_mut() = Some(run(WORK)));
            }
            other => panic!("unknown case {other}"),
        }
    })
    .join()
    .expect("thread ends without a panic");
}

fn check(case: &str) {
    if std::env::var(ENV).as_deref() == Ok(case) {
        child(case);
        return;
    }
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("drop_in_thread_local_{case}"),
            "--nocapture",
        ])
        .env(ENV, case)
        .output()
        .expect("re-run the test binary");
    assert!(
        out.status.success(),
        "case {case}: child {:?}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn drop_in_thread_local_empty() {
    check("empty");
}

#[test]
fn drop_in_thread_local_one() {
    check("one");
}

#[test]
fn drop_in_thread_local_two() {
    check("two");
}

#[test]
fn drop_in_thread_local_two_reversed() {
    check("two-reversed");
}

#[test]
fn drop_in_thread_local_two_mixed() {
    check("two-mixed");
}
