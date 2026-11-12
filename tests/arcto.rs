//! `arcto` and `arct` (#980), PLRM 3rd ed. §8.2: the tangent points, the
//! degenerate cases, the errors, the three exports, and a case-by-case
//! comparison with Ghostscript (the comparison is skipped without `gs`).
//!
//! The numbers pinned below were read off Ghostscript 10.07.1.

use std::process::Command;

mod common;

use common::{CommandDeadline as _, pdf_text};
use pscat::Interp;

/// Plain PostScript, valid in pscat and in gs: `case` runs a proc under
/// `stopped` on a fresh path and prints `name => ok|errorname`, the
/// stack and the path, numbers rounded to 0.01.
const PRELUDE: &str = "\
/r2 { 100 mul round 100 div } def \
/out { dup type /stringtype eq { print } { 64 string cvs print } ifelse } def \
/num { dup type dup /integertype eq { pop out } { /realtype eq { r2 } if out } ifelse ( ) print } def \
/dumppath { { (m ) print 2 array astore { num } forall } \
            { (l ) print 2 array astore { num } forall } \
            { (c ) print 6 array astore { num } forall } \
            { (z ) print } pathforall } def \
/dumpstack { count array astore { num } forall } def \
/case { exch out ( => ) print newpath \
  stopped { $error /errorname get out ( stack: ) print } { (ok stack: ) print } ifelse \
  dumpstack clear (path: ) print { dumppath } stopped pop (\\n) print newpath } def \
";

/// Run `body` (a PostScript program using `case`) and return what it printed.
fn run_cases(body: &str) -> String {
    let mut it = Interp::new();
    it.capture_stdout();
    let src = format!("{PRELUDE}{body}");
    it.run_str(&src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    String::from_utf8_lossy(it.captured_stdout()).into_owned()
}

/// One case's line, e.g. `run_case("{ 0 0 moveto 100 0 100 100 10 arcto }")`.
fn run_case(proc_: &str) -> String {
    run_cases(&format!("(c) {proc_} case"))
        .trim_end()
        .to_string()
}

/// Operand stack of a program as `==` reprs (a program that errors fails
/// the test; use `run_case` to see an error).
fn stack_of(src: &str) -> Vec<String> {
    let mut it = Interp::new();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{src:?} failed: {}", it.error_report(&e)));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

const NORMAL: &str = "{ 0 0 moveto 100 0 100 100 10 arcto }";

#[test]
fn right_angle_corner_matches_gs() {
    assert_eq!(
        run_case(NORMAL),
        "c => ok stack: 90.0 0.0 100.0 10.0 path: m 0.0 0.0 l 90.0 0.0 \
         c 95.52 0.0 100.0 4.48 100.0 10.0"
    );
}

#[test]
fn arct_is_arcto_without_the_tangent_points() {
    let arct = run_case("{ 0 0 moveto 100 0 100 100 10 arct }");
    assert_eq!(
        arct,
        "c => ok stack: path: m 0.0 0.0 l 90.0 0.0 c 95.52 0.0 100.0 4.48 100.0 10.0"
    );
    assert_eq!(stack_of("0 0 moveto 100 0 100 100 10 arct count"), ["0"]);
    assert_eq!(stack_of("0 0 moveto 100 0 100 100 10 arcto count").len(), 5);
}

#[test]
fn the_current_point_ends_at_the_second_tangent_point() {
    let s = stack_of("0 0 moveto 100 0 100 100 10 arcto currentpoint");
    let n: Vec<f64> = s.iter().map(|v| v.parse().unwrap()).collect();
    assert_eq!(n.len(), 6);
    assert!(
        (n[4] - n[2]).abs() < 1e-4 && (n[5] - n[3]).abs() < 1e-4,
        "{n:?}"
    );
}

#[test]
fn obtuse_acute_and_clockwise_corners_match_gs() {
    // Sweeps of 45, 174 and (mirrored) 90 degrees: one cubic each, as gs.
    assert_eq!(
        run_case("{ 0 0 moveto 100 0 150 50 10 arcto }"),
        "c => ok stack: 95.86 0.0 102.93 2.93 path: m 0.0 0.0 l 95.86 0.0 \
         c 98.51 0.0 101.05 1.05 102.93 2.93"
    );
    // The acute corner's last control-point x is 87.815: either rounding
    // of that tie is within gs's float noise, so pin the coarse prefix.
    let acute = run_case("{ 0 0 moveto 100 0 0 10 10 arcto }");
    assert!(
        acute.starts_with(
            "c => ok stack: -100.5 0.0 -99.5 19.95 path: m 0.0 0.0 l -100.5 0.0 \
             c -87.8"
        ),
        "{acute}"
    );
    assert_eq!(
        run_case("{ 0 0 moveto 100 0 100 -100 10 arcto }"),
        "c => ok stack: 90.0 0.0 100.0 -10.0 path: m 0.0 0.0 l 90.0 0.0 \
         c 95.52 0.0 100.0 -4.48 100.0 -10.0"
    );
}

#[test]
fn collinear_points_draw_a_line_to_the_corner() {
    // Straight on, and folded back on itself: both are a line to (x1, y1)
    // and both tangent points are (x1, y1).
    for p2 in ["200 0", "-50 0", "0 0"] {
        let want = "c => ok stack: 100.0 0.0 100.0 0.0 path: m 0.0 0.0 l 100.0 0.0";
        assert_eq!(
            run_case(&format!("{{ 0 0 moveto 100 0 {p2} 10 arcto }}")),
            want,
            "{p2}"
        );
    }
    assert_eq!(
        run_case("{ 0 0 moveto 100 0 200 0 10 arct }"),
        "c => ok stack: path: m 0.0 0.0 l 100.0 0.0"
    );
    // A rotated frame puts the current point through single-precision
    // device space: still collinear, still a line.
    assert_eq!(
        run_case("{ 30 rotate 0 0 moveto 100 0 -50 0 10 arcto }"),
        "c => ok stack: 100.0 0.0 100.0 0.0 path: m 0.0 0.0 l 100.0 0.0"
    );
}

#[test]
fn a_distant_direction_point_keeps_the_rounded_corner() {
    // p2 only gives the second leg's direction: moving it far along the
    // same ray must not turn the corner into a collinear line (Codex,
    // combined review batch 1). gs agrees: see the comparison below.
    for p2 in ["100 100", "100 1000000", "100 1000000000"] {
        assert_eq!(
            run_case(&format!("{{ 0 0 moveto 100 0 {p2} 10 arcto }}")),
            "c => ok stack: 90.0 0.0 100.0 10.0 path: m 0.0 0.0 l 90.0 0.0 \
             c 95.52 0.0 100.0 4.48 100.0 10.0",
            "{p2}"
        );
    }
}

#[test]
fn a_magnified_corner_is_still_a_corner() {
    // Tiny user coordinates under a huge CTM are the same 100-point
    // corner as the unscaled one: the collinearity tolerance lives in
    // device space (Codex, combined review batch 1, round 2). gs 10.07.1
    // also draws the curve (9e-08 0 1e-07 1e-08). Not in the shared gs
    // table: `case` does not save the graphics state, so the scale would
    // leak into the cases after it.
    let scaled = run_case(
        "{ 1000000000 1000000000 scale 0 0 moveto 0.0000001 0 0.0000001 0.0000001 0.00000001 arcto }",
    );
    assert!(scaled.contains(" c "), "{scaled}");
    let shrunk = run_case(
        "{ 0.000001 0.000001 scale 0 0 moveto 100000000 0 100000000 100000000 10000000 arcto }",
    );
    assert!(shrunk.contains(" c "), "{shrunk}");
    // A fold-back is still a plain line under the magnifying CTM.
    let line = run_case(
        "{ 1000000000 1000000000 scale 30 rotate 0 0 moveto 0.0000001 0 -0.00000005 0 0.00000001 arcto }",
    );
    assert!(line.contains(" l ") && !line.contains(" c "), "{line}");
}

#[test]
fn zero_radius_is_a_line_and_a_zero_length_curve_as_in_gs() {
    let want = "c => ok stack: 100.0 0.0 100.0 0.0 path: m 0.0 0.0 l 100.0 0.0 \
                c 100.0 0.0 100.0 0.0 100.0 0.0";
    assert_eq!(run_case("{ 0 0 moveto 100 0 100 100 0 arcto }"), want);
    assert_eq!(
        run_case("{ 0 0 moveto 100 0 100 100 0 arct }"),
        "c => ok stack: path: m 0.0 0.0 l 100.0 0.0 c 100.0 0.0 100.0 0.0 100.0 0.0"
    );
}

#[test]
fn negative_radius_mirrors_the_arc_through_the_corner_as_in_gs() {
    assert_eq!(
        run_case("{ 0 0 moveto 100 0 100 100 -10 arcto }"),
        "c => ok stack: 110.0 0.0 100.0 -10.0 path: m 0.0 0.0 l 110.0 0.0 \
         c 104.48 0.0 100.0 -4.48 100.0 -10.0"
    );
}

#[test]
fn a_radius_larger_than_the_legs_is_still_geometry() {
    assert_eq!(
        run_case("{ 0 0 moveto 100 0 100 100 1000 arcto }"),
        "c => ok stack: -900.0 0.0 100.0 1000.0 path: m 0.0 0.0 l -900.0 0.0 \
         c -347.72 0.0 100.0 447.72 100.0 1000.0"
    );
}

#[test]
fn a_non_uniform_ctm_bends_the_circle_into_an_ellipse() {
    // Tangent points are in user space; the cubic goes through the CTM.
    assert_eq!(
        run_case("{ 2 0.5 scale 0 0 moveto 50 0 50 100 10 arcto }"),
        "c => ok stack: 40.0 0.0 50.0 10.0 path: m 0.0 0.0 l 40.0 0.0 \
         c 45.52 0.0 50.0 4.48 50.0 10.0"
    );
}

#[test]
fn coincident_points_are_undefinedresult_and_leave_the_path_alone() {
    for (args, name) in [("0 0 100 100 10", "P0 = P1"), ("100 0 100 0 10", "P1 = P2")] {
        let got = run_case(&format!("{{ 0 0 moveto {args} arcto }}"));
        // pscat consumes the operands of a failed operator (HANDOFF.md,
        // "error-time operand restoration"); gs leaves them. The path is
        // untouched in both.
        assert_eq!(got, "c => undefinedresult stack: path: m 0.0 0.0", "{name}");
    }
}

#[test]
fn a_coincident_corner_is_undefinedresult_under_a_rotation() {
    // The current point comes back through the inverse CTM, so p0 = p1
    // has to be decided in device space (Codex, combined review batch 1,
    // round 3). gs 10.07.1 raises undefinedresult for both.
    for op in ["arcto", "arct"] {
        let got = run_case(&format!(
            "{{ 30 rotate 100 100 moveto 100 100 200 0 10 {op} }}"
        ));
        assert!(
            got.starts_with("c => undefinedresult") && got.ends_with("path: m 100.0 100.0"),
            "{op}: {got}"
        );
    }
}

#[test]
fn errors_are_the_ones_gs_raises() {
    let cases = [
        (
            "no current point",
            "{ 100 0 100 100 10 arcto }",
            "nocurrentpoint",
        ),
        (
            "no current point, arct",
            "{ 100 0 100 100 10 arct }",
            "nocurrentpoint",
        ),
        (
            "string radius",
            "{ 0 0 moveto 100 0 100 100 (a) arcto }",
            "typecheck",
        ),
        (
            "name x1",
            "{ 0 0 moveto /a 0 100 100 10 arcto }",
            "typecheck",
        ),
        (
            "string, arct",
            "{ 0 0 moveto 100 0 100 (a) 10 arct }",
            "typecheck",
        ),
        (
            "four operands",
            "{ 0 0 moveto 100 0 100 10 arcto }",
            "stackunderflow",
        ),
        ("empty stack", "{ 0 0 moveto arcto }", "stackunderflow"),
        // Operand checks come before the current-point check.
        (
            "typecheck beats nocurrentpoint",
            "{ 100 0 100 (a) 10 arcto }",
            "typecheck",
        ),
        (
            "underflow beats nocurrentpoint",
            "{ 100 0 100 10 arcto }",
            "stackunderflow",
        ),
    ];
    for (what, proc_, err) in cases {
        let got = run_case(proc_);
        assert!(
            got.starts_with(&format!("c => {err} stack: ")),
            "{what}: {got}"
        );
        // Nothing was appended to the path.
        assert!(
            !got.contains(" l ") && !got.contains(" c "),
            "{what}: path changed: {got}"
        );
    }
    // pscat's convention (the same as `lineto`'s): the operands the
    // operator popped before it failed are gone.
    assert_eq!(
        run_case("{ 0 0 moveto 100 0 100 (a) 10 arcto }"),
        "c => typecheck stack: 100 0 100 path: m 0.0 0.0"
    );
}

#[test]
fn arcto_after_closepath_starts_from_the_closed_subpath() {
    // closepath resets the current point to the subpath start.
    assert_eq!(
        run_case("{ 0 0 moveto 10 10 lineto closepath 100 0 100 100 10 arcto }"),
        "c => ok stack: 90.0 0.0 100.0 10.0 path: m 0.0 0.0 l 10.0 10.0 z \
         m 0.0 0.0 l 90.0 0.0 c 95.52 0.0 100.0 4.48 100.0 10.0"
    );
}

// --- exports ---------------------------------------------------------

/// A 100x100 rounded rectangle, 20 radius, drawn with `arcto` the way
/// the PLRM's own example does (start mid-edge, four corners).
const ROUNDRECT: &str = "\
/rrect { /r exch def /h exch def /w exch def /y exch def /x exch def \
  x w 2 div add y moveto \
  x w add y x w add y h add r arcto 4 { pop } repeat \
  x w add y h add x y h add r arcto 4 { pop } repeat \
  x y h add x y r arcto 4 { pop } repeat \
  x y x w add y r arcto 4 { pop } repeat \
  closepath } def \
1 setgray 0 0 200 200 rectfill \
0.1 0.3 0.8 setrgbcolor newpath 40 50 120 100 30 rrect fill \
";

/// The same shape through a non-uniform CTM: the corners become ellipses.
const SCALED_ROUNDRECT: &str = "\
/rrect { /r exch def /h exch def /w exch def /y exch def /x exch def \
  x w 2 div add y moveto \
  x w add y x w add y h add r arcto 4 { pop } repeat \
  x w add y h add x y h add r arcto 4 { pop } repeat \
  x y h add x y r arcto 4 { pop } repeat \
  x y x w add y r arcto 4 { pop } repeat \
  closepath } def \
1 setgray 0 0 200 200 rectfill \
0.8 0.2 0.1 setrgbcolor 30 40 translate 1.5 0.6 scale \
newpath 10 20 100 150 45 rrect fill \
";

fn render(src: &str) -> Interp {
    let mut it = Interp::with_page(200, 200).expect("page");
    it.gfx_mut().enable_svg();
    it.gfx_mut().enable_pdf();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn px(it: &Interp, x: u32, y_up: u32) -> [u8; 3] {
    let pm = &it.gfx().pixmap;
    let p = pm.pixel(x, pm.height() - 1 - y_up).expect("in range");
    [p.red(), p.green(), p.blue()]
}

#[test]
fn rounded_rectangle_png_has_round_corners() {
    let it = render(ROUNDRECT);
    // Edge midpoints and the centre are filled; the sharp corner is not,
    // and the corner is filled again 30 units in along the diagonal arc.
    for (x, y) in [(100, 51), (100, 149), (41, 100), (159, 100), (100, 100)] {
        assert_ne!(px(&it, x, y), [255, 255, 255], "({x},{y}) should be inked");
    }
    for (x, y) in [(41, 51), (159, 51), (41, 149), (159, 149)] {
        assert_eq!(
            px(&it, x, y),
            [255, 255, 255],
            "({x},{y}) is outside the arc"
        );
    }
    // 30 along each axis from the corner is inside; the arc's mid-point
    // sits at 30 - 30/sqrt(2) = 8.8 from each side.
    assert_ne!(px(&it, 40 + 10, 50 + 10), [255, 255, 255]);
    assert_eq!(px(&it, 40 + 6, 50 + 6), [255, 255, 255]);
}

#[test]
fn rounded_rectangle_svg_and_pdf_carry_the_curves() {
    let it = render(ROUNDRECT);
    let svg = it.gfx().svg_pages().expect("svg on").concat();
    assert!(
        svg.contains(
            "<path d=\"M100 150L130 150C146.569 150 160 136.569 160 120L160 80\
             C160 63.431 146.569 50 130 50L70 50C53.431 50 40 63.431 40 80L40 120\
             C40 136.569 53.431 150 70 150Z\" fill=\"#1a4dcc\""
        ),
        "the rounded rectangle's path: {svg}"
    );
    let pdf = pdf_text(&it.gfx().pdf_document().expect("pdf on"));
    // Four cubic segments; `h` closes the page fill and the shape.
    let ops = |op: &str| pdf.split_whitespace().filter(|w| *w == op).count();
    assert_eq!((ops("c"), ops("h")), (4, 2), "four corner curves: {pdf}");
}

// --- Ghostscript -----------------------------------------------------

fn gs_available() -> bool {
    Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn gs_output(body: &str) -> String {
    let dir = common::scratch_dir("arcto_gs");
    let ps = dir.join("cases.ps");
    std::fs::write(&ps, format!("{PRELUDE}{body}")).expect("write ps");
    let out = Command::new("gs")
        .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH"])
        .arg(&ps)
        .output_deadline()
        .expect("run gs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "gs failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The words of a report line, numbers parsed. The operands an erroring
/// operator leaves differ by design (see the error test), so the
/// operand stack of an error case is dropped before comparing.
fn words(line: &str) -> Vec<Result<f64, String>> {
    let mut line = line.to_string();
    if let (false, Some(a), Some(b)) = (
        line.contains("=> ok"),
        line.find("stack:"),
        line.find("path:"),
    ) {
        line.replace_range(a..b, "stack: ");
    }
    line.split_whitespace()
        .map(|w| w.parse::<f64>().map_err(|_| w.to_string()))
        .collect()
}

fn same_report(a: &str, b: &str) -> bool {
    let (wa, wb) = (words(a), words(b));
    wa.len() == wb.len()
        && wa.iter().zip(&wb).all(|(x, y)| match (x, y) {
            (Ok(x), Ok(y)) => (x - y).abs() <= 0.0151,
            (Err(x), Err(y)) => x == y,
            _ => false,
        })
}

#[test]
fn ghostscript_agrees_case_by_case() {
    if !gs_available() {
        eprintln!("skipping the gs comparison: gs is not installed");
        return;
    }
    let cases: &[(&str, &str)] = &[
        ("right angle", "0 0 moveto 100 0 100 100 10 arcto"),
        ("right angle arct", "0 0 moveto 100 0 100 100 10 arct"),
        (
            "distant direction point",
            "0 0 moveto 100 0 100 1000000000 10 arcto",
        ),
        ("clockwise turn", "0 0 moveto 100 0 100 -100 10 arcto"),
        ("obtuse", "0 0 moveto 100 0 150 50 10 arcto"),
        ("acute", "0 0 moveto 100 0 0 10 10 arcto"),
        ("very acute", "0 0 moveto 100 0 -80 3 4 arcto"),
        ("collinear ahead", "0 0 moveto 100 0 200 0 10 arcto"),
        ("collinear fold-back", "0 0 moveto 100 0 -50 0 10 arcto"),
        ("collinear arct", "0 0 moveto 100 0 200 0 10 arct"),
        ("P2 = P0", "0 0 moveto 100 0 0 0 10 arcto"),
        ("zero radius", "0 0 moveto 100 0 100 100 0 arcto"),
        ("negative radius", "0 0 moveto 100 0 100 100 -10 arcto"),
        ("huge radius", "0 0 moveto 100 0 100 100 1000 arcto"),
        (
            "integers and reals",
            "0 0 moveto 100 0.5 99.5 100 10.25 arcto",
        ),
        ("P0 = P1", "0 0 moveto 0 0 100 100 10 arcto"),
        ("P1 = P2", "0 0 moveto 100 0 100 0 10 arcto"),
        ("no current point", "100 0 100 100 10 arcto"),
        ("no current point arct", "100 0 100 100 10 arct"),
        ("typecheck", "0 0 moveto 100 0 100 (a) 10 arcto"),
        ("stackunderflow", "0 0 moveto 100 0 100 10 arcto"),
        (
            "after closepath",
            "0 0 moveto 10 10 lineto closepath 100 0 100 100 10 arcto",
        ),
        (
            "after curveto",
            "0 0 moveto 10 20 30 40 50 60 curveto 90 60 90 0 12 arcto",
        ),
        ("scaled", "2 0.5 scale 0 0 moveto 50 0 50 100 10 arcto"),
        ("rotated", "30 rotate 0 0 moveto 100 0 100 100 10 arcto"),
        // No rotated fold-back here: gs's answer to it depends on its own
        // rounding noise (a line, or limitcheck, by CTM). The pscat side is
        // pinned in `collinear_points_draw_a_line_to_the_corner`.
        (
            "rotated and scaled",
            "30 rotate 3 1.7 scale 0 0 moveto 50 20 80 -30 10 arcto",
        ),
        (
            "sheared",
            "[1 0.5 0 2 10 20] concat 0 0 moveto 30 5 10 60 7 arcto",
        ),
        (
            "translated far",
            "500 700 translate 0.3 rotate 0 0 moveto 100 0 100 100 10 arcto",
        ),
        (
            "two corners",
            "0 0 moveto 100 0 100 100 10 arcto 100 100 0 100 10 arcto",
        ),
    ];
    let body: String = cases
        .iter()
        .map(|(name, code)| format!("({name}) {{ {code} }} case\n"))
        .collect();
    let (ours, theirs) = (run_cases(&body), gs_output(&body));
    let (ours, theirs): (Vec<_>, Vec<_>) = (ours.lines().collect(), theirs.lines().collect());
    assert_eq!(ours.len(), cases.len(), "{ours:?}");
    assert_eq!(theirs.len(), cases.len(), "{theirs:?}");
    for ((name, _), (a, b)) in cases.iter().zip(ours.iter().zip(&theirs)) {
        assert!(same_report(a, b), "{name}:\n  pscat {a}\n  gs    {b}");
    }
}

#[test]
fn ghostscript_draws_the_same_rounded_rectangles() {
    if !gs_available() {
        eprintln!("skipping the gs raster comparison: gs is not installed");
        return;
    }
    for (name, prog) in [("plain", ROUNDRECT), ("scaled", SCALED_ROUNDRECT)] {
        let it = render(prog);
        let dir = common::scratch_dir("arcto_gs_raster");
        let (ps, png) = (dir.join("rr.ps"), dir.join("rr.png"));
        std::fs::write(&ps, format!("{prog} showpage")).expect("write ps");
        let out = Command::new("gs")
            .args([
                "-q",
                "-dNOPAUSE",
                "-dBATCH",
                "-dNOSAFER",
                "-sDEVICE=png16m",
                "-dGraphicsAlphaBits=4",
                "-g200x200",
                "-r72",
            ])
            .arg(format!("-o{}", png.display()))
            .arg(&ps)
            .output_deadline()
            .expect("run gs");
        assert!(out.status.success(), "{name}: gs failed");
        let gs = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).expect("png")).expect("decode");
        let _ = std::fs::remove_dir_all(&dir);
        let ours = &it.gfx().pixmap;
        assert_eq!((gs.width(), gs.height()), (ours.width(), ours.height()));
        let ink = ours.pixels().iter().filter(|p| p.green() < 200).count();
        assert!(
            ink > 5000,
            "{name}: the shape is missing ({ink} inked pixels)"
        );
        let differing = gs
            .pixels()
            .iter()
            .zip(ours.pixels())
            .filter(|(a, b)| {
                let d = |x: u8, y: u8| x.abs_diff(y) as u32;
                d(a.red(), b.red()) + d(a.green(), b.green()) + d(a.blue(), b.blue()) > 96
            })
            .count();
        // Edge pixels may differ in antialiasing; the shape may not.
        assert!(differing < 80, "{name}: {differing} pixels differ from gs");
    }
}

#[test]
fn ghostscript_rasterises_our_pdf_like_our_canvas() {
    if !gs_available() {
        eprintln!("skipping the gs PDF comparison: gs is not installed");
        return;
    }
    for (name, prog) in [("plain", ROUNDRECT), ("scaled", SCALED_ROUNDRECT)] {
        let it = render(prog);
        let pdf = it.gfx().pdf_document().expect("pdf on");
        let dir = common::scratch_dir("arcto_pdf_raster");
        let (pdf_path, png) = (dir.join("rr.pdf"), dir.join("rr.png"));
        std::fs::write(&pdf_path, pdf).expect("write pdf");
        let out = Command::new("gs")
            .args([
                "-q",
                "-dNOPAUSE",
                "-dBATCH",
                "-sDEVICE=png16m",
                "-dGraphicsAlphaBits=4",
                "-g200x200",
                "-r72",
            ])
            .arg(format!("-o{}", png.display()))
            .arg(&pdf_path)
            .output_deadline()
            .expect("run gs");
        assert!(out.status.success(), "{name}: gs rejected our PDF");
        let gs = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).expect("png")).expect("decode");
        let _ = std::fs::remove_dir_all(&dir);
        let ours = &it.gfx().pixmap;
        let differing = gs
            .pixels()
            .iter()
            .zip(ours.pixels())
            .filter(|(a, b)| {
                let d = |x: u8, y: u8| x.abs_diff(y) as u32;
                d(a.red(), b.red()) + d(a.green(), b.green()) + d(a.blue(), b.blue()) > 96
            })
            .count();
        assert!(
            differing < 80,
            "{name}: {differing} pixels differ between gs(PDF) and the canvas"
        );
    }
}
