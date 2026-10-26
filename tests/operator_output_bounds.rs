//! `==`, `pstack` and `stack` bound their output (issue #745), and the
//! undefined-name error line caps the name. The first group pins ordinary
//! output byte-for-byte as literals (not via `repr()`, which these
//! operators share code with), so the bound cannot change what a normal
//! program prints.

use pscat::Interp;

fn run(src: &str) -> String {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{src:?} failed: {e}"));
    String::from_utf8_lossy(it.captured_stdout()).into_owned()
}

#[test]
fn scalars_print_exactly_as_before() {
    let src = "1 == -7 == 2.0 == 0.1 == 1e-7 == 1.5e10 == true == false == null == mark == \
               /name == /name1 cvx == 2 dict == 3 4 add ==";
    assert_eq!(
        run(src),
        "1\n-7\n2.0\n0.1\n1e-7\n15000000000.0\ntrue\nfalse\nnull\n-mark-\n/name\nname1\n-dict-\n7\n"
    );
}

#[test]
fn strings_names_and_containers_print_exactly_as_before() {
    assert_eq!(run("(plain) =="), "(plain)\n");
    assert_eq!(
        run("(a\\(b\\)\\\\c\\n\\r\\t\\b\\f\\001\\377 z) =="),
        "(a\\(b\\)\\\\c\\n\\r\\t\\b\\f\\001\\377 z)\n"
    );
    assert_eq!(run("[] == {} =="), "[]\n{}\n");
    assert_eq!(
        run("[1 [2.5 (s) /n] {x 1 add {y}} null] =="),
        "[1 [2.5 (s) /n] {x 1 add {y}} null]\n"
    );
    assert_eq!(run("/add load =="), "--add--\n");
}

#[test]
fn pstack_and_stack_print_top_first_exactly_as_before() {
    let src = "1 2.0 (a\\nb) /n {x 1 add} [1 [2]] null";
    assert_eq!(
        run(&format!("{src} pstack")),
        "null\n[1 [2]]\n{x 1 add}\n/n\n(a\\nb)\n2.0\n1\n"
    );
    assert_eq!(
        run(&format!("{src} stack")),
        "--nostringval--\n--nostringval--\n--nostringval--\nn\na\nb\n2.0\n1\n"
    );
    assert_eq!(run("pstack stack"), "");
}

#[test]
fn a_few_hundred_elements_and_a_page_of_text_print_in_full() {
    let out = run("[ 0 1 499 { 2 div } for ] ==");
    assert!(out.starts_with("[0.0 0.5 1.0 "), "{}", &out[..30]);
    assert!(out.ends_with(" 249.5]\n"), "{out}");
    assert!(!out.contains("truncated"));
    // 3000 bytes of text: under the 4096-byte string limit.
    let out = run("3000 string dup 0 1 2999 { 1 index exch 97 put } for ==");
    assert_eq!(out.len(), 3000 + 3);
    assert!(!out.contains("truncated"));
}

#[test]
fn a_huge_array_prints_a_few_kb_ending_in_truncated() {
    let out = run("1000000 array ==");
    assert!(out.len() < 8 * 1024, "{} bytes", out.len());
    assert!(
        out.ends_with("... truncated]\n"),
        "{}",
        &out[out.len() - 30..]
    );
    assert!(out.starts_with("[null null "));
}

#[test]
fn a_huge_string_is_cut() {
    let out = run("1000000 string ==");
    assert!(out.len() < 4096 * 4 + 64, "{} bytes", out.len());
    assert!(out.ends_with(" ... truncated)\n"));
    let out = run("1000000 string stack");
    assert!(out.len() < 4096 + 64, "{} bytes", out.len());
    assert!(out.ends_with(" ... truncated\n"));
}

#[test]
fn deep_and_shared_structures_stay_bounded() {
    // 10000 levels: elided, not a stack overflow.
    let out = run(&format!("{}{} ==", "[".repeat(10_000), "]".repeat(10_000)));
    assert!(out.contains("... truncated"));
    assert!(out.len() < 4096, "{} bytes", out.len());
    // 2^40 leaves if expanded.
    let out = run("[0] 40 { dup 2 array astore } repeat ==");
    assert!(out.len() < 20 * 1024, "{} bytes", out.len());
    assert!(out.contains("... truncated"));
}

#[test]
fn pstack_and_stack_stop_after_the_stack_budget() {
    let out = run("100000 { (0123456789) } repeat pstack");
    assert!(out.len() < 40 * 1024, "{} bytes", out.len());
    assert!(out.ends_with("(0123456789)\n... truncated\n"));
    let out = run("100000 { 7 } repeat stack");
    assert!(out.len() < 40 * 1024, "{} bytes", out.len());
    assert!(out.ends_with("7\n... truncated\n"));
}

#[test]
fn the_undefined_name_in_an_error_line_is_capped() {
    let name = "x".repeat(5000);
    let mut it = Interp::new();
    let e = it.run_str(&name).unwrap_err();
    let report = it.error_report(&e);
    assert!(report.len() < 700, "{} bytes", report.len());
    assert!(report.contains(&format!(
        "OffendingCommand: {} ... truncated",
        "x".repeat(512)
    )));
    // A short name is untouched.
    let mut it = Interp::new();
    let e = it.run_str("frobnicate").unwrap_err();
    assert_eq!(
        it.error_report(&e),
        "%%[ Error: undefined; OffendingCommand: frobnicate; Line: 1 ]%%"
    );
}
