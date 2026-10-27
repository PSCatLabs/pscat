//! Pins what the interpreter's per-token dispatch must keep exactly (issue
//! #863): the operand stack an operator leaves when it fails, the error name
//! and offending command that `$error` and the top-level report give, and the
//! number of steps a program takes. The fast paths in the step loop and in the
//! arithmetic, comparison and stack operators must change none of these, and
//! the identity corpus never errors, so these tests are what holds them.
//!
//! Every case runs three ways: unbound inside a procedure (operators are
//! reached by name lookup), bound (operators are fn pointers in the body),
//! and as the last element of a bound procedure (the frame is popped before
//! that element runs).

use pscat::Interp;

/// One caught run: the operand stack left under the `stopped` result, then
/// `$error`'s errorname and command, all as `==` text.
fn caught(src: &str) -> String {
    let mut it = Interp::new();
    it.run_str(&format!(
        "{src} stopped /res exch def res {{ $error /errorname get $error /command get }} if res"
    ))
    .unwrap_or_else(|e| panic!("{src}: {e}"));
    it.operand_stack()
        .iter()
        .map(|o| o.repr())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One uncaught run: the top-level report and the operand stack left.
fn uncaught(src: &str) -> String {
    let mut it = Interp::new();
    let report = match it.run_str(src) {
        Ok(()) => "ok".to_string(),
        Err(e) => it.error_report(&e),
    };
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    format!("{report} | {}", stack.join(" "))
}

/// The cases: body text, then what `caught` gives for it unbound, then
/// bound (and in tail position, which matches bound). The body runs after
/// `0 /pre { } def pre`. Unbound, the offending command is the failing
/// operator's own name; bound, an operator fetched as a fn pointer does not
/// update it, so it stays `pre` (or the last name executed, like `true`).
/// Pinned as it is: a fast path must not change either.
const STEP_CAP_NEED: u64 = 756;

const CASES: &[(&str, &str, &str)] = &[
    // Arithmetic: operands come off one at a time, each checked as it is
    // popped, so a failure leaves the stack partly consumed.
    (
        "1 (a) add",
        "0 1 /typecheck add true",
        "0 1 /typecheck pre true",
    ),
    (
        "(a) 1 add",
        "0 /typecheck add true",
        "0 /typecheck pre true",
    ),
    (
        "clear 1 add",
        "/stackunderflow add true",
        "/stackunderflow pre true",
    ),
    (
        "clear 2 add",
        "/stackunderflow add true",
        "/stackunderflow pre true",
    ),
    (
        "clear add",
        "/stackunderflow add true",
        "/stackunderflow pre true",
    ),
    (
        "1 0 div",
        "0 /undefinedresult div true",
        "0 /undefinedresult pre true",
    ),
    (
        "1 0.0 div",
        "0 /undefinedresult div true",
        "0 /undefinedresult pre true",
    ),
    ("1.5 2 div", "0 0.75 false", "0 0.75 false"),
    ("7 2 div", "0 3.5 false", "0 3.5 false"),
    (
        "9223372036854775807 1 add",
        "0 9.223372036854776e18 false",
        "0 9.223372036854776e18 false",
    ),
    (
        "-9223372036854775808 1 sub",
        "0 -9.223372036854776e18 false",
        "0 -9.223372036854776e18 false",
    ),
    (
        "-9223372036854775808 -1 mul",
        "0 9.223372036854776e18 false",
        "0 9.223372036854776e18 false",
    ),
    ("3 4 mul 2.5 add 1 sub", "0 13.5 false", "0 13.5 false"),
    ("1 2 add 3 mul", "0 9 false", "0 9 false"),
    ("2.5 2 mul", "0 5.0 false", "0 5.0 false"),
    (
        "true 1 add",
        "0 /typecheck add true",
        "0 /typecheck true true",
    ),
    (
        "1 null mul",
        "0 1 /typecheck mul true",
        "0 1 /typecheck null true",
    ),
    (
        "7 0 idiv",
        "0 /undefinedresult idiv true",
        "0 /undefinedresult pre true",
    ),
    (
        "7 0 mod",
        "0 /undefinedresult mod true",
        "0 /undefinedresult pre true",
    ),
    // Edge values the number fast paths must treat as the general path does.
    (
        "1e308 10 mul dup sub 1 lt",
        "0 /undefinedresult lt true",
        "0 /undefinedresult pre true",
    ),
    (
        "1e308 10 mul dup sub dup eq",
        "0 false false",
        "0 false false",
    ),
    ("-0.0 0 eq", "0 true false", "0 true false"),
    (
        "1 -0.0 div",
        "0 /undefinedresult div true",
        "0 /undefinedresult pre true",
    ),
    ("3 -0.0 mul", "0 -0.0 false", "0 -0.0 false"),
    ("1 cvx 2 add xcheck", "0 false false", "0 false false"),
    (
        "1 2 cvx 0 index xcheck",
        "0 1 2 true false",
        "0 1 2 true false",
    ),
    // Comparisons.
    (
        "1 2 lt 2 1 gt 1.5 1 ge 1 1.0 le",
        "0 true true true true false",
        "0 true true true true false",
    ),
    ("(a) (b) lt", "0 true false", "0 true false"),
    ("1 (a) lt", "0 /typecheck lt true", "0 /typecheck pre true"),
    ("(a) 1 gt", "0 /typecheck gt true", "0 /typecheck pre true"),
    (
        "clear 1 lt",
        "/stackunderflow lt true",
        "/stackunderflow pre true",
    ),
    (
        "clear lt",
        "/stackunderflow lt true",
        "/stackunderflow pre true",
    ),
    ("1 1.0 eq 1 2 ne", "0 true true false", "0 true true false"),
    // Stack operators.
    (
        "clear pop",
        "/stackunderflow pop true",
        "/stackunderflow pre true",
    ),
    (
        "clear 1 exch",
        "1 /stackunderflow exch true",
        "1 /stackunderflow pre true",
    ),
    (
        "clear dup",
        "/stackunderflow dup true",
        "/stackunderflow pre true",
    ),
    ("1 2 3 1 index", "0 1 2 3 2 false", "0 1 2 3 2 false"),
    (
        "1 2 3 9 index",
        "0 1 2 3 /stackunderflow index true",
        "0 1 2 3 /stackunderflow pre true",
    ),
    (
        "1 2 3 -1 index",
        "0 1 2 3 /rangecheck index true",
        "0 1 2 3 /rangecheck pre true",
    ),
    (
        "1 2 (x) index",
        "0 1 2 /typecheck index true",
        "0 1 2 /typecheck pre true",
    ),
    ("1 2 exch pop", "0 2 false", "0 2 false"),
    // Literals of every kind come through the procedure fetch.
    (
        "1 2.5 true null mark /n (s) [1] { 2 } count 1 sub array astore length",
        "0 9 false",
        "0 9 false",
    ),
    // Names, procedures and conditionals.
    (
        "nosuchname",
        "0 /undefined nosuchname true",
        "0 /undefined nosuchname true",
    ),
    ("true { 1 } if", "0 1 false", "0 1 false"),
    (
        "1 { 1 } if",
        "0 /typecheck if true",
        "0 /typecheck pre true",
    ),
    ("/v 3 def v v mul", "0 9 false", "0 9 false"),
    ("/f { 2 mul } def 5 f f", "0 20 false", "0 20 false"),
    // A procedure literal followed by `if` or `ifelse` (#864 fuses these
    // into one step when the condition is a boolean; anything else must
    // fail exactly as the two separate elements do).
    ("1 2 lt { 5 } if", "0 5 false", "0 5 false"),
    ("2 1 lt { 5 } if", "0 false", "0 false"),
    ("true { } if", "0 false", "0 false"),
    ("false { } if", "0 false", "0 false"),
    ("3 dup 0 lt { pop 0 } if", "0 3 false", "0 3 false"),
    ("-3 dup 0 lt { pop 0 } if", "0 0 false", "0 0 false"),
    ("true { false { 1 } if 2 } if", "0 2 false", "0 2 false"),
    (
        "clear { 5 } if",
        "/stackunderflow if true",
        "/stackunderflow pre true",
    ),
    (
        "(a) { 5 } if",
        "0 /typecheck if true",
        "0 /typecheck pre true",
    ),
    (
        "null { 5 } if",
        "0 /typecheck if true",
        "0 /typecheck null true",
    ),
    ("true { 1 } { 2 } ifelse", "0 1 false", "0 1 false"),
    ("false { 1 } { 2 } ifelse", "0 2 false", "0 2 false"),
    ("true { } { 2 } ifelse", "0 false", "0 false"),
    ("false { 1 } { } ifelse", "0 false", "0 false"),
    (
        "1 { 1 } { 2 } ifelse",
        "0 /typecheck ifelse true",
        "0 /typecheck pre true",
    ),
    (
        "clear { 1 } { 2 } ifelse",
        "/stackunderflow ifelse true",
        "/stackunderflow pre true",
    ),
    ("true { 1 } 2 ifelse", "0 1 false", "0 1 false"),
    // The true branch fails: its own error, raised inside the branch.
    (
        "true { 1 (a) add } if",
        "0 1 /typecheck add true",
        "0 1 /typecheck true true",
    ),
    (
        "false { 1 } { (a) 1 sub } ifelse",
        "0 /typecheck sub true",
        "0 /typecheck false true",
    ),
    // A redefined `if` or `ifelse` is what an unbound body runs; a bound
    // one already holds the operator.
    (
        "1 dict begin /if { pop pop 7 } def true { 1 } if end",
        "0 7 false",
        "0 1 false",
    ),
    (
        "1 dict begin /ifelse { pop pop pop 8 } def true { 1 } { 2 } ifelse end",
        "0 8 false",
        "0 1 false",
    ),
    // A literal name, `exch` and `def`: the "/name exch def" idiom. #864
    // measured fusing it and did not (every other name literal paid more
    // than this one saved); pinned for whoever tries next.
    ("5 /v exch def v", "0 5 false", "0 5 false"),
    ("1 2 /v exch def v", "0 1 2 false", "0 1 2 false"),
    ("5 /v exch def 6 /v exch def v", "0 6 false", "0 6 false"),
    (
        "clear /v exch def",
        "/v /stackunderflow exch true",
        "/v /stackunderflow pre true",
    ),
    (
        "/v 1 def save 5 /v exch def restore v",
        "0 1 false",
        "0 1 false",
    ),
    (
        "1 dict begin /def { pop pop 4 } def 5 /v exch def end",
        "0 4 false",
        "0 false",
    ),
    (
        "1 dict begin /exch { 7 } def 5 /v exch def v end",
        "0 5 7 false",
        "0 5 false",
    ),
    ("5 /v exch def /v load", "0 5 false", "0 5 false"),
    // `if` reached as a name that resolves to another name.
    (
        "/myif /if load def true { 9 } myif",
        "0 9 false",
        "0 9 false",
    ),
];

#[test]
fn errors_leave_the_same_stack_name_and_command_unbound_bound_and_in_tail_position() {
    let mut failures = Vec::new();
    for (body, want_unbound, want_bound) in CASES {
        for (shape, src) in [
            ("unbound", format!("{{ 0 /pre {{ }} def pre {body} }}")),
            ("bound", format!("{{ 0 /pre {{ }} def pre {body} }} bind")),
            // The failing operator is the last element: its frame is gone
            // before it runs. `exec` of a bound procedure from a bound one.
            (
                "tail",
                format!("{{ 0 /pre {{ }} def pre {{ {body} }} bind exec }} bind"),
            ),
        ] {
            let got = caught(&src);
            let want = if shape == "unbound" {
                want_unbound
            } else {
                want_bound
            };
            if got != *want {
                failures.push(format!("{shape} {body:?}: got {got:?}, want {want:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn uncaught_errors_report_the_same_command_and_keep_the_same_stack() {
    let cases: &[(&str, &str)] = &[
        (
            "/p { 1 (a) add } bind def 5 p",
            "%%[ Error: typecheck; OffendingCommand: p; Line: 1 ]%% | 5 1",
        ),
        (
            "/p { 1 (a) add } def 5 p",
            "%%[ Error: typecheck; OffendingCommand: add; Line: 1 ]%% | 5 1",
        ),
        (
            "/p { pop pop } bind def 5 p",
            "%%[ Error: stackunderflow; OffendingCommand: p; Line: 1 ]%% | ",
        ),
        (
            "/p { 1 0 div } bind def p",
            "%%[ Error: undefinedresult; OffendingCommand: p; Line: 1 ]%% | ",
        ),
        (
            "/p { 1 2 nosuch } bind def p",
            "%%[ Error: undefined; OffendingCommand: nosuch; Line: 1 ]%% | 1 2",
        ),
        (
            "1 1 3 { 10 mul } bind for (x) 1 add",
            "%%[ Error: typecheck; OffendingCommand: add; Line: 1 ]%% | 10 20 30",
        ),
    ];
    let mut failures = Vec::new();
    for (src, want) in cases {
        let got = uncaught(src);
        if got != *want {
            failures.push(format!("{src:?}: got {got:?}, want {want:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Steps `step_n(1)` takes to run `src` to the end: one element (or one
/// frame transition that yields nothing) per call. Live rendering and the
/// step limit both rely on this granularity.
fn single_steps(src: &str) -> usize {
    let mut it = Interp::new();
    it.begin_source(src.as_bytes());
    let mut n = 0;
    while it.step_n(1).unwrap_or_else(|e| panic!("{src}: {e}")) {
        n += 1;
        assert!(n < 1_000_000, "{src}: runaway");
    }
    n
}

#[test]
fn step_granularity_is_one_element_per_step() {
    let cases: &[(&str, usize)] = &[
        ("1 2 add pop", 4),
        ("{ 1 2 add pop } exec", 6),
        ("{ 1 2 add pop } bind exec", 7),
        ("/p { 1 2 add pop } bind def p p", 14),
        ("1 1 3 { pop } bind for", 9),
        ("3 { 1 pop } bind repeat", 10),
        ("true { 1 pop } bind if", 6),
        ("/p { 1 pop } def 4 { p } repeat", 18),
        // Procedure literal + `if`/`ifelse` inside a procedure: one step per
        // element whether or not the pair runs fused (#864).
        ("{ true { 1 pop } if } bind exec", 8),
        ("{ false { 1 pop } if } bind exec", 6),
        ("{ true { 1 pop } if } exec", 7),
        ("{ false { 1 pop } if } exec", 5),
        ("{ true { 1 } { 2 } ifelse pop } bind exec", 9),
        ("{ false { 1 } { 2 } ifelse pop } bind exec", 9),
        ("{ true { } if 1 pop } bind exec", 8),
        ("3 { 2 dup 0 lt { pop 0 } if pop } bind repeat", 25),
        ("{ 5 /v exch def } bind exec", 7),
        ("{ 5 /v exch def v pop } exec", 8),
    ];
    let mut failures = Vec::new();
    for (src, want) in cases {
        let got = single_steps(src);
        if got != *want {
            failures.push(format!("{src:?}: got {got}, want {want}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_put_into_a_running_bound_procedure_is_seen_by_the_same_run() {
    let mut it = Interp::new();
    it.run_str("/p { /p load 8 /mul load put 2 3 add } bind def p")
        .expect("runs");
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(stack, ["6"]);
}

/// The operand stack, as `==` text, after each `step_n(budget)` call until
/// the program ends.
fn trace(src: &str, budget: usize) -> Vec<String> {
    let mut it = Interp::new();
    it.begin_source(src.as_bytes());
    let mut out = Vec::new();
    loop {
        let more = it.step_n(budget).unwrap_or_else(|e| panic!("{src}: {e}"));
        let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
        out.push(stack.join(" "));
        if !more {
            return out;
        }
        assert!(out.len() < 1_000_000, "{src}: runaway");
    }
}

#[test]
fn a_slice_of_n_steps_ends_where_n_single_steps_do() {
    // A fused pair or triple (#864) is never split across a slice: a slice
    // with fewer steps left runs the elements one at a time. So after
    // every `step_n(b)` the stack is the one `b` single steps reach.
    let programs = [
        "{ 1 1 6 { dup 3 lt { pop 0 } if dup 4 gt { 10 } { 20 } ifelse } for } bind exec",
        "{ 1 1 6 { dup 3 lt { pop 0 } if dup 4 gt { 10 } { 20 } ifelse } for } exec",
        "/p { dup 2 mod 0 eq { 1 add } { 3 mul } ifelse } bind def 1 8 { p } repeat",
        "/q { /a exch def a a mul /b exch def } bind def 1 1 6 { q b } for",
        "/q { /a exch def a a mul /b exch def } def 1 1 6 { q b } for",
    ];
    for src in programs {
        let singles = trace(src, 1);
        for b in [2, 3, 4, 5, 7] {
            let sliced = trace(src, b);
            let want: Vec<String> = singles
                .iter()
                .skip(b - 1)
                .step_by(b)
                .cloned()
                .chain(std::iter::once(singles.last().unwrap().clone()))
                .collect();
            // The last slice can end early: compare all but the final entry
            // against the sampled single-step trace, then the final stacks.
            let n = sliced.len();
            assert_eq!(&sliced[..n - 1], &want[..n - 1], "{src}: budget {b}");
            assert_eq!(sliced.last(), singles.last(), "{src}: budget {b}");
        }
    }
}

#[test]
fn the_step_cap_counts_every_element_of_a_fused_run() {
    use pscat::{Limits, PsError};
    let src =
        "{ 1 1 50 { dup 25 lt { pop 0 } if dup 40 gt { 1 } { 2 } ifelse pop pop } for } bind exec";
    let run = |max: u64| {
        let mut it = Interp::new();
        it.set_limits(Limits::none().with_max_steps(max))
            .expect("limits apply");
        it.run_str(src)
    };
    // Pinned on the tree before #864: the smallest cap the program passes.
    let need = STEP_CAP_NEED;
    assert_eq!(run(need), Ok(()));
    assert_eq!(run(need - 1), Err(PsError::StepsExceeded));
}

#[test]
fn a_view_that_ends_before_the_operator_is_not_fused_with_it() {
    // The storage holds `if` (or `ifelse`) right after the view's last
    // element; the view must not reach it.
    for src in [
        "{ true { 1 } if } 0 2 getinterval cvx exec",
        "{ true { 1 } if } bind 0 2 getinterval cvx exec",
        "{ true { 1 } { 2 } ifelse } bind 0 3 getinterval cvx exec",
        "{ true { 1 } { 2 } ifelse } bind 0 2 getinterval cvx exec",
    ] {
        let mut it = Interp::new();
        it.run_str(src).unwrap_or_else(|e| panic!("{src}: {e}"));
        let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
        let want = if src.contains("0 3 getinterval") {
            "true {1} {2}"
        } else {
            "true {1}"
        };
        assert_eq!(stack.join(" "), want, "{src}");
    }
}

#[test]
fn a_changed_body_runs_its_new_elements() {
    // Nothing about a fused pair is remembered between runs (#864): every
    // run reads the body as it is now. In `{ true { 1 } if }` the element
    // indices are: 0 true, 1 { 1 }, 2 if.
    let cases: &[(&str, &str)] = &[
        // Replace `if` after one run.
        (
            "/p { true { 1 } if } bind def p /p load 2 /pop load put p",
            "1 true",
        ),
        // Replace the procedure literal.
        (
            "/p { true { 1 } if } bind def p /p load 1 { 2 } put p",
            "1 2",
        ),
        // Through a `getinterval` view onto the same storage.
        (
            "/p { true { 1 } if } bind def p /p load 2 1 getinterval 0 /pop load put p",
            "1 true",
        ),
        // `putinterval` over the pair.
        (
            "/p { true { 1 } if } bind def p /p load 1 [ 3 /pop load ] putinterval p",
            "1 true",
        ),
        // A `restore` puts the old element back.
        (
            "/p { true { 1 } if } bind def save /p load 2 /pop load put p exch restore p",
            "true 1",
        ),
        // The running procedure overwrites its own `if` before reaching it:
        // elements 0 /p 1 load 2 8 3 /pop 4 load 5 put 6 true 7 { 1 } 8 if.
        (
            "/p { /p load 8 /pop load put true { 1 } if } bind def p",
            "true",
        ),
        // Or its `ifelse`: 0 /p 1 load 2 9 3 /pop 4 load 5 put 6 true
        // 7 { 1 } 8 { 2 } 9 ifelse.
        (
            "/p { /p load 9 /pop load put true { 1 } { 2 } ifelse } bind def p",
            "true {1}",
        ),
        // `if` redefined between runs of an unbound body.
        (
            "/p { true { 1 } if } def p 1 dict begin /if { pop pop 5 } def p end",
            "1 5",
        ),
    ];
    for (src, want) in cases {
        let mut it = Interp::new();
        it.run_str(src).unwrap_or_else(|e| panic!("{src}: {e}"));
        let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
        assert_eq!(stack.join(" "), *want, "{src}");
    }
}

#[test]
fn a_true_branch_that_overflows_the_execution_stack_fails_as_if_does() {
    // Non-tail recursion through `true { r 1 } if` until the execution
    // stack is full: the error, its command and the operand stack must be
    // what the separate `if` gives.
    for (src, want) in [
        (
            "/r { true { r 1 } if } def { r } stopped $error /errorname get $error /command get count",
            "true /execstackoverflow r 3",
        ),
        (
            "/r { true { r 1 } if } bind def { r } stopped $error /errorname get $error /command get count",
            "true /execstackoverflow r 3",
        ),
        (
            "/r { true { r 1 } { } ifelse } bind def { r } stopped $error /errorname get $error /command get count",
            "true /execstackoverflow r 3",
        ),
    ] {
        let mut it = Interp::new();
        it.run_str(src).unwrap_or_else(|e| panic!("{src}: {e}"));
        let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
        assert_eq!(stack.join(" "), want, "{src}");
    }
}

/// Runs `src` to the end in slices of `budget` steps; returns the fused
/// steps taken.
fn fused_in(src: &str, budget: usize) -> u64 {
    let mut it = Interp::new();
    it.begin_source(src.as_bytes());
    while it.step_n(budget).unwrap_or_else(|e| panic!("{src}: {e}")) {}
    it.fused_steps()
}

#[test]
fn the_fused_paths_are_taken_where_they_apply_and_only_there() {
    // A positive control for every test above (#864): those tests give the
    // same answers with fusion off, so this pins that it is on.
    let cases: &[(&str, u64)] = &[
        ("{ true { 1 } if } bind exec", 1),
        ("{ false { 1 } if } bind exec", 1),
        ("{ true { 1 } if } exec", 1),
        ("{ false { 1 } if } exec", 1),
        ("{ true { 1 } { 2 } ifelse } bind exec", 1),
        ("{ false { 1 } { 2 } ifelse } exec", 1),
        ("{ true { } if } bind exec", 1),
        ("{ false { } { } ifelse } bind exec", 1),
        (
            "/p { 3 dup 0 lt { pop 0 } if } bind def 10 { p pop } repeat",
            10,
        ),
        ("1 1 5 { 2 mod 0 eq { 1 } { 2 } ifelse pop } bind for", 5),
        // Nothing fuses: not a boolean, no conditional after, a name that
        // is not `if` itself, a redefined `if`, a view that ends first,
        // or the top level (not a procedure).
        ("{ 1 { 1 } if } stopped", 0),
        ("{ true { 1 } exec } bind exec", 0),
        ("{ true { 1 } 2 ifelse } bind exec", 0),
        ("/myif /if load def { true { 9 } myif } exec", 0),
        (
            "1 dict begin /if { pop pop 7 } def { true { 1 } if } exec end",
            0,
        ),
        ("{ true { 1 } if } bind 0 2 getinterval cvx exec", 0),
        ("true { 1 } if", 0),
    ];
    let mut failures = Vec::new();
    for (src, want) in cases {
        let got = fused_in(src, 1000);
        if got != *want {
            failures.push(format!("{src:?}: fused {got}, want {want}"));
        }
        // With one step per slice nothing ever fuses: no steps to spare.
        let single = fused_in(src, 1);
        if single != 0 {
            failures.push(format!("{src:?}: fused {single} with step_n(1)"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn operator_timing_runs_every_conditional_on_its_own() {
    // `--profile-ops` times each operator, so its loop never fuses.
    let mut it = Interp::new();
    it.enable_profile();
    it.enable_profile_ops();
    it.run_str("1 1 5 { 2 mod 0 eq { 1 } { 2 } ifelse true { pop } if } bind for")
        .expect("runs");
    assert_eq!(it.fused_steps(), 0);
    let report = it.profile_report().expect("profiling is on");
    let ops = report.operators.expect("operators are on");
    let calls = |name: &str| ops.iter().find(|o| o.name == name).map(|o| o.calls);
    assert_eq!(calls("ifelse"), Some(5));
    assert_eq!(calls("if"), Some(5));
}

// ---------------------------------------------------------------------------
// Loops (#875). `repeat`, `loop`, `for` and `forall` run their body from the
// loop's own frame. These pin what the separate per-iteration procedure frame
// gave: where `exit`, `stop` and errors leave the operand stack and `$error`,
// the steps a loop takes, the depth at which the execution stack overflows,
// what the profiles count, and a body changed while it runs. All were written
// against the tree before #875 and pass on it unchanged.

/// Loop programs for `caught`: the source, then what it gives unbound and
/// bound. Each runs inside `{ ... } stopped`.
const LOOP_CASES: &[(&str, &str, &str)] = &[
    // `exit` at the first, a middle and the last element of the body.
    ("0 3 { exit 1 } repeat", "0 false", "0 false"),
    ("0 3 { 1 exit 2 } repeat", "0 1 false", "0 1 false"),
    ("0 3 { 1 exit } repeat", "0 1 false", "0 1 false"),
    // Through a named procedure, called last and not last.
    (
        "/q { exit } def 0 3 { 1 q } repeat",
        "0 1 false",
        "0 1 false",
    ),
    (
        "/q { 7 exit 8 } def 0 3 { 1 q 2 } repeat",
        "0 1 7 false",
        "0 1 7 false",
    ),
    // Through `exec` of a procedure that ends the body.
    ("0 { 1 { exit } exec } loop", "0 1 false", "0 1 false"),
    ("0 { 1 { exit } exec 2 } loop", "0 1 false", "0 1 false"),
    // From a conditional's branch, last in the body and not last.
    ("0 { 1 true { exit } if } loop", "0 1 false", "0 1 false"),
    (
        "1 1 10 { dup 3 ge { exit } if } for",
        "1 2 3 false",
        "1 2 3 false",
    ),
    (
        "1 1 10 { dup 3 ge { exit } { } ifelse 10 mul } for",
        "10 20 3 false",
        "10 20 3 false",
    ),
    (
        "0 { 1 add dup 5 eq { exit } if } loop",
        "5 false",
        "5 false",
    ),
    // Only the inner loop ends.
    (
        "2 { 3 { 1 exit } repeat 2 } repeat",
        "1 2 1 2 false",
        "1 2 1 2 false",
    ),
    (
        "1 1 2 { 1 1 3 { exit } for 5 } for",
        "1 1 5 2 1 5 false",
        "1 1 5 2 1 5 false",
    ),
    // `exit` cannot cross `stopped`: invalidexit, caught each time.
    (
        "3 { { exit } stopped } repeat $error /errorname get",
        "true true true /invalidexit false",
        "true true true /invalidexit false",
    ),
    (
        "3 { { 1 exit } stopped } repeat",
        "1 true 1 true 1 true false",
        "1 true 1 true 1 true false",
    ),
    // Every kind of `forall`, and `for` with reals.
    (
        "[1 2 3 4] { dup 2 gt { exit } if } forall",
        "1 2 3 false",
        "1 2 3 false",
    ),
    (
        "(abcd) { dup 98 gt { exit } if } forall",
        "97 98 99 false",
        "97 98 99 false",
    ),
    ("<< /a 1 >> { pop exit } forall", "/a false", "/a false"),
    (
        "0 0.5 2 { dup 1 gt { exit } if } for",
        "0.0 0.5 1.0 1.5 false",
        "0.0 0.5 1.0 1.5 false",
    ),
    (
        "1 -1 -3 { } for 1 -1 -3 { } for",
        "1 0 -1 -2 -3 1 0 -1 -2 -3 false",
        "1 0 -1 -2 -3 1 0 -1 -2 -3 false",
    ),
    (
        "0 1 0 { } for 0 { } repeat [] { } forall () { } forall",
        "0 false",
        "0 false",
    ),
    // Errors at the first, a middle and the last element.
    (
        "0 3 { nosuch 1 } repeat",
        "0 /undefined nosuch true",
        "0 /undefined nosuch true",
    ),
    (
        "0 3 { 1 nosuch 2 } repeat",
        "0 1 /undefined nosuch true",
        "0 1 /undefined nosuch true",
    ),
    (
        "0 3 { 1 2 nosuch } repeat",
        "0 1 2 /undefined nosuch true",
        "0 1 2 /undefined nosuch true",
    ),
    (
        "0 3 { 1 (a) add } repeat",
        "0 1 /typecheck add true",
        "0 1 /typecheck stopped true",
    ),
    (
        "1 1 3 { 0 div } for",
        "/undefinedresult div true",
        "/undefinedresult stopped true",
    ),
    (
        "1 1 3 { dup 2 eq { 0 div } if } for",
        "1 /undefinedresult div true",
        "1 /undefinedresult stopped true",
    ),
    (
        "[1 (a) 3] { 1 add } forall",
        "2 /typecheck add true",
        "2 /typecheck [ true",
    ),
    (
        "(ab) { pop pop } forall",
        "/stackunderflow pop true",
        "/stackunderflow stopped true",
    ),
    (
        "<< /a 1 >> { pop pop pop } forall",
        "/stackunderflow pop true",
        "/stackunderflow << true",
    ),
    (
        "0 { 1 add dup 3 eq { pop pop } if } loop",
        "/stackunderflow pop true",
        "/stackunderflow stopped true",
    ),
    (
        "/q { 1 1 3 { 0 div } for } def q",
        "/undefinedresult div true",
        "/undefinedresult q true",
    ),
    (
        "2 { 2 { 1 0 idiv } repeat } repeat",
        "/undefinedresult idiv true",
        "/undefinedresult stopped true",
    ),
    (
        "-1 { } repeat",
        "/rangecheck repeat true",
        "/rangecheck stopped true",
    ),
    (
        "1 (a) 3 { } for",
        "1 /typecheck for true",
        "1 /typecheck stopped true",
    ),
    (
        "5 { } forall",
        "/typecheck forall true",
        "/typecheck stopped true",
    ),
    ("3 { exit } stopped", "3 true false", "3 true false"),
    // `stop` inside a loop inside a `stopped` context.
    (
        "{ 3 { 1 stop 2 } repeat } stopped 4",
        "1 true 4 false",
        "1 true 4 false",
    ),
    (
        "{ 3 { 1 stop } repeat } stopped 4",
        "1 true 4 false",
        "1 true 4 false",
    ),
    (
        "{ 0 { 1 add dup 3 eq { stop } if } loop } stopped 4",
        "3 true 4 false",
        "3 true 4 false",
    ),
    (
        "{ [1 2] { { stop } loop } forall } stopped",
        "1 true false",
        "1 true false",
    ),
    // A body changed while it runs: every iteration reads it as it is now.
    (
        "/b { 1 /b load 0 2 index 1 add put } def 3 /b load repeat",
        "1 2 3 false",
        "1 2 3 false",
    ),
    (
        "/b { /b load 5 8 put 0 } def 2 /b load repeat",
        "8 8 false",
        "8 8 false",
    ),
    (
        "/a { /a load 4 7 put 1 } def 2 /a load 0 5 getinterval cvx repeat",
        "{/a load 4 7 7 1} 4 7 7 false",
        "{/a --load-- 4 7 7 1} 4 7 7 false",
    ),
    (
        "/a [ 1 2 3 4 ] cvx def 2 /a load 1 2 getinterval repeat",
        "2 3 2 3 false",
        "2 3 2 3 false",
    ),
    (
        "/b { 1 /b load 1 [ 9 /pop load ] putinterval 2 3 } def 2 /b load repeat",
        "1 2 3 /typecheck putinterval true",
        "1 2 3 /typecheck [ true",
    ),
    // `restore` puts back an element of the running body.
    (
        "/b { 1 pop /b load 0 get exch restore save } def save /b load 0 2 put 2 /b load repeat pop",
        "2 1 false",
        "2 1 false",
    ),
    (
        "/b { 1 } def 3 { /b load } repeat",
        "{1} {1} {1} false",
        "{1} {1} {1} false",
    ),
];

#[test]
fn loops_leave_the_same_stack_and_error_after_exit_stop_and_errors() {
    let mut failures = Vec::new();
    for (body, want_unbound, want_bound) in LOOP_CASES {
        for (src, want) in [
            (format!("{{ {body} }}"), want_unbound),
            (format!("{{ {body} }} bind"), want_bound),
        ] {
            let got = caught(&src);
            if got != *want {
                failures.push(format!("{src:?}: got {got:?}, want {want:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn uncaught_errors_in_loops_report_the_same_command_and_stack() {
    let cases: &[(&str, &str)] = &[
        (
            "0 3 { 1 nosuch } repeat",
            "%%[ Error: undefined; OffendingCommand: nosuch; Line: 1 ]%% | 0 1",
        ),
        (
            "0 3 { 1 nosuch } bind repeat",
            "%%[ Error: undefined; OffendingCommand: nosuch; Line: 1 ]%% | 0 1",
        ),
        (
            "1 1 3 { 0 div } for",
            "%%[ Error: undefinedresult; OffendingCommand: div; Line: 1 ]%% | ",
        ),
        (
            "1 1 3 { 0 div } bind for",
            "%%[ Error: undefinedresult; OffendingCommand: for; Line: 1 ]%% | ",
        ),
        (
            "/p { 1 1 3 { 0 div } bind for } bind def p",
            "%%[ Error: undefinedresult; OffendingCommand: p; Line: 1 ]%% | ",
        ),
        (
            "[1 (a)] { 1 add } forall",
            "%%[ Error: typecheck; OffendingCommand: add; Line: 1 ]%% | 2",
        ),
        (
            "[1 (a)] { 1 add } bind forall",
            "%%[ Error: typecheck; OffendingCommand: forall; Line: 1 ]%% | 2",
        ),
        (
            "0 { 1 add dup 3 eq { pop pop } if } loop",
            "%%[ Error: stackunderflow; OffendingCommand: pop; Line: 1 ]%% | ",
        ),
        ("3 { 1 exit } stopped", "ok | 3 1 true"),
        (
            "exit",
            "%%[ Error: invalidexit; OffendingCommand: exit; Line: 1 ]%% | ",
        ),
        (
            "{ exit } exec",
            "%%[ Error: invalidexit; OffendingCommand: exit; Line: 1 ]%% | ",
        ),
        (
            "1 1 3 { pop exit } for 1 (a) add",
            "%%[ Error: typecheck; OffendingCommand: add; Line: 1 ]%% | 1",
        ),
    ];
    let mut failures = Vec::new();
    for (src, want) in cases {
        let got = uncaught(src);
        if got != *want {
            failures.push(format!("{src:?}: got {got:?}, want {want:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn loop_step_counts_are_one_per_element() {
    let cases: &[(&str, usize)] = &[
        ("3 { } repeat", 3),
        ("1 1 3 { } for", 5),
        ("[1 2 3] { pop } forall", 10),
        ("[1 2 3] { pop } bind forall", 11),
        ("(ab) { pop } forall", 5),
        ("<< /a 1 /b 2 >> { pop pop } forall", 12),
        ("0 { 1 add dup 3 eq { exit } if } loop", 25),
        ("0 { 1 add dup 3 eq { exit } if } bind loop", 26),
        ("3 { 1 pop } repeat", 9),
        ("2 { 2 { 1 pop } repeat } repeat", 17),
        ("2 { 2 { 1 pop } bind repeat } bind repeat", 20),
        ("{ 3 { 1 pop } repeat } exec", 11),
        ("{ 3 { 1 pop } repeat 2 pop } bind exec", 14),
        ("[1 2] { { pop } exec } forall", 12),
        ("/q { exit } def 3 { 1 q } repeat", 9),
        ("3 { { 1 exit } stopped pop } repeat", 18),
        (
            "1 1 4 { dup 2 mod 0 eq { pop } { 1 add pop } ifelse } bind for",
            46,
        ),
        ("[] { } forall 0 { } repeat 1 1 0 { } for", 12),
    ];
    let mut failures = Vec::new();
    for (src, want) in cases {
        let got = single_steps(src);
        if got != *want {
            failures.push(format!("{src:?}: got {got}, want {want}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_slice_of_n_steps_ends_where_n_single_steps_do_in_every_loop() {
    let programs = [
        "0 5 { 1 add dup 3 mod 0 eq { 1 } { 2 } ifelse pop } repeat",
        "0 5 { 1 add dup 3 mod 0 eq { 1 } { 2 } ifelse pop } bind repeat",
        "0 { 1 add dup 7 eq { exit } if } loop",
        "0 { 1 add dup 7 eq { exit } if } bind loop",
        "[1 2 3 4 5] { 2 mod 0 eq { 7 } if } bind forall",
        "(abcde) { 2 mod 0 eq { 7 } { 8 } ifelse } forall",
        "<< /a 1 /b 2 /c 3 >> { pop pop } forall",
        "1 1 4 { 1 1 3 { 2 copy lt { pop } { exch pop } ifelse } for } bind for",
        "3 { [1 2] { { pop } exec } forall } repeat",
        "/q { exit } def 1 1 5 { dup 3 eq { q } if } for",
    ];
    for src in programs {
        let singles = trace(src, 1);
        for b in [2, 3, 4, 5, 7] {
            let sliced = trace(src, b);
            let want: Vec<String> = singles
                .iter()
                .skip(b - 1)
                .step_by(b)
                .cloned()
                .chain(std::iter::once(singles.last().unwrap().clone()))
                .collect();
            let n = sliced.len();
            assert_eq!(&sliced[..n - 1], &want[..n - 1], "{src}: budget {b}");
            assert_eq!(sliced.last(), singles.last(), "{src}: budget {b}");
        }
    }
}

/// The smallest `--max-steps` cap `src` passes.
fn steps_needed(src: &str) -> u64 {
    use pscat::Limits;
    let passes = |max: u64| {
        let mut it = Interp::new();
        it.set_limits(Limits::none().with_max_steps(max))
            .expect("limits apply");
        it.run_str(src).is_ok()
    };
    let (mut lo, mut hi) = (0u64, 1u64 << 24);
    assert!(passes(hi), "{src}: needs more than {hi} steps");
    while lo < hi {
        let mid = (lo + hi) / 2;
        if passes(mid) { hi = mid } else { lo = mid + 1 }
    }
    lo
}

#[test]
fn the_step_cap_counts_loops_as_before() {
    // Pinned on the tree before #875. A loop iteration counts as a step
    // of its own under limits, on top of its body's elements.
    let cases: &[(&str, u64)] = &[
        ("5000 { } repeat", 5000),
        ("0 1 1 3000 { add } for pop", 3007),
        ("0 1 1 3000 { add } bind for pop", 3008),
        ("0 { 1 add dup 2500 eq { exit } if } bind loop pop", 19846),
        ("2000 array { pop } forall", 2004),
        ("1500 string { pop } bind forall", 1505),
        ("50 { 50 { 1 pop } repeat } repeat", 7180),
        (
            "1 1 600 { dup 2 mod 0 eq { pop } { 3 mul pop } ifelse } bind for",
            6415,
        ),
    ];
    let mut failures = Vec::new();
    for (src, want) in cases {
        let got = steps_needed(src);
        if got != *want {
            failures.push(format!("{src:?}: needs {got}, want {want}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn recursion_through_loop_bodies_overflows_the_execution_stack_at_the_same_depth() {
    // The operand count pins the depth reached: each level leaves one
    // operand. The `{ r 0 }` variants shift the depth by one frame, so the
    // overflow lands on a different push.
    // (recursion, then the operand stack's top four after `{ r } stopped`
    // and after `{ r 0 } stopped`: the `stopped` result, the operand count,
    // `$error`'s errorname and command).
    let cases: &[(&str, &str, &str)] = &[
        (
            "/r { 1 2 { r } repeat } def",
            "true 10001 /execstackoverflow repeat",
            "true 10000 /execstackoverflow repeat",
        ),
        (
            "/r { 1 2 { r 1 } repeat } def",
            "true 5002 /execstackoverflow r",
            "true 5002 /execstackoverflow repeat",
        ),
        (
            "/r { 1 2 { 1 r } repeat } def",
            "true 19998 /execstackoverflow repeat",
            "true 19996 /execstackoverflow repeat",
        ),
        (
            "/r { 1 1 1 2 { pop r } for } def",
            "true 10002 /execstackoverflow for",
            "true 10001 /execstackoverflow for",
        ),
        (
            "/r { 1 [1 2] { pop r 1 } forall } def",
            "true 5002 /execstackoverflow r",
            "true 5003 /execstackoverflow forall",
        ),
        (
            "/r { 1 { r } loop } def",
            "true 10001 /execstackoverflow loop",
            "true 10000 /execstackoverflow loop",
        ),
        (
            "/r { 1 { r 1 } loop } def",
            "true 5002 /execstackoverflow r",
            "true 5002 /execstackoverflow loop",
        ),
        (
            "/r { 1 2 { true { r 1 } if } repeat } def",
            "true 5002 /execstackoverflow r",
            "true 5002 /execstackoverflow repeat",
        ),
        (
            "/r { 1 2 { true { r } if } repeat } def",
            "true 10001 /execstackoverflow repeat",
            "true 10000 /execstackoverflow repeat",
        ),
        (
            "/r { 1 2 { r } bind repeat } def",
            "true 10001 /execstackoverflow repeat",
            "true 10000 /execstackoverflow repeat",
        ),
        (
            "/r { 1 2 { r 1 } bind repeat } def",
            "true 5002 /execstackoverflow r",
            "true 5002 /execstackoverflow repeat",
        ),
        (
            "/r { 1 2 { true { r 1 } if } bind repeat } def",
            "true 5002 /execstackoverflow r",
            "true 5002 /execstackoverflow repeat",
        ),
        (
            "/r { 1 2 { 2 { r } repeat } repeat } def",
            "true 5002 /execstackoverflow repeat",
            "true 5002 /execstackoverflow repeat",
        ),
    ];
    let mut failures = Vec::new();
    for (def, want_r, want_r0) in cases {
        for (call, want) in [("{ r } stopped", want_r), ("{ r 0 } stopped", want_r0)] {
            let src =
                format!("{def} {call} $error /errorname get $error /command get count 3 1 roll");
            let mut it = Interp::new();
            it.run_str(&src).unwrap_or_else(|e| panic!("{src}: {e}"));
            let n = it.operand_stack().len();
            let top: Vec<String> = it.operand_stack()[n - 4..]
                .iter()
                .map(|o| o.repr())
                .collect();
            let got = top.join(" ");
            if got != *want {
                failures.push(format!("{src:?}: got {got:?}, want {want:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_profiles_count_calls_and_operators_in_loops_as_before() {
    let mut it = Interp::new();
    it.enable_profile();
    it.run_str(
        "/p { 1 pop } def /q { exit } def /s { 7 exit 8 } def \
         5 { p } repeat 3 { p 2 pop } repeat { q } loop 4 { 1 s } repeat pop \
         1 1 3 { pop p } for [1 2] { pop { q } loop p } forall",
    )
    .expect("runs");
    let report = it.profile_report().expect("profiling is on");
    let mut rows: Vec<String> = report
        .rows
        .iter()
        .map(|r| format!("{} {}", r.name, r.calls))
        .collect();
    rows.sort();
    assert_eq!(rows.join(", "), "p 13, q 3, s 1");

    let mut it = Interp::new();
    it.enable_profile();
    it.enable_profile_ops();
    it.run_str(
        "3 { 1 2 add pop } bind repeat 1 1 4 { 2 mul pop } bind for \
         [1 2] { pop } bind forall 0 { 1 add dup 3 eq { exit } if } bind loop pop",
    )
    .expect("runs");
    let report = it.profile_report().expect("profiling is on");
    let ops = report.operators.expect("operators are on");
    let mut got: Vec<String> = ops
        .iter()
        .map(|o| format!("{} {}", o.name, o.calls))
        .collect();
    got.sort();
    assert_eq!(
        got.join(", "),
        "] 1, add 6, bind 4, dup 3, eq 3, exit 1, for 1, forall 1, if 3, loop 1, mul 4, pop 10, repeat 1"
    );
}
