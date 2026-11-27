//! `shfill`'s degenerate and extreme cases in the SVG export (#1041),
//! checked against pscat's own PNG. #1028/#1031 fixed these for the PDF;
//! here each is checked in the SVG's text and rasterised by resvg (a
//! compiled dev-dependency, so nothing skips). resvg rasterises with
//! tiny-skia, as the PNG does, so its agreement with the PNG is a floor,
//! not the whole check: the structural assertions pin what a browser reads.

use pscat::Interp;
use tiny_skia::Pixmap;

const SIZE: u32 = 500;

const RED_BLUE: &str = "/Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >>";

/// A stitching function with zero-width legs at both ends: green across
/// the axis and blue from its last instant (red, the first leg, spans
/// nothing: t = 0 already takes the green leg), so the stops end in a hard
/// edge at offset 1.
const HARD_EDGES: &str = "/Function << /FunctionType 3 /Domain [0 1] /Bounds [0 0.99999988 1]
     /Encode [0 1 0 1 0 1 0 1] /Functions [
       << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [1 0 0] /N 1 >>
       << /FunctionType 2 /Domain [0 1] /C0 [0 1 0] /C1 [0 1 0] /N 1 >>
       << /FunctionType 2 /Domain [0 1] /C0 [0 1 0] /C1 [0 1 0] /N 1 >>
       << /FunctionType 2 /Domain [0 1] /C0 [0 0 1] /C1 [0 0 1] /N 1 >> ] >>";

const EXTENDS: [&str; 4] = ["false false", "true false", "false true", "true true"];

/// (name, what goes before the dictionary, the dictionary's own keys).
const CASES: [(&str, &str, &str); 17] = [
    ("edges", "", "/ShadingType 2 /Coords [100 0 400 0]"),
    (
        "equal",
        "",
        "/ShadingType 3 /Coords [250 250 150 250 250 150]",
    ),
    (
        "near-shrink",
        "",
        "/ShadingType 3 /Coords [250 250 150.00001 250 250 150]",
    ),
    (
        "near-grow",
        "",
        "/ShadingType 3 /Coords [250 250 150 250 250 150.00001]",
    ),
    (
        "max-equal",
        "",
        "/ShadingType 3 /Coords [0 0 2e38 0 0 2e38]",
    ),
    (
        "huge-equal",
        "-999800 0 translate",
        "/ShadingType 3 /Coords [0 250 1000000 0 250 1000000]",
    ),
    (
        "collapsed-axis",
        "",
        "/ShadingType 2 /Coords [250 0 250.000001 0]",
    ),
    (
        "tiny-axis",
        "1e8 1e8 scale",
        "/ShadingType 2 /Coords [0 0 1e-7 0]",
    ),
    (
        "tiny-radial",
        "250 250 translate 1e8 1e8 scale",
        "/ShadingType 3 /Coords [0 0 1e-7 0 0 2e-6]",
    ),
    (
        "max-radial",
        "",
        "/ShadingType 3 /Coords [250 250 1e38 250 250 3e38]",
    ),
    ("max-axial", "", "/ShadingType 2 /Coords [-3e38 0 3e38 0]"),
    (
        "collapsed-axis-off-page",
        "",
        "/ShadingType 2 /Coords [1000 0 1000.00001 0]",
    ),
    (
        "max-radial-shrink",
        "",
        "/ShadingType 3 /Coords [250 250 3e38 250 250 1e38]",
    ),
    // Circles neither inside the other: a point can lie on two of them, and
    // gs paints the larger t in range (#1370).
    (
        "cone-grow",
        "",
        "/ShadingType 3 /Coords [160 250 20 260 250 60]",
    ),
    (
        "cone-shrink",
        "",
        "/ShadingType 3 /Coords [260 250 60 160 250 20]",
    ),
    // A cone from a point and one to a point: SVG's outer circle of radius
    // 0 would paint the last stop everywhere.
    (
        "cone-from-point",
        "",
        "/ShadingType 3 /Coords [160 250 0 300 250 60]",
    ),
    (
        "cone-to-point",
        "",
        "/ShadingType 3 /Coords [300 250 60 160 250 0]",
    ),
];

fn program(name: &str, pre: &str, dict: &str, ext: &str) -> String {
    let function = if name == "edges" {
        HARD_EDGES
    } else {
        RED_BLUE
    };
    format!(
        "0.5 setgray 0 0 500 500 rectfill {pre}
         << {dict} /ColorSpace /DeviceRGB /Extend [{ext}] {function} >> shfill"
    )
}

fn png_and_svg(src: &str) -> (Pixmap, String) {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.gfx_mut().enable_svg();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let svg = it
        .gfx()
        .svg_pages()
        .expect("recording on")
        .pop()
        .expect("a page");
    (it.gfx().pixmap.clone(), svg)
}

/// The SVG rasterised by resvg at 1 px per point (the root is in `pt`, so
/// parse at 72 dpi).
fn resvg_raster(svg: &str) -> Pixmap {
    let options = resvg::usvg::Options {
        dpi: 72.0,
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(svg, &options).expect("parse SVG");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(SIZE, SIZE).expect("pixmap");
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Pixmap::from_vec(
        pixmap.take(),
        tiny_skia::IntSize::from_wh(SIZE, SIZE).expect("size"),
    )
    .expect("same size")
}

fn rgb(p: &Pixmap, x: u32, y: u32) -> [u8; 3] {
    let c = p.pixel(x, y).expect("pixel").demultiply();
    [c.red(), c.green(), c.blue()]
}

/// The gradient defs of the page, for messages and structural checks.
fn defs(svg: &str) -> &str {
    svg.split("<defs>")
        .nth(1)
        .and_then(|s| s.split("</defs>").next())
        .unwrap_or("")
}

/// The numeric attribute `name` of the first element that has it.
fn attr(svg: &str, name: &str) -> f32 {
    let key = format!(" {name}=\"");
    let rest = svg
        .split(&key)
        .nth(1)
        .unwrap_or_else(|| panic!("no {name}: {svg}"));
    rest.split('"')
        .next()
        .and_then(|v| v.parse().ok())
        .expect("a number")
}

/// Every case under every `/Extend`: the SVG has no non-finite number, and
/// resvg's raster of it is the PNG, pixel for pixel to within 24 levels.
#[test]
fn every_edge_case_rasterises_as_the_png() {
    for (name, pre, dict) in CASES {
        for ext in EXTENDS {
            let (png, svg) = png_and_svg(&program(name, pre, dict, ext));
            assert!(
                !svg.contains("inf") && !svg.contains("NaN"),
                "{name} [{ext}]: {svg}"
            );
            let theirs = resvg_raster(&svg);
            let mut bad = Vec::new();
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let (a, b) = (rgb(&png, x, y), rgb(&theirs, x, y));
                    if a.iter().zip(b).any(|(p, q)| p.abs_diff(q) > 24) {
                        bad.push((x, y, a, b));
                    }
                }
            }
            assert!(
                bad.is_empty(),
                "{name} [{ext}]: {} pixels differ, first {:?}\n{}",
                bad.len(),
                bad.first(),
                defs(&svg)
            );
        }
    }
}

/// A hard edge at the end, when it pads: never two stops at offset 1,
/// which renderers settle differently; the inner one moves in by a sliver,
/// and the outermost stops keep the padded colours.
#[test]
fn a_hard_edge_at_a_padded_end_is_not_two_stops_at_one_terminal_offset() {
    let (_, svg) = png_and_svg(&program("edges", "", CASES[0].2, "true true"));
    let stops: Vec<&str> = svg.split("<stop ").skip(1).collect();
    let offsets: Vec<&str> = stops
        .iter()
        .map(|s| s.split('"').nth(1).expect("offset"))
        .collect();
    assert_eq!(
        offsets.iter().filter(|o| **o == "0").count(),
        1,
        "{offsets:?}"
    );
    assert_eq!(
        offsets.iter().filter(|o| **o == "1").count(),
        1,
        "{offsets:?}"
    );
    assert!(stops[0].contains("#00ff00"), "{}", stops[0]);
    assert!(
        stops[stops.len() - 1].contains("#0000ff"),
        "{}",
        stops[stops.len() - 1]
    );
    assert_eq!(attr(&svg, "offset").to_bits(), 0f32.to_bits());
}

/// Equal concentric circles (#1041's decision, after gs; see
/// `notes/1041-svg-shading-edge-cases.md`): a disk of the start colour when
/// the start extends, else nothing, and nothing outside the circle under
/// any `/Extend`. The PNG and the SVG both. The SVG writes the disk as a
/// radial from the centre (`r0` 0), one colour, transparent past the
/// radius; with nothing to paint it writes no gradient at all.
#[test]
fn equal_concentric_circles_paint_the_start_disk_only_when_the_start_extends() {
    for ext in EXTENDS {
        let (png, svg) = png_and_svg(&program("equal", "", CASES[1].2, ext));
        let theirs = resvg_raster(&svg);
        let start = ext.starts_with("true");
        let inside = if start { [255, 0, 0] } else { [128, 128, 128] };
        for (what, p) in [("PNG", &png), ("SVG", &theirs)] {
            // (250, 250) the centre; (250, 105) just inside the circle
            // (device y is flipped); (250, 50) and (20, 20) outside.
            assert_eq!(rgb(p, 250, 250), inside, "{what} [{ext}] centre");
            assert_eq!(rgb(p, 250, 105), inside, "{what} [{ext}] r=145");
            assert_eq!(rgb(p, 250, 50), [128, 128, 128], "{what} [{ext}] r=200");
            assert_eq!(rgb(p, 20, 20), [128, 128, 128], "{what} [{ext}] corner");
        }
        if start {
            let d = defs(&svg);
            assert!(d.contains("cx=\"250\" cy=\"250\" r=\"300\""), "{d}");
            assert!(!d.contains("fr=\""), "a disk from the centre: {d}");
            assert!(!d.contains("#0000ff"), "one colour: {d}");
        } else {
            assert!(!svg.contains("Gradient"), "nothing recorded: {svg}");
        }
    }
}

/// Geometry far under a unit, magnified by the CTM: the SVG carries it
/// rescaled to a unit, the transform shrunk to match, so a renderer with a
/// small-length tolerance (resvg's tiny-skia: 2^-15) does not flatten it.
#[test]
fn tiny_geometry_is_written_at_unit_scale() {
    let (_, svg) = png_and_svg(&program("tiny-axis", CASES[7].1, CASES[7].2, "true true"));
    let x2 = attr(&svg, "x2");
    assert!((0.5..2.0).contains(&x2), "x2 {x2}: {}", defs(&svg));
    let (_, svg) = png_and_svg(&program("tiny-radial", CASES[8].1, CASES[8].2, "true true"));
    let r = attr(&svg, "r");
    assert!((0.5..2.0).contains(&r), "r {r}: {}", defs(&svg));
}

/// A radial near f32::MAX whose end does not extend: lengthening it for
/// the transparent run would overflow to `inf`, so it is written at an
/// eighth of the size with the transform multiplying it back.
#[test]
fn huge_geometry_is_lengthened_at_an_eighth_of_its_size() {
    let (_, svg) = png_and_svg(&program("max-radial", "", CASES[9].2, "false false"));
    let d = defs(&svg);
    assert!(d.contains("<radialGradient"), "{d}");
    assert!(
        d.contains("gradientTransform=\"matrix(8 0 0 -8 0 500)\""),
        "{d}"
    );
    assert!(attr(d, "r").is_finite(), "{d}");
}

/// What each remaining case writes, under each `/Extend`, in the SVG's own
/// terms (the pixel test above checks what a renderer makes of it):
/// - an axis that collapses in f32, across the page or off it, is nothing,
///   as gs paints it (and the PNG);
/// - circles nearly equal are the disk, in the colour of the end that
///   pads: the end colour when shrinking by a hair, the start colour when
///   growing;
/// - huge circles stay one finite radial.
#[test]
fn each_degenerate_case_writes_what_the_png_paints() {
    for ext in EXTENDS {
        let [start, end] = {
            let mut it = ext.split(' ').map(|e| e == "true");
            [it.next().unwrap(), it.next().unwrap()]
        };
        for case in [6, 11] {
            let (name, pre, dict) = CASES[case];
            let (png, svg) = png_and_svg(&program(name, pre, dict, ext));
            assert!(!svg.contains("Gradient"), "{name} [{ext}]: {svg}");
            assert!(!svg.contains("#0000ff"), "{name} [{ext}]: {svg}");
            assert_eq!(rgb(&png, 100, 100), [128, 128, 128], "{name} [{ext}]");
        }
        for (case, colour, on) in [(2, "#0000ff", end), (3, "#ff0000", start)] {
            let (name, pre, dict) = CASES[case];
            let (_, svg) = png_and_svg(&program(name, pre, dict, ext));
            let d = defs(&svg);
            assert_eq!(d.contains("<radialGradient"), on, "{name} [{ext}]: {d}");
            if on {
                assert!(
                    d.contains(colour) && !d.contains("fr=\""),
                    "{name} [{ext}]: {d}"
                );
                assert_eq!(
                    d.matches("stop-opacity=\"0\"").count(),
                    2,
                    "{name} [{ext}]: {d}"
                );
            }
        }
        for case in [5, 12] {
            let (name, pre, dict) = CASES[case];
            let (_, svg) = png_and_svg(&program(name, pre, dict, ext));
            let d = defs(&svg);
            if name == "huge-equal" && !start {
                assert!(!d.contains("Gradient"), "{name} [{ext}]: {d}");
                continue;
            }
            assert_eq!(
                d.matches("<radialGradient").count(),
                1,
                "{name} [{ext}]: {d}"
            );
            assert!(attr(d, "r").is_finite(), "{name} [{ext}]: {d}");
        }
    }
}

/// A radial whose circles are not nested, its end not extended (#1370): a
/// point whose larger root is past the end is painted from its smaller one,
/// as the PNG, the PDF and gs do. SVG's gradient has only the larger root,
/// so the smaller root is a second gradient (the same circles run
/// backwards) painted first, and the larger over it. A cone that does not
/// need it, with its end extended, is one gradient as before.
#[test]
fn a_cone_with_its_end_not_extended_paints_its_smaller_root_first() {
    for &(name, pre, dict) in &CASES[13..] {
        for ext in ["false false", "true false"] {
            let (_, svg) = png_and_svg(&program(name, pre, dict, ext));
            assert_eq!(
                svg.matches("<radialGradient").count(),
                2,
                "{name} [{ext}]: {svg}"
            );
            assert_eq!(
                svg.matches("fill=\"url(#").count(),
                2,
                "{name} [{ext}]: {svg}"
            );
        }
        for ext in ["false true", "true true"] {
            let (_, svg) = png_and_svg(&program(name, pre, dict, ext));
            assert_eq!(
                svg.matches("<radialGradient").count(),
                1,
                "{name} [{ext}]: {svg}"
            );
        }
        // The hole: a point right of the end circle (260, 250), 110 from
        // the start circle's centre, where the larger root is past the end.
        // The PNG paints it; so must the SVG.
        let (png, svg) = png_and_svg(&program(name, pre, dict, "true false"));
        let theirs = resvg_raster(&svg);
        let mut hole = 0;
        for y in 0..SIZE {
            for x in 0..SIZE {
                if rgb(&png, x, y) != [128, 128, 128] && rgb(&theirs, x, y) == [128, 128, 128] {
                    hole += 1;
                }
            }
        }
        assert_eq!(
            hole, 0,
            "{name}: {hole} pixels the PNG paints are bare in the SVG"
        );
    }
}

/// A translucent cone composites once: the two gradients share one group
/// that carries the opacity, so where both roots paint the page shows
/// through as it does in the PNG, not twice over.
#[test]
fn a_translucent_cone_composites_its_two_roots_as_one() {
    let (name, pre, dict) = CASES[13];
    let src = program(name, &format!("0.5 setalpha {pre}"), dict, "false false");
    let (png, svg) = png_and_svg(&src);
    assert!(svg.contains("<g opacity=\"0.5\"><path"), "{svg}");
    assert_eq!(svg.matches("opacity=\"0.5\"").count(), 1, "{svg}");
    let theirs = resvg_raster(&svg);
    for (x, y) in [(150, 250), (210, 250), (270, 250), (300, 250)] {
        let (a, b) = (rgb(&png, x, y), rgb(&theirs, x, y));
        assert!(
            a.iter().zip(b).all(|(p, q)| p.abs_diff(q) <= 24),
            "({x},{y}): PNG {a:?}, SVG {b:?}"
        );
    }
}

/// A cone to a point whose far circle is 1e7 away (Codex round 1 on PR
/// #1461): the radius-0 workaround must not scale with the centres'
/// distance. gs leaves (160, 200) gray, as does the PNG; the SVG painted a
/// region about 200 wide there.
#[test]
fn a_cone_to_a_point_with_a_far_circle_keeps_its_apex_in_place() {
    let dict = "/ShadingType 3 /Coords [10000000 250 1 160 250 0]";
    for ext in ["false false", "true false"] {
        let (png, svg) = png_and_svg(&program("far-apex", "", dict, ext));
        let theirs = resvg_raster(&svg);
        let mut bad = Vec::new();
        for y in 0..SIZE {
            for x in 0..SIZE {
                let (a, b) = (rgb(&png, x, y), rgb(&theirs, x, y));
                if a.iter().zip(b).any(|(p, q)| p.abs_diff(q) > 24) {
                    bad.push((x, y, a, b));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "[{ext}] {} pixels differ, first {:?}\n{}",
            bad.len(),
            bad.first(),
            defs(&svg)
        );
        assert_eq!(rgb(&png, 160, 200), [128, 128, 128], "[{ext}] PNG");
        assert_eq!(rgb(&theirs, 160, 200), [128, 128, 128], "[{ext}] SVG");
    }
}
