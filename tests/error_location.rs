//! Issue #1043: an error inside a file loaded with `run` reports that
//! file and its own line, plus the chain of open `run` frames, instead of
//! the top-level program's `run` line. A single-file program's report is
//! unchanged (pinned across the suite by exact-string tests).

mod common;

use std::process::Command;

use common::CommandDeadline as _;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// Write `files` into a fresh directory and run `main` there headless;
/// returns stderr's first line (the error report).
fn report(tag: &str, files: &[(&str, &str)], main: &str) -> String {
    let dir = common::scratch_dir(tag);
    for (name, body) in files {
        std::fs::write(dir.join(name), body).expect("write scratch file");
    }
    let out = Command::new(BIN)
        .args(["--headless", main])
        .current_dir(&dir)
        .output_deadline()
        .expect("run pscat");
    assert!(!out.status.success(), "the program should fail");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    stderr.lines().next().unwrap_or_default().to_string()
}

#[test]
fn an_error_in_a_run_file_names_that_file_and_line() {
    // The issue's repro: line 5 of inner3.ps, run from line 2 of outer3.ps.
    let got = report(
        "run-inner",
        &[
            ("inner3.ps", "%!PS\n\n\n\n1 0 div\n"),
            ("outer3.ps", "%!PS\n(inner3.ps) run\n"),
        ],
        "outer3.ps",
    );
    assert_eq!(
        got,
        "%%[ Error: undefinedresult; OffendingCommand: div; Line: 5 (inner3.ps); \
         Run chain: inner3.ps:5 <- line 2 ]%%"
    );
}

#[test]
fn nested_runs_report_the_whole_chain() {
    let got = report(
        "run-nested",
        &[
            ("inner.ps", "\n\n1 0 div\n"),
            ("mid.ps", "%!PS\n\n\n(inner.ps) run\n"),
            ("top.ps", "\n(mid.ps) run\n"),
        ],
        "top.ps",
    );
    assert!(
        got.ends_with("Line: 3 (inner.ps); Run chain: inner.ps:3 <- mid.ps:4 <- line 2 ]%%"),
        "{got}"
    );
}

#[test]
fn a_syntax_error_in_a_run_file_names_it() {
    let got = report(
        "run-syntax",
        &[("bad.ps", "1 2\n(abc\n"), ("main.ps", "\n(bad.ps) run\n")],
        "main.ps",
    );
    assert!(got.contains("syntaxerror"), "{got}");
    assert!(
        got.contains("Line: 2 (bad.ps); Run chain: bad.ps:2 <- line 2 ]%%"),
        "{got}"
    );
}

#[test]
fn a_procedure_from_a_run_file_called_later_reports_the_call_line() {
    // No object carries a source position, so a procedure defined in a
    // run file and called from the main program reports the call site,
    // exactly as before #1043 (HANDOFF.md, "Deliberate deviations").
    let got = report(
        "run-proc",
        &[
            ("defs.ps", "/f { 1 0 div } def\n"),
            ("main.ps", "(defs.ps) run\n\nf\n"),
        ],
        "main.ps",
    );
    assert_eq!(
        got,
        "%%[ Error: undefinedresult; OffendingCommand: div; Line: 3 ]%%"
    );
}

#[test]
fn a_caught_error_in_a_run_file_leaves_no_stale_location() {
    // The run file's error is caught; the later uncaught error is the
    // main program's own, so its report has no file name or chain.
    let got = report(
        "run-caught",
        &[
            ("inner.ps", "{ 1 0 div } stopped pop\n"),
            ("main.ps", "(inner.ps) run\n\n2 0 div\n"),
        ],
        "main.ps",
    );
    assert_eq!(
        got,
        "%%[ Error: undefinedresult; OffendingCommand: div; Line: 3 ]%%"
    );
}

#[test]
fn the_run_chain_is_capped_for_deep_recursion() {
    // A file that runs itself until the counter hits 20, then fails.
    let dir_files = [
        (
            "self.ps",
            "/n n 1 add def\nn 20 lt { (self.ps) run } if\n1 0 div\n",
        ),
        ("main.ps", "/n 0 def\n(self.ps) run\n"),
    ];
    let got = report("run-deep", &dir_files, "main.ps");
    assert!(
        got.contains("Line: 3 (self.ps); Run chain: self.ps:3 <- self.ps:2"),
        "{got}"
    );
    assert!(got.contains(" <- ... <- line 2 ]%%"), "{got}");
    // Eight named frames, the elision, and the main program.
    let links = got.split(" <- ").count();
    assert_eq!(links, 10, "{got}");
}

#[test]
fn separators_in_a_file_name_are_escaped() {
    // `;` and `]` are escaped, so `pscat-mcp`'s `; ` / ` ]%%` split holds.
    let got = report(
        "run-escape",
        &[
            ("a; b ]c.ps", "1 0 div\n"),
            ("main.ps", "(a; b ]c.ps) run\n"),
        ],
        "main.ps",
    );
    assert!(got.contains("Line: 1 (a%3B b %5Dc.ps); "), "{got}");
}

/// `:` and a newline cannot be in a Windows file name.
#[cfg(unix)]
#[test]
fn a_file_name_cannot_forge_the_report_fields() {
    let got = report(
        "run-forge",
        &[
            ("a; Line: 9 ]x.ps", "1 0 div\n"),
            ("main.ps", "(a; Line: 9 ]x.ps) run\n"),
        ],
        "main.ps",
    );
    assert!(got.contains("Line: 1 (a%3B Line: 9 %5Dx.ps); "), "{got}");
    assert_eq!(got.matches("; Line: ").count(), 1, "{got}");
    // A newline in the name would split the report across stderr lines.
    let got = report(
        "run-newline",
        &[("a\nb.ps", "1 0 div\n"), ("main.ps", "(a\\nb.ps) run\n")],
        "main.ps",
    );
    assert!(got.contains("Line: 1 (a%0Ab.ps); "), "{got}");
    assert!(got.ends_with(" ]%%"), "{got}");
}
