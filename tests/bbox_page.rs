//! The page a file's `%%BoundingBox` chooses (#921): an EPSF file's box is
//! the page unless `--page` is given, any file's with `--bbox`, and user
//! space moves so the box's lower-left corner is the page's.

mod common;
use common::CommandDeadline as _;

use std::process::{Command, Output};

use tiny_skia::Pixmap;

/// The issue's repro: a plain (not EPSF) header with a 400x300 box.
const REPRO: &str =
    "%!PS-Adobe-3.0\n%%BoundingBox: 0 0 400 300\n0 0 1 setrgbcolor 0 0 400 300 rectfill showpage\n";

/// An EPSF file whose box starts at (100, 50): a red square in the box's
/// lower-left corner, a green one in its upper-right.
const OFFSET: &str = "%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 100 50 500 350\n%%EndComments\n\
     1 0 0 setrgbcolor 100 50 10 10 rectfill\n\
     0 1 0 setrgbcolor 490 340 10 10 rectfill showpage\n";

fn pscat(args: &[&str], stdin: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(args)
        .output_with_input(stdin.as_bytes())
        .expect("run pscat")
}

/// Render `source` to PNG with `args`; the page and pscat's stderr.
fn render(tag: &str, args: &[&str], source: &str) -> (Pixmap, String) {
    let out = common::scratch_dir(tag).join("page.png");
    let out_str = out.to_str().expect("utf-8 path");
    let mut all = args.to_vec();
    all.extend(["--png", out_str, "-"]);
    let run = pscat(&all, source);
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(run.status.success(), "pscat {all:?} failed: {stderr}");
    let png = std::fs::read(&out).expect("read the PNG");
    (Pixmap::decode_png(&png).expect("decode"), stderr)
}

fn size(pm: &Pixmap) -> (u32, u32) {
    (pm.width(), pm.height())
}

fn rgb(pm: &Pixmap, x: u32, y: u32) -> (u8, u8, u8) {
    let p = pm.pixel(x, y).expect("in bounds");
    (p.red(), p.green(), p.blue())
}

fn epsf(header: &str) -> String {
    format!(
        "%!PS-Adobe-3.0 EPSF-3.0\n{header}0 0 1 setrgbcolor -1000 -1000 3000 3000 rectfill showpage\n"
    )
}

#[test]
fn the_issue_repro_is_400x300_with_bbox_and_letter_without() {
    let (pm, _) = render("bbox-repro", &["--bbox"], REPRO);
    assert_eq!(size(&pm), (400, 300));
    assert_eq!(rgb(&pm, 0, 0), (0, 0, 255));
    assert_eq!(rgb(&pm, 399, 299), (0, 0, 255));
    // A plain document's box is where its marks are, not its media (DSC),
    // and Ghostscript ignores it even with -dEPSCrop: no crop by default.
    let (pm, _) = render("bbox-repro-plain", &[], REPRO);
    assert_eq!(size(&pm), (612, 792));
    // The same file as EPSF is cropped with no flag.
    let eps = REPRO.replace("%!PS-Adobe-3.0\n", "%!PS-Adobe-3.0 EPSF-3.0\n");
    let (pm, _) = render("bbox-repro-eps", &[], &eps);
    assert_eq!(size(&pm), (400, 300));
}

#[test]
fn a_non_zero_origin_moves_user_space() {
    let (pm, _) = render("bbox-offset", &[], OFFSET);
    assert_eq!(size(&pm), (400, 300));
    assert_eq!(
        rgb(&pm, 2, 297),
        (255, 0, 0),
        "box lower-left at the page's"
    );
    assert_eq!(
        rgb(&pm, 397, 2),
        (0, 255, 0),
        "box upper-right at the page's"
    );
    // The origin is part of the default matrix: initgraphics and the reset
    // after showpage keep it, and clippath is the box in user space.
    let probe = "%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 100 50 500 350\n\
                 7 7 translate initgraphics clippath pathbbox 4 array astore ==\n\
                 showpage clippath pathbbox 4 array astore ==\n";
    let run = pscat(&["--headless", "-"], probe);
    assert!(run.status.success());
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        ["[100.0 50.0 500.0 350.0]", "[100.0 50.0 500.0 350.0]"],
        "{out}"
    );
}

#[test]
fn bands_give_the_same_bytes_and_a_band_box_in_user_space() {
    let dir = common::scratch_dir("bbox-bands");
    let (one, two) = (dir.join("one.png"), dir.join("two.png"));
    let (one_s, two_s) = (one.to_str().unwrap(), two.to_str().unwrap());
    // Band 0 (the top band) prints its box.
    let source = format!("{OFFSET}/BandBBox where {{ pop BandBBox == }} if\n");
    assert!(pscat(&["--png", one_s, "-"], &source).status.success());
    let run = pscat(&["--bands", "2", "--png", two_s, "-"], &source);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(std::fs::read(&one).unwrap(), std::fs::read(&two).unwrap());
    let out = String::from_utf8_lossy(&run.stdout);
    let nums: Vec<f64> = out
        .lines()
        .next()
        .unwrap_or_default()
        .trim_matches(['[', ']'])
        .split_whitespace()
        .map(|n| n.parse().unwrap_or_else(|_| panic!("a number: {out:?}")))
        .collect();
    assert_eq!(nums.len(), 4, "{out}");
    assert_eq!((nums[0], nums[2], nums[3]), (100.0, 500.0, 350.0), "{out}");
    assert!(nums[1] > 50.0 && nums[1] < 350.0, "{out}");
}

#[test]
fn hires_atend_and_line_endings() {
    // HiRes wins and each side rounds to the nearest point, as gs -dEPSCrop.
    let (pm, _) = render(
        "bbox-hires",
        &[],
        &epsf("%%BoundingBox: 10 10 211 111\n%%HiResBoundingBox: 10.5 10.5 210.25 110.75\n"),
    );
    assert_eq!(size(&pm), (200, 100));
    let (pm, _) = render(
        "bbox-atend",
        &[],
        &format!(
            "{}%%Trailer\n%%BoundingBox: 0 0 123 45\n%%EOF\n",
            epsf("%%BoundingBox: (atend)\n")
        ),
    );
    assert_eq!(size(&pm), (123, 45));
    let cr = epsf("%%BoundingBox: 0 0 64 32\n").replace('\n', "\r");
    let (pm, _) = render("bbox-cr", &[], &cr);
    assert_eq!(size(&pm), (64, 32));
}

#[test]
fn an_unusable_box_warns_and_keeps_the_default_page() {
    for header in [
        "%%BoundingBox: 0 0 abc 300\n",
        "%%BoundingBox: 50 50 50 300\n",
        "%%BoundingBox: (atend)\n",
    ] {
        let (pm, stderr) = render("bbox-bad", &[], &epsf(header));
        assert_eq!(size(&pm), (612, 792), "{header}");
        assert!(
            stderr.contains("pscat: warning: ignoring the bounding box"),
            "{header}: {stderr}"
        );
    }
    // --bbox on a file without a box says so too.
    let (pm, stderr) = render("bbox-none", &["--bbox"], "%!PS\nshowpage\n");
    assert_eq!(size(&pm), (612, 792));
    assert!(stderr.contains("no %%BoundingBox"), "{stderr}");
}

#[test]
fn page_always_wins_and_refuses_bbox_beside_it() {
    let (pm, _) = render("bbox-page", &["--page", "100x80"], OFFSET);
    assert_eq!(size(&pm), (100, 80));
    // No translation either: user (0, 0) is the page corner again.
    assert_eq!(rgb(&pm, 2, 77), (255, 255, 255));
    let run = pscat(
        &["--bbox", "--page", "100x80", "--png", "x.png", "-"],
        REPRO,
    );
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("--bbox doesn't combine"));
}

#[test]
fn svg_and_pdf_take_the_box_page() {
    let dir = common::scratch_dir("bbox-vector");
    let (svg, pdf) = (dir.join("p.svg"), dir.join("p.pdf"));
    let full = "%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 100 50 500 350\n\
                0 0 1 setrgbcolor 100 50 400 300 rectfill showpage\n";
    for (flag, path) in [("--svg", &svg), ("--pdf", &pdf)] {
        let run = pscat(&[flag, path.to_str().unwrap(), "-"], full);
        assert!(run.status.success(), "{flag}");
    }
    let svg = std::fs::read_to_string(&svg).unwrap();
    assert!(svg.contains(r#"width="400" height="300""#), "{svg}");
    assert!(svg.contains(r#"d="M0 300L400 300L400 0L0 0Z""#), "{svg}");
    let pdf = String::from_utf8_lossy(&std::fs::read(&pdf).unwrap()).into_owned();
    assert!(pdf.contains("/MediaBox [0 0 400 300]"), "{pdf}");
}

#[test]
fn dpi_scales_the_box_page() {
    let (pm, _) = render(
        "bbox-dpi",
        &["--dpi", "144"],
        &epsf("%%BoundingBox: 0 0 40 30\n"),
    );
    assert_eq!(size(&pm), (80, 60));
}
