//! The parameter and cache operators under the step and time limits (#1270).
//! They look at a fixed table of parameters, or at the operand stack down to
//! a mark, never at a program's whole dictionary, and each charges that walk
//! to `--max-steps` and looks at `--max-time` before it changes anything.
//! The memory limit has its own binary (`params_memory.rs`).
//!
//! No test here measures wall time (AGENTS.md): a limit is tested by the
//! error it produces and the state it leaves.

use pscat::{Interp, Limits, PsError};

/// The operand stack with limits lifted, one line.
fn stack_after(it: &mut Interp, src: &str) -> String {
    it.set_limits(Limits::none()).expect("no memory limit");
    it.capture_stdout();
    it.run_str(src).expect("the interpreter still runs");
    String::from_utf8_lossy(it.captured_stdout())
        .trim()
        .to_string()
}

/// A mark with a million operands above it: `aload` pushes them in one
/// step, so building it is cheap and walking it down is the cost.
const DEEP: &str = "mark 1000000 array aload pop";

#[test]
fn setcacheparams_charges_the_walk_down_a_deep_stack() {
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_steps(5_000))
        .expect("no memory limit");
    it.run_str(DEEP).expect("a deep stack is cheap to make");
    assert_eq!(it.run_str("setcacheparams"), Err(PsError::StepsExceeded));
    // Refused before anything was done: the mark and the million operands
    // are still there, and the cache parameters are the defaults.
    assert_eq!(
        stack_after(
            &mut it,
            "count = currentcacheparams exch pop exch pop exch pop ="
        ),
        "1000001\n10000"
    );
}

#[test]
fn setucacheparams_charges_the_walk_down_a_deep_stack() {
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_steps(5_000))
        .expect("no memory limit");
    it.run_str(DEEP).expect("a deep stack is cheap to make");
    assert_eq!(it.run_str("setucacheparams"), Err(PsError::StepsExceeded));
    assert_eq!(
        stack_after(&mut it, "count = ucachestatus 5 1 roll pop pop pop pop ="),
        "1000001\n0"
    );
}

#[test]
fn a_deep_stack_under_generous_limits_is_dropped_whole() {
    // The same walk, with room for it: the operands and the mark go, and the
    // last three values are the parameters.
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_steps(1_000_000))
        .expect("no memory limit");
    it.run_str("7 mark 1000000 array aload pop pop pop 500000 100 2000 setcacheparams")
        .expect("within the limits");
    assert_eq!(
        stack_after(&mut it, "count = cachestatus 7 1 roll pop pop pop pop pop pop ="),
        "1\n2000"
    );
}

/// Steps a program of `n` repeats of `body` takes, less what the same loop
/// takes with an empty body.
fn steps_per_call(body: &str, n: u64) -> u64 {
    let run = |body: &str| {
        let mut it = Interp::new();
        // Steps are counted only while some limit is set.
        it.set_limits(Limits::none().with_max_steps(u64::MAX / 2))
            .expect("no memory limit");
        it.run_str(&format!("{n} {{ {body} }} repeat")).expect("runs");
        it.steps_used()
    };
    (run(body) - run("")) / n
}

#[test]
fn every_parameter_call_is_charged_for_its_table_walk() {
    // The loop and the operator's own step are two or three; the walk over a
    // table of twenty or more parameters adds to that. A call that were free
    // would show as the object count alone.
    for (body, floor) in [
        ("currentuserparams pop", 4),
        ("currentsystemparams pop", 4),
        ("<< /MaxFormItem 1 >> setuserparams", 4),
        ("<< /MaxFormCache 1 >> setsystemparams", 4),
        ("(%os%) currentdevparams pop", 3),
    ] {
        let per = steps_per_call(body, 1000);
        assert!(per >= floor, "{body}: {per} steps a call");
    }
}

#[test]
fn a_save_loop_of_parameter_changes_hits_the_step_limit() {
    // `save` shares the user parameters, and the first write after it copies
    // them: the loop is still bounded by the step limit, and `restore` brings
    // the value back each round.
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_steps(50_000))
        .expect("no memory limit");
    assert_eq!(
        it.run_str("100000000 { save << /MaxFormItem 1 >> setuserparams restore } repeat"),
        Err(PsError::StepsExceeded)
    );
    assert_eq!(
        stack_after(&mut it, "currentuserparams /MaxFormItem get ="),
        "100000"
    );
}

#[test]
fn a_dictionary_of_a_million_entries_costs_the_table_not_the_dictionary() {
    // The operators read the keys they know, never walk the operand.
    let mut it = Interp::new();
    it.run_str("/big 1000000 dict def 0 1 999999 { big exch 0 put } for")
        .expect("a big dictionary");
    it.set_limits(Limits::none().with_max_steps(1_000))
        .expect("no memory limit");
    it.run_str("big setuserparams big setsystemparams")
        .expect("two table walks fit in a thousand steps");
}
