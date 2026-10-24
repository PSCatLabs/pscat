//! `pscat sheet` (issue #693): labelled review sheets of files at mixed
//! sizes. The geometry is checked against numbers worked out here from
//! the documented layout (12 px margin, 14 px column gap, 1 px border),
//! not by calling the layout code under test.

use std::path::{Path, PathBuf};
use std::process::Command;

use tiny_skia::Pixmap;

mod common;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

fn dir() -> PathBuf {
    common::scratch_dir("pscat-sheet")
}

fn write(dir: &Path, name: &str, src: &str) -> String {
    let p = dir.join(name);
    std::fs::write(&p, src).unwrap();
    p.to_str().unwrap().to_string()
}

/// Run `pscat sheet args`, returning (success, stdout, stderr).
fn sheet(args: &[&str]) -> (bool, String, String) {
    let out = Command::new(BIN)
        .arg("sheet")
        .args(args)
        .output()
        .expect("spawn pscat");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn px(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * p.width() + x) * 4) as usize;
    p.data()[i..i + 4].try_into().unwrap()
}

const RED: &str = "1 0 0 setrgbcolor 0 0 200 200 rectfill";
const BLUE: &str = "0 0 1 setrgbcolor 0 0 200 200 rectfill";
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// One file at two dpis: each cell keeps its own pixel size, sits
/// centred in a column as wide as the larger, with a hairline around it.
#[test]
fn cells_keep_their_pixel_size_and_are_centred_in_the_column() {
    let d = dir();
    let a = write(&d, "a.ps", RED);
    let out = d.join("two-sizes.png");
    let (ok, stdout, stderr) = sheet(&[
        "-o",
        out.to_str().unwrap(),
        "--no-labels",
        "--no-sizes",
        "--page",
        "100x100",
        "--dpi",
        "72,36",
        &a,
    ]);
    assert!(ok, "{stderr}");
    assert!(stdout.contains("wrote"), "{stdout}");
    let p = Pixmap::load_png(&out).unwrap();
    // 12 + (100+2) + 14 + (50+2) + 12 wide; 12 + 102 + 12 high.
    assert_eq!((p.width(), p.height()), (192, 126));
    let red = [255, 0, 0, 255];
    // The large cell: image at (13, 13), 100x100, hairline just outside it.
    assert_eq!(px(&p, 13, 13), red);
    assert_eq!(px(&p, 112, 112), red);
    assert_eq!(px(&p, 12, 13), [190, 190, 190, 255]);
    assert_eq!(px(&p, 113, 112), [190, 190, 190, 255]);
    assert_eq!(px(&p, 11, 13), WHITE);
    // The small cell: 50x50 centred vertically in the 102 px band
    // (offset 25), so its image starts at x 12+102+14+1 = 129, y 12+25+1 = 38.
    assert_eq!(px(&p, 129, 38), red);
    assert_eq!(px(&p, 178, 87), red);
    assert_eq!(px(&p, 179, 87), [190, 190, 190, 255]);
    assert_eq!(px(&p, 129, 37), [190, 190, 190, 255]);
    assert_eq!(px(&p, 129, 36), WHITE);
    assert_eq!(px(&p, 129, 88), [190, 190, 190, 255]);
    assert_eq!(px(&p, 129, 89), WHITE);
}

/// Files are rows and `--page`/`--dpi` stick to the files after them;
/// `--label` applies to the next file only. Labels and size captions
/// are drawn as text (dark pixels in the strips) unless switched off.
#[test]
fn labels_and_captions_add_text_and_the_switches_remove_it() {
    let d = dir();
    let a = write(&d, "a.ps", RED);
    let b = write(&d, "b.ps", BLUE);
    let dark = |p: &Pixmap, x0: u32, y0: u32, x1: u32, y1: u32| {
        (y0..y1).any(|y| (x0..x1).any(|x| px(p, x, y)[0] < 100 && px(p, x, y)[1] < 100))
    };
    let run = |name: &str, extra: &[&str]| {
        let out = d.join(name);
        let mut args = vec![
            "-o",
            out.to_str().unwrap(),
            "--page",
            "60x60",
            "--dpi",
            "72",
        ];
        args.extend_from_slice(extra);
        let (ok, _, stderr) = sheet(&args);
        assert!(ok, "{stderr}");
        Pixmap::load_png(&out).unwrap()
    };
    let bare = run("bare.png", &["--no-labels", "--no-sizes", &a, &b]);
    assert_eq!(
        (bare.width(), bare.height()),
        (12 + 62 + 12, 12 + 62 + 18 + 62 + 12)
    );
    let full = run("full.png", &["--label", "Alpha", &a, &b]);
    assert!(full.height() > bare.height() && full.width() >= bare.width());
    // The header strip above row one has ink in it (the label), and the
    // caption strip under it too ("60x60 px"); with both off they are
    // plain white.
    assert!(dark(&full, 12, 12, 60, 25), "no label ink");
    assert!(!dark(&bare, 0, 0, bare.width(), 12));
    let no_labels = run("nolabels.png", &["--no-labels", &a, &b]);
    let no_sizes = run("nosizes.png", &["--no-sizes", &a, &b]);
    assert!(no_labels.height() > bare.height() && no_sizes.height() > bare.height());
    assert!(full.height() > no_labels.height() && full.height() > no_sizes.height());
}

/// `--page` between files gives each row its own page size; the column
/// takes the wider of the two.
#[test]
fn page_and_dpi_are_sticky_per_file() {
    let d = dir();
    let a = write(&d, "a.ps", RED);
    let b = write(&d, "b.ps", BLUE);
    let out = d.join("mixed.png");
    let (ok, _, stderr) = sheet(&[
        "-o",
        out.to_str().unwrap(),
        "--no-labels",
        "--no-sizes",
        "--page",
        "80x40",
        &a,
        "--page",
        "30x50",
        "--dpi",
        "144",
        &b,
    ]);
    assert!(ok, "{stderr}");
    let p = Pixmap::load_png(&out).unwrap();
    // Row one 80x40 at 72 dpi; row two 30x50 at 144 dpi = 60x100.
    // Width: 12 + max(82, 62) + 12; height: 12 + 42 + 18 + 102 + 12.
    assert_eq!((p.width(), p.height()), (106, 186));
    assert_eq!(px(&p, 13, 13), [255, 0, 0, 255]);
    // Row two, 60 px wide in an 82 px column: offset (82-62)/2 = 10, so
    // the image starts at 12+10+1 with its hairline just left of it.
    assert_eq!(px(&p, 23, 12 + 42 + 18 + 1), [0, 0, 255, 255]);
    assert_eq!(px(&p, 22, 12 + 42 + 18 + 1), [190, 190, 190, 255]);
    assert_eq!(px(&p, 21, 12 + 42 + 18 + 1), WHITE);
}

/// `9x4`-style entries enlarge with no smoothing: every source pixel is
/// an exact block.
#[test]
fn a_zoom_entry_enlarges_without_smoothing() {
    let d = dir();
    // A 2x1 px page: left pixel black, right pixel white.
    let a = write(&d, "px.ps", "0 setgray 0 0 1 1 rectfill");
    let out = d.join("zoom.png");
    let (ok, _, stderr) = sheet(&[
        "-o",
        out.to_str().unwrap(),
        "--no-labels",
        "--no-sizes",
        "--page",
        "2x1",
        "--dpi",
        "72x4",
        &a,
    ]);
    assert!(ok, "{stderr}");
    let p = Pixmap::load_png(&out).unwrap();
    assert_eq!((p.width(), p.height()), (12 + 8 + 2 + 12, 12 + 4 + 2 + 12));
    for x in 13..17 {
        for y in 13..17 {
            assert_eq!(px(&p, x, y), [0, 0, 0, 255], "({x},{y})");
        }
        for y in 13..17 {
            assert_eq!(px(&p, x + 4, y), WHITE, "({},{y})", x + 4);
        }
    }
}

/// A program that errors is still drawn as far as it got, the sheet is
/// written, and the run fails.
#[test]
fn a_failing_cell_is_drawn_and_the_run_fails() {
    let d = dir();
    let bad = write(
        &d,
        "bad.ps",
        "1 0 0 setrgbcolor 0 0 20 20 rectfill undefinedthing",
    );
    let out = d.join("failed.png");
    let (ok, _, stderr) = sheet(&[
        "-o",
        out.to_str().unwrap(),
        "--no-labels",
        "--no-sizes",
        "--page",
        "20x20",
        &bad,
    ]);
    assert!(!ok);
    assert!(stderr.contains("undefined"), "{stderr}");
    let p = Pixmap::load_png(&out).expect("the partial sheet is written");
    assert_eq!(px(&p, 13, 13), [255, 0, 0, 255]);
}

#[test]
fn bad_input_is_refused_with_a_message() {
    let d = dir();
    let a = write(&d, "a.ps", RED);
    let out = d.join("never.png");
    let out = out.to_str().unwrap();
    let cases: &[(&[&str], &str)] = &[
        (&["-o", out], "no files given"),
        (&["-o", out, "/nonexistent/x.ps"], "cannot read"),
        (&["-o", out, "--dpi", "8", &a], "--dpi"),
        (&["-o", out, "--dpi", "9x1", &a], "zoom"),
        (&["-o", out, "--page", "0x5", &a], "--page"),
        (&["-o", out, "--bogus", &a], "unknown option"),
        (&["-o", out, &a, "--label", "x"], "no file after"),
        // 8000 px of page at 72 dpi, twice across: past the sheet's limit.
        (
            &["-o", out, "--page", "5000x100", "--dpi", "72,72", &a],
            "limit",
        ),
    ];
    for (args, want) in cases {
        let (ok, _, stderr) = sheet(args);
        assert!(!ok, "{args:?} should fail");
        assert!(stderr.contains(want), "{args:?}: {stderr}");
    }
    assert!(!Path::new(out).exists(), "nothing is written on a refusal");
}

#[test]
fn help_prints_usage_and_succeeds() {
    let (ok, stdout, _) = sheet(&["--help"]);
    assert!(ok);
    assert!(stdout.contains("usage: pscat sheet"), "{stdout}");
}
