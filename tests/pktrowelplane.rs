//! The palette-knife plane (issue #180, `lib/paintkit.ps`, `pktrowelplane`):
//! fill the current path's area with overlapping `pktrowel` pulls in one
//! direction. The guards live in a `%%SelfTest` block next to the code; this
//! file holds what needs pixels: the clipped edge (the issue's "crisp where
//! the facet meets its surroundings"), the pull direction, what /Overlap does,
//! the caller's path and clip surviving the call, a rotated CTM, /Touch,
//! determinism, cost, and that Ghostscript draws the same area.

mod common;

use common::within_cpu_budget;
use pscat::{Interp, capabilities};
use std::time::Duration;

fn fresh(w: u32, h: u32) -> Interp {
    let mut it = Interp::with_page(w, h).expect("page");
    for path in ["lib/artkit.ps", "lib/paintkit.ps"] {
        let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
    it.run_str("1 1 1 setrgbcolor clippath fill")
        .expect("white background");
    it
}

fn run(it: &mut Interp, src: &str) {
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
}

/// Luminance 0..255 of the pixel at (x, y), top-left origin.
fn luma_at(it: &Interp, x: u32, y: u32) -> f32 {
    let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
    0.3 * p.red() as f32 + 0.59 * p.green() as f32 + 0.11 * p.blue() as f32
}

fn inked(it: &Interp, x: u32, y: u32) -> bool {
    luma_at(it, x, y) < 160.0
}

/// Signed distance-ish test: is (px, py) inside the convex polygon `poly`
/// (counter-clockwise or clockwise) with every edge pushed `margin` outward?
/// Negative margin shrinks it.
fn inside(poly: &[(f64, f64)], px: f64, py: f64, margin: f64) -> bool {
    let mut sign = 0.0f64;
    for i in 0..poly.len() {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % poly.len()];
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = (dx * dx + dy * dy).sqrt();
        // distance to the edge's line, positive on the left
        let d = (dx * (py - y0) - dy * (px - x0)) / len;
        if sign == 0.0 && d.abs() > 1e-9 {
            sign = d.signum();
        }
        if d * sign < -margin {
            return false;
        }
    }
    true
}

/// Page y (up) to pixel row (down): the page is `h` high.
fn rows(h: u32) -> impl Iterator<Item = u32> {
    0..h
}

/// Counts inked pixels outside the polygon (padded by `margin` px) and the
/// fraction of the polygon (shrunk by `margin` px) that is inked. The polygon
/// is given in device pixel coordinates, top-left origin.
fn edge_report(it: &Interp, poly: &[(f64, f64)], w: u32, h: u32, margin: f64) -> (usize, f64) {
    let mut outside = 0;
    let mut in_total = 0usize;
    let mut in_ink = 0usize;
    for y in rows(h) {
        for x in 0..w {
            let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
            let ink = inked(it, x, y);
            if ink && !inside(poly, cx, cy, margin) {
                outside += 1;
            }
            if inside(poly, cx, cy, -margin) {
                in_total += 1;
                if ink {
                    in_ink += 1;
                }
            }
        }
    }
    (outside, in_ink as f64 / in_total.max(1) as f64)
}

const SOLID: &str = "/Coverage 1 /Scrape 0 /EdgeBuildup 0 /ColorJitter 0";

// A triangle in page space on a 100x100 page. Device y = 100 - page y.
const TRI_PAGE: &str = "newpath 10 10 moveto 90 10 lineto 40 85 lineto closepath";
fn tri_device() -> Vec<(f64, f64)> {
    vec![(10.0, 90.0), (90.0, 90.0), (40.0, 15.0)]
}

#[test]
fn the_area_is_filled_and_the_edge_is_the_path() {
    let mut it = fresh(100, 100);
    run(
        &mut it,
        &format!(
            "7 srand 0 0 0 setrgbcolor {TRI_PAGE} << /Width 12 /Direction -80 /Angle 12 {SOLID} >> pktrowelplane"
        ),
    );
    let (outside, filled) = edge_report(&it, &tri_device(), 100, 100, 1.5);
    assert_eq!(outside, 0, "paint crossed the clip edge");
    assert!(filled > 0.97, "only {filled:.3} of the facet is covered");
}

#[test]
fn a_patchy_blade_still_closes_up_into_a_plane() {
    // The default /Coverage leaves holes in each pull; the default overlap
    // follows it so the plane still reads as one mass.
    let mut it = fresh(100, 100);
    run(
        &mut it,
        &format!(
            "3 srand 0 0 0 setrgbcolor {TRI_PAGE} << /Width 12 /Direction -80 /Angle 12 /Scrape 0 /EdgeBuildup 0 >> pktrowelplane"
        ),
    );
    let (outside, filled) = edge_report(&it, &tri_device(), 100, 100, 1.5);
    assert_eq!(outside, 0);
    assert!(filled > 0.9, "only {filled:.3} of the facet is covered");
}

/// Mean absolute luma step between horizontal neighbours and between
/// vertical ones, over the interior of the page.
fn steps(it: &Interp, w: u32, h: u32) -> (f32, f32) {
    let (mut hs, mut vs, mut n) = (0.0, 0.0, 0.0);
    for y in 20..h - 20 {
        for x in 20..w - 20 {
            hs += (luma_at(it, x, y) - luma_at(it, x + 1, y)).abs();
            vs += (luma_at(it, x, y) - luma_at(it, x, y + 1)).abs();
            n += 1.0;
        }
    }
    (hs / n, vs / n)
}

fn square_plane(opts: &str) -> Interp {
    let mut it = fresh(120, 120);
    run(
        &mut it,
        &format!(
            "5 srand 0.45 0.45 0.45 setrgbcolor newpath 5 5 moveto 115 5 lineto 115 115 lineto 5 115 lineto closepath << {opts} >> pktrowelplane"
        ),
    );
    it
}

#[test]
fn direction_sets_the_way_the_pulls_run() {
    // Each pull is streaked along its own direction by the lane colour
    // jitter, so luminance changes little along a pull and a lot across.
    let opts =
        "/Width 10 /Angle 0 /Coverage 1 /Scrape 0 /EdgeBuildup 0 /ColorJitter 0.35 /Lanes 24";
    let along_x = square_plane(&format!("{opts} /Direction 0"));
    let (hs, vs) = steps(&along_x, 120, 120);
    assert!(hs < vs * 0.6, "Direction 0: along {hs:.2}, across {vs:.2}");
    let along_y = square_plane(&format!("{opts} /Direction 90"));
    let (hs, vs) = steps(&along_y, 120, 120);
    assert!(vs < hs * 0.6, "Direction 90: along {vs:.2}, across {hs:.2}");
}

fn coverage_of(opts: &str) -> f64 {
    let it = square_plane(&format!(
        "/Width 10 /Direction 90 /Angle 0 /Scrape 0.5 /EdgeBuildup 0 {opts}"
    ));
    // black ink check: the colour above is grey, so use luma < 200
    let mut ink = 0usize;
    for y in 10..110 {
        for x in 10..110 {
            if luma_at(&it, x, y) < 200.0 {
                ink += 1;
            }
        }
    }
    ink as f64 / 10000.0
}

#[test]
fn overlap_sets_the_row_spacing() {
    // A scraped blade leaves bare tracks; rows laid closer cover each
    // other's nicks, so more overlap means more of the area inked.
    let loose = coverage_of("/Overlap 0");
    let tight = coverage_of("/Overlap 0.75");
    assert!(
        tight > loose + 0.05,
        "overlap 0.75 covers {tight:.3}, overlap 0 covers {loose:.3}"
    );
}

/// A black plane over the page's square with a solid blade, so any white left
/// inside the square is a gap between rows (issue #362).
fn solid_plane(pressure: &str) -> Interp {
    let mut it = fresh(120, 120);
    run(
        &mut it,
        &format!(
            "5 srand 0 0 0 setrgbcolor newpath 10 10 moveto 110 10 lineto 110 110 lineto \
             10 110 lineto closepath << /Width 10 /Direction -90 /Angle 0 /Coverage 1 \
             /Scrape 0 /EdgeBuildup 0 /Lanes 20 /Overlap 0.3 {pressure} >> pktrowelplane"
        ),
    );
    it
}

fn bare_fraction(it: &Interp) -> f64 {
    let mut bare = 0usize;
    for y in 14..106 {
        for x in 14..106 {
            if luma_at(it, x, y) > 160.0 {
                bare += 1;
            }
        }
    }
    bare as f64 / (92.0 * 92.0)
}

#[test]
fn a_tapering_pressure_does_not_open_gaps_between_rows() {
    // The blade is half as wide at the top of each pull as at the bottom.
    // Rows spaced for the nominal width left a bare line between neighbours
    // at the narrow end; they are now spaced for the thinnest the profile gets.
    let taper = "/Pressure { 0.5 mul 0.5 add }";
    let it = solid_plane(taper);
    let bare = bare_fraction(&it);
    assert!(
        bare < 0.01,
        "a tapering pressure left {bare:.3} of the area bare"
    );
    // and a flat pressure must still be solid.
    let flat = bare_fraction(&solid_plane("/Pressure { pop 1 }"));
    assert!(flat < 0.01, "a flat pressure left {flat:.3} bare");
}

#[test]
fn a_pressure_that_never_dips_spaces_the_rows_as_before() {
    // Byte for byte: no /Pressure, a flat one, and one above 1 all give the
    // same rows, because only a dip below 1 narrows the spacing.
    let base = solid_plane("").gfx().pixmap.data().to_vec();
    let flat = solid_plane("/Pressure /pkflat load")
        .gfx()
        .pixmap
        .data()
        .to_vec();
    assert!(base == flat, "pkflat changed the plane");
}

#[test]
fn reading_the_pressure_leaves_the_random_stream_alone() {
    // The profile draws a random number on its first 17 calls only. Those are
    // the plane's own reads of it for the spacing (the rows call it once per
    // stop, after). If those draws were not put back, the plane's own draws
    // would shift and the two renders would differ.
    let with = |draw: &str| {
        let mut it = fresh(120, 120);
        run(
            &mut it,
            &format!(
                "/cnt 0 def 5 srand 0 0 0 setrgbcolor newpath 10 10 moveto 110 10 lineto \
                 110 110 lineto 10 110 lineto closepath << /Width 10 /Direction -90 /Overlap 0.3 \
                 /Pressure {{ /cnt cnt 1 add def {draw} 0.5 mul 0.5 add }} >> pktrowelplane \
                 rrand /after exch def"
            ),
        );
        run(&mut it, "after");
        let after = it.pop_int().ok();
        (it.gfx().pixmap.data().to_vec(), after)
    };
    let drawn = with("cnt 17 le { rand pop } if");
    let quiet = with("");
    assert!(drawn.0 == quiet.0, "the sampling's draws moved the plane");
    assert_eq!(drawn.1, quiet.1, "the sampling's draws moved the stream");
}

#[test]
fn the_path_and_clip_survive_the_call() {
    let mut it = fresh(100, 100);
    run(
        &mut it,
        &format!("1 srand 0 0 0 setrgbcolor {TRI_PAGE} << /Width 12 {SOLID} >> pktrowelplane"),
    );
    // the path is the caller's still: filling it paints the whole triangle
    run(&mut it, "1 0 0 setrgbcolor fill");
    assert!(
        it.gfx()
            .pixmap
            .pixel(40, 50)
            .is_some_and(|p| p.red() > 200 && p.green() < 60),
        "the triangle was not filled red after the plane"
    );
    // and the clip is whole again: a fill far outside the triangle lands
    run(&mut it, "0 0 1 setrgbcolor 80 80 15 15 rectfill");
    let p = it.gfx().pixmap.pixel(87, 12).expect("pixel");
    assert!(p.blue() > 200 && p.red() < 60, "the clip was left behind");
}

#[test]
fn a_rejected_key_leaves_the_callers_clip_and_matrix_alone() {
    // /Lanes is checked by pktrowel, not by the plane. The error must come
    // before the plane's clip and rotation, or a caller that catches it is
    // left clipped to the facet and rotated.
    let mut it = fresh(100, 100);
    run(
        &mut it,
        &format!(
            "{TRI_PAGE} {{ << /Width 12 /Lanes 50 >> pktrowelplane }} stopped pop \
             newpath 0 0 1 setrgbcolor 80 80 15 15 rectfill"
        ),
    );
    let p = it.gfx().pixmap.pixel(87, 12).expect("pixel");
    assert!(
        p.blue() > 200 && p.red() < 60,
        "the facet's clip or a rotation was left behind by the error"
    );
}

#[test]
fn a_touch_budget_error_leaves_the_state_alone() {
    // A fine weave makes the catch's cell count exceed its budget only once a
    // row's own extent is known, which the plane cannot see in advance.
    let mut it = fresh(100, 100);
    run(
        &mut it,
        &format!(
            "<< /Weave 0.01 >> pkcanvas {TRI_PAGE} \
             {{ << /Width 12 /Direction 30 /Touch 0.5 >> pktrowelplane }} stopped \
             {{ $error /command get /pktrowel-touch-area-exceeds-safety-limit eq }} {{ false }} ifelse \
             /tplseen exch def \
             newpath 0 0 1 setrgbcolor 80 80 15 15 rectfill"
        ),
    );
    run(&mut it, "tplseen { 1 } { 0 } ifelse");
    assert_eq!(it.pop_int().ok(), Some(1), "the touch guard did not fire");
    let p = it.gfx().pixmap.pixel(87, 12).expect("pixel");
    assert!(
        p.blue() > 200 && p.red() < 60,
        "the facet's clip or a rotation was left behind"
    );
}

#[test]
fn a_pressure_that_stops_is_passed_on_as_a_stop() {
    // A /Pressure procedure may `stop`. That is no error: the plane must undo
    // its clip and rotation and stop too, without replaying an old error.
    let mut it = fresh(100, 100);
    run(
        &mut it,
        &format!(
            "$error /errorname /undefined put $error /command /an-old-failed-command put \
             {TRI_PAGE} {{ << /Width 12 /Pressure {{ pop stop }} >> pktrowelplane }} stopped \
             /tplstopped exch def \
             newpath 0 0 1 setrgbcolor 80 80 15 15 rectfill \
             $error /newerror get /tplnew exch def"
        ),
    );
    run(
        &mut it,
        "tplstopped { 1 } { 0 } ifelse tplnew { 0 } { 1 } ifelse",
    );
    assert_eq!(it.pop_int().ok(), Some(1), "an old error was replayed");
    assert_eq!(
        it.pop_int().ok(),
        Some(1),
        "the stop did not reach the caller"
    );
    let p = it.gfx().pixmap.pixel(87, 12).expect("pixel");
    assert!(
        p.blue() > 200 && p.red() < 60,
        "the facet's clip or a rotation was left behind"
    );
}

#[test]
fn a_row_error_that_is_not_undefined_is_raised_again_under_its_name() {
    // The plane catches a row's error, undoes its clip and rotation, and
    // raises it through errordict. That used to report `undefined` in pscat
    // (errordict had no handlers, issue #372); Ghostscript keeps the name
    // and the command, and so must this.
    let mut it = fresh(100, 100);
    run(
        &mut it,
        &format!(
            "{TRI_PAGE} {{ << /Width 12 /Pressure {{ pop [1] 5 get }} >> pktrowelplane }} stopped \
             {{ $error /errorname get $error /command get }} if"
        ),
    );
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(stack[stack.len() - 2..], ["/rangecheck", "get"]);
    let mut it = fresh(100, 100);
    let src = format!("{TRI_PAGE} << /Width 12 /Pressure {{ pop [1] 5 get }} >> pktrowelplane");
    let e = it.run_str(&src).unwrap_err();
    assert_eq!(e, pscat::PsError::Rangecheck);
    assert!(
        it.error_report(&e).contains("OffendingCommand: get"),
        "{}",
        it.error_report(&e)
    );
}

#[test]
fn a_rotated_matrix_changes_nothing_about_the_edge() {
    // Under `100 0 translate 90 rotate` user (x, y) is device (100 - y, x)
    // page-wise, so the same triangle sits at a known place.
    let mut it = fresh(100, 100);
    run(
        &mut it,
        "5 srand 0 0 0 setrgbcolor gsave 100 0 translate 90 rotate \
         newpath 10 10 moveto 90 10 lineto 40 85 lineto closepath \
         << /Width 12 /Direction -80 /Angle 12 /Coverage 1 /Scrape 0 /EdgeBuildup 0 >> \
         pktrowelplane grestore",
    );
    // page (px, py) = (100 - y, x); device row = 100 - py = 100 - x
    let poly: Vec<(f64, f64)> = [(10.0, 10.0), (90.0, 10.0), (40.0, 85.0)]
        .iter()
        .map(|&(x, y)| (100.0 - y, 100.0 - x))
        .collect();
    let (outside, filled) = edge_report(&it, &poly, 100, 100, 1.5);
    assert_eq!(outside, 0, "paint crossed the clip edge under rotate");
    assert!(
        filled > 0.95,
        "only {filled:.3} of the rotated facet is covered"
    );
}

/// Pixels that differ clearly between two renders.
fn differing(a: &Interp, b: &Interp, w: u32, h: u32) -> usize {
    let mut n = 0;
    for y in 0..h {
        for x in 0..w {
            let (p, q) = (
                a.gfx().pixmap.pixel(x, y).expect("in bounds"),
                b.gfx().pixmap.pixel(x, y).expect("in bounds"),
            );
            let d = (p.red() as i32 - q.red() as i32).abs()
                + (p.green() as i32 - q.green() as i32).abs()
                + (p.blue() as i32 - q.blue() as i32).abs();
            if d > 90 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn light_is_in_the_callers_space_not_the_rows() {
    // The same facet drawn two ways: a plane at /Direction 90 with the light
    // from 90 degrees, and the same plane built in a space rotated 90
    // degrees, where it is /Direction 0 and /Light 0. They must agree. (If
    // /Light were passed on unchanged, the first would see the light 90
    // degrees from where it was asked, and the colours would sit on the
    // wrong sides of the blade.)
    let opts = "/Width 14 /Angle 0 /Coverage 1 /Scrape 0 /EdgeBuildup 0 /ColorJitter 0 \
                /Colors [ [ 1 0 0 ] [ 0 0 1 ] ] /ColorMix 0";
    let mut direct = fresh(100, 100);
    run(
        &mut direct,
        &format!(
            "3 srand newpath 10 10 moveto 90 10 lineto 40 85 lineto closepath \
             << {opts} /Direction 90 /Light 90 >> pktrowelplane"
        ),
    );
    let mut turned = fresh(100, 100);
    run(
        &mut turned,
        &format!(
            "3 srand gsave 90 rotate newpath 10 -10 moveto 10 -90 lineto 85 -40 lineto closepath \
             << {opts} /Direction 0 /Light 0 >> pktrowelplane grestore"
        ),
    );
    let n = differing(&direct, &turned, 100, 100);
    assert!(n < 150, "{n} pixels differ between the two ways to say it");
    // and a different light does change the picture, so the test can fail
    let mut other = fresh(100, 100);
    run(
        &mut other,
        &format!(
            "3 srand newpath 10 10 moveto 90 10 lineto 40 85 lineto closepath \
             << {opts} /Direction 90 /Light 0 >> pktrowelplane"
        ),
    );
    assert!(
        differing(&direct, &other, 100, 100) > 500,
        "/Light made no difference"
    );
}

#[test]
fn a_row_past_the_deposit_budget_leaves_the_state_alone() {
    let mut it = fresh(100, 100);
    run(
        &mut it,
        "newpath 0 0 moveto 100000 0 lineto 100000 10 lineto 0 10 lineto closepath \
         { << /Width 12 /Direction 0 >> pktrowelplane } stopped pop \
         newpath 0 0 1 setrgbcolor 80 80 15 15 rectfill",
    );
    let p = it.gfx().pixmap.pixel(87, 12).expect("pixel");
    assert!(
        p.blue() > 200 && p.red() < 60,
        "the facet's clip or a rotation was left behind"
    );
}

#[test]
fn a_scaled_matrix_scales_the_plane_with_it() {
    let mut it = fresh(100, 100);
    run(
        &mut it,
        "5 srand 0 0 0 setrgbcolor gsave 2 2 scale \
         newpath 5 5 moveto 45 5 lineto 20 42 lineto closepath \
         << /Width 6 /Direction -80 /Coverage 1 /Scrape 0 /EdgeBuildup 0 >> \
         pktrowelplane grestore",
    );
    let poly = vec![(10.0, 90.0), (90.0, 90.0), (40.0, 16.0)];
    let (outside, filled) = edge_report(&it, &poly, 100, 100, 1.5);
    assert_eq!(outside, 0);
    assert!(filled > 0.95, "only {filled:.3} covered");
}

#[test]
fn touch_takes_the_planes_blade_onto_the_canvas() {
    let ink = |touch: &str| {
        let mut it = fresh(100, 100);
        run(
            &mut it,
            &format!(
                "<< >> pkcanvas 2 srand 0 0 0 setrgbcolor {TRI_PAGE} << /Width 12 {SOLID} {touch} >> pktrowelplane"
            ),
        );
        let (outside, _) = edge_report(&it, &tri_device(), 100, 100, 1.5);
        assert_eq!(outside, 0, "paint crossed the clip edge ({touch})");
        (0..100)
            .flat_map(|y| (0..100).map(move |x| (x, y)))
            .filter(|&(x, y)| inked(&it, x, y))
            .count()
    };
    let full = ink("/Touch 1");
    let light = ink("/Touch 0.45");
    assert!(
        light < full * 3 / 4,
        "touch 0.45 inked {light}, touch 1 inked {full}"
    );
}

#[test]
fn same_seed_same_plane_and_a_different_seed_differs() {
    let render = |seed: u32| {
        let mut it = fresh(100, 100);
        run(
            &mut it,
            &format!(
                "{seed} srand 0 0 0 setrgbcolor {TRI_PAGE} << /Width 12 /Direction -80 >> pktrowelplane"
            ),
        );
        it.gfx().pixmap.data().to_vec()
    };
    assert_eq!(render(9), render(9));
    assert_ne!(render(9), render(10));
}

#[test]
fn a_whole_plane_is_cheap() {
    within_cpu_budget("a 300x300 plane", Duration::from_secs(2), 3, || {
        let mut it = fresh(320, 320);
        run(
            &mut it,
            "4 srand 0.2 0.2 0.2 setrgbcolor newpath 10 10 moveto 310 10 lineto 160 300 lineto \
             closepath << /Width 14 /Direction -80 /Angle 12 >> pktrowelplane",
        );
    });
}

#[test]
fn ghostscript_draws_the_same_area() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs check: gs not installed");
        return;
    }
    let artkit = std::fs::read_to_string("lib/artkit.ps").expect("artkit");
    let paintkit = std::fs::read_to_string("lib/paintkit.ps").expect("paintkit");
    // packing on, as in the other gs tests, so the {proc} literals the
    // library tests with xcheck/type are packed arrays
    let driver = format!(
        "true setpacking 7 srand 0 0 0 setrgbcolor {TRI_PAGE} \
         << /Width 12 /Direction -80 /Angle 12 {SOLID} >> pktrowelplane"
    );
    let dir = std::env::temp_dir().join(format!("pscat-plane-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let src = dir.join("plane_gs.ps");
    let out = dir.join("plane_gs.pgm");
    std::fs::write(&src, format!("{artkit}\n{paintkit}\n{driver}\nshowpage\n")).expect("write");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=pgmraw",
            "-g100x100",
            "-r72",
        ])
        .arg(format!("-sOutputFile={}", out.display()))
        .arg(&src)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected pktrowelplane");
    let pgm = std::fs::read(&out).expect("pgm");
    // P5\n100 100\n255\n then 100*100 bytes
    let body = &pgm[pgm.len() - 100 * 100..];
    let poly = tri_device();
    let (mut outside, mut in_total, mut in_ink) = (0, 0, 0);
    for y in 0..100u32 {
        for x in 0..100u32 {
            let ink = body[(y * 100 + x) as usize] < 160;
            let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
            if ink && !inside(&poly, cx, cy, 1.5) {
                outside += 1;
            }
            if inside(&poly, cx, cy, -1.5) {
                in_total += 1;
                if ink {
                    in_ink += 1;
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(outside, 0, "gs: paint crossed the clip edge");
    assert!(
        in_ink as f64 / in_total as f64 > 0.97,
        "gs: facet not covered"
    );
}

#[test]
fn the_catalog_lists_it() {
    let cap = capabilities::catalog()
        .into_iter()
        .find(|c| c.source == "lib/paintkit.ps" && c.name == "pktrowelplane")
        .expect("pktrowelplane in the catalog");
    assert_eq!(cap.load, "(lib/artkit.ps) run (lib/paintkit.ps) run");
    assert!(!cap.description.is_empty() && !cap.example.is_empty());
}
