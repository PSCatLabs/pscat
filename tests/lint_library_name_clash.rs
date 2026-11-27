//! Issue #1456: `--lint` says when a program's own name shadows one a library
//! defines, after an error (notes/1456-lint-library-name-clash.md).
//!
//! #1225: a program that kept a colour in a local `/shade` while a library
//! procedure read `shade` by name failed with a typecheck that named
//! `setrgbcolor`, deep inside the library, and nothing said the program's own
//! `/shade` was the cause. The note is made only when a run ends in an error,
//! and only when the clash can be shown to be in effect (Codex round 1 on
//! PR #1479): a library procedure executed the name before the error, the name
//! now resolves to something of a different kind than the library's own
//! definition, and nothing else explains it. A wrong explanation is worse than
//! none, so every case below that must stay silent also asserts how the run
//! ended, and has a positive control that is named.

mod common;

use std::process::Command;

use common::CommandDeadline as _;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// A library that reaches its helper by bare name, as a library written
/// before #1225 did: `use` calls `helper`, looked up when it runs.
const LIB: &str = "%!PS\n\
/helper { 2 mul } def\n\
/use { 3 helper 1 add pop } def\n";

const CHECK: &str = "[shadowed-library-name]";

/// How a `--lint` run ended.
struct Run {
    stderr: String,
    success: bool,
}

impl Run {
    /// The finding line, if any.
    fn finding(&self) -> Option<&str> {
        self.stderr.lines().find(|l| l.contains(CHECK))
    }
    /// The run failed with this offending command.
    fn failed_at(&self, op: &str) {
        assert!(!self.success, "the run should fail: {}", self.stderr);
        assert!(
            self.stderr.contains(&format!("OffendingCommand: {op};")),
            "expected OffendingCommand: {op}: {}",
            self.stderr
        );
    }
    /// The run is silent about the clash.
    fn silent(&self) {
        assert!(self.finding().is_none(), "{}", self.stderr);
    }
    /// The run names `name`, defined by `lib`; returns the line.
    fn names(&self, name: &str, lib: &str) -> &str {
        let line = self
            .finding()
            .unwrap_or_else(|| panic!("no finding: {}", self.stderr));
        assert!(line.contains(&format!("`{name}` is")), "{line}");
        assert!(line.contains(lib), "{line}");
        line
    }
}

/// `pscat --lint` on the program `main` in a fresh directory holding `files`.
fn lint(tag: &str, files: &[(&str, &str)], main: &str) -> Run {
    let dir = common::scratch_dir(tag);
    for (name, body) in files {
        std::fs::write(dir.join(name), body).expect("write scratch file");
    }
    std::fs::write(dir.join("main.ps"), main).expect("write the program");
    run_in(&dir, &dir.join("main.ps"))
}

/// The same, run from the repository root so `lib/` and `examples/` resolve.
fn lint_repo(tag: &str, main: &str) -> Run {
    let dir = common::scratch_dir(tag);
    let path = dir.join("main.ps");
    std::fs::write(&path, main).expect("write the program");
    run_in(std::path::Path::new(env!("CARGO_MANIFEST_DIR")), &path)
}

fn run_in(cwd: &std::path::Path, main: &std::path::Path) -> Run {
    let out = Command::new(BIN)
        .args(["--headless", "--lint"])
        .arg(main)
        .current_dir(cwd)
        .output_deadline()
        .expect("run pscat");
    Run {
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        success: out.status.success(),
    }
}

/// A path as the inside of a PostScript string: no backslashes (Windows).
fn ps_path(p: &std::path::Path) -> String {
    p.display().to_string().replace('\\', "/")
}

// ---- the cases that are named ------------------------------------------------

#[test]
fn a_local_name_the_library_reads_by_name_is_named_when_the_run_fails() {
    // The issue's shape: a local dictionary holds `helper` while a library
    // procedure that calls `helper` by name runs.
    let r = lint(
        "clash-local",
        &[("lib1.ps", LIB)],
        "%!PS\n(lib1.ps) run\n/f { 2 dict begin\n/helper [1 2] def\nuse end } def\nf\n",
    );
    r.failed_at("add");
    let line = r.names("helper", "lib1.ps");
    assert!(line.contains("line 4"), "{line}");
    assert!(
        line.contains("a procedure") && line.contains("an array"),
        "{line}"
    );
    // It follows the run-error line, as every error-explaining finding does.
    let at = |needle: &str| r.stderr.find(needle).expect(needle);
    assert!(at("[run-error]") < at(CHECK), "{}", r.stderr);
}

#[test]
fn a_global_redefinition_of_a_library_name_is_named_too() {
    let r = lint(
        "clash-global",
        &[("lib1.ps", LIB)],
        "%!PS\n(lib1.ps) run\n/helper [1 2] def\nuse\n",
    );
    r.failed_at("add");
    assert!(r.names("helper", "lib1.ps").contains("line 3"));
}

#[test]
fn a_local_definition_made_before_the_library_loads_is_named() {
    // F3. The library loads into `userdict`, the program's own `helper` sits in
    // a local dictionary that is still open when `use` runs.
    let r = lint(
        "clash-local-before",
        &[("lib1.ps", LIB)],
        "%!PS\n1 dict begin /helper [1 2] def userdict begin (lib1.ps) run end use\n",
    );
    r.failed_at("add");
    r.names("helper", "lib1.ps");
}

#[test]
fn a_definition_made_by_a_callback_is_named() {
    // F4. The `def` runs inside a library procedure (`exec`), so it is not
    // written as a `def` of the program's own text.
    let lib = format!("{LIB}/callback {{ exec }} def\n");
    let r = lint(
        "clash-callback",
        &[("lib3.ps", &lib)],
        "%!PS\n(lib3.ps) run\n{ /helper [1 2] def } callback\nuse\n",
    );
    r.failed_at("add");
    r.names("helper", "lib3.ps");
    // The control: the same callback defining something the library does not
    // read changes nothing.
    let r = lint(
        "clash-callback-control",
        &[("lib3.ps", &lib)],
        "%!PS\n(lib3.ps) run\n{ /other [1 2] def } callback\nuse\n",
    );
    assert!(r.success, "{}", r.stderr);
    r.silent();
}

#[test]
fn a_rebinding_of_the_callee_at_a_call_site_is_not_hidden_by_an_earlier_verdict() {
    // F7. The first iteration runs the library's own `libset` (a library
    // procedure defining the library's `helper`) through the name `a`; the
    // second runs the program's `pset` through the same call site.
    let lib = format!("{LIB}/libset {{ /helper {{ 2 mul }} def }} def\n");
    let r = lint(
        "clash-site",
        &[("lib4.ps", &lib)],
        "%!PS\n(lib4.ps) run\n/pset { /helper [1 2] def } def\n\
         /a /libset load def\n0 1 1 { pop a /a /pset load def } for\nuse\n",
    );
    r.failed_at("add");
    r.names("helper", "lib4.ps");
}

#[test]
fn a_program_definition_after_the_librarys_own_runs_is_still_named() {
    // `bump` keeps the library's `tally` from a loop first; the program then
    // sets it to something of another kind, from inside a procedure it calls
    // from a loop.
    let lib = "%!PS\n/tally 0 def\n\
        /bump { tally 1 add /tally exch def } def\n\
        /fail { tally (x) add } def\n";
    let r = lint(
        "clash-after-lib-runs",
        &[("lib2.ps", lib)],
        "%!PS\n(lib2.ps) run\n0 1 5 { pop bump } for\n\
         /setit { /tally [9] def } def\n0 1 2 { pop setit } for\nfail\n",
    );
    r.failed_at("add");
    // The `def` is the last element of `setit`, run through a name, so no line
    // is claimed for it (a wrong line is worse than none).
    let line = r.names("tally", "lib2.ps");
    assert!(!line.contains("set at line"), "{line}");
}

#[test]
fn the_line_is_the_one_of_the_binding_that_fails() {
    // F6: repeated definitions name the last one, which is what `use` found.
    let r = lint(
        "clash-last-line",
        &[("lib1.ps", LIB)],
        "%!PS\n(lib1.ps) run\n/helper 1 def\n/helper [1 2] def\nuse\n",
    );
    r.failed_at("add");
    assert!(r.names("helper", "lib1.ps").contains("line 4"));
}

#[test]
fn the_library_that_defines_the_name_now_is_named_after_a_restore() {
    // F6: `A.ps` is gone after the restore; `use` is `B.ps`'s.
    let a = "%!PS\n/helper { 1 } def\n";
    let r = lint(
        "clash-restore-lib",
        &[("A.ps", a), ("B.ps", LIB)],
        "%!PS\nsave (A.ps) run restore (B.ps) run /helper [1 2] def use\n",
    );
    r.failed_at("add");
    let line = r.names("helper", "B.ps");
    assert!(!line.contains("A.ps"), "{line}");
}

#[test]
fn a_second_program_that_defines_the_name_again_is_named_again() {
    // T1's positive control for an interpreter a host reuses: each program
    // loads the library and defines `helper` itself.
    use pscat::Interp;
    let dir = common::scratch_dir("clash-reuse");
    let lib = dir.join("lib1.ps");
    std::fs::write(&lib, LIB).expect("write lib");
    let load = format!("({}) run", ps_path(&lib));
    let mut it = Interp::with_page(50, 50).expect("page");
    it.trace_operator_shadows();
    it.run_str(&format!("{load} /helper [1 2] def use"))
        .expect_err("typecheck");
    let first = it.lint(true);
    assert!(
        first.iter().any(|f| f.check == "shadowed-library-name"),
        "first program"
    );
    // The same again: a fresh definition in a fresh program is a fresh finding.
    it.run_str(&format!("{load}\n/helper 7 def\n/helper [3] def\nuse"))
        .expect_err("typecheck");
    let second: Vec<_> = it
        .lint(true)
        .into_iter()
        .filter(|f| f.check == "shadowed-library-name")
        .collect();
    assert_eq!(
        second.len(),
        1,
        "{:?}",
        second.iter().map(|f| &f.message).collect::<Vec<_>>()
    );
    assert!(
        second[0].message.contains("line 3"),
        "{}",
        second[0].message
    );
    // A program that defines nothing is not given the last one's line.
    it.run_str("use").expect_err("typecheck");
    assert!(
        it.lint(true)
            .iter()
            .all(|f| f.check != "shadowed-library-name" || !f.message.contains("line 3")),
        "the first program's line was named for the next"
    );
}

#[test]
fn a_library_with_thousands_of_names_is_followed_up_to_a_cap() {
    // 5000 definitions: the table holds a bounded number, so a name late in
    // the file is not followed (the finding is silently not made). Both runs
    // fail.
    let mut lib = String::from("%!PS\n");
    for i in 0..5000 {
        lib.push_str(&format!("/nm{i} {{ {i} }} def\n"));
    }
    lib.push_str("/use2 { nm0 nm4999 (a) add } def\n");
    let main = |name: &str| format!("%!PS\n(big.ps) run\n/{name} (s) def\nuse2\n");
    let early = lint("clash-cap-a", &[("big.ps", &lib)], &main("nm0"));
    early.failed_at("add");
    assert!(early.names("nm0", "big.ps").contains("line 3"));
    let late = lint("clash-cap-b", &[("big.ps", &lib)], &main("nm4999"));
    late.failed_at("add");
    late.silent();
}

// ---- the cases that stay silent ----------------------------------------------

#[test]
fn a_run_that_works_gets_no_note() {
    // The same shadowing, no error: nothing new on a correct program.
    let r = lint(
        "clash-clean",
        &[("lib1.ps", LIB)],
        "%!PS\n(lib1.ps) run\n/f { 2 dict begin /helper { 4 mul } def use end } def f\n",
    );
    assert!(r.success, "{}", r.stderr);
    r.silent();
    assert!(
        !r.stderr.contains("[run-error]"),
        "the run works: {}",
        r.stderr
    );
}

#[test]
fn an_error_that_does_not_involve_the_name_gets_no_note() {
    // The program defines `helper` locally, and a library procedure fails,
    // but the name is nowhere near the failure.
    let r = lint(
        "clash-unrelated",
        &[(
            "lib1.ps",
            "%!PS\n/helper { 2 mul } def\n/other { (a) 1 add } def\n",
        )],
        "%!PS\n(lib1.ps) run\n/f { 1 dict begin /helper [3] def other end } def f\n",
    );
    r.failed_at("add");
    r.silent();
}

#[test]
fn a_name_after_the_failing_element_is_not_a_name_the_library_ran() {
    // F1. `helper` stands after the failing `add`, in the excerpt the error
    // report shows, but it never ran.
    let lib = "%!PS\n/helper { 2 mul } def\n/use { (x) 1 add helper } def\n\
               /use3 { 3 helper 1 add pop } def\n";
    let r = lint(
        "clash-after",
        &[("lib5.ps", lib)],
        "%!PS\n(lib5.ps) run /helper [1 2] def use\n",
    );
    r.failed_at("add");
    r.silent();
    // The control: the library procedure that does run `helper` before it
    // fails.
    let r = lint(
        "clash-after-control",
        &[("lib5.ps", lib)],
        "%!PS\n(lib5.ps) run /helper [1 2] def use3\n",
    );
    r.failed_at("add");
    r.names("helper", "lib5.ps");
}

#[test]
fn a_hook_overridden_by_the_same_kind_is_deliberate_and_silent() {
    // F2, with synthetic hooks: a procedure by a procedure, a number by a
    // number, a name by a name. The failure is the library's own `add`.
    let lib = "%!PS\n/hook { 5 } def\n/num 5 def\n/nam /brass def\n\
               /go { hook pop num nam (x) add } def\n";
    for (tag, over) in [
        ("proc", "/hook { 6 } def"),
        ("num", "/num 7 def"),
        ("name", "/nam /verdigris def"),
    ] {
        let r = lint(
            &format!("clash-hook-{tag}"),
            &[("lib6.ps", lib)],
            &format!("%!PS\n(lib6.ps) run {over} go\n"),
        );
        r.failed_at("add");
        r.silent();
    }
    // The control: a hook replaced by something of another kind is named.
    let r = lint(
        "clash-hook-control",
        &[("lib6.ps", lib)],
        "%!PS\n(lib6.ps) run /num [7] def go\n",
    );
    r.failed_at("add");
    r.names("num", "lib6.ps");
}

#[test]
fn the_shipped_hooks_are_not_blamed_for_unrelated_failures() {
    // F2, the programs of the review. `ty.max-height` is data a program sets;
    // the failure is a zero height for whitespace-only text.
    let r = lint_repo(
        "clash-ty",
        "%!PS\n(examples/functional/type-common.ps) run /ty.max-height 180 def ( ) 0 0 100 ty-layout\n",
    );
    r.failed_at("div");
    r.silent();
    // `sfworld` is a hook; `pal` is replaced by the program's own procedure,
    // which is where the failure is.
    let r = lint_repo(
        "clash-scifi",
        "%!PS\n(lib/artkit.ps) run (lib/styles/scifi.ps) run /sfworld /hologram def \
         /pal { pop (x) 1 add } def 0 0 10 planet\n",
    );
    r.failed_at("add");
    r.silent();
    // `spmetal` is a hook, and it stands after the failing `arc`.
    let r = lint_repo(
        "clash-steampunk",
        "%!PS\n(lib/artkit.ps) run (lib/styles/steampunk.ps) run /spmetal /verdigris def (x) 10 5 rivet\n",
    );
    r.failed_at("arc");
    r.silent();
}

#[test]
fn an_ended_definition_is_not_blamed_on_a_later_failure() {
    // F5. The program's `helper` is gone when `use` runs; the error is `use`'s
    // own `(x) 1 add`.
    let lib = "%!PS\n/helper { 2 mul } def\n/use { 3 helper pop (x) 1 add } def\n";
    let r = lint(
        "clash-ended",
        &[("lib7.ps", lib)],
        "%!PS\n(lib7.ps) run 1 dict begin /helper 99 def end use\n",
    );
    r.failed_at("add");
    r.silent();
    // A restored one.
    let r = lint(
        "clash-restored",
        &[("lib7.ps", lib)],
        "%!PS\n(lib7.ps) run save /helper [99] def restore use\n",
    );
    r.failed_at("add");
    r.silent();
    // A definition the library then replaced.
    let r = lint(
        "clash-replaced",
        &[("lib7.ps", lib)],
        "%!PS\n/helper [99] def (lib7.ps) run use\n",
    );
    r.failed_at("add");
    r.silent();
    // The control: the same library, the definition still in force.
    let r = lint(
        "clash-ended-control",
        &[("lib7.ps", lib)],
        "%!PS\n(lib7.ps) run /helper [99] def use\n",
    );
    r.failed_at("add");
    r.names("helper", "lib7.ps");
}

#[test]
fn the_programs_own_procedures_are_not_library_procedures() {
    // The program's own `g` calls the program's own `helper` by name and fails
    // next to it. Under the CLI the main program has a file name too, so a
    // frame with a file is not the same as a library's frame.
    let r = lint(
        "clash-own-proc",
        &[("lib1.ps", LIB)],
        "%!PS\n(lib1.ps) run\n/helper { 2 mul } def\n/g { 1 helper (x) add } def\ng\n",
    );
    r.failed_at("add");
    r.silent();
    // The control: the program's `helper` is of another kind and a library
    // procedure runs it.
    let r = lint(
        "clash-own-proc-control",
        &[("lib1.ps", LIB)],
        "%!PS\n(lib1.ps) run\n/helper [1 2] def\nuse\n",
    );
    r.failed_at("add");
    r.names("helper", "lib1.ps");
}

#[test]
fn a_library_redefining_its_own_state_is_not_blamed_on_the_program() {
    // `bump` is a library procedure that `def`s the library's own `tally`
    // while the program is running; `fail` then reads it. The program never
    // defined `tally`.
    let lib = "%!PS\n/tally 0 def\n\
        /bump { tally 1 add /tally exch def } def\n\
        /fail { tally (x) add } def\n";
    let r = lint(
        "clash-libstate",
        &[("lib2.ps", lib)],
        "%!PS\n(lib2.ps) run\nbump bump fail\n",
    );
    r.failed_at("add");
    r.silent();
}

#[test]
fn a_library_state_name_the_program_sets_to_the_same_kind_is_silent() {
    // The program stores a number into the library's number: deliberate.
    let lib = "%!PS\n/tally 0 def\n/fail { tally (x) add } def\n";
    let r = lint(
        "clash-libstate-program",
        &[("lib2.ps", lib)],
        "%!PS\n(lib2.ps) run\n/tally 7 def\nfail\n",
    );
    r.failed_at("add");
    r.silent();
}

// ---- the text, the allow comment, the plumbing -------------------------------

#[test]
fn the_advice_is_true_and_hedged() {
    // F8: `//name` captures the binding visible when the program is scanned.
    let r = lint(
        "clash-advice",
        &[("lib1.ps", LIB)],
        "%!PS\n(lib1.ps) run\n/helper [1 2] def\nuse\n",
    );
    let line = r.names("helper", "lib1.ps");
    assert!(line.contains("looks unrelated to your code"), "{line}");
    assert!(line.contains("rename yours"), "{line}");
    assert!(line.contains("`//helper` captures"), "{line}");
    assert!(line.contains("cannot recover"), "{line}");
    assert!(!line.contains("write `//helper exec`"), "{line}");
    // The advice is as it says: `//helper exec` scanned after the program's
    // own definition gets the program's array, and fails.
    let r = lint(
        "clash-advice-run",
        &[("lib1.ps", LIB)],
        "%!PS\n(lib1.ps) run /helper [1 2] def /f { 3 //helper exec 1 add pop } def f\n",
    );
    assert!(!r.success, "{}", r.stderr);
}

#[test]
fn the_allow_comment_silences_it() {
    let r = lint(
        "clash-allow",
        &[("lib1.ps", LIB)],
        "%!PS\n%lint: allow shadowed-library-name\n(lib1.ps) run\n/helper [1 2] def\nuse\n",
    );
    r.failed_at("add");
    r.silent();
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
    it.run_str(&format!("({}) run /helper [1 2] def use", ps_path(&lib)))
        .expect_err("typecheck");
    assert!(
        it.lint(true)
            .iter()
            .all(|f| f.check != "shadowed-library-name"),
        "no tracing was asked for"
    );
}
