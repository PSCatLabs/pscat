//! Interpreter execution limits (issue #604): steps and time. The memory
//! limit has its own test binary (`limits_memory.rs`), because it counts the
//! whole process's heap and these tests run on parallel threads.
//!
//! No test here measures wall time (AGENTS.md): a time limit is tested by the
//! error it produces, never by how long the run took.

use std::time::Duration;

use pscat::{Interp, Limits, PsError};

fn limited(limits: Limits) -> Interp {
    let mut it = Interp::new();
    it.set_limits(limits)
        .expect("limits without memory always apply");
    it
}

fn steps(n: u64) -> Limits {
    Limits::none().with_max_steps(n)
}

#[test]
fn a_runaway_loop_stops_with_a_structured_step_error() {
    let mut it = limited(steps(10_000));
    let err = it.run_str("{ 1 pop } loop").unwrap_err();
    assert_eq!(err, PsError::StepsExceeded);
    assert_eq!(err.name(), "limitexceeded");
    let report = it.error_report(&err);
    assert!(
        report.starts_with("%%[ Error: limitexceeded (steps); OffendingCommand: ")
            && report.ends_with("; Line: 1 ]%%"),
        "{report}"
    );
}

#[test]
fn loops_with_empty_bodies_are_counted_too() {
    // These never hand the step loop an object, so the iterations themselves
    // have to be counted.
    for src in [
        "{ } loop",
        "1 1 100000000000 { pop } for",
        "1 1 100000000000 { } for",
        "100000000000 { } repeat",
        "{ { } loop } loop",
    ] {
        let mut it = limited(steps(5_000));
        assert_eq!(it.run_str(src), Err(PsError::StepsExceeded), "{src}");
    }
}

#[test]
fn stopped_cannot_catch_a_limit() {
    // A caught limit would let a program carry on past it forever.
    let mut it = limited(steps(5_000));
    assert_eq!(
        it.run_str("{ { } loop } stopped pop /survived true def"),
        Err(PsError::StepsExceeded)
    );
    assert!(it.load("survived").is_none(), "the program was cut off");

    let mut it = limited(steps(5_000));
    assert_eq!(
        it.run_str("{ { { } loop } stopped pop } loop"),
        Err(PsError::StepsExceeded),
        "a loop around `stopped` still terminates"
    );
    // And nothing was recorded for `$error` to replay.
    assert!(it.run_str("$error /errorname known").is_ok());
}

#[test]
fn a_program_within_its_limits_runs_to_the_end() {
    let mut it = limited(steps(10_000_000));
    it.capture_stdout();
    it.run_str("0 1 1 100000 { add } for = ").unwrap();
    assert_eq!(it.captured_stdout(), b"5000050000\n");
    assert!(it.steps_used() > 0, "counted while a limit is set");
}

#[test]
fn a_program_of_exactly_the_cap_is_not_refused() {
    // Seven objects: `1 2 add pop 3 4 add`. Whatever the exact accounting,
    // a cap equal to the count must not fail the program.
    let mut it = limited(steps(7));
    it.run_str("1 2 add pop 3 4 add").unwrap();
}

#[test]
fn limits_restart_with_each_program() {
    let mut it = limited(steps(3_000));
    for _ in 0..10 {
        // Each run uses a good share of the cap; together they'd pass it.
        it.run_str("1 1 1000 { pop } for").unwrap();
    }
}

#[test]
fn no_limits_changes_nothing() {
    let mut it = Interp::new();
    assert!(it.limits().is_none());
    it.run_str("1 1 100000 { pop } for").unwrap();
    assert_eq!(it.steps_used(), 0, "nothing is counted without limits");
    // Taking limits off again restores the old behaviour.
    it.set_limits(steps(10)).unwrap();
    it.set_limits(Limits::none()).unwrap();
    it.run_str("1 1 100000 { pop } for").unwrap();
}

#[test]
fn a_time_limit_stops_a_loop() {
    let mut it = limited(Limits::none().with_max_time(Duration::from_millis(50)));
    assert_eq!(it.run_str("{ } loop"), Err(PsError::TimeExceeded));
    let mut it = limited(Limits::none().with_max_time(Duration::from_millis(50)));
    assert_eq!(it.run_str("{ 1 pop } loop"), Err(PsError::TimeExceeded));
}

#[test]
fn a_memory_limit_without_the_tracking_allocator_is_refused_loudly() {
    // This test binary does not install `TrackingAllocator`.
    let mut it = Interp::new();
    let err = it
        .set_limits(Limits::none().with_max_memory(1 << 20))
        .unwrap_err();
    assert_eq!(err, pscat::LimitsError::AllocatorNotInstalled);
    assert!(it.limits().is_none(), "nothing was changed");
}

#[test]
fn the_page_survives_a_limit() {
    // A stopped run keeps what it drew, as after any other error.
    let mut it = limited(steps(50_000));
    let err = it
        .run_str("1 0 0 setrgbcolor 0 0 200 200 rectfill { } loop")
        .unwrap_err();
    assert_eq!(err, PsError::StepsExceeded);
    let px = it.gfx().pixmap.pixel(10, 780).expect("pixel");
    assert_eq!((px.red(), px.green(), px.blue()), (255, 0, 0));
}

#[test]
fn the_untrusted_preset_is_on() {
    let l = Limits::untrusted();
    assert!(l.max_steps.is_some() && l.max_time.is_some() && l.max_memory.is_some());
    assert!(!l.is_none());
}

#[test]
fn a_step_limit_of_u64_max_is_just_no_practical_limit() {
    let mut it = limited(steps(u64::MAX));
    it.run_str("1 1 10000 { pop } for").unwrap();
}
