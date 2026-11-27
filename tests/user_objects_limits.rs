//! The user-object operators under the step and time limits (#1265).
//! `defineuserobject` can copy a 65535-slot array in one operator, and a
//! program can do it tens of thousands of times, so every copy is charged
//! to `--max-steps` and looked at by `--max-time`, before it is made. The
//! memory limit has its own binary (`user_objects_memory.rs`).
//!
//! No test here measures wall time (AGENTS.md): a limit is tested by the
//! error it produces and the state it leaves.

use std::time::Duration;

use pscat::{Interp, Limits, PsError};

/// Grow by two each time: the index is one past the length, so the new
/// array is `index + 1` long and the whole old one is copied. About 32 000
/// copies of up to 65 000 slots, the worst shape the operator has.
const GROW_BY_TWO: &str =
    "100000 { userdict /UserObjects get length 1 add {} defineuserobject } repeat";

/// The array a limit stop left, read with the limits lifted.
fn length_after(it: &mut Interp) -> usize {
    it.set_limits(Limits::none()).expect("no memory limit");
    it.capture_stdout();
    it.run_str("userdict /UserObjects get length =")
        .expect("the interpreter still runs");
    String::from_utf8_lossy(it.captured_stdout())
        .trim()
        .parse()
        .expect("a length")
}

fn started() -> Interp {
    let mut it = Interp::new();
    it.run_str("0 {} defineuserobject").expect("first define");
    it
}

#[test]
fn growing_the_array_over_and_over_hits_the_step_limit() {
    let mut it = started();
    it.set_limits(Limits::none().with_max_steps(1_000_000))
        .expect("no memory limit");
    assert_eq!(it.run_str(GROW_BY_TWO), Err(PsError::StepsExceeded));
    // A copy costs about a step per 64 slots, so a million steps pays for
    // some 8 000 growths (length about 16 000), nowhere near the 65 000 cap:
    // the loop was stopped by the copies, not by its own count.
    let len = length_after(&mut it);
    assert!((10..40_000).contains(&len), "length {len}");
}

#[test]
fn growing_the_array_over_and_over_hits_the_time_limit() {
    let mut it = started();
    it.set_limits(Limits::none().with_max_time(Duration::from_millis(50)))
        .expect("no memory limit");
    assert_eq!(it.run_str(GROW_BY_TWO), Err(PsError::TimeExceeded));
    let len = length_after(&mut it);
    assert!(len >= 10, "length {len}");
}

#[test]
fn a_save_that_snapshots_the_big_array_every_time_is_charged() {
    // The first write at a save level snapshots the whole array, so
    // `save ... defineuserobject restore` in a loop copies it every time:
    // 3000 rounds are 200 million slots, about a thousand steps each.
    let mut it = Interp::new();
    it.run_str("userdict /UserObjects 65535 array put")
        .expect("a big array");
    it.set_limits(Limits::none().with_max_steps(1_000_000))
        .expect("no memory limit");
    assert_eq!(
        it.run_str("3000 { save 0 (x) defineuserobject restore } repeat"),
        Err(PsError::StepsExceeded)
    );
    assert_eq!(length_after(&mut it), 65535, "restore kept the array");
}

#[test]
fn an_undefine_in_a_save_loop_is_charged_too() {
    let mut it = Interp::new();
    it.run_str("userdict /UserObjects 65535 array put")
        .expect("a big array");
    it.set_limits(Limits::none().with_max_steps(1_000_000))
        .expect("no memory limit");
    assert_eq!(
        it.run_str("3000 { save 0 undefineuserobject restore } repeat"),
        Err(PsError::StepsExceeded)
    );
}

#[test]
fn a_huge_index_is_a_plain_rangecheck_and_changes_nothing() {
    let mut it = started();
    it.set_limits(Limits::none().with_max_steps(1_000_000))
        .expect("no memory limit");
    for src in [
        "1000000000 {} defineuserobject",
        "9223372036854775807 {} defineuserobject",
        "65536 {} defineuserobject",
        "1000000000 execuserobject",
        "1000000000 undefineuserobject",
    ] {
        assert_eq!(it.run_str(src), Err(PsError::Rangecheck), "{src}");
    }
    assert_eq!(length_after(&mut it), 10);
}

#[test]
fn a_tail_recursive_object_is_stopped_by_the_step_limit() {
    // `{ 0 execuserobject }` ends in the call, so it is a tail call and
    // never fills the execution stack (as `/f { f } def f`): the step
    // limit is what ends it.
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_steps(100_000))
        .expect("no memory limit");
    assert_eq!(
        it.run_str("0 { 0 execuserobject } defineuserobject 0 execuserobject"),
        Err(PsError::StepsExceeded)
    );
}
