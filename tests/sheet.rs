//! `pscat sheet` (issue #693): labelled review sheets of files at mixed
//! sizes. The geometry is checked against numbers worked out here from
//! the documented layout (12 px margin, 14 px column gap, 1 px border,
//! and for issue #706 a 10 px mat pad, 10 px between stacked cells and
//! between the lines of a wrapped row), not by calling the layout code
//! under test.

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

const GREY: [u8; 4] = [190, 190, 190, 255];

/// `@MAT` sets a cell on a padded mat: opaque art is framed by it, and a
/// page that paints nothing shows the mat through (it is drawn on a
/// transparent page). The cell is not scaled.
#[test]
fn a_mat_pads_the_cell_and_shows_through_a_transparent_page() {
    let d = dir();
    let a = write(&d, "a.ps", RED);
    let blank = write(&d, "blank.ps", "% paints nothing");
    let out = d.join("mat.png");
    let (ok, _, stderr) = sheet(&[
        "-o",
        out.to_str().unwrap(),
        "--no-labels",
        "--no-sizes",
        "--page",
        "100x100",
        "--dpi",
        "72@#222,72@fff",
        &a,
        &blank,
    ]);
    assert!(ok, "{stderr}");
    let p = Pixmap::load_png(&out).unwrap();
    // Each cell is 100 + 2*10 pad + 2 border = 122 px; two rows.
    assert_eq!(
        (p.width(), p.height()),
        (12 + 122 + 14 + 122 + 12, 12 + 122 + 18 + 122 + 12)
    );
    let dark = [0x22, 0x22, 0x22, 255];
    // Row one, first cell: hairline at x 12, mat from 13, art from 23.
    assert_eq!(px(&p, 12, 13), GREY);
    assert_eq!(px(&p, 13, 13), dark);
    assert_eq!(px(&p, 22, 22), dark);
    assert_eq!(px(&p, 23, 23), [255, 0, 0, 255]);
    assert_eq!(px(&p, 122, 122), [255, 0, 0, 255]);
    assert_eq!(px(&p, 123, 123), dark);
    // Row one, second cell (white mat, same red art): x starts 12+122+14.
    let x2 = 12 + 122 + 14 + 1;
    assert_eq!(px(&p, x2, 13), WHITE);
    assert_eq!(px(&p, x2 + 10, 23), [255, 0, 0, 255]);
    // Row two, the blank page: all mat, dark then white.
    let y2 = 12 + 122 + 18 + 1;
    assert_eq!(px(&p, 13 + 60, y2 + 60), dark);
    assert_eq!(px(&p, x2 + 60, y2 + 60), WHITE);
}

/// `+` stacks cells in one column: the column is as wide as its widest,
/// the row as tall as its tallest stack, and a lone cell is centred.
#[test]
fn a_plus_stacks_cells_in_one_column() {
    let d = dir();
    let a = write(&d, "a.ps", RED);
    let out = d.join("stack.png");
    let (ok, _, stderr) = sheet(&[
        "-o",
        out.to_str().unwrap(),
        "--no-labels",
        "--no-sizes",
        "--page",
        "100x100",
        "--dpi",
        "72,36+36",
        &a,
    ]);
    assert!(ok, "{stderr}");
    let p = Pixmap::load_png(&out).unwrap();
    // Stack: 52 + 10 + 52 = 114 high; the 102 px cell is centred in it.
    assert_eq!(
        (p.width(), p.height()),
        (12 + 102 + 14 + 52 + 12, 12 + 114 + 12)
    );
    let red = [255, 0, 0, 255];
    assert_eq!(px(&p, 13, 12 + 6 + 1), red);
    assert_eq!(px(&p, 13, 12 + 6), GREY);
    // The stack starts at the top of the band, x 12+102+14 = 128.
    assert_eq!(px(&p, 129, 13), red);
    assert_eq!(px(&p, 129, 12), GREY);
    assert_eq!(px(&p, 129 + 49, 13 + 49), red);
    // Second stacked cell: 12 + 52 + 10 = 74, image at 75.
    assert_eq!(px(&p, 129, 75), red);
    assert_eq!(px(&p, 129, 74), GREY);
    assert_eq!(px(&p, 129, 73), WHITE);
}

/// `--cols N` continues a long row on the next line, under the same
/// column slots, with the label only above the first line.
#[test]
fn cols_wraps_a_long_row_onto_further_lines() {
    let d = dir();
    let a = write(&d, "a.ps", RED);
    let out = d.join("wrap.png");
    let run = |extra: &[&str]| {
        let mut args = vec![
            "-o",
            out.to_str().unwrap(),
            "--no-labels",
            "--no-sizes",
            "--page",
            "50x50",
            "--dpi",
            "72,72,72,72,72",
        ];
        args.extend_from_slice(extra);
        args.push(&a);
        let (ok, _, stderr) = sheet(&args);
        assert!(ok, "{stderr}");
        Pixmap::load_png(&out).unwrap()
    };
    let flat = run(&[]);
    assert_eq!(
        (flat.width(), flat.height()),
        (12 + 5 * 52 + 4 * 14 + 12, 76)
    );
    let p = run(&["--cols", "2"]);
    // Two slots wide, three lines (2+2+1) 10 px apart.
    assert_eq!(
        (p.width(), p.height()),
        (12 + 52 + 14 + 52 + 12, 12 + 3 * 52 + 2 * 10 + 12)
    );
    let red = [255, 0, 0, 255];
    for (x, y) in [
        (13, 13),
        (13 + 66, 13),
        (13, 13 + 62),
        (13 + 66, 13 + 62),
        (13, 13 + 124),
    ] {
        assert_eq!(px(&p, x, y), red, "({x},{y})");
    }
    // The fifth cell is alone on its line: nothing in the second slot.
    assert_eq!(px(&p, 13 + 66, 13 + 124), WHITE);
}

/// A sweep makes one cell per value, each with its value as the caption;
/// `--sweep` defines the name before the program runs.
#[test]
fn a_parameter_sweep_draws_one_labelled_cell_per_value() {
    let d = dir();
    let prog = write(
        &d,
        "w.ps",
        "/W where { pop } { /W 1 def } ifelse 1 0 0 setrgbcolor 0 0 W 50 rectfill",
    );
    let out = d.join("param.png");
    let (ok, stdout, stderr) = sheet(&[
        "-o",
        out.to_str().unwrap(),
        "--no-labels",
        "--no-sizes",
        "--page",
        "100x50",
        "--sweep",
        "W=20,60",
        &prog,
    ]);
    assert!(ok, "{stderr}");
    assert!(stdout.contains("wrote"), "{stdout}");
    let p = Pixmap::load_png(&out).unwrap();
    assert_eq!(p.width(), 12 + 102 + 14 + 102 + 12);
    let red = [255, 0, 0, 255];
    // First cell (W=20): red at x 13..33, white beyond; second (W=60).
    assert_eq!(px(&p, 13 + 10, 30), red);
    assert_eq!(px(&p, 13 + 40, 30), WHITE);
    let x2 = 12 + 102 + 14 + 1;
    assert_eq!(px(&p, x2 + 40, 30), red);
    assert_eq!(px(&p, x2 + 70, 30), WHITE);
    // The value labels are the only text: a strip under the cells.
    let ink = |x0: u32, x1: u32| {
        (12 + 52 + 5..p.height()).any(|y| (x0..x1).any(|x| px(&p, x, y)[0] < 100))
    };
    assert!(ink(12, 114) && ink(x2, x2 + 100));
    // Without the sweep the same program is one cell with no caption.
    let out2 = d.join("param-none.png");
    let (ok, _, _) = sheet(&[
        "-o",
        out2.to_str().unwrap(),
        "--no-labels",
        "--no-sizes",
        "--page",
        "100x50",
        &prog,
    ]);
    assert!(ok);
    assert_eq!(Pixmap::load_png(&out2).unwrap().height(), 12 + 52 + 12);
}

/// `--sweep-seed` reseeds each cell; a program that never seeds is told so.
#[test]
fn a_seed_sweep_reseeds_each_cell_and_warns_when_nothing_seeds() {
    let d = dir();
    let seeded = write(
        &d,
        "seeded.ps",
        "1 srand rand 2 mod 0 eq { 1 0 0 } { 0 0 1 } ifelse setrgbcolor 0 0 20 20 rectfill",
    );
    let out = d.join("seeds.png");
    let (ok, _, stderr) = sheet(&[
        "-o",
        out.to_str().unwrap(),
        "--no-labels",
        "--no-sizes",
        "--page",
        "20x20",
        "--sweep-seed",
        "1:8",
        &seeded,
    ]);
    assert!(ok, "{stderr}");
    assert!(!stderr.contains("srand"), "{stderr}");
    let p = Pixmap::load_png(&out).unwrap();
    // The captions widen the columns, so look along a row through the
    // cells for the two colours the program can draw.
    let colours: std::collections::HashSet<[u8; 4]> = (0..p.width())
        .map(|x| px(&p, x, 23))
        .filter(|c| *c == [255, 0, 0, 255] || *c == [0, 0, 255, 255])
        .collect();
    assert_eq!(colours.len(), 2, "eight seeds should give both colours");

    let plain = write(&d, "plain.ps", RED);
    let (ok, _, stderr) = sheet(&[
        "-o",
        out.to_str().unwrap(),
        "--page",
        "20x20",
        "--sweep-seed",
        "1:3",
        &plain,
    ]);
    assert!(ok);
    assert!(stderr.contains("never called srand"), "{stderr}");
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
        (&["-o", out, &a, "--sweep-seed", "1:3"], "no file after"),
        (
            &["-o", out, "--sweep-seed", "1:3", "--sweep", "A=1,2", &a],
            "mutually exclusive",
        ),
        (&["-o", out, "--sweep", "a;b=1,2", &a], "identifier"),
        (&["-o", out, "--sweep", "nope", &a], "NAME=SPEC"),
        (&["-o", out, "--sweep-seed", "x", &a], "seed"),
        (&["-o", out, "--cols", "0", &a], "--cols"),
        (&["-o", out, "--dpi", "72@red", &a], "mat colour"),
        (&["-o", out, "--dpi", "72+", &a], "--dpi"),
        // 8000 px of page at 72 dpi, twice across: past the sheet's limit.
        (
            &["-o", out, "--page", "5000x100", "--dpi", "72,72", &a],
            "limit",
        ),
        (
            &["-o", out, "--page", "5000x100", "--dpi", "72,72", &a],
            "--cols",
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
