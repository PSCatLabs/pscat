//! `currentfilename` (issue #1056), a pscat extension: the name of the file
//! being run, as the program or the command line gave it. Through the real
//! binary, because the main program's name comes from the command line.

mod common;
use common::CommandDeadline as _;

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// `pscat --headless ARGS` from `dir`, with nothing of the developer's own
/// `PSCAT_*` setup and no checkout `lib/` to find, so that a `lib/` name is
/// served from where the test says.
fn pscat(dir: &Path, args: &[&str]) -> Output {
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    Command::new(BIN)
        .current_dir(dir)
        .env_remove("PSCAT_HOME")
        .env_remove("PSCAT_ROOT")
        .env_remove("PSCAT_LIB")
        .env_remove("XDG_DATA_HOME")
        .env_remove("APPDATA")
        .env("HOME", &home)
        .env("PSCAT_NO_CHECKOUT", "1")
        .env("PSCAT_TRACE_RESOURCES", "1")
        .arg("--headless")
        .args(args)
        .output_deadline()
        .unwrap()
}

fn ok_stdout(out: &Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {err}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The two procedures a library needs to load a sibling: `dirname` (the
/// path up to and including its last `/` or `\`, empty if it has none) and
/// `concat`. Both separators, because on Windows a name is as the command
/// line gave it, backslashes included (#1346).
const HELPERS: &str = r"
/dirname { dup length { dup 0 eq { pop pop () exit } if
    1 sub 2 copy get dup 47 eq exch 92 eq or { 1 add 0 exch getinterval exit } if } loop } def
/concat { /b exch def /a exch def
    a length b length add string dup 0 a putinterval dup a length b putinterval } def
";

#[test]
fn the_main_file_is_named_as_the_command_line_gave_it() {
    let dir = common::scratch_dir("cfn-main");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/main.ps"), "currentfilename =\n").unwrap();
    assert_eq!(
        ok_stdout(&pscat(&dir, &["sub/main.ps"])).trim(),
        "sub/main.ps"
    );
    assert_eq!(
        ok_stdout(&pscat(&dir, &["./sub/../sub/main.ps"])).trim(),
        "./sub/../sub/main.ps"
    );
}

#[test]
fn nested_run_names_the_innermost_file_and_restores_it() {
    let dir = common::scratch_dir("cfn-nested");
    std::fs::write(
        dir.join("outer.ps"),
        "(outer1: ) print currentfilename =\n(mid.ps) run\n(outer2: ) print currentfilename =\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("mid.ps"),
        "(mid1: ) print currentfilename =\n(inner.ps) run\n(mid2: ) print currentfilename =\n\
         % an executable string inside a file is part of that file\n\
         (mid3: ) print { currentfilename = } cvx exec\n",
    )
    .unwrap();
    std::fs::write(dir.join("inner.ps"), "(inner: ) print currentfilename =\n").unwrap();
    let out = ok_stdout(&pscat(&dir, &["outer.ps"]));
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            "outer1: outer.ps",
            "mid1: mid.ps",
            "inner: inner.ps",
            "mid2: mid.ps",
            "mid3: mid.ps",
            "outer2: outer.ps"
        ]
    );
}

#[test]
fn a_run_name_is_kept_as_written_not_resolved() {
    let dir = common::scratch_dir("cfn-asis");
    std::fs::create_dir_all(dir.join("a/b")).unwrap();
    std::fs::write(dir.join("a/b/x.ps"), "currentfilename =\n").unwrap();
    let out = ok_stdout(&pscat(&dir, &["-e", "(a/../a/b/x.ps) run"]));
    assert_eq!(out.trim(), "a/../a/b/x.ps");
}

#[test]
fn the_result_is_a_fresh_string() {
    let dir = common::scratch_dir("cfn-fresh");
    std::fs::write(
        dir.join("f.ps"),
        "currentfilename dup 0 120 put =\ncurrentfilename =\n",
    )
    .unwrap();
    assert_eq!(ok_stdout(&pscat(&dir, &["f.ps"])), "x.ps\nf.ps\n");
}

#[test]
fn an_embedded_library_is_named_lib_slash_name() {
    // No lib/ on disk anywhere the search looks, so the compiled-in copy
    // serves; `def` is wrapped to record the file it ran in, because no
    // library of ours asks for its own name.
    let dir = common::scratch_dir("cfn-embedded");
    let prog = "userdict /def { currentfilename userdict /fname 3 -1 roll put \
                systemdict /def get exec } systemdict /def get exec \
                (lib/functional.ps) run fname =";
    let out = pscat(&dir, &["-e", prog]);
    let stdout = ok_stdout(&out);
    assert_eq!(stdout.trim(), "lib/functional.ps");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("pscat: resource lib/functional.ps <- embedded"),
        "{stderr}"
    );
}

#[test]
fn a_resolved_library_keeps_the_name_it_was_asked_for() {
    let dir = common::scratch_dir("cfn-resolved");
    std::fs::create_dir_all(dir.join("lib/pkg")).unwrap();
    std::fs::write(dir.join("lib/pkg/x.ps"), "currentfilename =\n").unwrap();
    let out = ok_stdout(&pscat(&dir, &["-e", "(lib/pkg/x.ps) run"]));
    assert_eq!(out.trim(), "lib/pkg/x.ps");
}

#[test]
fn outside_a_named_file_it_is_undefined() {
    let dir = common::scratch_dir("cfn-undefined");
    // `-e` has no file, even when a file argument comes with it.
    let out = pscat(&dir, &["-e", "currentfilename ="]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Error: undefined; OffendingCommand: currentfilename"),
        "{stderr}"
    );
    // So does the program on stdin, and an executable string.
    let mut child = Command::new(BIN)
        .current_dir(&dir)
        .arg("--headless")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    common::feed_stdin(&mut child, b"currentfilename =");
    let out = common::wait_with_deadline(child, common::default_deadline());
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Error: undefined"));
    // A library user's interpreter has no main file name either, and a
    // program can test for the operator.
    let out = ok_stdout(&pscat(
        &dir,
        &[
            "-e",
            "systemdict /currentfilename known { (known) } { (no) } ifelse =",
        ],
    ));
    assert_eq!(out.trim(), "known");
    // `stopped` catches it as any undefined.
    let out = ok_stdout(&pscat(&dir, &["-e", "{ currentfilename } stopped ="]));
    assert_eq!(out.trim(), "true");
}

#[test]
fn dirname_splits_on_either_separator() {
    // Needs no file: the helper is plain PostScript, so this runs on every
    // platform. A Windows path is as the command line gave it (#1346).
    let dir = common::scratch_dir("cfn-dirname");
    let prog = format!(
        "{HELPERS}\n(a\\\\b\\\\c.ps) dirname = (C:\\\\x/y\\\\z.ps) dirname = \
         (a/b/c.ps) dirname = (c.ps) dirname = () dirname = (dir\\\\) dirname ="
    );
    let out = ok_stdout(&pscat(&dir, &["-e", &prog]));
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        ["a\\b\\", "C:\\x/y\\", "a/b/", "", "", "dir\\"]
    );
}

#[cfg(windows)]
#[test]
fn a_library_named_with_backslashes_loads_its_siblings() {
    // The main program's name is the argument as written, so a Windows
    // user's `vendor\pkg\main.ps` carries backslashes (#1346).
    let dir = common::scratch_dir("cfn-backslash");
    std::fs::create_dir_all(dir.join("vendor/pkg")).unwrap();
    std::fs::write(
        dir.join("vendor/pkg/main.ps"),
        format!(
            "{HELPERS}\n/pkg-dir currentfilename dirname def\n\
             pkg-dir (sib.ps) concat run\n"
        ),
    )
    .unwrap();
    std::fs::write(dir.join("vendor/pkg/sib.ps"), "currentfilename =\n").unwrap();
    let out = ok_stdout(&pscat(&dir, &["vendor\\pkg\\main.ps"]));
    assert_eq!(out.trim(), "vendor\\pkg\\sib.ps");
}

#[test]
fn a_library_loads_its_siblings_from_its_own_directory() {
    let dir = common::scratch_dir("cfn-sibling");
    std::fs::create_dir_all(dir.join("vendor/pkg")).unwrap();
    // The directory is read while the library file is being loaded, at its
    // top level: a procedure called later is read in its caller's file.
    std::fs::write(
        dir.join("vendor/pkg/main.ps"),
        format!(
            "{HELPERS}\n/pkg-dir currentfilename dirname def\n\
             /pkg-load {{ pkg-dir exch concat run }} def\n(sib.ps) pkg-load\n"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("vendor/pkg/sib.ps"),
        "(sibling loaded from ) print currentfilename =\n",
    )
    .unwrap();
    // Run from another directory than the library's own, by a relative
    // path, and then through a path of `..`s.
    let out = ok_stdout(&pscat(
        &dir,
        &[
            "-e",
            "(vendor/pkg/main.ps) run (more.ps) pkg-dir exch concat =",
        ],
    ));
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            "sibling loaded from vendor/pkg/sib.ps",
            "vendor/pkg/more.ps"
        ]
    );
    let out = ok_stdout(&pscat(&dir, &["-e", "(vendor/../vendor/pkg/main.ps) run"]));
    assert_eq!(
        out.trim(),
        "sibling loaded from vendor/../vendor/pkg/sib.ps"
    );
}

#[test]
fn a_procedure_reports_the_file_that_is_running_when_it_is_called() {
    let dir = common::scratch_dir("cfn-late");
    std::fs::write(dir.join("lib.ps"), "/whoami { currentfilename } def\n").unwrap();
    std::fs::write(dir.join("main.ps"), "(lib.ps) run whoami =\n").unwrap();
    // Called from the main program: the main program's name, not lib.ps.
    assert_eq!(ok_stdout(&pscat(&dir, &["main.ps"])).trim(), "main.ps");
}
