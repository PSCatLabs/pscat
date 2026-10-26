//! `pscat --profile` (issue #737): call counts per named procedure, and
//! inclusive and exclusive time that stay consistent through recursion,
//! tail calls, `stop`/`stopped`, `exit` and errors.

use std::process::{Command, Stdio};

use pscat::Interp;
use pscat::profile::{EdgeRow, ProfileReport, ProfileRow};

mod common;

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

/// Like [`profile`], with the call graph on.
fn profile_graph(src: &str) -> (Result<(), pscat::PsError>, ProfileReport) {
    let mut interp = Interp::new();
    interp.enable_profile();
    interp.enable_profile_graph();
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
fn an_expensive_last_operator_is_the_procedures_own_time() {
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
fn mutual_tail_recursion_stays_shallow_and_the_time_goes_to_the_innermost() {
    let (result, report) = profile(
        "/ping { dup 0 gt { 1 sub pong } { pop } ifelse } def
         /pong { 0 1 500 { pop } for dup 0 gt { 1 sub ping } { pop } ifelse } def
         20000 ping",
    );
    result.expect("runs");
    let (ping, pong) = (row(&report, "ping"), row(&report, "pong"));
    assert_eq!(ping.calls + pong.calls, 20001);
    assert_consistent(&report);
    // One open `ping` and one open `pong` cover the whole chain; the bodies
    // all ran under `pong`'s.
    assert!(pong.exclusive > ping.exclusive * 10, "{report:?}");
    assert!(ping.inclusive >= pong.inclusive);
}

#[test]
fn deep_non_tail_recursion_fails_cleanly_and_still_reports() {
    // About 10,000 levels fit in a plain run, about 5,000 with the markers.
    let src =
        |n: u32| format!("/r {{ dup 0 gt {{ 1 sub r 1 add }} {{ pop 0 }} ifelse }} def {n} r pop");
    let mut plain = Interp::new();
    plain
        .run_str(&src(9000))
        .expect("9000 levels fit without --profile");
    let (result, report) = profile(&src(3000));
    result.expect("3000 levels fit with it");
    assert_eq!(row(&report, "r").calls, 3001);
    let (result, report) = profile(&src(9000));
    assert!(matches!(result, Err(pscat::PsError::ExecStackOverflow)));
    assert_consistent(&report);
    assert!(row(&report, "r").inclusive > std::time::Duration::ZERO);
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
    let (result, report) = profile(&format!("/f {{ {{ {BURN} }} exec 1 pop }} def f"));
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
    assert!(
        !stdout.contains("calls"),
        "report belongs on stderr: {stdout}"
    );
    assert!(stderr.contains("pscat: profile"), "{stderr}");
    assert!(
        stderr
            .lines()
            .any(|l| l.trim_end().ends_with("sq") && l.contains(" 2 ")),
        "{stderr}"
    );
}

#[test]
fn the_flag_reports_after_a_failed_run_too() {
    let (ok, _, stderr) = pscat(&["--profile", "-e", "/f { 1 0 div } def f"], "");
    assert!(!ok);
    assert!(
        stderr.contains("undefinedresult") || stderr.contains("Error"),
        "{stderr}"
    );
    assert!(
        stderr.contains("pscat: profile") && stderr.contains(" f"),
        "{stderr}"
    );
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
        &[
            "--profile",
            "--bands",
            "2",
            "--png",
            "/dev/null",
            "-e",
            "1 pop",
        ][..],
        &[
            "--profile",
            "-e",
            "1 pop",
            "--sweep-seed",
            "1:2",
            "--png",
            "/dev/null",
        ][..],
        &["--profile", "--spool", "no-such-spool-dir"][..],
        &["--profile", "--interactive"][..],
        &["--profile", "--headless"][..],
        &["--profile", "somefile.ps"][..],
        &["--profile-top", "5", "-e", "1 pop"][..],
        &[
            "--profile-graph",
            "--bands",
            "2",
            "--png",
            "/dev/null",
            "-e",
            "1 pop",
        ][..],
        &["--profile-graph", "--interactive"][..],
        &["--selftest", "lib/artkit.ps", "--profile"][..],
    ] {
        let (ok, _, stderr) = pscat(args, "");
        assert!(!ok, "{args:?} should be refused");
        assert!(stderr.contains("pscat:"), "{args:?}: {stderr}");
    }
}

// ---------------------------------------------------------------------
// --profile-graph (issue #799)
// ---------------------------------------------------------------------

fn edge<'a>(report: &'a ProfileReport, caller: Option<&str>, callee: &str) -> &'a EdgeRow {
    report
        .edges
        .as_ref()
        .expect("graph is on")
        .iter()
        .find(|e| e.caller.as_deref() == caller && e.callee == callee)
        .unwrap_or_else(|| panic!("no edge {caller:?} -> {callee}: {report:?}"))
}

/// Every call arrives along exactly one edge, so a callee's incoming edges
/// (the top level included) add up to its call count.
fn assert_edges_add_up(report: &ProfileReport) {
    let edges = report.edges.as_ref().expect("graph is on");
    for r in &report.rows {
        let incoming: u64 = edges
            .iter()
            .filter(|e| e.callee == r.name)
            .map(|e| e.calls)
            .sum();
        assert_eq!(incoming, r.calls, "{}: {report:?}", r.name);
    }
    assert!(
        edges
            .iter()
            .all(|e| report.rows.iter().any(|r| r.name == e.callee))
    );
    for w in edges.windows(2) {
        assert!(w[0].inclusive >= w[1].inclusive, "not sorted: {report:?}");
    }
}

#[test]
fn a_known_call_graph_gives_the_expected_edges() {
    let (result, report) = profile_graph(&format!(
        "/c {{ {BURN} }} def
         /b {{ c c }} def
         /a {{ b 1 pop b }} def
         a a a"
    ));
    result.expect("runs");
    assert_eq!(report.edges.as_ref().unwrap().len(), 3);
    assert_eq!(edge(&report, None, "a").calls, 3);
    assert_eq!(edge(&report, Some("a"), "b").calls, 6);
    assert_eq!(edge(&report, Some("b"), "c").calls, 12);
    assert_edges_add_up(&report);
    // Time nests along the edges, and an edge's time is the callee's here
    // (each has one caller).
    let (top_a, a_b, b_c) = (
        edge(&report, None, "a").inclusive,
        edge(&report, Some("a"), "b").inclusive,
        edge(&report, Some("b"), "c").inclusive,
    );
    assert!(top_a >= a_b && a_b >= b_c && b_c > std::time::Duration::ZERO);
    assert_eq!(top_a, row(&report, "a").inclusive);
    assert_eq!(b_c, row(&report, "c").inclusive);
}

#[test]
fn one_callee_from_two_callers_splits_its_time_between_two_edges() {
    let (result, report) = profile_graph(&format!(
        "/leaf {{ {BURN} }} def
         /x {{ leaf 1 pop }} def
         /y {{ leaf leaf 1 pop }} def
         x y"
    ));
    result.expect("runs");
    assert_eq!(edge(&report, Some("x"), "leaf").calls, 1);
    assert_eq!(edge(&report, Some("y"), "leaf").calls, 2);
    assert_edges_add_up(&report);
    let sum =
        edge(&report, Some("x"), "leaf").inclusive + edge(&report, Some("y"), "leaf").inclusive;
    assert_eq!(sum, row(&report, "leaf").inclusive);
}

#[test]
fn a_tail_call_has_its_own_edge() {
    let (result, report) = profile_graph(&format!(
        "/heavy {{ {BURN} }} def
         /caller {{ 1 pop heavy }} def
         caller"
    ));
    result.expect("runs");
    let e = edge(&report, Some("caller"), "heavy");
    assert_eq!(e.calls, 1);
    assert!(e.inclusive > std::time::Duration::ZERO);
    assert_edges_add_up(&report);
}

#[test]
fn recursion_counts_each_call_on_its_edge_but_the_time_once() {
    let (result, report) = profile_graph(
        "/fib { dup 2 lt { } { dup 1 sub fib exch 2 sub fib add } ifelse } def
         15 fib pop",
    );
    result.expect("runs");
    // fib(15) makes 1973 calls: one from the top, 1972 from itself.
    assert_eq!(edge(&report, None, "fib").calls, 1);
    let own = edge(&report, Some("fib"), "fib");
    assert_eq!(own.calls, 1972);
    assert_edges_add_up(&report);
    // Counting every nested frame would make the self edge ~14x the run.
    assert!(own.inclusive <= edge(&report, None, "fib").inclusive);
}

#[test]
fn tail_recursion_counts_calls_on_the_edges_without_inventing_time() {
    let src = "/down { dup 0 gt { 1 sub down } { pop } ifelse } def 60000 down
               /ping { dup 0 gt { 1 sub pong } { pop } ifelse } def
               /pong { dup 0 gt { 1 sub ping } { pop } ifelse } def
               5000 ping";
    let (result, report) = profile_graph(src);
    result.expect("runs");
    assert_eq!(edge(&report, None, "down").calls, 1);
    assert_eq!(edge(&report, Some("down"), "down").calls, 60000);
    assert_edges_add_up(&report);
    let own = edge(&report, Some("down"), "down");
    assert!(own.inclusive <= edge(&report, None, "down").inclusive);
}

#[test]
fn unwinding_closes_the_edges_it_passes() {
    let (result, report) = profile_graph(&format!(
        "/g {{ {BURN} stop }} def
         /f {{ 1 pop g }} def
         {{ f }} stopped pop
         /after {{ 1 }} def after pop"
    ));
    result.expect("runs");
    assert_edges_add_up(&report);
    assert!(edge(&report, Some("f"), "g").inclusive > std::time::Duration::ZERO);
    assert!(edge(&report, None, "f").inclusive >= edge(&report, Some("f"), "g").inclusive);
    // After the unwinding the stack of open calls is empty again.
    assert_eq!(edge(&report, None, "after").calls, 1);
}

#[test]
fn the_graph_is_off_unless_asked_for() {
    let (_, plain) = profile("/a { } def a");
    assert!(plain.edges.is_none());
    assert!(!plain.to_text(None).contains("call graph"));
    assert!(!plain.to_json(None).contains("edges"));
}

#[test]
fn the_graph_text_and_json_list_the_edges_and_honour_the_row_limit() {
    let (_, report) = profile_graph("/b { } def /a { b b } def a");
    let text = report.to_text(None);
    assert!(text.contains("call graph, 2 of 2 edges"), "{text}");
    assert!(text.contains("<top level> -> a"), "{text}");
    assert!(text.contains("a -> b"), "{text}");
    let limited = report.to_text(Some(1));
    assert!(limited.contains("call graph, 1 of 2 edges"), "{limited}");
    assert!(limited.contains("1 more"), "{limited}");

    let v: serde_json::Value = serde_json::from_str(&report.to_json(None)).expect("valid JSON");
    assert_eq!(v["total_edges"], 2);
    let edges = v["edges"].as_array().expect("edges");
    let ab = edges.iter().find(|e| e["callee"] == "b").expect("a -> b");
    assert_eq!(
        (ab["caller"].as_str(), ab["calls"].as_u64()),
        (Some("a"), Some(2))
    );
    let top = edges.iter().find(|e| e["callee"] == "a").expect("top -> a");
    assert!(top["caller"].is_null());
    assert!(top["inclusive_ms"].is_f64());
    let v: serde_json::Value = serde_json::from_str(&report.to_json(Some(1))).unwrap();
    assert_eq!(v["edges"].as_array().unwrap().len(), 1);
    assert_eq!(v["total_edges"], 2);
}

#[test]
fn profile_graph_on_the_command_line_implies_profile() {
    let (ok, stdout, stderr) = pscat(&["--profile-graph", "-e", "/b { } def /a { b } def a"], "");
    assert!(ok, "{stderr}");
    assert!(!stdout.contains("call graph"), "report belongs on stderr");
    assert!(stderr.contains("pscat: profile"), "{stderr}");
    assert!(stderr.contains("a -> b"), "{stderr}");
    let (_, _, json) = pscat(
        &["--profile-json", "--profile-graph", "-e", "/a { } def a"],
        "",
    );
    let v: serde_json::Value = serde_json::from_str(json.trim()).expect("one JSON line");
    assert_eq!(v["total_edges"], 1);
    // Without the flag the report is what it was.
    let (_, _, plain) = pscat(&["--profile", "-e", "/a { } def a"], "");
    assert!(!plain.contains("call graph"), "{plain}");
}

// ---------------------------------------------------------------------
// --profile-json PATH (issue #799)
// ---------------------------------------------------------------------

const TWO_CALLS: &str = "/a { } def a a";

#[test]
fn profile_json_path_writes_the_file_inside_the_write_root() {
    let dir = common::scratch_dir("profile-json");
    let root = dir.to_str().unwrap();
    let out = dir.join("p.json");
    let (ok, _, stderr) = pscat(
        &[
            "--allow-write",
            root,
            "--profile-json",
            out.to_str().unwrap(),
            "-e",
            TWO_CALLS,
        ],
        "",
    );
    assert!(ok, "{stderr}");
    assert_eq!(stderr, "", "nothing goes to stderr when the file takes it");
    let text = std::fs::read_to_string(&out).expect("file written");
    assert!(text.ends_with('\n') && !text[..text.len() - 1].contains('\n'));
    let v: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(v["procedures"][0]["name"], "a");
    assert_eq!(v["procedures"][0]["calls"], 2);

    // The graph and the row limit carry over, and a rerun replaces the file.
    let (ok, _, stderr) = pscat(
        &[
            "--allow-write",
            root,
            "--profile-graph",
            "--profile-top",
            "1",
            "--profile-json",
            out.to_str().unwrap(),
            "-e",
            "/b { } def /a { b } def a",
        ],
        "",
    );
    assert!(ok, "{stderr}");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).expect("valid JSON");
    assert_eq!(v["procedures"].as_array().unwrap().len(), 1);
    assert_eq!(v["total_edges"], 2);
}

#[test]
fn profile_json_path_needs_a_write_root_and_runs_nothing_without_one() {
    let dir = common::scratch_dir("profile-json-noroot");
    let out = dir.join("p.json");
    let (ok, stdout, stderr) = pscat(
        &[
            "--profile-json",
            out.to_str().unwrap(),
            "-e",
            "(ran) print /a { } def a",
        ],
        "",
    );
    assert!(!ok);
    assert!(stderr.contains("--allow-write"), "{stderr}");
    assert_eq!(stdout, "", "the program must not run");
    assert!(!out.exists());
}

#[test]
fn profile_json_path_outside_the_write_root_is_refused_before_the_run() {
    let root = common::scratch_dir("profile-json-root");
    let elsewhere = common::scratch_dir("profile-json-elsewhere");
    let out = elsewhere.join("p.json");
    let (ok, stdout, stderr) = pscat(
        &[
            "--allow-write",
            root.to_str().unwrap(),
            "--profile-json",
            out.to_str().unwrap(),
            "-e",
            "(ran) print /a { } def a",
        ],
        "",
    );
    assert!(!ok);
    assert!(stderr.contains("invalidfileaccess"), "{stderr}");
    assert_eq!(stdout, "", "the program must not run");
    assert!(!out.exists());
    // `..` does not get out of the root either.
    let sneaky = format!(
        "{}/../{}/p.json",
        root.display(),
        elsewhere.file_name().unwrap().to_str().unwrap()
    );
    let (ok, stdout, _) = pscat(
        &[
            "--allow-write",
            root.to_str().unwrap(),
            "--profile-json",
            &sneaky,
            "-e",
            "(ran) print",
        ],
        "",
    );
    assert!(!ok && stdout.is_empty());
    assert!(!out.exists());
}

#[test]
fn profile_json_path_that_cannot_be_written_fails_the_run_and_says_why() {
    let dir = common::scratch_dir("profile-json-dir");
    // A directory where the file should go: allowed by the roots, not writable.
    let out = dir.join("p.json");
    std::fs::create_dir(&out).unwrap();
    let (ok, _, stderr) = pscat(
        &[
            "--allow-write",
            dir.to_str().unwrap(),
            "--profile-json",
            out.to_str().unwrap(),
            "-e",
            TWO_CALLS,
        ],
        "",
    );
    assert!(!ok, "a profile that can't be saved is a failed run");
    assert!(stderr.contains("--profile-json"), "{stderr}");
}

#[test]
fn profile_json_without_a_json_path_still_takes_a_program_file() {
    let dir = common::scratch_dir("profile-json-prog");
    let prog = dir.join("prog.ps");
    std::fs::write(&prog, "/a { } def a a a").unwrap();
    let (ok, _, stderr) = pscat(
        &["--headless", "--profile-json", prog.to_str().unwrap()],
        "",
    );
    assert!(ok, "{stderr}");
    let v: serde_json::Value = serde_json::from_str(stderr.trim()).expect("JSON on stderr");
    assert_eq!(v["procedures"][0]["calls"], 3);
}
