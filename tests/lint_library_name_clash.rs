//! Issue #1456: `--lint` says when a program's own name shadows one a library
//! defines, after an error (notes/1456-lint-library-name-clash.md).
//!
//! #1225: a program that kept a colour in a local `/shade` while a library
//! procedure read `shade` by name failed with a typecheck that named
//! `setrgbcolor`, deep inside the library, and nothing said the program's own
//! `/shade` was the cause. The note is made only when a run ends in an error:
//! a program that works gets no new line.

mod common;

use std::process::Command;

use common::CommandDeadline as _;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// A library that reaches its helper by bare name, as a library written
/// before #1225 did: `use` calls `helper`, looked up when it runs.
const LIB: &str = "%!PS\n\
/helper { 2 mul } def\n\
/use { 3 helper 1 add pop } def\n";

/// `pscat --lint` on the program `main` in a fresh directory holding `files`:
/// stderr.
fn lint(tag: &str, files: &[(&str, &str)], main: &str) -> String {
    let dir = common::scratch_dir(tag);
    for (name, body) in files {
        std::fs::write(dir.join(name), body).expect("write scratch file");
    }
    std::fs::write(dir.join("main.ps"), main).expect("write the program");
    let out = Command::new(BIN)
        .args(["--headless", "--lint", "main.ps"])
        .current_dir(&dir)
        .output_deadline()
        .expect("run pscat");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

const CHECK: &str = "[shadowed-library-name]";

fn finding(stderr: &str) -> Option<&str> {
    stderr.lines().find(|l| l.contains(CHECK))
}

#[test]
fn a_local_name_the_library_reads_by_name_is_named_when_the_run_fails() {
    // The issue's shape: a local dictionary holds `helper` while a library
    // procedure that calls `helper` by name runs.
    let stderr = lint(
        "clash-local",
        &[("lib1.ps", LIB)],
        "%!PS\n(lib1.ps) run\n/f { 2 dict begin\n/helper [1 2] def\nuse end } def\nf\n",
    );
    assert!(
        stderr.contains("OffendingCommand: add"),
        "the library's own typecheck: {stderr}"
    );
    let line = finding(&stderr).unwrap_or_else(|| panic!("no finding: {stderr}"));
    assert!(line.contains("`helper`"), "{line}");
    assert!(line.contains("line 4 of your program"), "{line}");
    assert!(line.contains("lib1.ps"), "{line}");
    assert!(
        line.contains("rename") && line.contains("//helper exec"),
        "it says what to do: {line}"
    );
    // It follows the run-error line, as every error-explaining finding does.
    let at = |needle: &str| stderr.find(needle).expect(needle);
    assert!(at("[run-error]") < at(CHECK), "{stderr}");
}

#[test]
fn a_global_redefinition_of_a_library_name_is_named_too() {
    let stderr = lint(
        "clash-global",
        &[("lib1.ps", LIB)],
        "%!PS\n(lib1.ps) run\n/helper (str) def\nuse\n",
    );
    let line = finding(&stderr).unwrap_or_else(|| panic!("no finding: {stderr}"));
    assert!(
        line.contains("`helper`") && line.contains("line 3 of your program"),
        "{line}"
    );
}

#[test]
fn a_name_the_program_defined_before_loading_the_library_is_named_without_a_line() {
    let stderr = lint(
        "clash-before",
        &[("lib1.ps", LIB)],
        "%!PS\n/helper [1 2] def\n(lib1.ps) run\n/helper [1 2] def\nuse\n",
    );
    // The program's second definition is the one that reaches the library.
    let line = finding(&stderr).unwrap_or_else(|| panic!("no finding: {stderr}"));
    assert!(
        line.contains("`helper`") && line.contains("line 4 of your program"),
        "{line}"
    );

    let stderr = lint(
        "clash-before-only",
        &[("lib1.ps", LIB)],
        "%!PS\n/helper [1 2] def\n(lib1.ps) run\nuse\n",
    );
    // The library's definition replaced the program's, so `use` runs: no error,
    // and so no note.
    assert!(finding(&stderr).is_none(), "{stderr}");
}

#[test]
fn a_run_that_works_gets_no_note() {
    // The same shadowing, no error: nothing new on a correct program.
    let stderr = lint(
        "clash-clean",
        &[("lib1.ps", LIB)],
        "%!PS\n(lib1.ps) run\n/f { 2 dict begin /helper { 4 mul } def use end } def f\n",
    );
    assert!(finding(&stderr).is_none(), "{stderr}");
    assert!(!stderr.contains("[run-error]"), "the run works: {stderr}");
}

#[test]
fn an_error_that_does_not_involve_the_name_gets_no_note() {
    // The program defines `helper` locally, and a library procedure fails,
    // but the name is nowhere near the failure.
    let stderr = lint(
        "clash-unrelated",
        &[(
            "lib1.ps",
            "%!PS\n/helper { 2 mul } def\n/other { (a) 1 add } def\n",
        )],
        "%!PS\n(lib1.ps) run\n/f { 1 dict begin /helper 3 def other end } def f\n",
    );
    assert!(stderr.contains("OffendingCommand: add"), "{stderr}");
    assert!(finding(&stderr).is_none(), "{stderr}");
}

#[test]
fn a_library_redefining_its_own_state_is_not_blamed_on_the_program() {
    // `bump` is a library procedure that `def`s the library's own `count`
    // while the program is running; `fail` then reads it. The program never
    // defined `count`.
    let lib = "%!PS\n/count 0 def\n\
        /bump { count 1 add /count exch def } def\n\
        /fail { count (x) add } def\n";
    let stderr = lint(
        "clash-libstate",
        &[("lib2.ps", lib)],
        "%!PS\n(lib2.ps) run\nbump bump fail\n",
    );
    assert!(stderr.contains("OffendingCommand: add"), "{stderr}");
    assert!(finding(&stderr).is_none(), "{stderr}");
}

#[test]
fn a_library_state_name_the_program_also_sets_is_named() {
    // The same library, but now the program stores into `count` too.
    let lib = "%!PS\n/count 0 def\n/fail { count (x) add } def\n";
    let stderr = lint(
        "clash-libstate-program",
        &[("lib2.ps", lib)],
        "%!PS\n(lib2.ps) run\n/count 7 def\nfail\n",
    );
    let line = finding(&stderr).unwrap_or_else(|| panic!("no finding: {stderr}"));
    assert!(
        line.contains("`count`") && line.contains("line 3 of your program"),
        "{line}"
    );
}

#[test]
fn the_allow_comment_silences_it() {
    let stderr = lint(
        "clash-allow",
        &[("lib1.ps", LIB)],
        "%!PS\n%lint: allow shadowed-library-name\n(lib1.ps) run\n/helper (s) def\nuse\n",
    );
    assert!(finding(&stderr).is_none(), "{stderr}");
}

#[test]
fn a_library_with_thousands_of_names_is_followed_up_to_a_cap() {
    // 5000 definitions: the table holds a bounded number, so a name late in
    // the file is not followed. Neither run fails for it.
    let mut lib = String::from("%!PS\n");
    for i in 0..5000 {
        lib.push_str(&format!("/nm{i} {{ {i} }} def\n"));
    }
    lib.push_str("/use2 { nm0 nm4999 (a) add } def\n");
    let main = |name: &str| format!("%!PS\n(big.ps) run\n/{name} (s) def\nuse2\n");
    let early = lint("clash-cap-a", &[("big.ps", &lib)], &main("nm0"));
    assert!(
        finding(&early).is_some_and(|l| l.contains("`nm0`")),
        "{early}"
    );
    let late = lint("clash-cap-b", &[("big.ps", &lib)], &main("nm4999"));
    assert!(finding(&late).is_none(), "{late}");
}

#[test]
fn the_note_is_for_the_program_that_defined_the_name_not_the_next_one() {
    use pscat::Interp;
    let dir = common::scratch_dir("clash-reuse");
    let lib = dir.join("lib1.ps");
    std::fs::write(&lib, LIB).expect("write lib");
    let mut it = Interp::with_page(50, 50).expect("page");
    it.trace_operator_shadows();
    let load = format!("({}) run", lib.display());
    it.run_str(&format!("{load} /helper [1 2] def use"))
        .expect_err("typecheck");
    let first = it.lint(true);
    assert!(
        first.iter().any(|f| f.check == "shadowed-library-name"),
        "first program"
    );
    // A second program on the same interpreter fails in the library the same
    // way (the first program's `helper` is still in `userdict`), but this
    // program defined nothing: the first program's line is not its line.
    it.run_str("use").expect_err("typecheck");
    assert!(
        it.lint(true)
            .iter()
            .all(|f| f.check != "shadowed-library-name"),
        "the first program's definition was named for the second"
    );
}

#[test]
fn the_check_is_one_that_explains_an_error() {
    assert!(pscat::lint::explains_an_error("shadowed-library-name"));
    assert!(pscat::lint::explains_an_error("shadowed-operator"));
}

#[test]
fn without_lint_nothing_is_traced() {
    use pscat::Interp;
    let dir = common::scratch_dir("clash-untraced");
    let lib = dir.join("lib1.ps");
    std::fs::write(&lib, LIB).expect("write lib");
    let mut it = Interp::with_page(50, 50).expect("page");
    it.run_str(&format!("({}) run /helper [1 2] def use", lib.display()))
        .expect_err("typecheck");
    assert!(
        it.lint(true)
            .iter()
            .all(|f| f.check != "shadowed-library-name"),
        "no tracing was asked for"
    );
}
