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
