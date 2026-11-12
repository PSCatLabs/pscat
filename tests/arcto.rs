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
    // corner as the unscaled one (Codex, combined review batch 1, round
    // 2): exact collinearity has no scale to get wrong. gs 10.07.1
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
fn a_short_first_leg_keeps_its_corner() {
    // A 0.0001-long first leg into a right angle is a well-conditioned
    // corner: the page translation (device y near 772) must not swamp it
    // (Codex, combined review batch 1, round 4). gs 10.07.1 draws the
    // radius-10 curve ending near (20.0001, 30).
    // The vertical counterpart (round 5) runs along the page's y
    // translation itself; gs gives 20 10.0001 30 20.0001.
    let cases = [
        (
            "20 20 moveto 20.0001 20 20.0001 120 10 arcto",
            [10.0001, 20.0, 20.0001, 30.0],
        ),
        (
            "20 20 moveto 20 20.0001 120 20.0001 10 arcto",
            [20.0, 10.0001, 30.0, 20.0001],
        ),
    ];
    for (src, want) in cases {
        let got = run_case(&format!("{{ {src} }}"));
        assert!(got.contains(" c "), "{src}: {got}");
        let stack: Vec<f64> = got
            .split("stack: ")
            .nth(1)
            .and_then(|s| s.split(" path:").next())
            .unwrap()
            .split_whitespace()
            .map(|w| w.parse().unwrap())
            .collect();
        for (g, w) in stack.iter().zip(want) {
            assert!((g - w).abs() < 1e-3, "{src}: {got}");
        }
    }
}

#[test]
fn an_anisotropic_ctm_keeps_a_user_space_corner() {
    // 1 x 0.000001 squashes a right angle nearly flat in device space;
    // the corner is still a corner in user space (Codex, combined review
    // batch 1, round 6). gs 10.07.1 gives about 92.93 92.93 107.07 92.93.
    let got = run_case("{ 1 0.000001 scale 0 0 moveto 100 100 200 0 10 arcto }");
    assert!(got.contains(" c "), "{got}");
    let stack: Vec<f64> = got
        .split("stack: ")
        .nth(1)
        .and_then(|s| s.split(" path:").next())
        .unwrap()
        .split_whitespace()
        .map(|w| w.parse().unwrap())
        .collect();
    for (g, w) in stack.iter().zip([92.93, 92.93, 107.07, 92.93]) {
        assert!((g - w).abs() < 0.05, "{got}");
    }
}

#[test]
fn a_hairpin_past_the_device_range_is_limitcheck_as_in_gs() {
    // gs's fixed-point path holds 2^31/256 device units: its tangent point
    // at -8.3868e6 fits, the next step out is limitcheck, and the path is
    // left as it was. pscat uses the same limit.
    let fits = run_case("{ 0 0 moveto 100 0 -50 0.0003577 10 arcto }");
    assert!(fits.contains("ok stack: -838"), "{fits}");
    assert_eq!(
        run_case("{ 0 0 moveto 100 0 -50 0.0003576 10 arcto }"),
        "c => limitcheck stack: path: m 0.0 0.0"
    );
    assert_eq!(
        run_case("{ 0 0 moveto 100 0 -50 0.0001 10 arct }"),
        "c => limitcheck stack: path: m 0.0 0.0"
    );
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

// ---- Sweep: the straight-or-curved decision, tangent points and errors
// across transforms, against gs where the input is well-conditioned
// (combined Codex review batch 1; the dispatcher's redesign). pscat keeps
// the current point in f32 device space and gs on a 1/256-unit
// fixed-point grid, so where a leg or a corner's angle is near either
// precision (short legs or an exact fold-back under a rotation, a 1e-3
// scale) the two differ in noise, and only sanity is asserted there.

const SWEEP_PRELUDE: &str = "\
/sw-r { { 64 string cvs print ( ) print } forall } def \
/sw { gsave mark exch stopped \
  { cleartomark $error /errorname get 64 string cvs print } \
  { counttomark array astore exch pop sw-r } ifelse () = grestore } def \
";

fn rotations() -> Vec<String> {
    (0..360)
        .step_by(15)
        .map(|a| format!("{a} rotate"))
        .collect()
}

/// Transforms where both programs keep plenty of precision.
fn well_conditioned_ctms() -> Vec<String> {
    let mut v = rotations();
    for s in [
        "0.1 0.1 scale",
        "10 10 scale",
        "1000 1000 scale",
        "1000 1000 scale 37 rotate",
    ] {
        v.push(s.to_string());
    }
    for s in [
        "[1 0 0.5 1 0 0] concat",
        "[1 0.3 0 1 0 0] concat 20 rotate",
        "1 0.001 scale",
        "1000 1 scale",
    ] {
        v.push(s.to_string());
    }
    v
}

/// Transforms that push a leg towards gs's fixed-point grid or pscat's f32.
fn extreme_ctms() -> Vec<String> {
    [
        "0.001 0.001 scale",
        "0.001 0.001 scale 37 rotate",
        "1 0.000001 scale 30 rotate",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Corners well away from either precision limit, and the clear errors.
const WELL_CONDITIONED: &[(&str, &str)] = &[
    ("right", "13 17 moveto 113 17 113 117 10"),
    ("obtuse", "13 17 moveto 113 17 163 67 10"),
    ("acute", "13 17 moveto 113 17 23 27 10"),
    ("negative radius", "13 17 moveto 113 17 113 117 -10"),
    ("radius past the legs", "13 17 moveto 113 17 113 117 500"),
    ("long legs", "0 0 moveto 100000 0 100000 100000 10"),
    ("far p2", "0 0 moveto 100 0 100 1000000000 10"),
    ("near-collinear ahead", "13 17 moveto 113 17 213 17.001 10"),
    (
        "collinear ahead from the origin",
        "0 0 moveto 100 0 200 0 10",
    ),
    ("hairpin", "0 0 moveto 100 0 -50 0.0001 10"),
    ("p1 = p2", "13 17 moveto 113 17 113 17 10"),
];

/// Inputs whose answer is decided by rounding, in gs and in pscat.
const NOISE_DOMINATED: &[(&str, &str)] = &[
    (
        "short horizontal leg",
        "20 20 moveto 20.0001 20 20.0001 120 10",
    ),
    (
        "short vertical leg",
        "20 20 moveto 20 20.0001 120 20.0001 10",
    ),
    ("near fold-back", "13 17 moveto 113 17 -37 17.01 10"),
    ("fold-back", "13 17 moveto 113 17 -37 17 10"),
    ("fold-back from the origin", "0 0 moveto 100 0 -50 0 10"),
    ("collinear ahead", "13 17 moveto 113 17 213 17 10"),
    ("p0 = p1", "13 17 moveto 13 17 113 117 10"),
];

fn sweep_program(ctms: &[String], geoms: &[(&str, &str)]) -> (String, Vec<String>) {
    let mut body = String::from(SWEEP_PRELUDE);
    let mut labels = Vec::new();
    for c in ctms {
        for (name, g) in geoms {
            labels.push(format!("{c} | {name}"));
            body.push_str(&format!("{{ {c} {g} arcto }} sw\n"));
        }
    }
    (body, labels)
}

fn sweep_pscat(body: &str) -> Vec<String> {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(body)
        .unwrap_or_else(|e| panic!("sweep failed: {}", it.error_report(&e)));
    String::from_utf8_lossy(it.captured_stdout())
        .lines()
        .map(str::to_string)
        .collect()
}

/// Numbers, or an error name, from one sweep line.
fn sweep_words(line: &str) -> Result<Vec<f64>, String> {
    let w: Vec<&str> = line.split_whitespace().collect();
    match w.as_slice() {
        [name] if name.parse::<f64>().is_err() => Err(name.to_string()),
        _ => Ok(w.iter().map(|x| x.parse().expect("a number")).collect()),
    }
}

fn close(a: &[f64], b: &[f64], rel: f64) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| (x - y).abs() <= rel * x.abs().max(y.abs()).max(1.0))
}

#[test]
fn sweep_matches_ghostscript_where_well_conditioned() {
    if !gs_available() {
        eprintln!("skipping the gs sweep: gs is not installed");
        return;
    }
    let (body, labels) = sweep_program(&well_conditioned_ctms(), WELL_CONDITIONED);
    let ours = sweep_pscat(&body);
    let theirs: Vec<String> = gs_output(&body).lines().map(str::to_string).collect();
    assert_eq!(ours.len(), labels.len());
    assert_eq!(theirs.len(), labels.len(), "gs printed {theirs:?}");
    let mut bad = Vec::new();
    for ((label, a), b) in labels.iter().zip(&ours).zip(&theirs) {
        let same = match (sweep_words(a), sweep_words(b)) {
            (Ok(x), Ok(y)) => close(&x, &y, 1e-3),
            (Err(x), Err(y)) => x == y,
            _ => false,
        };
        if !same {
            bad.push(format!("{label}:\n  pscat {a}\n  gs    {b}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {} differ:\n{}",
        bad.len(),
        labels.len(),
        bad.join("\n")
    );
}

/// Transforms, geometries and the relative tolerance for one check.
type SweepRun<'a> = (&'a [String], &'a [(&'a str, &'a str)], f64);

/// How much a sweep transform magnifies user space (its largest scale).
fn ctm_gain(ctm: &str) -> f64 {
    if ctm.starts_with("1000") {
        1000.0
    } else if ctm.starts_with("10 ") {
        10.0
    } else {
        1.0
    }
}

/// Within `rel` of the larger magnitude in `want` (the corner's size).
fn close_to(want: &[f64], got: &[f64], rel: f64) -> bool {
    let size = want.iter().fold(1.0_f64, |m, x| m.max(x.abs()));
    want.len() == got.len()
        && want
            .iter()
            .zip(got)
            .all(|(x, y)| (x - y).abs() <= rel * size)
}

#[test]
fn sweep_tangent_points_do_not_depend_on_the_ctm() {
    // User-space answers are CTM-independent, so each transform must give
    // the identity's result. Strictly (1e-5 of the corner's size) where
    // the current point keeps its precision; within 1% under a 0.1 or
    // 1e-3 scale, where the f32 device current point carries ~3e-5 of the
    // corner as noise; and the 6.7e-5 rad near fold-back (tangent length
    // ~3000 legs) within 1% under the precise transforms only (under a
    // small scale it is noise-dominated). The 1e-6 anisotropic transform
    // is checked for sanity only, below. The hairpin is left out: whether it
    // fits the device's coordinate range depends on the scale, as in gs.
    let coarse_names = [
        "0.1 0.1 scale",
        "1 0.001 scale",
        "0.001 0.001 scale",
        "0.001 0.001 scale 37 rotate",
    ];
    let precise: Vec<String> = well_conditioned_ctms()
        .into_iter()
        .filter(|c| !coarse_names.contains(&c.as_str()))
        .collect();
    let coarse: Vec<String> = coarse_names.iter().map(|s| s.to_string()).collect();
    let clear: Vec<(&str, &str)> = WELL_CONDITIONED
        .iter()
        .filter(|(n, _)| *n != "hairpin")
        .copied()
        .collect();
    let near_fold = [("near fold-back", "13 17 moveto 113 17 -37 17.01 10")];
    let runs: [SweepRun; 3] = [
        (&precise, &clear, 1e-5),
        (&coarse, &clear, 1e-2),
        (&precise, &near_fold, 1e-2),
    ];
    let mut bad = Vec::new();
    for (ctms, geoms, rel) in runs {
        for (name, g) in geoms {
            let (id_body, _) = sweep_program(&["".to_string()], &[(name, g)]);
            let want = sweep_words(&sweep_pscat(&id_body)[0]);
            let (body, labels) = sweep_program(ctms, &[(name, g)]);
            for ((label, ctm), line) in labels.iter().zip(ctms.iter()).zip(sweep_pscat(&body)) {
                // A corner the transform carries past the device's
                // coordinate range is limitcheck, as in gs.
                let leaves_range = want.as_ref().is_ok_and(|x| {
                    x.iter().fold(0.0_f64, |m, v| m.max(v.abs())) * ctm_gain(ctm) > 8.0e6
                });
                let same = match (&want, sweep_words(&line)) {
                    (Ok(_), Err(e)) if leaves_range => e == "limitcheck",
                    (Ok(x), Ok(y)) => close_to(x, &y, rel),
                    (Err(x), Err(y)) => *x == y,
                    _ => false,
                };
                if !same {
                    bad.push(format!("{label}: {line} (identity: {want:?})"));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn sweep_collinear_from_the_origin_is_a_line_under_every_ctm() {
    // The origin maps to the CTM's translation exactly, so the current
    // point comes back exact and an exactly collinear corner stays a line
    // (gs's fixed-point grid turns some rotations of this into limitcheck).
    let mut ctms = well_conditioned_ctms();
    ctms.extend(extreme_ctms());
    for g in ["0 0 moveto 100 0 -50 0 10", "0 0 moveto 100 0 200 0 10"] {
        let (body, labels) = sweep_program(&ctms, &[("collinear", g)]);
        for (label, line) in labels.iter().zip(sweep_pscat(&body)) {
            assert_eq!(
                sweep_words(&line),
                Ok(vec![100.0, 0.0, 100.0, 0.0]),
                "{label}"
            );
        }
    }
}

#[test]
fn sweep_noise_dominated_inputs_stay_sane() {
    // Where rounding decides, pscat (like gs) may answer either way, but it
    // must answer: finite tangent points, or undefinedresult/limitcheck.
    let mut ctms = well_conditioned_ctms();
    ctms.extend(extreme_ctms());
    let (body, labels) = sweep_program(&ctms, NOISE_DOMINATED);
    let lines = sweep_pscat(&body);
    assert_eq!(lines.len(), labels.len());
    for (label, line) in labels.iter().zip(lines) {
        match sweep_words(&line) {
            Ok(v) => assert!(
                v.len() == 4 && v.iter().all(|x| x.is_finite()),
                "{label}: {line}"
            ),
            Err(e) => assert!(
                e == "undefinedresult" || e == "limitcheck",
                "{label}: {line}"
            ),
        }
    }
}
