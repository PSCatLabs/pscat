//! The one-line note when a non-EPSF file's `%%BoundingBox` is ignored for
//! the page size (#1036): on stderr only, and only when it would matter.

mod common;
use common::CommandDeadline as _;

use std::process::{Command, Output};

/// The issue's repro: a plain (not EPSF) header with a 720x486 box.
const PLAIN: &str = "%!PS-Adobe-3.0\n%%BoundingBox: 0 0 720 486\n\
     0 0 1 setrgbcolor 0 0 720 486 rectfill showpage\n";

const NOTE: &str = "pscat: note: %%BoundingBox 720x486 ignored (not EPSF); pass --bbox or --page";

fn render(tag: &str, args: &[&str], source: &str) -> Output {
    let out = common::scratch_dir(tag).join("page.png");
    let out_str = out.to_str().expect("utf-8 path");
    let mut all = args.to_vec();
    all.extend(["--png", out_str, "-"]);
    let run = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(&all)
        .output_with_input(source.as_bytes())
        .expect("run pscat");
    assert!(run.status.success(), "pscat {all:?}: {run:?}");
    run
}

fn stderr(run: &Output) -> String {
    String::from_utf8_lossy(&run.stderr).into_owned()
}

#[test]
fn a_plain_files_ignored_box_is_one_stderr_line_and_not_stdout() {
    let run = render("note-plain", &[], PLAIN);
    let err = stderr(&run);
    let notes: Vec<_> = err.lines().filter(|l| l.contains("BoundingBox")).collect();
    assert_eq!(notes, [NOTE], "{err}");
    // stdout carries only the receipt ("pscat: wrote ..."), never the note.
    let out = String::from_utf8_lossy(&run.stdout);
    assert!(!out.contains("BoundingBox"), "stdout: {out}");
}

#[test]
fn the_note_is_left_out_when_nothing_was_ignored() {
    let cases: [(&str, Vec<&str>, String); 7] = [
        // An EPSF file's box is the page.
        (
            "note-eps",
            vec![],
            PLAIN.replace("%!PS-Adobe-3.0\n", "%!PS-Adobe-3.0 EPSF-3.0\n"),
        ),
        // --bbox asked for it.
        ("note-bbox", vec!["--bbox"], PLAIN.to_string()),
        // --page chose the size.
        ("note-page", vec!["--page", "300x200"], PLAIN.to_string()),
        // Even a page that happens to match the default letter size.
        (
            "note-page-letter",
            vec!["--page", "612x792"],
            PLAIN.to_string(),
        ),
        // The box is the page in use.
        (
            "note-same",
            vec![],
            PLAIN.replace("0 0 720 486", "0 0 612 792"),
        ),
        // An unusable box in a plain file is not the file's business.
        (
            "note-bad",
            vec![],
            PLAIN.replace("0 0 720 486\n", "0 0 abc 486\n"),
        ),
        // No DSC header at all.
        (
            "note-none",
            vec![],
            "0 0 1 setrgbcolor 0 0 20 20 rectfill showpage".to_string(),
        ),
    ];
    for (tag, args, source) in cases {
        let err = stderr(&render(tag, &args, &source));
        assert!(!err.contains("ignored (not EPSF)"), "{tag}: {err}");
    }
}

#[test]
fn a_banded_render_and_a_dpi_change_note_it_once() {
    for (tag, args) in [
        ("note-bands", vec!["--bands", "2"]),
        ("note-dpi", vec!["--dpi", "36"]),
    ] {
        let err = stderr(&render(tag, &args, PLAIN));
        assert_eq!(err.matches("ignored (not EPSF)").count(), 1, "{tag}: {err}");
    }
}

#[test]
fn a_text_only_run_draws_no_page_and_says_nothing() {
    // `--headless` with no output file prints text; the page size is moot.
    let run = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .arg("--headless")
        .arg("-")
        .output_with_input(PLAIN.as_bytes())
        .expect("run pscat");
    assert!(run.status.success(), "{run:?}");
    let err = stderr(&run);
    assert!(!err.contains("BoundingBox"), "{err}");
}
