//! `pscat --profile` (issue #737): call counts per named procedure, and
//! inclusive and exclusive time that stay consistent through recursion,
//! tail calls, `stop`/`stopped`, `exit` and errors.

use std::process::{Command, Stdio};

use pscat::Interp;
use pscat::profile::{ProfileReport, ProfileRow};

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// Run `src` with profiling on; the run's result is returned too, since an
/// error is part of what some tests check.
fn profile(src: &str) -> (Result<(), pscat::PsError>, ProfileReport) {
    let mut interp = Interp::new();
    interp.enable_profile();
    let result = interp.run_str(src);
    let report = interp.profile_report().expect("profiling is on");
    (result, report)
}

fn row<'a>(report: &'a ProfileReport, name: &str) -> &'a ProfileRow {
    report
        .rows
        .iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no row for {name}: {report:?}"))
}

/// Exclusive time is never more than inclusive, and rows are sorted by
/// exclusive time.
fn assert_consistent(report: &ProfileReport) {
    for r in &report.rows {
        assert!(r.exclusive <= r.inclusive, "{r:?}");
    }
    for w in report.rows.windows(2) {
        assert!(w[0].exclusive >= w[1].exclusive, "not sorted: {report:?}");
    }
}

/// A short body that takes measurable time.
const BURN: &str = "0 1 3000 { pop } for";

#[test]
fn a_known_call_graph_gives_the_expected_counts() {
    let (result, report) = profile(&format!(
        "/c {{ {BURN} }} def
         /b {{ c c }} def
         /a {{ b 1 pop b }} def
         a a a"
    ));
    result.expect("runs");
    assert_eq!(row(&report, "a").calls, 3);
    assert_eq!(row(&report, "b").calls, 6);
    assert_eq!(row(&report, "c").calls, 12);
    assert_eq!(report.rows.len(), 3, "operators are not procedures");
    assert_consistent(&report);
    // Inclusive time nests: a contains b contains c.
    assert!(row(&report, "a").inclusive >= row(&report, "b").inclusive);
    assert!(row(&report, "b").inclusive >= row(&report, "c").inclusive);
    // A leaf has no children to subtract.
    let c = row(&report, "c");
    assert_eq!(c.exclusive, c.inclusive);
}

#[test]
fn recursion_counts_every_call_but_inclusive_time_once() {
    let (result, report) = profile(
        "/fib { dup 2 lt { } { dup 1 sub fib exch 2 sub fib add } ifelse } def
         15 fib pop",
    );
    result.expect("runs");
    // fib(15) makes 1973 calls.
    let fib = row(&report, "fib");
    assert_eq!(fib.calls, 1973);
    assert_consistent(&report);
    // Counting each frame's inclusive time would make it ~14x the run; once
    // per outermost frame, it is the run, and the frames' own time adds up
    // to no more than that.
    assert_eq!(fib.exclusive, fib.inclusive);
}

#[test]
fn time_in_a_tail_call_belongs_to_the_caller_too() {
    let (result, report) = profile(&format!(
        "/heavy {{ {BURN} }} def
         /caller {{ 1 pop heavy }} def
         caller"
    ));
    result.expect("runs");
    assert_consistent(&report);
    let (caller, heavy) = (row(&report, "caller"), row(&report, "heavy"));
    assert_eq!((caller.calls, heavy.calls), (1, 1));
    assert!(caller.inclusive >= heavy.inclusive, "{report:?}");
    assert!(heavy.inclusive > std::time::Duration::ZERO);
}

#[test]
fn an_expensive_last_operator_is_the_procedure's_own_time() {
    // `for` is the last element, so the proc's frame is gone while it runs.
    let (result, report) = profile(&format!("/f {{ 1 pop {BURN} }} def f"));
    result.expect("runs");
    let f = row(&report, "f");
    assert!(f.exclusive > std::time::Duration::ZERO, "{report:?}");
    assert_eq!(f.exclusive, f.inclusive);
}

#[test]
fn deep_tail_recursion_still_runs_and_is_counted() {
    // Without --profile this runs at constant exec-stack depth; with it, a
    // marker per call would pass the 10,000-frame limit.
    let src = "/down { dup 0 gt { 1 sub down } { pop } ifelse } def 60000 down";
    let mut plain = Interp::new();
    plain.run_str(src).expect("runs without profiling");
    let (result, report) = profile(src);
    result.expect("runs with profiling");
    assert_eq!(row(&report, "down").calls, 60001);
    assert_consistent(&report);
}

#[test]
fn mutual_tail_recursion_stays_shallow_too() {
    let (result, report) = profile(
        "/ping { dup 0 gt { 1 sub pong } { pop } ifelse } def
         /pong { dup 0 gt { 1 sub ping } { pop } ifelse } def
         50000 ping",
    );
    result.expect("runs");
    assert_eq!(row(&report, "ping").calls + row(&report, "pong").calls, 50001);
    assert_consistent(&report);
}

#[test]
fn stop_caught_by_stopped_closes_the_calls_it_unwinds() {
    let (result, report) = profile(&format!(
        "/g {{ {BURN} stop }} def
         /f {{ 1 pop g }} def
         {{ f }} stopped pop
         /after {{ 1 }} def after pop"
    ));
    result.expect("runs");
    assert_consistent(&report);
    let (f, g) = (row(&report, "f"), row(&report, "g"));
    assert_eq!((f.calls, g.calls), (1, 1));
    // Both were closed by the unwinding: they have time, nested properly.
    assert!(g.inclusive > std::time::Duration::ZERO, "{report:?}");
    assert!(f.inclusive >= g.inclusive);
    assert_eq!(row(&report, "after").calls, 1);
}

#[test]
fn an_error_caught_by_stopped_closes_the_calls_it_unwinds() {
    let (result, report) = profile(&format!(
        "/bad {{ {BURN} 1 0 div }} def
         /f {{ 1 pop bad 2 pop }} def
         {{ f }} stopped pop"
    ));
    result.expect("runs");
    assert_consistent(&report);
    assert!(row(&report, "bad").inclusive > std::time::Duration::ZERO);
    assert!(row(&report, "f").inclusive >= row(&report, "bad").inclusive);
}

#[test]
fn an_uncaught_error_still_reports_the_open_calls() {
    let (result, report) = profile(&format!(
        "/bad {{ {BURN} 1 0 div }} def
         /outer {{ 1 pop bad 2 pop }} def
         outer"
    ));
    assert!(result.is_err());
    assert_consistent(&report);
    assert!(row(&report, "bad").inclusive > std::time::Duration::ZERO);
    assert!(row(&report, "outer").inclusive >= row(&report, "bad").inclusive);
}

#[test]
fn exit_from_inside_a_named_procedure_closes_it() {
    let (result, report) = profile(&format!(
        "/leave {{ {BURN} exit }} def
         /n 0 def
         {{ /n n 1 add def leave }} loop
         /after {{ 1 pop }} def after"
    ));
    result.expect("runs");
    assert_consistent(&report);
    assert_eq!(row(&report, "leave").calls, 1);
    assert!(row(&report, "leave").inclusive > std::time::Duration::ZERO);
    assert_eq!(row(&report, "after").calls, 1);
}

#[test]
fn quit_inside_a_procedure_still_reports() {
    let (_, report) = profile(&format!("/q {{ {BURN} quit }} def q"));
    assert_consistent(&report);
    assert!(row(&report, "q").inclusive > std::time::Duration::ZERO);
}

#[test]
fn an_unprofiled_interpreter_has_no_report() {
    let mut interp = Interp::new();
    interp.run_str("/f { 1 pop } def f").expect("runs");
    assert!(interp.profile_report().is_none());
}

#[test]
fn an_empty_procedure_is_counted() {
    let (result, report) = profile("/nop { } def nop nop");
    result.expect("runs");
    assert_eq!(row(&report, "nop").calls, 2);
}

#[test]
fn a_procedure_run_without_its_name_belongs_to_its_caller() {
    let (result, report) = profile(&format!(
        "/f {{ {{ {BURN} }} exec 1 pop }} def f"
    ));
    result.expect("runs");
    let f = row(&report, "f");
    assert_eq!(report.rows.len(), 1);
    assert!(f.exclusive > std::time::Duration::ZERO);
}

#[test]
fn the_text_report_has_the_columns_and_honours_the_row_limit() {
    let (_, report) = profile("/a { } def /b { } def /c { } def a b c");
    let text = report.to_text(Some(2));
    assert!(text.contains("calls") && text.contains("exclusive") && text.contains("inclusive"));
    assert!(text.contains("2 of 3"), "{text}");
    assert!(text.contains("1 more"), "{text}");
    assert!(report.to_text(None).contains("3 of 3"));
    let json = report.to_json(Some(1));
    assert!(json.starts_with("{\"procedures\":[{\"name\":"), "{json}");
    assert!(json.ends_with("\"total_procedures\":3}"), "{json}");
}

fn pscat(args: &[&str], stdin: &str) -> (bool, String, String) {
    use std::io::Write;
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pscat");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("feed stdin");
    let out = child.wait_with_output().expect("wait");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_flag_prints_the_report_on_stderr() {
    let (ok, stdout, stderr) = pscat(
        &["--headless", "--profile", "-"],
        "/sq { dup mul } def 3 sq pop 4 sq pop",
    );
    assert!(ok, "{stderr}");
    assert!(!stdout.contains("calls"), "report belongs on stderr: {stdout}");
    assert!(stderr.contains("pscat: profile"), "{stderr}");
    assert!(stderr.lines().any(|l| l.trim_end().ends_with("sq") && l.contains(" 2 ")), "{stderr}");
}

#[test]
fn the_flag_reports_after_a_failed_run_too() {
    let (ok, _, stderr) = pscat(&["--profile", "-e", "/f { 1 0 div } def f"], "");
    assert!(!ok);
    assert!(stderr.contains("undefinedresult") || stderr.contains("Error"), "{stderr}");
    assert!(stderr.contains("pscat: profile") && stderr.contains(" f"), "{stderr}");
}

#[test]
fn profile_top_limits_rows_and_zero_means_all() {
    let src = "/a { } def /b { } def /c { } def a b c";
    let (_, _, two) = pscat(&["--profile", "--profile-top", "2", "-e", src], "");
    assert!(two.contains("2 of 3") && two.contains("1 more"), "{two}");
    let (_, _, all) = pscat(&["--profile", "--profile-top", "0", "-e", src], "");
    assert!(all.contains("3 of 3") && !all.contains("more"), "{all}");
}

#[test]
fn profile_json_is_one_json_line() {
    let (ok, _, stderr) = pscat(&["--profile-json", "-e", "/a { } def a a"], "");
    assert!(ok, "{stderr}");
    let line = stderr.trim();
    assert!(line.starts_with("{\"procedures\":["), "{line}");
    assert!(line.contains("\"name\":\"a\",\"calls\":2"), "{line}");
    assert!(!line.contains('\n'));
}

#[test]
fn modes_without_a_single_headless_run_refuse_it() {
    for args in [
        &["--profile", "--bands", "2", "--png", "/dev/null", "-e", "1 pop"][..],
        &["--profile", "-e", "1 pop", "--sweep-seed", "1:2", "--png", "/dev/null"][..],
        &["--profile", "--spool", "/tmp"][..],
        &["--profile", "--interactive"][..],
        &["--profile", "--headless"][..],
        &["--profile", "somefile.ps"][..],
        &["--profile-top", "5", "-e", "1 pop"][..],
        &["--selftest", "lib/artkit.ps", "--profile"][..],
    ] {
        let (ok, _, stderr) = pscat(args, "");
        assert!(!ok, "{args:?} should be refused");
        assert!(stderr.contains("pscat:"), "{args:?}: {stderr}");
    }
}
