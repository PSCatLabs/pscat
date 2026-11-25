//! Issue #1405: a backtrace frame says which line (and file) of the program
//! the running element of a procedure was written on, through the host API
//! ([`pscat::host::TraceFrame::line`] and `file`).

use pscat::Interp;
use pscat::host::TraceFrame;

mod common;

/// Run `src` expecting an error; the backtrace, innermost first.
fn trace_of(interp: &mut Interp, src: &str) -> Vec<TraceFrame> {
    let err = interp.run_str(src).expect_err("the program should fail");
    interp.error_details(&err).backtrace
}

fn at(frame: &TraceFrame) -> (Option<&str>, Option<usize>, Option<&str>) {
    (
        frame.procedure.as_deref(),
        frame.line,
        frame.file.as_deref(),
    )
}

#[test]
fn errors_at_different_depths_each_get_their_own_line() {
    let src = "\
/inner {
  1 2 add pop
  (x) 3 add
  pop
} def
/mid {
  0 pop
  inner
  1 pop
} def
/outer {
  mid
  2 pop
} def
outer
";
    let mut interp = Interp::new();
    let t = trace_of(&mut interp, src);
    assert_eq!(t.len(), 3, "{t:?}");
    // The `add` on line 3, `inner` called on line 8, `mid` called on line 12.
    assert_eq!(at(&t[0]), (Some("inner"), Some(3), None));
    assert_eq!(at(&t[1]), (Some("mid"), Some(8), None));
    assert_eq!(at(&t[2]), (Some("outer"), Some(12), None));
}

#[test]
fn the_line_is_the_one_of_the_element_not_of_the_procedure() {
    // Several elements on a line, a body that starts and ends on other
    // lines, and a comment and blank lines between elements.
    let src = "/f { 1 2 add\n  3 4 % note\n\n  mul\n  (s) 0 add pop pop } def f\n";
    let mut interp = Interp::new();
    let t = trace_of(&mut interp, src);
    assert_eq!(at(&t[0]), (Some("f"), Some(5), None));
}

#[test]
fn a_nested_procedure_and_a_loop_body_have_lines() {
    let src = "\
/f {
  1
  {
    2 pop
    (a) 0 add
  } exec
  3
} def
f
";
    let mut interp = Interp::new();
    let t = trace_of(&mut interp, src);
    // The unnamed body, then `f`, which was running its `exec` (line 6).
    assert_eq!(at(&t[0]), (None, Some(5), None));
    assert_eq!(at(&t[1]), (Some("f"), Some(6), None));

    let src = "\
/g {
  3 {
    dup
    0 div
  } repeat
  pop
} def
g
";
    let t = trace_of(&mut interp, src);
    assert_eq!(at(&t[0]), (None, Some(4), None));
    assert_eq!(at(&t[1]), (Some("g"), Some(5), None));

    // `for`, `forall` and `loop` bodies the same way.
    let t = trace_of(
        &mut interp,
        "/h {\n  1 1 2 {\n    pop\n    (a) (b) add\n  } for\n} def\nh",
    );
    assert_eq!(at(&t[0]), (None, Some(4), None));
}

#[test]
fn a_run_file_names_itself_and_the_main_program_does_not() {
    let dir = common::scratch_dir("srcpos-run");
    let lib = dir.join("lib.ps");
    std::fs::write(
        &lib,
        "%!PS\n/deep {\n  1\n  0 div\n} def\n/shallow {\n  deep\n  0 pop\n} def\n",
    )
    .expect("write lib");
    let lib = lib.to_string_lossy().into_owned();
    let mut interp = Interp::new();
    // The error is in a later program than the one that ran the file.
    interp.run_str(&format!("({lib}) run")).expect("run lib");
    let t = trace_of(&mut interp, "\n/top {\n  shallow\n  1 pop\n} def\ntop");
    assert_eq!(at(&t[0]), (Some("deep"), Some(4), Some(lib.as_str())));
    assert_eq!(at(&t[1]), (Some("shallow"), Some(7), Some(lib.as_str())));
    // `top` is in the failed program's own text: no file, line 3.
    assert_eq!(at(&t[2]), (Some("top"), Some(3), None));
}

#[test]
fn an_executed_string_and_a_made_procedure_have_no_line() {
    let mut interp = Interp::new();
    // Code read from a string has lines of its own, which are not the
    // program's: none is reported, though the procedure is still named.
    let t = trace_of(&mut interp, "(\n/f {\n 1 0 div\n} def) cvx exec\nf");
    assert_eq!(at(&t[0]), (Some("f"), None, None));
    // A procedure scanned by `token` the same.
    let t = trace_of(
        &mut interp,
        "(\n{\n 1 0 div\n}) token pop exch pop /g exch def g",
    );
    assert_eq!(at(&t[0]), (Some("g"), None, None));
    // A procedure built by the program.
    let t = trace_of(&mut interp, "/h [ 1 0 /div load ] cvx def h");
    assert_eq!(at(&t[0]), (Some("h"), None, None));
}

#[test]
fn bind_cvx_cvlit_and_getinterval_keep_the_lines_and_copy_drops_them() {
    // One program per case: lines belong to the program that wrote them.
    let defs = "\
/f {
  5 pop
  1 0
  div
  7 pop
} def
/b {
  2 pop
  1 0 div
  3 pop
} bind def
/c { 1 0 div 2 pop } cvlit cvx def
/h /f load 2 3 getinterval cvx def
/k /f load dup length array copy cvx def
";
    for (call, line) in [
        // bind: the same storage, the names replaced by operators.
        ("b", Some(9)),
        // cvlit then cvx: the same storage with the attribute changed.
        ("c", Some(12)),
        // getinterval: a view from element 2, `1 0 div`; its element 2 is
        // the `div` of line 4.
        ("h", Some(4)),
        // copy: new storage, a new procedure.
        ("k", None),
    ] {
        let mut interp = Interp::new();
        let t = trace_of(&mut interp, &format!("{defs}{call}"));
        assert_eq!(at(&t[0]), (Some(call), line, None), "{call}: {t:?}");
    }
}

#[test]
fn an_element_put_into_a_body_keeps_the_line_of_the_token_it_replaced() {
    let mut interp = Interp::new();
    let t = trace_of(
        &mut interp,
        "/f {\n  1\n  0\n  nothing\n  pop\n} def\n/f load 2 /div load put\nf",
    );
    assert_eq!(at(&t[0]), (Some("f"), Some(4), None));
}

#[test]
fn a_host_names_the_program_that_defines_procedures() {
    let mut interp = Interp::new();
    interp.set_main_file_name(Some("scene.ps"));
    interp
        .run_str(
            "%!PS\n/label {\n  10 20 moveto\n  (x) 1 add\n} def\n/page {\n  label\n  1 pop\n} def",
        )
        .expect("load");
    interp.set_main_file_name(None);
    // The next program is another text; its line 1 is not scene.ps's.
    let err = interp.run_str("\n\npage").unwrap_err();
    let d = interp.error_details(&err);
    assert_eq!((d.line, d.file.as_deref()), (Some(3), None));
    assert_eq!(
        at(&d.backtrace[0]),
        (Some("label"), Some(4), Some("scene.ps"))
    );
    assert_eq!(
        at(&d.backtrace[1]),
        (Some("page"), Some(7), Some("scene.ps"))
    );
}

#[test]
fn an_unnamed_program_leaves_no_lines_for_the_next_one() {
    let mut interp = Interp::new();
    interp.run_str("\n\n/f { 1 0 div } def").expect("define");
    // `f` was written on line 3 of a program that had no name: the next
    // program's line 3 is another text, so no line is better than that one.
    let t = trace_of(&mut interp, "f");
    assert_eq!(at(&t[0]), (Some("f"), None, None));
    // Within the same program the line is there.
    let t = trace_of(&mut interp, "\n\n/f { 1 0 div } def\nf");
    assert_eq!(at(&t[0]), (Some("f"), Some(3), None));
}

#[test]
fn a_named_program_run_again_is_the_same_text() {
    let mut interp = Interp::new();
    interp.set_main_file_name(Some("a.ps"));
    for _ in 0..3 {
        interp.run_str("\n/f { 1 0 div } def").expect("define");
    }
    interp.set_main_file_name(None);
    let t = trace_of(&mut interp, "f");
    assert_eq!(at(&t[0]), (Some("f"), Some(2), Some("a.ps")));
}

#[test]
fn a_lint_run_still_finds_the_def_inside_a_body() {
    // #1338 reads the same table: the shadow's line is where the `def` is
    // written, not where the loop ran.
    let mut interp = Interp::new();
    interp.trace_operator_shadows();
    interp
        .run_str("/f {\n  /add 1 def\n} def\n2 { f } repeat\n")
        .expect("runs");
    let findings = pscat::lint::check(&interp, false);
    let text: Vec<String> = findings.iter().map(|f| f.message.clone()).collect();
    assert!(
        text.iter().any(|m| m.contains("line 2")),
        "the def is on line 2: {text:?}"
    );
}

#[test]
fn the_cli_lists_each_procedure_after_an_unchanged_report_line() {
    use common::CommandDeadline as _;
    let dir = common::scratch_dir("srcpos-cli");
    std::fs::write(
        dir.join("lib.ps"),
        "%!PS\n/deep {\n  1\n  0 div\n  pop\n} def\n",
    )
    .expect("write lib");
    std::fs::write(
        dir.join("art.ps"),
        "%!PS\n(lib.ps) run\n/petal {\n  0 pop\n  deep\n  1 pop\n} def\n\npetal\n",
    )
    .expect("write art");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["--headless", "art.ps"])
        .current_dir(&dir)
        .output_deadline()
        .expect("run pscat");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let lines: Vec<&str> = stderr.lines().collect();
    // The report is what it was: where execution was, the call on line 9.
    assert_eq!(
        lines[0],
        "%%[ Error: undefinedresult; OffendingCommand: div; Line: 9 ]%%"
    );
    assert_eq!(lines[1], "  in deep, line 4 (lib.ps): 1 0 >>> div <<< pop");
    assert_eq!(
        lines[2],
        "  in petal, line 5 (art.ps): 0 pop >>> deep <<< 1 pop"
    );
}
