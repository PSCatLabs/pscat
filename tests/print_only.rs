//! `=only` and `==only` (#1413): the Ghostscript extensions that print like
//! `=` and `==` without the trailing newline. The shared cases are pinned
//! from Ghostscript 10.07.1 and, when `gs` is installed, run through it too,
//! so both interpreters must print the same bytes.

use std::process::Command;

mod common;

use common::CommandDeadline as _;
use pscat::Interp;

/// Plain PostScript, valid in pscat and in gs. `T` holds one value of each
/// type on which the two interpreters print `=` and `==` identically (the
/// known gaps are `=` of an operator and of a string with high bytes, and
/// the spelling of large reals; those are covered by `matches_the_newline_forms`).
/// Each value goes through `=`, `=only`, `==` and `==only`, framed so the
/// absence of the newline shows.
const SHARED: &str = "\
/T [ 42 -3.5 true null /nm (a\\(b\\)\\n) [1 /a (s) {x y}] {1 2 add} << /a 1 >> \
/abc cvx (abc) cvx -1 0.0 1.0e-10 /with#20space () 100000000000 (a\\\\b) [] {} 2147483648 ] def \
T { dup (=: ) print = (=only: ) print =only (|\\n) print } forall \
T { dup (==: ) print == (==only: ) print ==only (|\\n) print } forall \
mark ==only (|\\n) print mark =only (|\\n) print\n";

/// What both interpreters print for `SHARED`.
const SHARED_OUT: &str = "\
    =: 42\n\
    =only: 42|\n\
    =: -3.5\n\
    =only: -3.5|\n\
    =: true\n\
    =only: true|\n\
    =: --nostringval--\n\
    =only: --nostringval--|\n\
    =: nm\n\
    =only: nm|\n\
    =: a(b)\n\
    \n\
    =only: a(b)\n\
    |\n\
    =: --nostringval--\n\
    =only: --nostringval--|\n\
    =: --nostringval--\n\
    =only: --nostringval--|\n\
    =: --nostringval--\n\
    =only: --nostringval--|\n\
    =: abc\n\
    =only: abc|\n\
    =: abc\n\
    =only: abc|\n\
    =: -1\n\
    =only: -1|\n\
    =: 0.0\n\
    =only: 0.0|\n\
    =: 1e-10\n\
    =only: 1e-10|\n\
    =: with#20space\n\
    =only: with#20space|\n\
    =: \n\
    =only: |\n\
    =: 100000000000\n\
    =only: 100000000000|\n\
    =: a\\b\n\
    =only: a\\b|\n\
    =: --nostringval--\n\
    =only: --nostringval--|\n\
    =: --nostringval--\n\
    =only: --nostringval--|\n\
    =: 2147483648\n\
    =only: 2147483648|\n\
    ==: 42\n\
    ==only: 42|\n\
    ==: -3.5\n\
    ==only: -3.5|\n\
    ==: true\n\
    ==only: true|\n\
    ==: null\n\
    ==only: null|\n\
    ==: /nm\n\
    ==only: /nm|\n\
    ==: (a\\(b\\)\\n)\n\
    ==only: (a\\(b\\)\\n)|\n\
    ==: [1 /a (s) {x y}]\n\
    ==only: [1 /a (s) {x y}]|\n\
    ==: {1 2 add}\n\
    ==only: {1 2 add}|\n\
    ==: -dict-\n\
    ==only: -dict-|\n\
    ==: abc\n\
    ==only: abc|\n\
    ==: (abc)\n\
    ==only: (abc)|\n\
    ==: -1\n\
    ==only: -1|\n\
    ==: 0.0\n\
    ==only: 0.0|\n\
    ==: 1e-10\n\
    ==only: 1e-10|\n\
    ==: /with#20space\n\
    ==only: /with#20space|\n\
    ==: ()\n\
    ==only: ()|\n\
    ==: 100000000000\n\
    ==only: 100000000000|\n\
    ==: (a\\\\b)\n\
    ==only: (a\\\\b)|\n\
    ==: []\n\
    ==only: []|\n\
    ==: {}\n\
    ==only: {}|\n\
    ==: 2147483648\n\
    ==only: 2147483648|\n\
    -mark-|\n\
    --nostringval--|\n\
";

fn run(src: &str) -> String {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    String::from_utf8_lossy(it.captured_stdout()).into_owned()
}

#[test]
fn pscat_prints_what_gs_printed() {
    assert_eq!(run(SHARED), SHARED_OUT);
}

#[test]
fn gs_prints_the_pinned_bytes() {
    if !common::gs_available() {
        return;
    }
    let dir = common::scratch_dir("print_only_gs");
    let ps = dir.join("cases.ps");
    std::fs::write(&ps, SHARED).expect("write ps");
    let out = Command::new("gs")
        .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH"])
        .arg(&ps)
        .output_deadline()
        .expect("run gs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "gs failed");
    assert_eq!(String::from_utf8_lossy(&out.stdout), SHARED_OUT);
}

/// For every type, `=only` is `=` and `==only` is `==`, minus the newline.
/// The newline-less forms share the printer, so this also holds where pscat
/// and gs differ in what `=` prints (an operator, a high-byte string, a real
/// of 1e20 or more).
#[test]
fn matches_the_newline_forms() {
    let values = [
        "42",
        "-3.5",
        "1e20",
        "123456789.5",
        "true",
        "null",
        "/nm",
        "(a\\(b\\)\\n)",
        "(\\000\\377\\200)",
        "[1 /a (s) {x y}]",
        "{1 2 add}",
        "<< /a 1 >>",
        "/abc cvx",
        "/add load",
        "mark",
        "/c currentfile",
        "systemdict",
        "4 array",
    ];
    for v in values {
        for (with, without) in [("=", "=only"), ("==", "==only")] {
            let a = run(&format!("{v} {with}"));
            let b = run(&format!("{v} {without} (\\n) print"));
            assert_eq!(a, b, "{v} {without}");
            assert_eq!(run(&format!("{v} {without}")).len() + 1, a.len(), "{v}");
        }
    }
}

/// A large value is cut off by `==only` exactly where `==` cuts it.
#[test]
fn the_output_bounds_are_shared_with_the_newline_forms() {
    let build = "[ 20000 { 7 } repeat ] ";
    let a = run(&format!("{build} =="));
    let b = run(&format!("{build} ==only (\\n) print"));
    assert_eq!(a, b);
}

/// Output order with `print` and a `%stdout` file, as gs prints it.
#[test]
fn shares_the_stdout_stream_with_print_and_the_stdout_file() {
    let out = run("/o (%stdout) (w) file def \
         (A) print o (B) writestring (C) =only o (D) writestring 12 ==only o (E\\n) writestring \
         o flushfile");
    assert_eq!(out, "ABCD12E\n");
}

/// One operand is popped, whatever is below it stays, and an empty stack
/// raises `stackunderflow` with the operator named, before any output.
#[test]
fn consumes_one_operand_and_underflows_on_an_empty_stack() {
    assert_eq!(run("1 2 =only count ="), "21\n");
    assert_eq!(run("1 2 ==only count ="), "21\n");
    for op in ["=only", "==only"] {
        // `=` of an operator is a pre-existing gap (pscat prints
        // --nostringval--, gs the name), so read the name through cvs.
        let out = run(&format!(
            "{{ {op} }} stopped {{ $error /errorname get = \
             $error /command get 32 string cvs = count = }} if"
        ));
        assert!(
            out.starts_with("stackunderflow\n") && out.ends_with("\n0\n"),
            "{op}: {out:?}"
        );
    }
}

/// An uncaught underflow is reported under the operator's own name.
#[test]
fn the_underflow_report_names_the_operator() {
    for op in ["=only", "==only"] {
        let mut it = Interp::new();
        let err = it.run_str(op).expect_err("underflow");
        let report = it.error_report(&err);
        assert!(report.contains("stackunderflow"), "{report}");
        assert!(report.contains(op), "{report}");
    }
}

/// Both are operators in `systemdict`, distinct from `=` and `==`.
#[test]
fn are_operators_in_systemdict() {
    assert_eq!(
        run("systemdict /=only known = systemdict /==only known = \
             /=only load type = /==only load type = \
             /=only load /= load eq = /==only load /== load eq = \
             /=only load /==only load eq ="),
        "true\ntrue\noperatortype\noperatortype\nfalse\nfalse\nfalse\n"
    );
}
