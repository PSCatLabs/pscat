//! `shfill` of Gouraud mesh shadings, ShadingType 4 and 5 (#1325): the PNG against
//! gs 10.07.1's render of the same program (pixels quoted, and the whole
//! page when gs is installed), the errors gs gives, seams, the limits, and
//! the SVG's embedded image. The PDF is checked in `tests/pdf.rs`.
//!
//! gs paints a Gouraud triangle by subdividing it until the colour is
//! within its smoothness tolerance and does not anti-alias, so its pixels
//! are a few levels off the exact interpolation pscat paints, most near a
//! vertex: the quoted values are matched within 12 levels. gs also paints
//! every pixel the mesh's outer edge touches, hard, where pscat
//! anti-aliases it, so the quoted pixels are a pixel or more inside.

mod common;

use common::CommandDeadline as _;
use pscat::{Interp, PsError};
use tiny_skia::Pixmap;

const SIZE: u32 = 100;

/// A white page, then `body`, 1 unit a pixel.
fn render(body: &str) -> Interp {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_str(body)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn run_err(body: &str) -> Result<(), PsError> {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_str(body)
}

/// The pixel at user (x, y), y up.
fn at(it: &Interp, x: u32, y: u32) -> [u8; 3] {
    let p = it.gfx().pixmap.pixel(x, SIZE - 1 - y).expect("on the page");
    [p.red(), p.green(), p.blue()]
}

/// Each (x, y) pixel within `tol` levels of gs's.
fn matches_gs(it: &Interp, pixels: &[((u32, u32), [u8; 3])], tol: i32, what: &str) {
    for &((x, y), gs) in pixels {
        let got = at(it, x, y);
        assert!(
            got.iter()
                .zip(gs)
                .all(|(&g, w)| (i32::from(g) - i32::from(w)).abs() <= tol),
            "{what} at ({x},{y}): got {got:?}, gs {gs:?}"
        );
    }
}

/// gs's render of `body` on the same page, when gs is installed.
fn gs_render(body: &str, name: &str) -> Option<Pixmap> {
    if !common::gs_available() {
        return None;
    }
    let dir = common::scratch_dir("pscat_mesh");
    let ps = dir.join(format!("{name}.ps"));
    let png = dir.join(format!("{name}.png"));
    std::fs::write(&ps, format!("{body}\nshowpage\n")).expect("write ps");
    let out = std::process::Command::new("gs")
        .args(["-q", "-dSAFER", "-dBATCH", "-dNOPAUSE", "-sDEVICE=png16m"])
        .arg(format!("-g{SIZE}x{SIZE}"))
        .arg("-r72")
        .arg(format!("-sOutputFile={}", png.display()))
        .arg(&ps)
        .output_deadline()
        .expect("run gs");
    assert!(out.status.success(), "gs failed on {name}");
    Some(Pixmap::decode_png(&std::fs::read(&png).expect("read png")).expect("decode"))
}

/// The mean absolute difference per channel between pscat's page and
/// gs's, over pixels at least two away from where either changes sharply
/// (so the anti-aliased outer edge, which gs leaves hard, does not count).
fn mean_diff_with_gs(it: &Interp, body: &str, name: &str) -> Option<f64> {
    let theirs = gs_render(body, name)?;
    let ours = &it.gfx().pixmap;
    let (w, h) = (SIZE as usize, SIZE as usize);
    let px = |p: &Pixmap, x: usize, y: usize, c: usize| f64::from(p.data()[(y * w + x) * 4 + c]);
    let edge = |p: &Pixmap, x: usize, y: usize| {
        (0..3).any(|c| {
            let v = px(p, x, y, c);
            [(x - 2, y), (x + 2, y), (x, y - 2), (x, y + 2)]
                .iter()
                .any(|&(a, b)| (px(p, a, b, c) - v).abs() > 40.0)
        })
    };
    let (mut sum, mut n) = (0.0, 0.0);
    for y in 2..h - 2 {
        for x in 2..w - 2 {
            if edge(ours, x, y) || edge(&theirs, x, y) {
                continue;
            }
            for c in 0..3 {
                sum += (px(ours, x, y, c) - px(&theirs, x, y, c)).abs();
                n += 1.0;
            }
        }
    }
    Some(sum / n)
}

/// gs's whole-page render of `body` agrees with pscat's within `limit`.
fn agrees_with_gs(it: &Interp, body: &str, name: &str, limit: f64) {
    if let Some(mean) = mean_diff_with_gs(it, body, name) {
        eprintln!("{name}: mean diff from gs {mean:.3}");
        assert!(mean < limit, "{name}: mean diff from gs {mean:.2}");
    }
}

const TRIANGLE: &str = "<< /ShadingType 4 /ColorSpace /DeviceRGB
   /DataSource [0 10 10 1 0 0  0 90 10 0 1 0  0 50 90 0 0 1] >> shfill";

const LATTICE: &str = "<< /ShadingType 5 /ColorSpace /DeviceRGB /VerticesPerRow 2
   /DataSource [10 10 1 0 0  90 10 0 1 0  10 90 0 0 1  90 90 1 1 1] >> shfill";

/// One free-form triangle: the colours of gs 10.07.1's png16m render at 72
/// dpi, near its vertices and in its middle; outside it the page is untouched.
#[test]
fn a_gouraud_triangle_matches_gs() {
    let it = render(TRIANGLE);
    matches_gs(
        &it,
        &[
            ((11, 11), [247, 2, 4]),
            ((85, 12), [10, 236, 7]),
            ((50, 85), [5, 8, 240]),
            ((50, 40), [77, 80, 97]),
        ],
        12,
        "triangle",
    );
    assert_eq!(at(&it, 5, 5), [255; 3]);
    assert_eq!(at(&it, 20, 60), [255; 3]);
    agrees_with_gs(&it, TRIANGLE, "triangle", 2.0);
}

/// A lattice cell is two triangles split along its (1,0)-(0,1) diagonal,
/// so the middle is the average of those two corners, as in gs.
#[test]
fn a_lattice_cell_is_two_triangles() {
    let it = render(LATTICE);
    matches_gs(
        &it,
        &[
            ((12, 12), [239, 7, 7]),
            ((88, 12), [3, 250, 7]),
            ((12, 88), [3, 7, 250]),
            ((88, 88), [245, 250, 250]),
            ((50, 50), [3, 129, 129]),
            ((30, 50), [60, 65, 129]),
        ],
        12,
        "lattice",
    );
    agrees_with_gs(&it, LATTICE, "lattice", 2.0);
}

/// Packed data: Type 4 and 5 vertices each start on a byte (gs paints the
/// unaligned packings as garbage or rejects them).
#[test]
fn packed_data_matches_gs() {
    let t4 = "<< /ShadingType 4 /ColorSpace /DeviceRGB /BitsPerFlag 2 /BitsPerCoordinate 4
        /BitsPerComponent 4 /Decode [0 150 0 150 0 1 0 1 0 1]
        /DataSource <047c003843c01f803c> >> shfill";
    matches_gs(&render(t4), &[((12, 12), [247, 2, 4])], 12, "type 4");
    let t5 = "<< /ShadingType 5 /ColorSpace /DeviceRGB /BitsPerCoordinate 4 /BitsPerComponent 4
        /VerticesPerRow 2 /Decode [0 100 0 100 0 1 0 1 0 1]
        /DataSource <11f000e10f001e00f0eefff0> >> shfill";
    let it = render(t5);
    matches_gs(
        &it,
        &[
            ((8, 8), [244, 5, 5]),
            ((92, 8), [2, 252, 5]),
            ((8, 92), [2, 5, 252]),
            ((92, 92), [250, 252, 252]),
            ((50, 50), [2, 128, 128]),
        ],
        12,
        "type 5",
    );
}

/// With a `/Function`, each vertex carries `t`, which is interpolated and
/// then mapped: a quadratic red-to-blue ramp, as gs paints it. A file
/// `/DataSource` (an inline filter) is read to its EOD and the program
/// goes on after it.
#[test]
fn a_function_and_a_file_data_source_match_gs() {
    let it = render(
        "<< /ShadingType 4 /ColorSpace /DeviceRGB /DataSource [0 10 10 0  0 90 10 1  0 50 90 0.5]
           /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 2 >> >> shfill",
    );
    matches_gs(
        &it,
        &[
            ((11, 11), [254, 0, 0]),
            ((85, 12), [25, 0, 229]),
            ((50, 85), [188, 0, 66]),
            ((50, 40), [187, 0, 67]),
            ((30, 20), [236, 0, 18]),
            ((70, 30), [105, 0, 149]),
        ],
        12,
        "function",
    );
    let it = render(
        "<< /ShadingType 4 /ColorSpace /DeviceRGB /BitsPerFlag 8 /BitsPerCoordinate 8
           /BitsPerComponent 8 /Decode [0 255 0 255 0 1 0 1 0 1]
           /DataSource currentfile /ASCIIHexDecode filter >> shfill
000a0aff0000 005a0a00ff00 00325a0000ff>
0 0 1 setrgbcolor 0 0 5 5 rectfill",
    );
    matches_gs(
        &it,
        &[
            ((11, 11), [247, 2, 4]),
            ((50, 40), [77, 80, 97]),
            ((2, 2), [0, 0, 255]),
        ],
        12,
        "file",
    );
}

/// A mesh of many triangles in one colour paints that colour at every
/// pixel inside it: no seam where triangles meet, however they are cut.
/// Only the outer edge is anti-aliased.
#[test]
fn shared_edges_leave_no_seams() {
    // An 11 x 11 lattice over (10,10)-(90,90), its rows sheared and its
    // points off the pixel grid, so edges cross pixels at every angle.
    let mut data = String::new();
    for j in 0..11 {
        for i in 0..11 {
            let (x, y) = (
                10.3 + 7.9 * f64::from(i) + 0.4 * f64::from(j),
                10.2 + 7.95 * f64::from(j),
            );
            data.push_str(&format!("{x} {y} 0.8 0.2 0.1 "));
        }
    }
    let lattice = format!(
        "<< /ShadingType 5 /ColorSpace /DeviceRGB /VerticesPerRow 11 /DataSource [{data}] >> shfill"
    );
    // A fan of thin triangles around (50,50), flagged 2 (each shares the
    // centre and the last point).
    let mut fan = String::from("0 50.3 50.1 0.8 0.2 0.1 0 90.2 50.1 0.8 0.2 0.1 ");
    for k in 1..=40 {
        let a = f64::from(k) * std::f64::consts::TAU / 40.0;
        let (x, y) = (
            50.3 + 40.0 * pscat::fmath::cos(a),
            50.1 + 40.0 * pscat::fmath::sin(a),
        );
        let flag = if k == 1 { 0 } else { 2 };
        fan.push_str(&format!("{flag} {x} {y} 0.8 0.2 0.1 "));
    }
    let fan = format!("<< /ShadingType 4 /ColorSpace /DeviceRGB /DataSource [{fan}] >> shfill");
    let want = [204, 51, 26];
    for (body, inside) in [
        (
            &lattice,
            Box::new(|x: u32, y: u32| (15..=88).contains(&x) && (11..=88).contains(&y))
                as Box<dyn Fn(u32, u32) -> bool>,
        ),
        (
            &fan,
            Box::new(|x: u32, y: u32| {
                let (dx, dy) = (f64::from(x) + 0.5 - 50.3, f64::from(y) + 0.5 - 50.1);
                pscat::fmath::hypot(dx, dy) < 38.0
            }),
        ),
    ] {
        let it = render(&format!("0 setgray 0 0 100 100 rectfill {body}"));
        for y in 0..SIZE {
            for x in 0..SIZE {
                if inside(x, y) {
                    assert_eq!(at(&it, x, y), want, "a seam at ({x},{y})");
                }
            }
        }
    }
}

/// `/BBox` cuts the mesh; the clip and `setalpha` apply as to any paint.
#[test]
fn bbox_clip_and_alpha_apply() {
    let it = render(&TRIANGLE.replace(">> shfill", "/BBox [0 0 50 50] >> shfill"));
    matches_gs(
        &it,
        &[((40, 40), [109, 48, 97]), ((49, 20), [112, 109, 33])],
        12,
        "bbox",
    );
    assert_eq!(at(&it, 51, 20), [255; 3]);
    assert_eq!(at(&it, 60, 20), [255; 3]);
    let it = render(&format!("0 0 30 100 rectclip {TRIANGLE}"));
    assert_eq!(at(&it, 40, 20), [255; 3], "outside the clip");
    assert_ne!(at(&it, 20, 20), [255; 3], "inside the clip");
    let opaque = at(&render(TRIANGLE), 50, 40);
    let half = at(&render(&format!("0.5 setalpha {TRIANGLE}")), 50, 40);
    for (h, o) in half.iter().zip(opaque) {
        let want = (f64::from(o) + 255.0) / 2.0;
        assert!(
            (f64::from(*h) - want).abs() <= 1.0,
            "{half:?} vs {opaque:?}"
        );
    }
}

/// gs accepts these and paints as quoted (or nothing).
#[test]
fn what_gs_accepts() {
    // Nothing to paint.
    for body in [
        "<< /ShadingType 4 /ColorSpace /DeviceRGB /DataSource [] >> shfill",
        // A single row has no cells.
        "<< /ShadingType 5 /ColorSpace /DeviceRGB /VerticesPerRow 4
           /DataSource [10 10 1 0 0  90 10 0 1 0  10 90 0 0 1  90 90 1 1 1] >> shfill",
        // No /Decode: gs paints nothing.
        "<< /ShadingType 4 /ColorSpace /DeviceRGB /BitsPerFlag 8 /BitsPerCoordinate 8
           /BitsPerComponent 8 /DataSource <000a0aff0000 005a0a00ff00 00325a0000ff> >> shfill",
    ] {
        let it = render(body);
        assert!(
            it.gfx().pixmap.data().iter().all(|&v| v == 255),
            "painted: {body}"
        );
    }
    // A first vertex flagged 1 starts a triangle; 2.0 VerticesPerRow and 8.0 bits.
    for body in [
        "<< /ShadingType 4 /ColorSpace /DeviceRGB
           /DataSource [1 10 10 1 0 0  1 90 10 0 1 0  1 50 90 0 0 1] >> shfill",
        &LATTICE.replace("/VerticesPerRow 2", "/VerticesPerRow 2.0"),
        "<< /ShadingType 4 /ColorSpace /DeviceRGB /BitsPerFlag 8.0 /BitsPerCoordinate 8
           /BitsPerComponent 8 /Decode [0 255 0 255 0 1 0 1 0 1]
           /DataSource <000a0aff0000 005a0a00ff00 00325a0000ff> >> shfill",
        // Gray and CMYK, an array of functions, a /BBox.
        "<< /ShadingType 4 /ColorSpace /DeviceGray /DataSource [0 10 10 0  0 90 10 1  0 50 90 0.5] >> shfill",
        "<< /ShadingType 4 /ColorSpace /DeviceCMYK
           /DataSource [0 10 10 1 0 0 0  0 90 10 0 0 0 1  0 50 90 0 1 0 0] >> shfill",
        "<< /ShadingType 4 /ColorSpace /DeviceRGB /DataSource [0 10 10 0  0 90 10 0.5  0 50 90 1]
           /Function [<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> dup dup] >> shfill",
    ] {
        let it = render(body);
        assert_ne!(at(&it, 50, 40), [255; 3], "{body}");
    }
    let it = render(
        "<< /ShadingType 4 /ColorSpace /DeviceGray /DataSource [0 10 10 0  0 90 10 1  0 50 90 0.5] >> shfill",
    );
    matches_gs(
        &it,
        &[
            ((11, 11), [4; 3]),
            ((89, 11), [251; 3]),
            ((50, 40), [128; 3]),
        ],
        12,
        "gray",
    );
}

/// The errors gs 10.07.1 gives (on a real device; nullpage skips decoding).
#[test]
fn errors_match_gs() {
    let t4 = "/ShadingType 4 /ColorSpace /DeviceRGB";
    let tri = "0 10 10 1 0 0  0 90 10 0 1 0  0 50 90 0 0 1";
    let bits = "/BitsPerFlag 8 /BitsPerCoordinate 8 /BitsPerComponent 8";
    let decode = "/Decode [0 255 0 255 0 1 0 1 0 1]";
    let hex = "<000a0aff0000 005a0a00ff00 00325a0000ff>";
    let f = "/Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >>";
    let cases: Vec<(String, &str)> = vec![
        (t4.into(), "rangecheck"),
        (format!("{t4} /DataSource 5"), "typecheck"),
        (format!("{t4} /DataSource [0 10 (a) 1 0 0]"), "typecheck"),
        // A vertex, then a triangle, cut short.
        (
            format!("{t4} /DataSource [0 10 10 1 0 0  0 90 10 0 1 0  0 50 90 0 0]"),
            "rangecheck",
        ),
        (
            format!("{t4} /DataSource [0 10 10 1 0 0  0 90 10 0 1 0]"),
            "rangecheck",
        ),
        (
            format!("{t4} /DataSource [{tri} 0 20 20 1 1 1]"),
            "rangecheck",
        ),
        (format!("{t4} /DataSource [{tri} 0]"), "rangecheck"),
        // Flags over 2, and one not an integer.
        (
            format!("{t4} /DataSource [3 10 10 1 0 0  0 90 10 0 1 0  0 50 90 0 0 1]"),
            "rangecheck",
        ),
        (
            format!("{t4} /DataSource [{tri} 3 20 20 1 1 1]"),
            "rangecheck",
        ),
        (
            format!("{t4} /DataSource [0 10 10 1 0 0  5 90 10 0 1 0  7 50 90 0 0 1]"),
            "rangecheck",
        ),
        (
            format!("{t4} /DataSource [0.5 10 10 1 0 0  0 90 10 0 1 0  0 50 90 0 0 1]"),
            "rangecheck",
        ),
        // Packed data: the bits keys, their values, Decode, a partial vertex.
        (format!("{t4} /DataSource {hex}"), "undefined"),
        (
            format!("{t4} /DataSource {hex} /BitsPerCoordinate 8 /BitsPerComponent 8"),
            "undefined",
        ),
        (
            format!("{t4} /DataSource {hex} /BitsPerFlag 8 /BitsPerComponent 8"),
            "undefined",
        ),
        (
            format!("{t4} /DataSource {hex} /BitsPerFlag 8 /BitsPerCoordinate 8"),
            "undefined",
        ),
        (
            format!(
                "{t4} /DataSource {hex} {} {decode}",
                bits.replace("Flag 8", "Flag 3")
            ),
            "rangecheck",
        ),
        (
            format!(
                "{t4} /DataSource {hex} {} {decode}",
                bits.replace("Flag 8", "Flag 1")
            ),
            "rangecheck",
        ),
        (
            format!(
                "{t4} /DataSource {hex} {} {decode}",
                bits.replace("Coordinate 8", "Coordinate 7")
            ),
            "rangecheck",
        ),
        (
            format!(
                "{t4} /DataSource {hex} {} {decode}",
                bits.replace("Component 8", "Component 32")
            ),
            "rangecheck",
        ),
        (
            format!("{t4} /DataSource {hex} {bits} /Decode [0 1 0 1 0 1 0 1]"),
            "rangecheck",
        ),
        (
            format!("{t4} /DataSource {hex} {bits} /Decode [0 1 0 1 0 1 0 1 0 1 9 9]"),
            "rangecheck",
        ),
        (
            format!("{t4} /DataSource {hex} {bits} /Decode [0 1 0 1 0 1 0 1 0 (a)]"),
            "typecheck",
        ),
        (
            format!("{t4} /DataSource {hex} {bits} /Decode 5"),
            "typecheck",
        ),
        (
            format!("{t4} /DataSource <000a0aff0000 005a0a00ff00 00325a0000ff 00> {bits} {decode}"),
            "rangecheck",
        ),
        (
            format!("{t4} /DataSource {hex} {bits} {decode} {f}"),
            "rangecheck",
        ),
        // A Function's outputs must match the colour space.
        (
            format!(
                "{t4} /DataSource [0 10 10 0  0 90 10 0.5  0 50 90 1] {}",
                f.replace("[1 0 0]", "[1]").replace("[0 0 1]", "[0]")
            ),
            "rangecheck",
        ),
        (
            format!("{t4} /DataSource [{tri}] /BBox [0 0 50]"),
            "rangecheck",
        ),
        // Type 5.
        (
            "/ShadingType 5 /ColorSpace /DeviceRGB /DataSource [10 10 1 0 0  90 10 0 1 0]".into(),
            "undefined",
        ),
        (format!("{} /VerticesPerRow 1", LATTICE_DICT), "rangecheck"),
        (format!("{} /VerticesPerRow 3", LATTICE_DICT), "rangecheck"),
        (
            format!("{} /VerticesPerRow 2.5", LATTICE_DICT),
            "rangecheck",
        ),
        (format!("{} /VerticesPerRow (a)", LATTICE_DICT), "typecheck"),
        (
            "/ShadingType 5 /ColorSpace /DeviceRGB /VerticesPerRow 2 /DataSource []".into(),
            "rangecheck",
        ),
        (
            "/ShadingType 8 /ColorSpace /DeviceRGB /DataSource []".into(),
            "rangecheck",
        ),
    ];
    for (dict, want) in cases {
        let src = format!("<< {dict} >> shfill");
        let err = run_err(&src).expect_err(&src);
        assert_eq!(err.name(), want, "{src}");
    }
}

const LATTICE_DICT: &str = "/ShadingType 5 /ColorSpace /DeviceRGB
   /DataSource [10 10 1 0 0  90 10 0 1 0  10 90 0 0 1  90 90 1 1 1]";

/// A crafted mesh is stopped by the step limit before anything is painted,
/// and the page is as it was.
#[test]
fn a_step_limit_stops_it_before_it_paints() {
    // Many big triangles, all over the page.
    let tris = "0 -500 -500 1 0 0  0 600 -500 0 1 0  0 50 600 0 0 1 ".repeat(400);
    let src = format!("<< /ShadingType 4 /ColorSpace /DeviceRGB /DataSource [{tris}] >> shfill");
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.set_limits(pscat::Limits::none().with_max_steps(100_000))
        .expect("limits");
    let r = it.run_str(&src);
    assert!(r.is_err(), "{r:?}");
    assert!(it.gfx().pixmap.data().iter().all(|&v| v == 255));
}

/// The SVG holds the PNG's layer as an image at device resolution.
#[test]
fn the_svg_embeds_the_png_layer() {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.gfx_mut().enable_svg();
    it.run_str(TRIANGLE).expect("runs");
    let svg = it
        .gfx()
        .svg_pages()
        .expect("recording")
        .pop()
        .expect("page");
    assert!(svg.contains("<image"), "{svg}");
    assert!(svg.contains("data:image/png;base64,"), "{svg}");
    assert!(svg.contains("width=\"80\" height=\"80\""), "{svg}");
}

/// An inline `/DataSource` under two filters is drained layer by layer, so
/// the program goes on right after the ASCII85 data's `~>` (gs 10.07.1
/// paints the triangle and the blue square after it).
#[test]
fn a_filter_chain_data_source_is_drained_to_its_end() {
    let it = render(
        "<< /ShadingType 4 /ColorSpace /DeviceRGB /BitsPerFlag 8 /BitsPerCoordinate 8
           /BitsPerComponent 8 /Decode [0 255 0 255 0 1 0 1 0 1]
           /DataSource currentfile /ASCII85Decode filter /FlateDecode filter >> shfill
Gar:%iqh!:^_\"K(!2PRJ(`<5S!$CD-!W~>
0 0 1 setrgbcolor 0 0 5 5 rectfill",
    );
    matches_gs(
        &it,
        &[
            ((11, 11), [247, 2, 4]),
            ((50, 40), [77, 80, 97]),
            ((2, 2), [0, 0, 255]),
        ],
        12,
        "filter chain",
    );
}

/// pscat on `src` (written to a file, so `currentfile` reads it) under
/// `limit`: whether it succeeded, and its stderr.
fn limited(name: &str, src: &[u8], limit: [&str; 2]) -> (bool, String) {
    let dir = common::scratch_dir("pscat_mesh_limits");
    let file = dir.join(format!("{name}.ps"));
    std::fs::write(&file, src).expect("write source");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["--page", "100x100", limit[0], limit[1], "--png"])
        .arg(dir.join(format!("{name}.png")))
        .arg(&file)
        .stdin(std::process::Stdio::null())
        .output_deadline()
        .expect("spawn pscat");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A raw `currentfile` DataSource of 8 MB (Codex round 1 on #1376): its
/// reading is held against the memory limit before each chunk and charged
/// as steps as it goes, whether or not anything is decoded (no `/Decode`).
#[test]
fn a_big_raw_file_data_source_answers_to_the_limits() {
    let mut src = b"<< /ShadingType 4 /ColorSpace /DeviceRGB /BitsPerFlag 8 /BitsPerCoordinate 8
        /BitsPerComponent 8 /DataSource currentfile >> shfill\n"
        .to_vec();
    src.extend(std::iter::repeat_n(b'A', 8 << 20));
    for (limit, what) in [
        (["--max-memory", "2"], "memory"),
        (["--max-steps", "2000"], "steps"),
    ] {
        let (ok, err) = limited("raw", &src, limit);
        assert!(
            !ok && err.contains(&format!("limitexceeded ({what})")),
            "{limit:?}: {err}"
        );
    }
    let (ok, err) = limited("raw", &src, ["--max-steps", "0"]);
    assert!(ok, "with no limit: {err}");
}

/// A large packed mesh wholly off the page, shaded again and again (Codex
/// round 1 on #1376): each `shfill` is charged its decoding, so the step
/// limit stops the loop.
#[test]
fn an_off_page_mesh_is_charged_its_decoding() {
    let src = b"/s 300006 string def
        /m << /ShadingType 4 /ColorSpace /DeviceRGB /BitsPerFlag 8 /BitsPerCoordinate 8
              /BitsPerComponent 8 /Decode [10000 20000 10000 20000 0 1 0 1 0 1]
              /DataSource s >> def
        1 1 200 { pop m shfill } for\n";
    let (ok, err) = limited("offpage", src, ["--max-steps", "100000"]);
    assert!(
        !ok && err.contains("limitexceeded (steps)") && err.contains("OffendingCommand: shfill"),
        "{err}"
    );
    let (ok, err) = limited("offpage", src, ["--max-steps", "0"]);
    assert!(ok, "with no limit: {err}");
}

/// Copying the DataSource is charged on every path (Codex round 2 on
/// #1376): a 4 MB string with no `/Decode` (nothing decoded, nothing
/// painted) and an array under `nulldevice` (painting suppressed), each
/// shaded in a loop, stop at the step limit.
#[test]
fn copying_the_data_source_is_charged_on_every_path() {
    let string = b"/s 4000000 string def
        /m << /ShadingType 4 /ColorSpace /DeviceRGB /BitsPerFlag 8 /BitsPerCoordinate 8
              /BitsPerComponent 8 /DataSource s >> def
        1 1 300 { pop m shfill } for\n";
    let array = b"/a [ 20000 { 0 } repeat ] def
        /m << /ShadingType 4 /ColorSpace /DeviceRGB /DataSource a >> def
        nulldevice 1 1 5000 { pop m shfill } for\n";
    for (name, src) in [("copy-string", &string[..]), ("copy-array", &array[..])] {
        let (ok, err) = limited(name, src, ["--max-steps", "400000"]);
        assert!(
            !ok && err.contains("limitexceeded (steps)")
                && err.contains("OffendingCommand: shfill"),
            "{name}: {err}"
        );
    }
}

/// Painting is counted as it goes, each triangle's box before it is
/// painted (Codex round 2 on #1376): two thousand thin slivers, each
/// spanning the page, stop at the step limit.
#[test]
fn overlapping_sliver_triangles_stop_at_the_step_limit() {
    // Flag, x, y and RGB, a byte each, Decode 0..100: (0,0), (100,100),
    // (1,0), all red.
    let tri = "000000ff0000 00ffffff0000 000300ff0000";
    let src = format!(
        "<< /ShadingType 4 /ColorSpace /DeviceRGB /BitsPerFlag 8 /BitsPerCoordinate 8
           /BitsPerComponent 8 /Decode [0 100 0 100 0 1 0 1 0 1]
           /DataSource <{}> >> shfill\n",
        tri.repeat(2000)
    );
    let (ok, err) = limited("slivers", src.as_bytes(), ["--max-steps", "50000"]);
    assert!(!ok && err.contains("limitexceeded (steps)"), "{err}");
    let (ok, err) = limited("slivers", src.as_bytes(), ["--max-steps", "0"]);
    assert!(ok, "with no limit: {err}");
}
