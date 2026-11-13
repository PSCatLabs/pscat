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
