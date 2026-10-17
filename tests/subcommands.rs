//! `pscat handwrite`, `pscat typewrite` and `pscat photo-etch` (issue #600):
//! the three tools that used to be bash scripts. They must work with only the
//! binary (no checkout, no shell, so on Windows too), render exactly what
//! the PostScript library's driver renders through the plain CLI, and refuse
//! anything that could carry PostScript into the driver.
//!
//! The byte-for-byte comparison with the old scripts was done once, by hand,
//! when they were ported (notes/600-*.md); what stays here is the same
//! equivalence against a driver this file assembles itself.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod common;

const PSCAT: &str = env!("CARGO_BIN_EXE_pscat");

fn scratch(tag: &str) -> PathBuf {
    common::scratch_dir(&format!("pscat-subcmd-{tag}"))
}

/// Run `pscat <args>` in `dir`, an empty directory outside the checkout,
/// with the environment that could point it at one removed.
fn pscat(dir: &Path, args: &[&str]) -> Output {
    Command::new(PSCAT)
        .args(args)
        .current_dir(dir)
        .env_remove("PSCAT_HOME")
        .env_remove("PSCAT_ROOT")
        .env_remove("PSCAT_LIB")
        .stdin(Stdio::null())
        .output()
        .expect("run pscat")
}

fn ok(out: &Output) {
    assert!(
        out.status.success(),
        "pscat failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn lib(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(name)).expect("library")
}

fn path(p: &Path) -> &str {
    p.to_str().expect("utf-8 path")
}

/// What `scripts/handwrite.sh` ran: the library, an options dict, and a
/// plain `pscat --page WxH --png` on the result.
fn plain_cli(dir: &Path, driver: &[u8], page: &str, out: &Path) {
    let ps = dir.join("driver.ps");
    std::fs::write(&ps, driver).unwrap();
    let res = pscat(dir, &["--page", page, "--png", path(out), path(&ps)]);
    ok(&res);
}

#[test]
fn the_subcommands_work_from_an_empty_directory() {
    let dir = scratch("bare");
    let hand = dir.join("hand.png");
    ok(&pscat(
        &dir,
        &["handwrite", "-o", path(&hand), "meet me at nine"],
    ));
    let note = dir.join("note.txt");
    std::fs::write(&note, "Dear Sir,\n\nTyped from nowhere.\n").unwrap();
    let typed = dir.join("typed.png");
    ok(&pscat(
        &dir,
        &["typewrite", "-o", path(&typed), path(&note)],
    ));
    let photo = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/sphere.jpg");
    for ext in ["png", "svg", "pdf"] {
        let etched = dir.join(format!("etched.{ext}"));
        ok(&pscat(
            &dir,
            &["photo-etch", "-o", path(&etched), path(&photo)],
        ));
        assert!(std::fs::metadata(&etched).unwrap().len() > 100, "{ext}");
    }
    for f in [&hand, &typed] {
        let img = tiny_skia::Pixmap::decode_png(&std::fs::read(f).unwrap()).unwrap();
        assert!(
            img.pixels().iter().any(|p| p.red() < 128),
            "{f:?} has no ink"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn handwrite_renders_what_the_library_driver_renders() {
    let dir = scratch("hw");
    let ours = dir.join("ours.png");
    let res = pscat(
        &dir,
        &[
            "handwrite",
            "--height",
            "300",
            "--seed",
            "7",
            "--ink",
            "0.6,0.1,0.1",
            "--paper",
            "ruled",
            "-o",
            path(&ours),
            "Hello (World) \\ again",
        ],
    );
    ok(&res);
    let mut driver = lib("lib/handscript.ps");
    driver.extend(
        b"/HandOpts <<\n  /Text (hello \\(world\\) \\\\ again)\n  /Size 36\n  /Width 612\n  \
          /Height 300\n  /Margin 54\n  /Leading 1.5\n  /Jitter 16\n  /Pen 22\n  \
          /Ink [ 0.6 0.1 0.1 ]\n  /Paper /ruled\n  /Seed 7\n>> def\n\
          HandOpts hs-write showpage\n",
    );
    let theirs = dir.join("theirs.png");
    plain_cli(&dir, &driver, "612x300", &theirs);
    assert_eq!(
        std::fs::read(&ours).unwrap(),
        std::fs::read(&theirs).unwrap(),
        "the subcommand and the plain CLI disagree"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn handwrite_sizes_the_page_to_its_text() {
    let dir = scratch("hw-fit");
    let one = dir.join("one.png");
    let many = dir.join("many.png");
    ok(&pscat(&dir, &["handwrite", "-o", path(&one), "hi"]));
    ok(&pscat(
        &dir,
        &["handwrite", "-o", path(&many), "a\nb\nc\nd"],
    ));
    let h = |p: &Path| {
        tiny_skia::Pixmap::decode_png(&std::fs::read(p).unwrap())
            .unwrap()
            .height()
    };
    // two 54-point margins and a 1.05 em first line, then 1.5 em per line
    assert_eq!(h(&one), 146);
    assert_eq!(h(&many), 146 + 3 * 54);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn typewrite_renders_the_driver_it_writes() {
    let dir = scratch("tw");
    let txt = dir.join("memo.txt");
    std::fs::write(&txt, "One <u>two</u> (three) \\ four.\n\nFive.\n").unwrap();
    let (ps, ours) = (dir.join("drv.ps"), dir.join("ours.png"));
    let args = ["--seed", "3", "--memo-to", "Staff", "--height", "fit"];
    let mut a = vec!["typewrite"];
    a.extend(args);
    a.extend(["--ps", path(&ps), path(&txt)]);
    ok(&pscat(&dir, &a));
    let mut a = vec!["typewrite"];
    a.extend(args);
    a.extend(["-o", path(&ours), path(&txt)]);
    ok(&pscat(&dir, &a));
    let driver = std::fs::read(&ps).unwrap();
    assert!(driver.starts_with(&lib("lib/typewriter_outlines.ps")));
    let (w, h) = {
        let img = tiny_skia::Pixmap::decode_png(&std::fs::read(&ours).unwrap()).unwrap();
        (img.width(), img.height())
    };
    let theirs = dir.join("theirs.png");
    plain_cli(&dir, &driver, &format!("{w}x{h}"), &theirs);
    assert_eq!(
        std::fs::read(&ours).unwrap(),
        std::fs::read(&theirs).unwrap()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn typewrite_reads_stdin() {
    let dir = scratch("tw-stdin");
    let (a, b) = (dir.join("a.png"), dir.join("b.png"));
    let mut child = Command::new(PSCAT)
        .args(["typewrite", "-o", path(&a), "-"])
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    common::feed_stdin(&mut child, b"from a pipe\n");
    assert!(child.wait().unwrap().success());
    ok(&pscat(
        &dir,
        &["typewrite", "-o", path(&b), "--text", "from a pipe\n"],
    ));
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn photo_etch_renders_what_the_library_driver_renders() {
    let dir = scratch("etch");
    let photo = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/sphere.jpg");
    let ours = dir.join("ours.png");
    ok(&pscat(
        &dir,
        &[
            "photo-etch",
            "--max-dim",
            "100",
            "--angle",
            "20",
            "-o",
            path(&ours),
            path(&photo),
        ],
    ));
    let img = tiny_skia::Pixmap::decode_png(&std::fs::read(&ours).unwrap()).unwrap();
    let (pw, ph) = (img.width(), img.height());
    assert_eq!(pw, 100, "the longer side is --max-dim");
    assert!(ph < 100, "the page follows the photo's aspect ratio, {ph}");
    let mut driver = lib("lib/etching.ps");
    let escaped = path(&photo).replace('\\', "\\\\");
    driver.extend(
        format!(
            "<< /Photo ({escaped})\n   /PageWidth {pw}\n   /PageHeight {ph}\n   /Spacing 3\n   \
             /Angle 20\n   /Threshold2 0.55\n   /Ink [ 0.08 0.07 0.05 ]\n   \
             /Paper [ 0.96 0.94 0.88 ]\n>> et-draw showpage\n"
        )
        .as_bytes(),
    );
    let theirs = dir.join("theirs.png");
    plain_cli(&dir, &driver, &format!("{pw}x{ph}"), &theirs);
    assert_eq!(
        std::fs::read(&ours).unwrap(),
        std::fs::read(&theirs).unwrap()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// SECURITY.md: `handwrite`'s `ink` (and every other value) used to be
/// spliced into PostScript unchecked.
#[test]
fn options_cannot_carry_postscript_into_the_driver() {
    let dir = scratch("inject");
    let out = dir.join("x.png");
    let photo = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/sphere.jpg");
    let evil = "1 ] def (x) (w) file";
    for args in [
        vec!["handwrite", "--ink", evil, "hi"],
        vec!["handwrite", "--size", "36 def", "hi"],
        vec!["handwrite", "--seed", "1)", "hi"],
        vec!["handwrite", "--paper", "plain /Text (x)", "hi"],
        vec!["handwrite", "--width", "612 def", "hi"],
        vec!["typewrite", "--ink", evil, "--text", "hi"],
        vec!["typewrite", "--seed", "1 def", "--text", "hi"],
        vec!["typewrite", "--tabs", "5 def", "--text", "hi"],
        vec!["photo-etch", "--paper", evil, path(&photo)],
        vec!["photo-etch", "--angle", "1 def", path(&photo)],
    ] {
        let mut a = args.clone();
        a.extend(["-o", path(&out)]);
        let res = pscat(&dir, &a);
        assert!(!res.status.success(), "{args:?} should be refused");
        assert!(!out.exists(), "{args:?} wrote something");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn text_that_looks_like_an_option_needs_the_double_dash() {
    let dir = scratch("dash");
    let out = dir.join("dash.png");
    assert!(
        !pscat(&dir, &["handwrite", "-o", path(&out), "-dash"])
            .status
            .success()
    );
    ok(&pscat(
        &dir,
        &["handwrite", "-o", path(&out), "--", "-dash"],
    ));
    assert!(out.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_arguments_fail_with_usage() {
    let dir = scratch("usage");
    for args in [
        &["handwrite"][..],
        &["handwrite", "--bogus", "x"][..],
        &["handwrite", "a", "b"][..],
        &["handwrite", "--paper", "lined", "x"][..],
        &["handwrite", "--width", "9999", "x"][..],
        &["photo-etch"][..],
        &["photo-etch", "nonexistent.jpg"][..],
        &["photo-etch", "-o", "x.gif", "nonexistent.jpg"][..],
        &["typewrite", "--bogus"][..],
    ] {
        let res = pscat(&dir, args);
        assert!(!res.status.success(), "{args:?} should fail");
    }
    let help = pscat(&dir, &["handwrite", "--help"]);
    ok(&help);
    assert!(String::from_utf8_lossy(&help.stdout).contains("usage: pscat handwrite"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The old entry points still work: each script is a thin wrapper that
/// passes its arguments to the subcommand. Needs bash, so not on Windows.
#[cfg(unix)]
#[test]
fn the_wrapper_scripts_still_work() {
    let dir = scratch("wrappers");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let photo = root.join("tests/data/sphere.jpg");
    let note = dir.join("note.txt");
    std::fs::write(&note, "typed\n").unwrap();
    let cases: [(&str, Vec<&str>, &str); 3] = [
        ("handwrite.sh", vec!["hello"], "hand.png"),
        ("typewrite.sh", vec![path(&note)], "typed.png"),
        ("photo_etch.sh", vec![path(&photo)], "etched.png"),
    ];
    for (script, args, out) in cases {
        let out = dir.join(out);
        let res = Command::new("bash")
            .arg(root.join("scripts").join(script))
            .arg("-o")
            .arg(&out)
            .args(&args)
            .env("PSCAT_BIN", PSCAT)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .output()
            .expect("run wrapper");
        ok(&res);
        assert!(std::fs::metadata(&out).unwrap().len() > 100, "{script}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
