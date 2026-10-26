//! `--max-steps`, `--max-time` and `--max-memory` on the tool subcommands
//! (issue #701): `pscat handwrite`, `typewrite`, `photo-etch`, `chalk` and
//! `sheet` bound a runaway program exactly as a plain `pscat` render does,
//! with the same `limitexceeded` report and a non-zero exit.
//!
//! The first four build their programs from a PostScript library and cannot
//! be handed a loop through their arguments (every value is validated or
//! escaped), so those tests stand a library in for `lib/` through
//! `$PSCAT_LIB` whose files are `{ } loop`: a real runaway, which nothing but
//! a limit (or the watchdog below, which fails the test instead of letting
//! it hang) can end.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

mod common;

const PSCAT: &str = env!("CARGO_BIN_EXE_pscat");

/// The library files the subcommands load, by `lib/` name.
const LIBS: &[&str] = &[
    "handscript.ps",
    "typewriter_outlines.ps",
    "typewriter.ps",
    "etching.ps",
    "strokeglyphs.ps",
    "mathkit.ps",
];

fn scratch(tag: &str) -> PathBuf {
    common::scratch_dir(&format!("pscat-sublimits-{tag}"))
}

/// A directory of library files that never finish.
fn runaway_lib(dir: &Path) -> PathBuf {
    let lib = dir.join("runaway-lib");
    std::fs::create_dir_all(&lib).unwrap();
    for name in LIBS {
        std::fs::write(lib.join(name), "{ } loop\n").unwrap();
    }
    lib
}

/// Run `pscat <args>` in `dir`, killed (and the test failed) if it has not
/// ended in 60 s: a limit that stops working must fail, not hang CI.
fn pscat_with(dir: &Path, lib: Option<&Path>, args: &[&str]) -> (Output, Duration) {
    let mut cmd = Command::new(PSCAT);
    cmd.args(args)
        .current_dir(dir)
        .env_remove("PSCAT_HOME")
        .env_remove("PSCAT_ROOT")
        .env_remove("PSCAT_LIB")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(lib) = lib {
        cmd.env("PSCAT_LIB", lib);
    }
    let started = Instant::now();
    let mut child = cmd.spawn().expect("run pscat");
    loop {
        if child.try_wait().expect("wait").is_some() {
            break;
        }
        if started.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("pscat {args:?} was not stopped by its limit within 60 s");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let took = started.elapsed();
    (child.wait_with_output().expect("output"), took)
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn assert_limited(out: &Output, limit: &str, what: &str) {
    let err = stderr(out);
    assert!(!out.status.success(), "{what}: should fail: {err}");
    assert!(
        err.contains(&format!("limitexceeded ({limit})")),
        "{what}: wanted limitexceeded ({limit}): {err}"
    );
}

fn photo() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/sphere.jpg")
        .to_str()
        .unwrap()
        .to_string()
}

#[test]
fn a_runaway_library_is_stopped_by_max_steps_in_every_subcommand() {
    let dir = scratch("steps");
    let lib = runaway_lib(&dir);
    let photo = photo();
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("handwrite", vec!["handwrite", "-o", "h.png", "hello"]),
        (
            "typewrite",
            vec!["typewrite", "--text", "hi", "-o", "t.png"],
        ),
        (
            "typewrite --height fit",
            vec![
                "typewrite",
                "--height",
                "fit",
                "--text",
                "hi",
                "-o",
                "t.png",
            ],
        ),
        ("photo-etch", vec!["photo-etch", "-o", "e.png", &photo]),
        ("chalk", vec!["chalk", "-o", "c.png", "x^2"]),
    ];
    for (what, mut args) in cases {
        args.extend(["--max-steps", "100000"]);
        let (out, took) = pscat_with(&dir, Some(&lib), &args);
        assert_limited(&out, "steps", what);
        assert!(took < Duration::from_secs(30), "{what} took {took:?}");
    }
}

#[test]
fn a_runaway_library_is_stopped_by_max_time() {
    let dir = scratch("time");
    let lib = runaway_lib(&dir);
    for args in [
        vec!["handwrite", "-o", "h.png", "hello"],
        vec!["chalk", "-o", "c.png", "x^2"],
    ] {
        let mut args = args;
        args.extend(["--max-time", "0.3"]);
        let (out, took) = pscat_with(&dir, Some(&lib), &args);
        assert_limited(&out, "time", &format!("{args:?}"));
        assert!(took < Duration::from_secs(30), "{args:?} took {took:?}");
    }
}

#[test]
fn a_runaway_library_is_stopped_by_max_memory() {
    // Memory growth, not steps: the library fills an array without end.
    let dir = scratch("memory");
    let lib = dir.join("hog-lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("handscript.ps"),
        "/a 100000 array def { /a 1000000 array def a 0 a put } loop\n",
    )
    .unwrap();
    let (out, took) = pscat_with(
        &dir,
        Some(&lib),
        &["handwrite", "-o", "h.png", "hello", "--max-memory", "50"],
    );
    let err = stderr(&out);
    assert!(!out.status.success(), "{err}");
    assert!(err.contains("limitexceeded (memory)"), "{err}");
    assert!(took < Duration::from_secs(30), "took {took:?}");
}

#[test]
fn without_a_limit_the_subcommands_render_as_before() {
    let dir = scratch("off");
    let photo = photo();
    for (args, out) in [
        (
            vec!["handwrite", "hello", "--max-steps", "0", "--max-time", "0"],
            "h.png",
        ),
        (
            vec!["typewrite", "--text", "hi", "--max-memory", "0"],
            "typed.png",
        ),
        (
            vec!["photo-etch", &photo, "--max-dim", "60", "--max-steps", "0"],
            "etching.png",
        ),
        (
            vec![
                "chalk",
                "x^2",
                "--width",
                "200",
                "--height",
                "100",
                "--dpi",
                "72",
                "--max-steps",
                "0",
            ],
            "chalkboard.png",
        ),
    ] {
        let mut args = args;
        if args[0] == "handwrite" {
            args.extend(["-o", out]);
        }
        let (res, _) = pscat_with(&dir, None, &args);
        assert!(res.status.success(), "{args:?}: {}", stderr(&res));
        assert!(dir.join(out).is_file(), "{args:?} wrote no {out}");
    }
}

#[test]
fn a_limit_that_fits_does_not_change_the_render() {
    let dir = scratch("same");
    let (a, _) = pscat_with(&dir, None, &["handwrite", "-o", "a.png", "same page"]);
    let (b, _) = pscat_with(
        &dir,
        None,
        &[
            "handwrite",
            "-o",
            "b.png",
            "--max-steps",
            "50000000",
            "--max-time",
            "120",
            "--max-memory",
            "2000",
            "same page",
        ],
    );
    assert!(a.status.success() && b.status.success(), "{}", stderr(&b));
    assert_eq!(
        std::fs::read(dir.join("a.png")).unwrap(),
        std::fs::read(dir.join("b.png")).unwrap()
    );
}

#[test]
fn bad_limit_values_are_refused_with_the_flag_clis_messages() {
    let dir = scratch("bad");
    for (args, want) in [
        (
            vec!["handwrite", "--max-steps", "many", "x"],
            "invalid --max-steps value: many",
        ),
        (
            vec!["typewrite", "--max-time", "-3", "--text", "x"],
            "invalid --max-time value: -3",
        ),
        (
            vec!["chalk", "--max-memory", "2000000000", "x"],
            "invalid --max-memory value",
        ),
        (
            vec!["sheet", "--max-steps"],
            "missing value after --max-steps",
        ),
    ] {
        let (out, _) = pscat_with(&dir, None, &args);
        assert!(!out.status.success(), "{args:?}");
        assert!(stderr(&out).contains(want), "{args:?}: {}", stderr(&out));
    }
}

#[test]
fn sheet_stops_a_runaway_cell_and_the_run_fails() {
    let dir = scratch("sheet");
    std::fs::write(dir.join("loop.ps"), "{ } loop\n").unwrap();
    let (out, took) = pscat_with(
        &dir,
        None,
        &["sheet", "-o", "s.png", "--max-steps", "100000", "loop.ps"],
    );
    assert_limited(&out, "steps", "sheet");
    assert!(took < Duration::from_secs(30), "took {took:?}");
    // The sheet is still written, as a failing cell always was.
    assert!(dir.join("s.png").is_file());
    let (out, _) = pscat_with(
        &dir,
        None,
        &["sheet", "-o", "t.png", "--max-time", "0.3", "loop.ps"],
    );
    assert_limited(&out, "time", "sheet");
}

#[test]
fn sheet_limits_are_per_cell_not_per_sheet() {
    // One cell of this file costs a few thousand steps. Six cells together
    // cost far more than the limit, yet each cell gets the whole of it.
    let dir = scratch("sheet-cells");
    std::fs::write(dir.join("work.ps"), "0 1 3000 { pop } for\n").unwrap();
    std::fs::write(dir.join("more.ps"), "0 1 30000 { pop } for\n").unwrap();
    let (out, _) = pscat_with(
        &dir,
        None,
        &[
            "sheet",
            "-o",
            "s.png",
            "--max-steps",
            "8000",
            "--dpi",
            "72,72,72,72,72,72",
            "work.ps",
        ],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    // A cell that alone costs more than the limit is stopped.
    let (out, _) = pscat_with(
        &dir,
        None,
        &["sheet", "-o", "s.png", "--max-steps", "8000", "more.ps"],
    );
    assert_limited(&out, "steps", "sheet");
}
