//! Strokes under a CTM that is not a similarity (#1196). PostScript strokes
//! in user space with a round pen, so `[2 0 0 0.5 0 0] concat` (or the same
//! matrix as `rectstroke`'s operand) makes the pen twice as wide across
//! vertical lines and half as wide across horizontal ones. pscat used to
//! stroke the device path with one width, `w·√|det CTM|`, which is 1·w here.
//!
//! Side widths are measured as ink: the summed coverage of each dark run on
//! one pixel row or column, which is the stroke's width there in device
//! pixels. tiny-skia places an anti-aliased edge on a quarter-pixel grid, so
//! an edge off that grid reads up to 1/8 px off per side. Two kinds of check:
//!
//! * analytic widths, with no Ghostscript needed,
//! * live gs comparisons (`false setstrokeadjust`, 4-bit anti-aliasing) for
//!   rotated and skewed matrices, dashes, `strokepath`, and the PDF export
//!   rasterised back by gs. gs's coverage of a slanted edge reads up to
//!   ~0.4 px wide (a 5.12 px skewed side, exact in both `strokepath`
//!   outlines, is 5.47 px of gs ink and 5.0 of pscat's), so the raster
//!   comparison allows 0.6 px. That still fails the old round pen on at
//!   least one probe of every case but the skewed concat, which the
//!   analytic table pins instead. The `strokepath` bbox is the exact
//!   geometric check.

use std::process::Command;

use common::CommandDeadline as _;
use pscat::Interp;
use tiny_skia::Pixmap;

mod common;

const SIZE: u32 = 200;

fn scratch(name: &str) -> std::path::PathBuf {
    common::scratch_dir("pscat_stroke_aniso").join(name)
}

fn interp(src: &str, setup: impl FnOnce(&mut Interp)) -> Interp {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    setup(&mut it);
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{src:?}: {}", it.error_report(&e)));
    it
}

fn render_pscat(src: &str) -> Pixmap {
    interp(src, |_| {}).gfx().pixmap.clone()
}

/// gs at the same page and resolution. `file` is either PostScript (run
/// after `false setstrokeadjust`) or a PDF.
fn gs_png(input: &std::path::Path, name: &str) -> Pixmap {
    let png = scratch(&format!("{name}.png"));
    let out = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-sDEVICE=png16m",
            "-dGraphicsAlphaBits=4",
            &format!("-g{SIZE}x{SIZE}"),
            "-r72",
            &format!("-o{}", png.display()),
        ])
        .arg(input)
        .output_deadline()
        .expect("run gs");
    assert!(out.status.success(), "gs failed on {}", input.display());
    let data = std::fs::read(&png).expect("gs output");
    Pixmap::decode_png(&data).expect("decode gs png")
}

fn render_gs(src: &str, name: &str) -> Pixmap {
    let ps = scratch(&format!("{name}.ps"));
    std::fs::write(&ps, format!("false setstrokeadjust\n{src}\nshowpage\n")).unwrap();
    gs_png(&ps, name)
}

/// Ink at a pixel, 0 (white) to 1 (black).
fn ink(px: &Pixmap, x: u32, y: u32) -> f64 {
    let p = px.pixel(x, y).expect("in page");
    let luma = (u32::from(p.red()) + u32::from(p.green()) + u32::from(p.blue())) as f64 / 3.0;
    1.0 - luma / 255.0
}

/// A probe: a pixel row (`Row(y)`, top-down as in the PNG) or column.
#[derive(Clone, Copy, Debug)]
enum Probe {
    Row(u32),
    Col(u32),
}

const MID_ROW: Probe = Probe::Row(SIZE / 2);
const MID_COL: Probe = Probe::Col(SIZE / 2);

/// Each dark run along the probe: (first pixel, summed ink).
fn runs(px: &Pixmap, probe: Probe) -> Vec<(u32, f64)> {
    let at = |i: u32| match probe {
        Probe::Row(y) => ink(px, i, y),
        Probe::Col(x) => ink(px, x, i),
    };
    let mut out = Vec::new();
    let mut cur: Option<(u32, f64)> = None;
    for i in 0..SIZE {
        let v = at(i);
        match (&mut cur, v > 0.02) {
            (Some((_, sum)), true) => *sum += v,
            (None, true) => cur = Some((i, v)),
            (Some(_), false) => out.extend(cur.take()),
            (None, false) => {}
        }
    }
    out.extend(cur);
    out
}

fn widths(px: &Pixmap, probe: Probe) -> Vec<f64> {
    runs(px, probe).into_iter().map(|(_, w)| w).collect()
}

fn assert_widths(what: &str, got: &[f64], want: &[f64], tol: f64) {
    assert_eq!(
        got.len(),
        want.len(),
        "{what}: runs {got:.2?}, expected {want:?}"
    );
    for (g, w) in got.iter().zip(want) {
        assert!(
            (g - w).abs() <= tol,
            "{what}: widths {got:.2?}, expected {want:?} (±{tol})"
        );
    }
}

/// The issue's two programs, scaled up: a 4-wide pen under
/// `[2 0 0 0.5 0 0]` is 8 px across the vertical sides and 2 px across the
/// horizontal ones. The rectangle is the device square 40..160 both ways.
const RECTSTROKE_MATRIX: &str = "4 setlinewidth 40 40 120 120 [2 0 0 0.5 0 0] rectstroke";
const STROKE_UNDER_CONCAT: &str = "4 setlinewidth [2 0 0 0.5 0 0] concat \
     20 80 moveto 80 80 lineto 80 320 lineto 20 320 lineto closepath stroke";

#[test]
fn rectstroke_matrix_scales_the_pen_per_axis() {
    let px = render_pscat(RECTSTROKE_MATRIX);
    assert_widths("vertical sides", &widths(&px, MID_ROW), &[8.0, 8.0], 0.1);
    assert_widths("horizontal sides", &widths(&px, MID_COL), &[2.0, 2.0], 0.1);
}

#[test]
fn stroke_under_a_non_uniform_concat_matches_rectstroke_with_the_matrix() {
    let px = render_pscat(STROKE_UNDER_CONCAT);
    assert_widths("vertical sides", &widths(&px, MID_ROW), &[8.0, 8.0], 0.1);
    assert_widths("horizontal sides", &widths(&px, MID_COL), &[2.0, 2.0], 0.1);
    // The same marks as rectstroke's matrix form: the PLRM defines that
    // operand as `gsave matrix concat stroke grestore` on the built path.
    let other = render_pscat(RECTSTROKE_MATRIX);
    assert_eq!(px.data(), other.data());
}

/// Rotated and skewed pens, with widths worked out by hand. For a pen
/// matrix `[a b c d]` (x' = a·x + c·y, y' = b·x + d·y, det 1 in every case
/// here) and line width w:
///
/// * a *device* vertical or horizontal side (rectstroke: the matrix shapes
///   only the pen, so the rectangle stays axis-aligned) is as wide as the
///   pen ellipse: w·√(a² + c²) across a row, w·√(b² + d²) across a column;
/// * a *user* vertical or horizontal side (stroke under concat) keeps its
///   user width along the user axis, which is w/|d| across a device row and
///   w/|a| across a device column (the diagonal of M⁻¹, det 1).
///
/// The old round pen was w = 4 everywhere (or 4/cos of a slanted side).
const ANALYTIC_CASES: &[(&str, &str, [f64; 2])] = &[
    (
        // rotate 30 · scale(2, 0.5): a b c d = 1.732 1 -0.25 0.433.
        "rectstroke, rotated non-uniform matrix",
        "4 setlinewidth 40 40 120 120 [1.7320508 1 -0.25 0.4330127 0 0] rectstroke",
        [7.0, 4.359],
    ),
    (
        "stroke under a rotated non-uniform concat",
        ROTATED_CONCAT,
        [9.238, 2.309],
    ),
    (
        "rectstroke, skewed matrix",
        "4 setlinewidth 40 40 120 120 [1 0 0.8 1 0 0] rectstroke",
        [5.122, 4.0],
    ),
    // Its vertical sides lean, and a row crosses them at their user width:
    // the old pen measured 4·√(1 + 0.6²) = 4.66 there.
    ("stroke under a skewed concat", SKEWED_CONCAT, [4.0, 4.0]),
];

const ROTATED_CONCAT: &str = "100 100 translate 30 rotate 2 0.5 scale 4 setlinewidth \
     -30 -120 moveto 30 -120 lineto 30 120 lineto -30 120 lineto closepath stroke";
const SKEWED_CONCAT: &str = "[1 0 0.6 1 0 0] concat 4 setlinewidth \
     -20 40 moveto 100 40 lineto 100 160 lineto -20 160 lineto closepath stroke";

#[test]
fn rotated_and_skewed_pens_have_the_analytic_side_widths() {
    for (what, src, [across_row, across_col]) in ANALYTIC_CASES {
        let px = render_pscat(src);
        // 0.25: two edges, each up to 1/8 px off on the quarter-pixel grid
        // (the skewed matrix's 5.122 px side reads 5.0).
        assert_widths(
            &format!("{what}, row"),
            &widths(&px, MID_ROW),
            &[*across_row; 2],
            0.25,
        );
        assert_widths(
            &format!("{what}, column"),
            &widths(&px, MID_COL),
            &[*across_col; 2],
            0.25,
        );
    }
}

#[test]
fn similarity_ctms_keep_a_round_pen() {
    // Uniform scale, rotation and reflection: the pen stays w·s across
    // every side, as before #1196 (these take the unchanged device path).
    for (what, src) in [
        (
            "uniform scale",
            "2 2 scale 2 setlinewidth 20 20 60 60 rectstroke",
        ),
        (
            "reflection",
            "[-2 0 0 2 200 0] concat 2 setlinewidth 20 20 60 60 rectstroke",
        ),
        (
            "rectstroke with a rotation by 90",
            "4 setlinewidth 40 40 120 120 [0 1 -1 0 0 0] rectstroke",
        ),
    ] {
        let px = render_pscat(src);
        assert_widths(what, &widths(&px, MID_ROW), &[4.0, 4.0], 0.1);
        assert_widths(what, &widths(&px, MID_COL), &[4.0, 4.0], 0.1);
    }
}

#[test]
fn a_hairline_stays_one_device_pixel_under_any_ctm() {
    // `0 setlinewidth` is the thinnest line the device draws, not a pen
    // the CTM could flatten to nothing or stretch to four pixels.
    let px = render_pscat("[4 0 0 0.25 0 0] concat 0 setlinewidth 10 160 30 480 rectstroke");
    for probe in [MID_ROW, MID_COL] {
        let got = widths(&px, probe);
        assert_eq!(got.len(), 2, "{probe:?}: {got:.2?}");
        assert!(
            got.iter().all(|w| (0.5..=1.5).contains(w)),
            "{probe:?}: {got:.2?}"
        );
    }
}

#[test]
fn strokepath_outline_is_the_user_space_pen() {
    // In user space the pen is round whatever the CTM, so the outline of a
    // 10-wide butt line is the user rectangle 0 -5 100 5. Before #1196 the
    // round device pen came back to user space as 0 -10 100 10.
    let it = interp(
        "[2 0 0 0.5 0 0] concat newpath 0 0 moveto 100 0 lineto 10 setlinewidth \
         strokepath pathbbox",
        |_| {},
    );
    let got: Vec<f64> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().parse().expect("number"))
        .collect();
    let want = [0.0, -5.0, 100.0, 5.0];
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < 0.01, "pathbbox {got:?}, expected {want:?}");
    }
}

#[test]
fn svg_puts_the_pen_on_the_path_inside_the_clip_group() {
    let it = interp(
        "0 0 150 150 rectclip [2 0 0 0.5 0 0] concat 4 setlinewidth [6 3] 0 setdash \
         20 80 moveto 80 80 lineto stroke",
        |it| it.gfx_mut().enable_svg(),
    );
    let svg = &it.gfx().svg_pages().expect("recording on")[0];
    // Device y is down, so the pen map from pen space is the matrix with
    // its y flipped; the width and dashes are user units times √|det| = 1.
    let path = svg
        .find("<path transform=\"matrix(2 0 0 -0.5 0 0)\"")
        .unwrap_or_else(|| panic!("no transformed stroke: {svg}"));
    let clip = svg.find("<g clip-path=").expect("clip group");
    assert!(clip < path, "the clip group must wrap the path: {svg}");
    let tag = &svg[path..svg[path..].find("/>").map_or(svg.len(), |e| path + e)];
    assert!(tag.contains("stroke-width=\"4\""), "{tag}");
    assert!(tag.contains("stroke-dasharray=\"6 3\""), "{tag}");

    // A similarity CTM writes the device path as before, with no transform.
    let it = interp(
        "2 2 scale 2 setlinewidth 20 20 moveto 80 20 lineto stroke",
        |it| it.gfx_mut().enable_svg(),
    );
    let svg = &it.gfx().svg_pages().expect("recording on")[0];
    assert!(!svg.contains("<path transform"), "{svg}");
    assert!(svg.contains("stroke-width=\"4\""), "{svg}");
}

#[test]
fn pdf_strokes_under_the_pen_cm() {
    let it = interp(STROKE_UNDER_CONCAT, |it| it.gfx_mut().enable_pdf());
    let pdf = it.gfx().pdf_document().expect("recording on");
    let text = common::pdf_text(&pdf);
    let stroke = text
        .lines()
        .find(|l| l.ends_with(" S"))
        .unwrap_or_else(|| panic!("no stroke: {text}"));
    assert!(
        stroke.contains("2 0 0 -0.5 0 0 cm ") && stroke.contains(" 4 w "),
        "{stroke}"
    );
    if !common::gs_available() {
        return;
    }
    // gs reads the PDF back with the elliptical pen. Its PDF interpreter
    // adjusts strokes (about +0.3 px here), hence the looser tolerance.
    let path = scratch("pen.pdf");
    std::fs::write(&path, &pdf).unwrap();
    let px = gs_png(&path, "pen-pdf");
    assert_widths(
        "PDF vertical sides",
        &widths(&px, MID_ROW),
        &[8.0, 8.0],
        0.6,
    );
    assert_widths(
        "PDF horizontal sides",
        &widths(&px, MID_COL),
        &[2.0, 2.0],
        0.6,
    );
}

/// Programs compared against Ghostscript, each with the probes to measure.
const GS_CASES: &[(&str, &str, &[Probe])] = &[
    (
        "rectstroke, non-uniform matrix",
        RECTSTROKE_MATRIX,
        &[MID_ROW, MID_COL],
    ),
    (
        "stroke under a non-uniform concat",
        STROKE_UNDER_CONCAT,
        &[MID_ROW, MID_COL],
    ),
    (
        // rotate 30 · scale(2, 0.5): the rectangle stays axis-aligned and
        // the elliptical pen is turned 30 degrees.
        "rectstroke, rotated non-uniform matrix",
        "4 setlinewidth 40 40 120 120 [1.7320508 1 -0.25 0.4330127 0 0] rectstroke",
        &[MID_ROW, MID_COL],
    ),
    (
        "stroke under a rotated non-uniform concat",
        ROTATED_CONCAT,
        &[MID_ROW, MID_COL],
    ),
    (
        "rectstroke, skewed matrix",
        "4 setlinewidth 40 40 120 120 [1 0 0.8 1 0 0] rectstroke",
        &[MID_ROW, MID_COL],
    ),
    (
        "stroke under a skewed concat",
        SKEWED_CONCAT,
        &[MID_ROW, MID_COL],
    ),
    (
        // Dashes are user lengths too: 20 on, 10 off along the stretched
        // horizontal line (device y 50, PNG row 149), 5 on, 2.5 off along
        // the squashed vertical one (device x 150).
        "dashes under a non-uniform concat",
        "[2 0 0 0.5 0 0] concat 4 setlinewidth [10 5] 0 setdash \
         10 100 moveto 90 100 lineto 75 40 moveto 75 360 lineto stroke",
        &[Probe::Row(149), Probe::Col(150)],
    ),
    (
        "round caps and joins under a non-uniform concat",
        "[2 0 0 0.5 0 0] concat 6 setlinewidth 1 setlinecap 1 setlinejoin \
         30 100 moveto 70 300 lineto 70 100 lineto stroke",
        &[MID_ROW, MID_COL, Probe::Row(150)],
    ),
];

#[test]
fn side_widths_match_ghostscript() {
    if !common::gs_available() {
        return;
    }
    for (i, (what, src, probes)) in GS_CASES.iter().enumerate() {
        let ours = render_pscat(src);
        let theirs = render_gs(src, &format!("case{i}"));
        for &probe in *probes {
            let (a, b) = (runs(&ours, probe), runs(&theirs, probe));
            assert_eq!(
                a.len(),
                b.len(),
                "{what} {probe:?}: pscat runs {a:.2?}, gs {b:.2?}"
            );
            for ((sa, wa), (sb, wb)) in a.iter().zip(&b) {
                assert!(
                    sa.abs_diff(*sb) <= 1 && (wa - wb).abs() <= 0.6,
                    "{what} {probe:?}: pscat runs {a:.2?}, gs {b:.2?}"
                );
            }
        }
    }
}

#[test]
fn strokepath_bbox_matches_ghostscript_under_an_anisotropic_ctm() {
    if !common::gs_available() {
        return;
    }
    for src in [
        "[2 0 0 0.5 0 0] concat newpath 0 0 moveto 100 0 lineto 100 50 lineto 10 setlinewidth strokepath",
        // Miter joins and butt or square caps only: a round join's curve
        // control points (which pathbbox counts) differ between the two
        // stroker's arc approximations.
        "200 200 translate 30 rotate 0.5 2 scale newpath 0 0 moveto 100 0 lineto 100 50 lineto \
         10 setlinewidth strokepath",
        "[1 0 0.5 1 100 100] concat newpath 0 0 moveto 100 0 lineto 100 50 lineto 10 setlinewidth \
         2 setlinecap strokepath",
    ] {
        let it = interp(&format!("{src} pathbbox"), |_| {});
        let ours: Vec<f64> = it
            .operand_stack()
            .iter()
            .map(|o| o.repr().parse().expect("number"))
            .collect();
        let ps = scratch("bbox.ps");
        std::fs::write(
            &ps,
            format!("false setstrokeadjust {src} pathbbox 4 array astore ==\n"),
        )
        .unwrap();
        let out = Command::new("gs")
            .args(["-q", "-dNOPAUSE", "-dBATCH", "-dNODISPLAY"])
            .arg(&ps)
            .output_deadline()
            .expect("run gs");
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let theirs: Vec<f64> = text
            .trim()
            .trim_matches(['[', ']'])
            .split_whitespace()
            .map(|n| n.parse().unwrap_or_else(|_| panic!("gs said {text:?}")))
            .collect();
        assert_eq!(theirs.len(), 4, "gs said {text:?}");
        for (a, b) in ours.iter().zip(&theirs) {
            assert!((a - b).abs() < 0.1, "{src}: pscat {ours:?}, gs {theirs:?}");
        }
    }
}

// Codex review, round 1 on PR #1203: one test per finding, with its cases.

/// The numbers `pathbbox` leaves on the stack.
fn bbox_of(src: &str) -> Vec<f64> {
    interp(&format!("{src} pathbbox"), |_| {})
        .operand_stack()
        .iter()
        .map(|o| o.repr().parse().expect("number"))
        .collect()
}

fn assert_bbox(src: &str, got: &[f64], want: [f64; 4], tol: f64) {
    assert_eq!(got.len(), 4, "{src}: {got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!(
            (g - w).abs() <= tol,
            "{src}: pathbbox {got:?}, expected {want:?}"
        );
    }
}

#[test]
fn the_thin_stroke_floor_is_a_device_width_not_a_pen_space_one() {
    // `[100 0 0 0.01]`, width 0.01: the pen is 1 px across a vertical line
    // and 0.0001 px across a horizontal one. The 0.1 floor used to apply in
    // pen space, where L's x stretch of 100 made it 10 px.
    let src = "[100 0 0 0.01 0 0] concat newpath 0 0 moveto 0 100 lineto 0.01 setlinewidth \
               strokepath";
    assert_bbox(src, &bbox_of(src), [-0.005, 0.0, 0.005, 100.0], 1e-4);
    let px = render_pscat(
        "[100 0 0 0.01 0 0] concat 0.01 setlinewidth 0.4 2000 moveto 0.4 8000 lineto stroke",
    );
    // Device x = 40, y 20..80: PNG row 150 is device y 49..50.
    assert_widths("vertical line", &widths(&px, Probe::Row(150)), &[1.0], 0.05);
}

#[test]
fn a_sub_pixel_pen_keeps_its_anisotropy() {
    // A 0.4 pen under [2 0 0 0.5] is 0.8 px across vertical lines and 0.2 px
    // across horizontal ones. tiny-skia's thin-stroke shortcut used to draw
    // both as one averaged hairline (about 0.5 px of ink each way). On and
    // off the pixel grid, the ink is the width.
    for (x, y) in [(10.0, 40.0), (10.17, 40.3)] {
        let src = format!(
            "[2 0 0 0.5 0 0] concat 0.4 setlinewidth {x} {y} moveto {} {y} lineto \
             {} {} lineto {x} {} lineto closepath stroke",
            x + 60.0,
            x + 60.0,
            y + 240.0,
            y + 240.0
        );
        let px = render_pscat(&src);
        assert_widths(
            &format!("{src}, row"),
            &widths(&px, MID_ROW),
            &[0.8, 0.8],
            0.05,
        );
        assert_widths(
            &format!("{src}, column"),
            &widths(&px, MID_COL),
            &[0.2, 0.2],
            0.05,
        );
    }
}

/// A 4.5 px wide vertical line at device x = 103.4 under a CTM that
/// stretches pen space 900 times horizontally: pen x is 0.114889, which two
/// decimals used to write as 0.11, landing at x = 99.
const PRECISE_PEN: &str = "[900 0 0 0.0011111111 0 0] concat 0.005 setlinewidth \
     0.1148889 9000 moveto 0.1148889 171000 lineto stroke";

/// The first `x y m` (PDF) or `Mx y` (SVG) x coordinate in `text`.
fn first_x(text: &str, pdf: bool) -> f64 {
    let n = if pdf {
        let end = text.find(" m ").expect("a moveto");
        let line = &text[..end];
        line.rsplit(' ').nth(1).expect("x y m").to_string()
    } else {
        let start = text.find(" d=\"M").expect("path data") + 5;
        text[start..].split(' ').next().expect("x").to_string()
    };
    n.parse().unwrap_or_else(|_| panic!("not a number: {n:?}"))
}

#[test]
fn pdf_and_svg_write_pen_space_coordinates_precisely_enough_for_the_pen_map() {
    let it = interp(PRECISE_PEN, |it| it.gfx_mut().enable_pdf());
    let pdf = it.gfx().pdf_document().expect("recording on");
    let text = common::pdf_text(&pdf);
    let stroke = text
        .lines()
        .find(|l| l.ends_with(" S"))
        .unwrap_or_else(|| panic!("no stroke: {text}"));
    assert!(stroke.contains("900 0 0 -0.0011"), "{stroke}");
    let x = first_x(stroke, true) * 900.0;
    assert!((x - 103.4).abs() < 0.05, "PDF x lands at {x}: {stroke}");

    let it = interp(PRECISE_PEN, |it| it.gfx_mut().enable_svg());
    let svg = &it.gfx().svg_pages().expect("recording on")[0];
    let at = svg.find("<path transform").expect("pen-space path");
    let x = first_x(&svg[at..], false) * 900.0;
    assert!((x - 103.4).abs() < 0.05, "SVG x lands at {x}");

    if !common::gs_available() {
        return;
    }
    // gs reads the PDF back with the line where the raster has it.
    let path = scratch("precise.pdf");
    std::fs::write(&path, &pdf).unwrap();
    let theirs = gs_png(&path, "precise-pdf");
    let ours = render_pscat(PRECISE_PEN);
    let centre = |px: &Pixmap| {
        let (mut sum, mut total) = (0.0, 0.0);
        for x in 0..SIZE {
            let v = ink(px, x, SIZE / 2);
            sum += v * (f64::from(x) + 0.5);
            total += v;
        }
        sum / total
    };
    let (a, b) = (centre(&ours), centre(&theirs));
    assert!((a - 103.4).abs() < 0.1, "pscat's line is centred at {a}");
    assert!((b - 103.4).abs() < 0.3, "gs draws the PDF's line at {b}");
}

#[test]
fn the_round_pen_shortcut_is_bounded_by_the_stroke_size() {
    // 0.15% out of round: a 4000-unit pen is 3 px off per edge if drawn
    // round (±1997.0 in user space), so it takes pen space: ±2000, as gs.
    let src = "[1.0015 0 0 0.9985022466 0 0] concat newpath 0 -10 moveto 0 10 lineto \
               4000 setlinewidth strokepath";
    let got = bbox_of(src);
    assert_eq!(got.len(), 4, "{src}: {got:?}");
    assert!(
        (got[0] + 2000.0).abs() < 0.01 && (got[2] - 2000.0).abs() < 0.01,
        "{src}: pathbbox {got:?}, expected x ±2000"
    );
    // The same CTM with an ordinary pen stays on the round-pen path: under
    // 1/16 px from the true one, and byte-identical to before.
    let thin = render_pscat(
        "[1.0015 0 0 0.9985022466 0 0] concat 4 setlinewidth 40 40 120 120 rectstroke",
    );
    let round = render_pscat("4 setlinewidth 40.06 39.94 120.18 119.82 rectstroke");
    assert_eq!(
        widths(&thin, MID_ROW),
        widths(&round, MID_ROW),
        "a 4-unit pen 0.15% out of round should keep the round shortcut"
    );
}

#[test]
fn pdf_dashes_are_pen_space_lengths_under_the_cm() {
    if !common::gs_available() {
        return;
    }
    // GS_CASES' dashes, with the lines kept apart (gs adjusts strokes it
    // reads from a PDF, which merges runs where they cross): 20 on, 10 off
    // along the stretched horizontal line, 5 on, 2.5 off along the
    // squashed vertical one, once gs reads the dash array under the `cm`.
    let src = "[2 0 0 0.5 0 0] concat 4 setlinewidth [10 5] 0 setdash \
               10 100 moveto 60 100 lineto 75 40 moveto 75 360 lineto stroke";
    let it = interp(src, |it| it.gfx_mut().enable_pdf());
    let pdf = it.gfx().pdf_document().expect("recording on");
    let path = scratch("dash.pdf");
    std::fs::write(&path, &pdf).unwrap();
    let theirs = gs_png(&path, "dash-pdf");
    let ours = it.gfx().pixmap.clone();
    for probe in [Probe::Row(149), Probe::Col(150)] {
        let (a, b) = (runs(&ours, probe), runs(&theirs, probe));
        assert_eq!(
            a.len(),
            b.len(),
            "{probe:?}: pscat {a:.2?}, gs on the PDF {b:.2?}"
        );
        for ((sa, wa), (sb, wb)) in a.iter().zip(&b) {
            assert!(
                sa.abs_diff(*sb) <= 1 && (wa - wb).abs() <= 0.6,
                "{probe:?}: pscat {a:.2?}, gs on the PDF {b:.2?}"
            );
        }
    }
}

// Codex review, round 2 on PR #1203: one test per finding, with its cases.

/// Total ink on the page.
fn total_ink(px: &Pixmap) -> f64 {
    let mut sum = 0.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            sum += ink(px, x, y);
        }
    }
    sum
}

#[test]
fn sub_pixel_hairlines_cap_only_the_true_ends() {
    // 8000 collinear 0.01-unit linetos, round caps: one 160 px line 0.2 px
    // thick (32 px of ink; gs 31.5). Capping every segment gave 310, and a
    // run of very short segments under-covers in tiny-skia's hairline.
    let many = "[2 0 0 .5 0 0] concat .4 setlinewidth 1 setlinecap \
                10 200 moveto 10.01 .01 90 {200 lineto} for stroke";
    let one = "[2 0 0 .5 0 0] concat .4 setlinewidth 1 setlinecap \
               10 200 moveto 90 200 lineto stroke";
    let (a, b) = (
        total_ink(&render_pscat(many)),
        total_ink(&render_pscat(one)),
    );
    assert!(
        (a - b).abs() < 0.5,
        "8000 segments: {a:.2} of ink, one segment: {b:.2}"
    );
    assert!((b - 32.0).abs() < 1.0, "one segment: {b:.2}");
    let px = render_pscat(many);
    for y in [99, 100] {
        let v = ink(&px, 100, y);
        assert!(
            (0.05..=0.15).contains(&v),
            "interior pixel (100, {y}) has ink {v:.3}"
        );
    }
    if common::gs_available() {
        let g = total_ink(&render_gs(many, "many-segments"));
        // gs fattens thin lines; it is the order of magnitude that matters.
        assert!(a < 2.0 * g, "pscat {a:.2}, gs {g:.2}");
    }
    // A closed subpath has no caps: round and butt caps paint the same.
    let circle = |cap: u8| {
        render_pscat(&format!(
            "[2 0 0 .5 0 0] concat .4 setlinewidth {cap} setlinecap 50 200 30 0 360 arc closepath stroke"
        ))
    };
    assert_eq!(
        circle(0).data(),
        circle(1).data(),
        "a closed curve got caps"
    );
    // An open curve gets exactly two: a little more ink than butt caps.
    let arc = |cap: u8| {
        total_ink(&render_pscat(&format!(
            "[2 0 0 .5 0 0] concat .4 setlinewidth {cap} setlinecap 50 200 30 0 270 arc stroke"
        )))
    };
    let (butt, round) = (arc(0), arc(1));
    assert!(
        round > butt && round - butt < 0.5,
        "butt {butt:.2}, round {round:.2}"
    );
}

#[test]
fn thin_pen_space_widths_and_dashes_are_never_written_as_zero() {
    // Codex's line, plus a dashed horizontal one: its dashes are
    // 0.0003 and 0.0002 pen units, 0.27 and 0.18 px once stretched.
    let src = "[900 0 0 .0011111111 0 0] concat .0004 setlinewidth \
               .1 18000 moveto .1 162000 lineto stroke \
               [.0003 .0002] 0 setdash .1 90000 moveto .2 90000 lineto stroke";
    let it = interp(src, |it| it.gfx_mut().enable_svg());
    let svg = &it.gfx().svg_pages().expect("recording on")[0];
    assert!(svg.contains("stroke-width=\"0.0004\""), "{svg}");
    assert!(svg.contains("stroke-dasharray=\"0.0003 0.0002\""), "{svg}");
    let it = interp(src, |it| it.gfx_mut().enable_pdf());
    let text = common::pdf_text(&it.gfx().pdf_document().expect("recording on"));
    let strokes: Vec<&str> = text.lines().filter(|l| l.ends_with(" S")).collect();
    assert_eq!(strokes.len(), 2, "{text}");
    assert!(
        strokes.iter().all(|l| l.contains(" 0.0004 w ")),
        "{strokes:?}"
    );
    assert!(strokes[1].contains("[0.0003 0.0002] 0 d "), "{strokes:?}");
}

#[test]
fn moveto_closepath_with_round_caps_is_a_dot_in_every_format() {
    let src = "[2 0 0 .5 0 0] concat .4 setlinewidth 1 setlinecap 50 200 moveto closepath stroke";
    let ours = total_ink(&render_pscat(src));
    // A 0.4 pen's dot under det 1: about π·0.2² of area, drawn as a
    // modulated hairline dot like any thin round-capped dot.
    assert!(ours > 0.05, "no dot: {ours:.3}");
    if common::gs_available() {
        let theirs = total_ink(&render_gs(src, "closepath-dot"));
        assert!(
            (ours - theirs).abs() < 0.2,
            "pscat {ours:.3}, gs {theirs:.3}"
        );
    }
    let it = interp(src, |it| it.gfx_mut().enable_svg());
    let svg = &it.gfx().svg_pages().expect("recording on")[0];
    assert!(
        svg.contains("<path transform") && svg.contains("Z\" fill=\"none\""),
        "{svg}"
    );
    let it = interp(src, |it| it.gfx_mut().enable_pdf());
    let text = common::pdf_text(&it.gfx().pdf_document().expect("recording on"));
    assert!(
        text.lines()
            .any(|l| l.contains(" 1 J ") && l.ends_with(" h S")),
        "{text}"
    );
}

#[test]
fn a_miter_tip_keeps_its_user_space_place_under_a_slight_stretch() {
    // 1% out of round is 0.05 px at a 10-wide pen's edge, but a 13.45:1
    // miter tip is 0.66 px off with the round pen. The tip of this join is
    // at user x = 167.03 (5/sin(4.27°) along the bisector), whatever the
    // CTM. (gs puts it at 167.27 even untransformed, and 167.35 here.)
    let path = "10 setlinewidth 20 setmiterlimit newpath 0 0 moveto 100 0 lineto 0 15 lineto \
                strokepath";
    let plain = bbox_of(path);
    let stretched = bbox_of(&format!("[1.01 0 0 .9900990099 0 0] concat {path}"));
    assert!(
        (plain[2] - 167.03).abs() < 0.02,
        "untransformed tip {plain:?}"
    );
    assert_bbox(
        "stretched",
        &stretched,
        [plain[0], plain[1], plain[2], plain[3]],
        0.02,
    );
}

// Codex confirmation round on PR #1203.

#[test]
fn a_straightened_zigzag_takes_its_own_width() {
    // 10,240 steps of 1/64 in x with y alternating 100 ± 1/128: a 320 px
    // device line about 0.216 px thick (the 0.2 px pen plus the zigzag's
    // 1/256 px swing each way), so about 69 px of ink. Merging the zigzag
    // but keeping its diagonals' 0.583 px width gave 92.2.
    let src = "[2 0 0 .5 0 0] concat .4 setlinewidth 1 setlinejoin 0 setlinecap 20 100 moveto \
               1 1 10240 { /i exch def i 64 div 20 add \
               i 2 mod 0 eq { 100 1 128 div sub } { 100 1 128 div add } ifelse lineto } for stroke";
    let mut it = Interp::with_page(400, SIZE).expect("page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{src:?}: {}", it.error_report(&e)));
    let px = it.gfx().pixmap.clone();
    let mut sum = 0.0;
    for y in 0..SIZE {
        for x in 0..400 {
            let p = px.pixel(x, y).expect("in page");
            sum += 1.0
                - f64::from(u32::from(p.red()) + u32::from(p.green()) + u32::from(p.blue()))
                    / 765.0;
        }
    }
    assert!((60.0..=72.0).contains(&sum), "zigzag ink {sum:.2}");
}

#[test]
fn a_zero_length_subpath_under_square_caps_paints_nothing_as_in_gs() {
    for sub in ["moveto closepath", "moveto 50 200 lineto"] {
        let src = format!("[2 0 0 .5 0 0] concat .4 setlinewidth 2 setlinecap 50 200 {sub} stroke");
        let ours = total_ink(&render_pscat(&src));
        assert!(ours < 1e-9, "{sub}: {ours:.3} of ink");
        if common::gs_available() {
            let theirs = total_ink(&render_gs(&src, "square-dot"));
            assert!(theirs < 1e-9, "{sub}: gs painted {theirs:.3}");
        }
        let it = interp(&src, |it| it.gfx_mut().enable_svg());
        let svg = &it.gfx().svg_pages().expect("recording on")[0];
        assert!(!svg.contains("<path"), "{sub}: {svg}");
        let it = interp(&src, |it| it.gfx_mut().enable_pdf());
        let text = common::pdf_text(&it.gfx().pdf_document().expect("recording on"));
        assert!(!text.lines().any(|l| l.ends_with(" S")), "{sub}: {text}");
    }
    // A round cap still makes the dot.
    let dot = total_ink(&render_pscat(
        "[2 0 0 .5 0 0] concat .4 setlinewidth 1 setlinecap 50 200 moveto closepath stroke",
    ));
    assert!(dot > 0.05, "round dot {dot:.3}");
}

// #1215: the same zero-length rule under an ordinary (similarity) CTM, where
// the stroke is tiny-skia's own on the device path rather than pen space.

/// The CTMs of a similarity: none, a uniform scale, a rotation.
const ORDINARY_CTMS: [&str; 3] = ["", "2 2 scale", "30 rotate"];

/// The four spellings of a zero-length subpath, at (25, 25).
const POINT_SUBPATHS: [&str; 4] = [
    "moveto closepath",
    "moveto 25 25 lineto",
    "moveto 25 25 lineto closepath",
    "moveto 0 0 rlineto",
];

fn svg_of(src: &str) -> String {
    let it = interp(src, |it| it.gfx_mut().enable_svg());
    it.gfx().svg_pages().expect("recording on")[0].clone()
}

fn pdf_of(src: &str) -> String {
    let it = interp(src, |it| it.gfx_mut().enable_pdf());
    common::pdf_text(&it.gfx().pdf_document().expect("recording on"))
}

/// Stroke operators in a PDF page's text.
fn pdf_strokes(text: &str) -> Vec<&str> {
    text.lines().filter(|l| l.ends_with(" S")).collect()
}

#[test]
fn a_zero_length_subpath_under_square_caps_paints_nothing_under_an_ordinary_ctm() {
    for ctm in ORDINARY_CTMS {
        for width in ["0", ".4", "4"] {
            for sub in POINT_SUBPATHS {
                let src = format!("{ctm} {width} setlinewidth 2 setlinecap 25 25 {sub} stroke");
                let ours = total_ink(&render_pscat(&src));
                assert!(ours < 1e-9, "{src}: {ours:.3} of ink");
                if common::gs_available() {
                    let theirs = total_ink(&render_gs(&src, "square-dot-ordinary"));
                    assert!(theirs < 1e-9, "{src}: gs painted {theirs:.3}");
                }
                let svg = svg_of(&src);
                assert!(!svg.contains("<path"), "{src}: {svg}");
                let pdf = pdf_of(&src);
                assert!(pdf_strokes(&pdf).is_empty(), "{src}: {pdf}");
            }
        }
    }
}

#[test]
fn a_zero_length_subpath_under_butt_caps_paints_nothing_under_an_ordinary_ctm() {
    for ctm in ORDINARY_CTMS {
        for sub in POINT_SUBPATHS {
            let src = format!("{ctm} 4 setlinewidth 0 setlinecap 25 25 {sub} stroke");
            let ours = total_ink(&render_pscat(&src));
            assert!(ours < 1e-9, "{src}: {ours:.3} of ink");
            if common::gs_available() {
                let theirs = total_ink(&render_gs(&src, "butt-dot-ordinary"));
                assert!(theirs < 1e-9, "{src}: gs painted {theirs:.3}");
            }
        }
    }
}

#[test]
fn a_round_cap_dot_stays_under_an_ordinary_ctm() {
    for ctm in ORDINARY_CTMS {
        for sub in POINT_SUBPATHS {
            // 4 wide: a disc, about pi * 2^2 of ink (scaled by the CTM).
            // 0 wide: a one-pixel hairline dot. gs's is fainter than
            // pscat's (0.27 against 0.79 of ink), a separate difference, so
            // only its presence is pinned.
            for width in ["0", "4"] {
                let src = format!("{ctm} {width} setlinewidth 1 setlinecap 25 25 {sub} stroke");
                let ours = total_ink(&render_pscat(&src));
                assert!(ours > 0.05, "{src}: no dot ({ours:.3})");
                if common::gs_available() {
                    let theirs = total_ink(&render_gs(&src, "round-dot-ordinary"));
                    assert!(theirs > 0.05, "{src}: gs has no dot ({theirs:.3})");
                    if width != "0" {
                        assert!(
                            (ours - theirs).abs() <= 0.25 * theirs,
                            "{src}: pscat {ours:.3}, gs {theirs:.3}"
                        );
                    }
                }
                let svg = svg_of(&src);
                assert!(svg.contains("<path"), "{src}: {svg}");
                let pdf = pdf_of(&src);
                assert_eq!(pdf_strokes(&pdf).len(), 1, "{src}: {pdf}");
            }
        }
    }
}

#[test]
fn only_the_zero_length_subpaths_of_a_mixed_path_are_left_out() {
    // Two dots and a real line in one stroke: the line keeps its square
    // caps, the dots go from the raster, the SVG `d` data and the PDF path.
    let src = "4 setlinewidth 2 setlinecap 25 25 moveto closepath 75 75 moveto 75 75 lineto \
               30 60 moveto 60 60 lineto stroke";
    let ours = total_ink(&render_pscat(src));
    // The line is 30 long, plus 2 of cap each end: 34 x 4, sampled to 4x4.
    assert!((ours - 136.0).abs() < 2.0, "pscat {ours:.3}");
    let pdf = pdf_of(src);
    if common::gs_available() {
        let theirs = total_ink(&render_gs(src, "square-mixed"));
        assert!(
            (ours - theirs).abs() < 1.0,
            "pscat {ours:.3}, gs {theirs:.3}"
        );
        // The PDF read back by gs paints what the line alone does (the
        // PDF interpreter has its own stroke adjustment, so not what the
        // PostScript run above does).
        let alone = "4 setlinewidth 2 setlinecap 30 60 moveto 60 60 lineto stroke";
        let mut inks = [0.0; 2];
        for (i, program) in [src, alone].into_iter().enumerate() {
            let file = scratch("square-mixed.pdf");
            let it = interp(program, |it| it.gfx_mut().enable_pdf());
            std::fs::write(&file, it.gfx().pdf_document().expect("recording on")).unwrap();
            inks[i] = total_ink(&gs_png(&file, "square-mixed-pdf"));
        }
        assert!(
            (inks[0] - inks[1]).abs() < 0.5,
            "gs on the PDF: {:.3} with the dots, {:.3} without",
            inks[0],
            inks[1]
        );
    }
    let svg = svg_of(src);
    assert_eq!(svg.matches("<path").count(), 1, "{svg}");
    // 25 25 and 75 75 are the dots; y is flipped, so 25 is 175 on the page.
    assert!(
        !svg.contains("M25 175") && !svg.contains("M75 125"),
        "dot in the SVG: {svg}"
    );
    assert!(svg.contains("M30 140"), "line missing from the SVG: {svg}");
    let strokes = pdf_strokes(&pdf);
    assert_eq!(strokes.len(), 1, "{pdf}");
    assert_eq!(strokes[0].matches(" m ").count(), 1, "{pdf}");
    assert!(strokes[0].contains("30 140 m"), "{pdf}");
}

/// What `strokepath` leaves of `src`: the segments `pathforall` counts.
fn outline_elements(src: &str) -> i64 {
    let probe =
        "0 {pop pop 1 add} {pop pop 1 add} {pop pop pop pop pop pop 1 add} {1 add} pathforall";
    let it = interp(&format!("{src} strokepath {probe}"), |_| {});
    it.operand_stack()
        .last()
        .and_then(|o| o.repr().parse().ok())
        .expect("count")
}

#[test]
fn strokepath_of_a_zero_length_subpath_follows_the_cap_as_in_gs() {
    for (cap, want_ink) in [(0, false), (1, true), (2, false)] {
        for sub in POINT_SUBPATHS {
            let src = format!("4 setlinewidth {cap} setlinecap newpath 25 25 {sub}");
            let ours = outline_elements(&src);
            assert_eq!(ours > 0, want_ink, "{src}: pscat counts {ours}");
            if common::gs_available() {
                let ps = scratch("strokepath-dot.ps");
                std::fs::write(
                    &ps,
                    format!(
                        "{src} strokepath 0 {{pop pop 1 add}} {{pop pop 1 add}} \
                         {{pop pop pop pop pop pop 1 add}} {{1 add}} pathforall ==\n"
                    ),
                )
                .unwrap();
                let out = Command::new("gs")
                    .args(["-q", "-dNOPAUSE", "-dBATCH", "-dNODISPLAY"])
                    .arg(&ps)
                    .output_deadline()
                    .expect("run gs");
                let text = String::from_utf8_lossy(&out.stdout).into_owned();
                let theirs: i64 = text
                    .trim()
                    .parse()
                    .unwrap_or_else(|_| panic!("gs: {text:?}"));
                assert_eq!(theirs > 0, want_ink, "{src}: gs counts {theirs}");
            }
        }
    }
}

#[test]
fn zero_length_dashes_of_a_real_subpath_still_draw_under_square_caps() {
    // The rule is about zero-length *subpaths*. A `[0 3]` dash pattern on a
    // line cuts zero-length dashes, which both rasterisers cap (gs: 7 squares
    // on 20 units), and a dashed point subpath stays empty.
    let line = "4 setlinewidth 2 setlinecap [0 6] 0 setdash 25 25 moveto 65 25 lineto stroke";
    let ours = total_ink(&render_pscat(line));
    assert!(ours > 10.0, "zero-length dashes vanished: {ours:.3}");
    if common::gs_available() {
        let theirs = total_ink(&render_gs(line, "square-dashes"));
        assert!(
            (ours - theirs).abs() <= 0.1 * theirs,
            "pscat {ours:.3}, gs {theirs:.3}"
        );
    }
    let point = "4 setlinewidth 2 setlinecap [0 6] 0 setdash 25 25 moveto closepath stroke";
    assert!(total_ink(&render_pscat(point)) < 1e-9);
    if common::gs_available() {
        assert!(total_ink(&render_gs(point, "square-dashed-point")) < 1e-9);
    }
}
