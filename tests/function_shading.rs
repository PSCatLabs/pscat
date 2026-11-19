//! `shfill` of a ShadingType 1 (function-based) shading, and FunctionType 0
//! (sampled) functions (#1039): known-answer pixels in the PNG, the errors
//! gs 10.07.1 gives, and the SVG's embedded image against the PNG. The PDF
//! is checked in `tests/pdf.rs`.

use pscat::{Interp, PsError};
use tiny_skia::Pixmap;

const SIZE: u32 = 200;

/// Red, green, blue, white at the corners (0,0), (1,0), (0,1), (1,1): the
/// first input varies fastest in the table.
const S0: &str = "<< /FunctionType 0 /Domain [0 1 0 1] /Range [0 1 0 1 0 1] /Size [2 2]
     /BitsPerSample 8 /DataSource <ff0000 00ff00 0000ff ffffff> >>";

/// A black page, then `body`.
fn render(body: &str) -> Interp {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_str(&format!("0 setgray 0 0 {SIZE} {SIZE} rectfill {body}"))
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn run_err(body: &str) -> Result<(), PsError> {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_str(body)
}

/// The pixel holding user point (x, y) (y up, 1 unit a pixel).
fn at(it: &Interp, x: f64, y: f64) -> [u8; 3] {
    let p = it
        .gfx()
        .pixmap
        .pixel(x as u32, SIZE - 1 - y as u32)
        .expect("on the page");
    [p.red(), p.green(), p.blue()]
}

fn near(got: [u8; 3], want: [f64; 3], tol: f64, what: &str) {
    assert!(
        got.iter()
            .zip(want)
            .all(|(&g, w)| (f64::from(g) - w).abs() <= tol),
        "{what}: got {got:?}, want {want:?}"
    );
}

/// The 2x2 table, bilinear, at (u, v) of [0,1]^2, in 0..255.
fn bilinear(u: f64, v: f64) -> [f64; 3] {
    let c = [
        [255.0, 0.0, 0.0],
        [0.0, 255.0, 0.0],
        [0.0, 0.0, 255.0],
        [255.0; 3],
    ];
    let mut out = [0.0; 3];
    for (k, o) in out.iter_mut().enumerate() {
        *o = c[0][k] * (1.0 - u) * (1.0 - v)
            + c[1][k] * u * (1.0 - v)
            + c[2][k] * (1.0 - u) * v
            + c[3][k] * u * v;
    }
    out
}

/// The Domain square mapped to user (50,50)-(150,150): every sampled pixel
/// (its centre is half a unit in) is the table's bilinear value, including
/// near the corners and at the centre; outside the square the page is
/// untouched. gs 10.07.1 paints the same picture (corners (247,3,3),
/// (3,247,0), (3,0,247), centre (127,127,128) at 72 dpi).
#[test]
fn a_two_by_two_table_is_bilinear_over_the_domain() {
    let it = render(&format!(
        "<< /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [100 0 0 100 50 50] /Function {S0} >> shfill"
    ));
    for (x, y) in [
        (50.0, 50.0),
        (149.0, 50.0),
        (50.0, 149.0),
        (149.0, 149.0),
        (100.0, 100.0),
        (75.0, 120.0),
        (133.0, 61.0),
    ] {
        let (u, v) = ((x + 0.5 - 50.0) / 100.0, (y + 0.5 - 50.0) / 100.0);
        near(at(&it, x, y), bilinear(u, v), 1.0, &format!("({x},{y})"));
    }
    for (x, y) in [(49.0, 100.0), (150.0, 100.0), (100.0, 49.0), (100.0, 150.0)] {
        assert_eq!(at(&it, x, y), [0, 0, 0], "outside the Domain at ({x},{y})");
    }
}

/// A 3x2 table: Encode maps the Domain onto the nodes 0..2, so x = 0.25 is
/// halfway between the first two. The gs 10.07.1 values (72 dpi, the same
/// placement) are quoted; pscat evaluates exactly, gs approximates in
/// patches, so they agree to a few levels.
#[test]
fn a_three_by_two_table_interpolates_between_its_nodes_as_gs_does() {
    let it = render(
        "<< /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [100 0 0 100 50 50]
           /Function << /FunctionType 0 /Domain [0 1 0 1] /Range [0 1 0 1 0 1] /Size [3 2]
             /BitsPerSample 8 /DataSource <ff0000 00ff00 0000ff 000000 ffffff ff00ff> >> >> shfill",
    );
    // (u, v), gs's pixel there.
    for ((u, v), gs) in [
        ((0.25, 0.02), [124.0, 129.0, 3.0]),
        ((0.25, 0.25), [126.0, 129.0, 33.0]),
        ((0.5, 0.5), [128.0, 252.0, 129.0]),
        ((0.75, 0.5), [128.0, 124.0, 192.0]),
        ((0.99, 0.98), [251.0, 0.0, 255.0]),
    ] {
        let got = at(&it, 50.0 + u * 100.0, 50.0 + v * 100.0);
        near(got, gs, 6.0, &format!("({u},{v})"));
    }
}

/// `/Domain` and `/Matrix` together: the Domain [-1 1 -1 1] scaled by 50
/// about (100,100) covers (50,50)-(150,150), and a rotated Matrix turns
/// the picture with it.
#[test]
fn domain_and_matrix_place_the_shading() {
    let f = "<< /FunctionType 0 /Domain [-1 1 -1 1] /Range [0 1 0 1 0 1] /Size [2 2]
         /BitsPerSample 8 /DataSource <ff0000 00ff00 0000ff ffffff> >>";
    let it = render(&format!(
        "<< /ShadingType 1 /ColorSpace /DeviceRGB /Domain [-1 1 -1 1]
           /Matrix [50 0 0 50 100 100] /Function {f} >> shfill"
    ));
    near(at(&it, 51.0, 51.0), [255.0, 0.0, 0.0], 8.0, "red corner");
    near(
        at(&it, 148.0, 148.0),
        [255.0, 255.0, 255.0],
        8.0,
        "white corner",
    );
    assert_eq!(at(&it, 48.0, 100.0), [0, 0, 0]);
    // A quarter turn: shading x runs up the page, so (1,0), green, is at
    // the top left and (0,1), blue, at the bottom right... of the turned
    // square, which is the same square.
    let it = render(&format!(
        "<< /ShadingType 1 /ColorSpace /DeviceRGB /Domain [-1 1 -1 1]
           /Matrix [0 50 -50 0 100 100] /Function {f} >> shfill"
    ));
    near(at(&it, 148.0, 51.0), [255.0, 0.0, 0.0], 8.0, "red, turned");
    near(
        at(&it, 148.0, 148.0),
        [0.0, 255.0, 0.0],
        8.0,
        "green, turned",
    );
    near(at(&it, 51.0, 51.0), [0.0, 0.0, 255.0], 8.0, "blue, turned");
}

/// `/BBox` cuts the painted region, the clip bounds it, and `setalpha`
/// reaches it, as for a gradient.
#[test]
fn bbox_clip_and_alpha_apply() {
    let sh = format!(
        "<< /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [100 0 0 100 50 50] /Function {S0}"
    );
    let it = render(&format!("{sh} /BBox [0 0 100 200] >> shfill"));
    assert_ne!(at(&it, 90.0, 100.0), [0, 0, 0]);
    assert_eq!(at(&it, 110.0, 100.0), [0, 0, 0], "past the BBox");
    let it = render(&format!("0 0 200 100 rectclip {sh} >> shfill"));
    assert_ne!(at(&it, 100.0, 90.0), [0, 0, 0]);
    assert_eq!(at(&it, 100.0, 110.0), [0, 0, 0], "past the clip");
    let it = render(&format!("0.5 setalpha {sh} >> shfill"));
    let full = bilinear(0.505, 0.505);
    near(
        at(&it, 100.0, 100.0),
        full.map(|c| c / 2.0),
        2.0,
        "half alpha over black",
    );
}

/// An array of one-output functions paints as the one three-output table;
/// DeviceGray and DeviceCMYK work too.
#[test]
fn function_arrays_and_other_colour_spaces() {
    let one = |hex: &str| {
        format!(
            "<< /FunctionType 0 /Domain [0 1 0 1] /Range [0 1] /Size [2 2] /BitsPerSample 8 /DataSource <{hex}> >>"
        )
    };
    let arr = format!(
        "[ {} {} {} ]",
        one("ff0000ff"),
        one("00ff00ff"),
        one("0000ffff")
    );
    let a = render(&format!(
        "<< /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [100 0 0 100 50 50] /Function {arr} >> shfill"
    ));
    let b = render(&format!(
        "<< /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [100 0 0 100 50 50] /Function {S0} >> shfill"
    ));
    assert_eq!(a.gfx().pixmap.data(), b.gfx().pixmap.data());
    let it = render(&format!(
        "<< /ShadingType 1 /ColorSpace /DeviceGray /Matrix [100 0 0 100 50 50] /Function {} >> shfill",
        one("00ff00ff")
    ));
    near(at(&it, 100.0, 100.0), [127.5; 3], 2.0, "gray centre");
    let it = render(
        "<< /ShadingType 1 /ColorSpace /DeviceCMYK /Matrix [100 0 0 100 50 50]
           /Function << /FunctionType 0 /Domain [0 1 0 1] /Range [0 1 0 1 0 1 0 1] /Size [2 2]
             /BitsPerSample 8 /DataSource <ff000000 ff000000 ff000000 ff000000> >> >> shfill",
    );
    near(at(&it, 100.0, 100.0), [0.0, 255.0, 255.0], 1.0, "cyan");
}

/// 1, 2, 4, 12, 16 and 32 bits a sample read the same table.
#[test]
fn every_bits_per_sample_reads_the_table() {
    // Corners red, green, blue, white as 0/max per component.
    let table = |bits: u32| -> String {
        let comps = [1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 1];
        let mut bitstr = String::new();
        for c in comps {
            let v: u64 = if c == 1 { (1u64 << bits) - 1 } else { 0 };
            bitstr += &format!("{v:0w$b}", w = bits as usize);
        }
        while !bitstr.len().is_multiple_of(8) {
            bitstr.push('0');
        }
        (0..bitstr.len() / 8)
            .map(|i| {
                format!(
                    "{:02x}",
                    u8::from_str_radix(&bitstr[i * 8..i * 8 + 8], 2).unwrap()
                )
            })
            .collect()
    };
    let reference = render(&format!(
        "<< /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [100 0 0 100 50 50] /Function {S0} >> shfill"
    ));
    for bits in [1, 2, 4, 12, 16, 24, 32] {
        let it = render(&format!(
            "<< /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [100 0 0 100 50 50]
               /Function << /FunctionType 0 /Domain [0 1 0 1] /Range [0 1 0 1 0 1] /Size [2 2]
                 /BitsPerSample {bits} /DataSource <{}> >> >> shfill",
            table(bits)
        ));
        for (x, y) in [(55.0, 55.0), (100.0, 100.0), (140.0, 70.0)] {
            assert_eq!(
                at(&it, x, y),
                at(&reference, x, y),
                "{bits} bits at ({x},{y})"
            );
        }
    }
}

/// What gs 10.07.1 accepts and paints: a short DataSource reads the
/// missing samples as 0 (gs paints the white corner black), a one-node
/// axis, `/Order 3` (interpolated linearly here: a documented gap), a real
/// `/Size`, stray `/Coords` and `/Background`, and a zero-area Domain,
/// which paints nothing.
#[test]
fn what_gs_accepts() {
    let sh = |extra: &str, f: &str| {
        format!(
            "<< /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [100 0 0 100 50 50] {extra}
               /Function << /FunctionType 0 /Domain [0 1 0 1] /Range [0 1 0 1 0 1] /BitsPerSample 8 {f} >> >> shfill"
        )
    };
    let it = render(&sh("", "/Size [2 2] /DataSource <ff0000 00ff00 0000ff>"));
    near(
        at(&it, 149.0, 149.0),
        [0.0, 3.0, 3.0],
        4.0,
        "missing corner (gs: 0 3 3)",
    );
    near(at(&it, 50.0, 50.0), [252.0, 0.0, 0.0], 4.0, "red corner");
    let it = render(&sh("", "/Size [1 2] /DataSource <ff0000 0000ff>"));
    near(
        at(&it, 140.0, 100.0),
        [127.5, 0.0, 127.5],
        2.0,
        "one node across",
    );
    for (extra, f) in [
        (
            "",
            "/Size [2 2] /Order 3 /DataSource <ff0000 00ff00 0000ff ffffff>",
        ),
        (
            "",
            "/Size [2 2.0] /DataSource <ff0000 00ff00 0000ff ffffff>",
        ),
        (
            "/Coords 5 /Background [1 0 0] /Extend 7",
            "/Size [2 2] /DataSource <ff0000 00ff00 0000ff ffffff>",
        ),
    ] {
        let it = render(&sh(extra, f));
        near(at(&it, 100.0, 100.0), [127.5; 3], 2.0, f);
    }
    let it = render(&sh(
        "/Domain [0 1 0 0]",
        "/Size [2 2] /DataSource <ff0000 00ff00 0000ff ffffff>",
    ));
    assert!(
        it.gfx()
            .pixmap
            .data()
            .chunks(4)
            .all(|p| p[..3] == [0, 0, 0])
    );
}

/// Each error as gs 10.07.1 reports it (`{ ... } stopped` and
/// `$error /errorname get`).
#[test]
fn errors_match_gs() {
    let table = "/Size [2 2] /BitsPerSample 8 /DataSource <ff0000 00ff00 0000ff ffffff>";
    let f =
        |keys: &str| format!("<< /FunctionType 0 /Domain [0 1 0 1] /Range [0 1 0 1 0 1] {keys} >>");
    let sh = |keys: &str| format!("<< /ShadingType 1 /ColorSpace /DeviceRGB {keys} >> shfill");
    let s0 = f(table);
    let cases: Vec<(String, PsError)> = vec![
        (sh(""), PsError::Undefined("Function".into())),
        (
            sh("/Function << /FunctionType 2 /Domain [0 1] /C0 [0 0 0] /C1 [1 1 1] /N 1 >>"),
            PsError::Rangecheck,
        ),
        (
            sh(&format!("/Function {s0} /Domain [0 1]")),
            PsError::Rangecheck,
        ),
        (
            sh(&format!("/Function {s0} /Domain [1 0 0 1]")),
            PsError::Rangecheck,
        ),
        (
            sh(&format!("/Function {s0} /Matrix [1 0 0 1 0]")),
            PsError::Rangecheck,
        ),
        (
            sh(&format!("/Function {s0} /Matrix [1 0 0 1 0 (a)]")),
            PsError::Typecheck,
        ),
        (
            sh(&format!("/Function {s0} /Matrix [0 0 0 0 0 0]")),
            PsError::UndefinedResult,
        ),
        (
            sh(&format!("/Function {s0} /BBox [0 0 1]")),
            PsError::Rangecheck,
        ),
        (
            format!("<< /ShadingType 1 /ColorSpace /DeviceGray /Function {s0} >> shfill"),
            PsError::Rangecheck,
        ),
        (
            sh(
                "/Function << /FunctionType 0 /Domain [0 1 0 1] /Size [2 2] /BitsPerSample 8 /DataSource <00> >>",
            ),
            PsError::Rangecheck,
        ),
        (
            sh(&format!(
                "/Function {}",
                f("/Size [2 2] /BitsPerSample 7 /DataSource <00>")
            )),
            PsError::Rangecheck,
        ),
        (
            sh(&format!(
                "/Function {}",
                f("/Size [0 2] /BitsPerSample 8 /DataSource <00>")
            )),
            PsError::Rangecheck,
        ),
        (
            sh(&format!(
                "/Function {}",
                f("/Size [2] /BitsPerSample 8 /DataSource <00>")
            )),
            PsError::Rangecheck,
        ),
        (
            sh(&format!(
                "/Function {}",
                f("/Size [2 2] /Order 2 /BitsPerSample 8 /DataSource <00>")
            )),
            PsError::Rangecheck,
        ),
        (
            sh(&format!(
                "/Function {}",
                f("/Size [2 2] /Encode [0 1] /BitsPerSample 8 /DataSource <00>")
            )),
            PsError::Rangecheck,
        ),
        (
            sh(&format!(
                "/Function {}",
                f("/Size [2 2] /Decode [0 1] /BitsPerSample 8 /DataSource <00>")
            )),
            PsError::Rangecheck,
        ),
        (
            sh(&format!(
                "/Function {}",
                f("/Size [2 2] /BitsPerSample 8 /DataSource [255 0 0]")
            )),
            PsError::Rangecheck,
        ),
        (
            sh(
                "/Function << /FunctionType 0 /Domain [0 1 1 0] /Range [0 1 0 1 0 1] /Size [2 2] /BitsPerSample 8 /DataSource <00> >>",
            ),
            PsError::Rangecheck,
        ),
        (
            sh(
                "/Function << /FunctionType 0 /Domain [0 1 0 1] /Range [0 1 1 0 0 1] /Size [2 2] /BitsPerSample 8 /DataSource <00> >>",
            ),
            PsError::Rangecheck,
        ),
        (
            sh(&format!(
                "/Function [ {} {} ]",
                f("/Size [2 2] /BitsPerSample 8 /DataSource <00>")
                    .replace("/Range [0 1 0 1 0 1]", "/Range [0 1]"),
                f("/Size [2 2] /BitsPerSample 8 /DataSource <00>")
                    .replace("/Range [0 1 0 1 0 1]", "/Range [0 1]"),
            )),
            PsError::Rangecheck,
        ),
    ];
    for (src, want) in cases {
        assert_eq!(run_err(&src), Err(want), "{src}");
    }
}

/// A table gs cannot allocate is refused (gs 10.07.1: `/Size [65536
/// 65536]` of RGB bytes is a VMerror, pscat's limitcheck; a Size whose
/// table size overflows is a rangecheck in both), and a huge Domain
/// mapped far past the page still paints all of it (gs paints the page).
#[test]
fn huge_tables_and_domains() {
    let sh = |size: &str, bits: u32, matrix: &str| {
        format!(
            "<< /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [{matrix}]
               /Function << /FunctionType 0 /Domain [0 1 0 1] /Range [0 1 0 1 0 1] /Size [{size}]
                 /BitsPerSample {bits} /DataSource <ff0000 00ff00 0000ff ffffff> >> >> shfill"
        )
    };
    assert_eq!(
        run_err(&sh("65536 65536", 8, "100 0 0 100 0 0")),
        Err(PsError::Limitcheck)
    );
    assert_eq!(
        run_err(&sh("4294967295 4294967295", 2, "100 0 0 100 0 0")),
        Err(PsError::Rangecheck)
    );
    // Within the cap, mostly past the data: reads 0 there, no panic.
    let it = render(&sh("8192 8192", 2, "200 0 0 200 0 0"));
    assert_eq!(at(&it, 199.0, 199.0), [0, 0, 0]);
    let it = render(&sh("2 2", 8, "1e6 0 0 1e6 -5e5 -5e5"));
    for (x, y) in [(0.0, 0.0), (100.0, 100.0), (199.0, 199.0)] {
        near(
            at(&it, x, y),
            bilinear(0.5, 0.5),
            2.0,
            &format!("({x},{y})"),
        );
    }
}

/// A one-input sampled function drives an axial shading too.
#[test]
fn a_sampled_function_drives_an_axial_shading() {
    let it = render(
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 200 0]
           /Function << /FunctionType 0 /Domain [0 1] /Range [0 1 0 1 0 1] /Size [3]
             /BitsPerSample 8 /DataSource <ff0000 00ff00 0000ff> >> >> shfill",
    );
    near(at(&it, 1.0, 10.0), [255.0, 0.0, 0.0], 6.0, "start");
    near(at(&it, 100.0, 10.0), [0.0, 255.0, 0.0], 6.0, "middle node");
    near(at(&it, 198.0, 10.0), [0.0, 0.0, 255.0], 6.0, "end");
    assert_eq!(
        run_err(
            "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 1 0] /Function
               << /FunctionType 0 /Domain [0 1 0 1] /Range [0 1 0 1 0 1] /Size [2 2] /BitsPerSample 8 /DataSource <00> >> >> shfill"
        ),
        Err(PsError::Rangecheck),
        "a two-input function on an axial shading"
    );
}

/// The SVG embeds the shading as an image (SVG has no function shading):
/// the PNG's own layer at device resolution, placed where the PNG painted
/// it. (resvg is built without raster images here, so the image is checked
/// by decoding it.) Composited over the page's grey, it is the PNG.
#[test]
fn the_svg_embeds_the_png_layer() {
    let src = format!(
        "0.5 setgray 0 0 200 200 rectfill 30 20 translate 20 rotate
         << /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [100 0 0 100 30 30] /BBox [0 0 120 90]
            /Function {S0} >> shfill"
    );
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.gfx_mut().enable_svg();
    it.run_str(&src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let svg = it
        .gfx()
        .svg_pages()
        .expect("recording")
        .pop()
        .expect("page");
    assert_eq!(svg.matches("<image ").count(), 1, "{svg}");
    let image = format!(" {}", svg.split("<image ").nth(1).expect("an image"));
    let num = |name: &str| -> f32 {
        image
            .split(&format!(" {name}=\""))
            .nth(1)
            .and_then(|r| r.split('"').next())
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("no {name}"))
    };
    let (w, h) = (num("width") as u32, num("height") as u32);
    let at = svg
        .split("transform=\"matrix(1 0 0 1 ")
        .nth(1)
        .and_then(|r| r.split(')').next())
        .expect("a translation");
    let xy: Vec<u32> = at.split(' ').map(|v| v.parse().expect("whole")).collect();
    let b64 = svg
        .split("data:image/png;base64,")
        .nth(1)
        .and_then(|r| r.split('"').next())
        .expect("a PNG");
    let png = base64_decode(b64);
    let layer = Pixmap::decode_png(&png).expect("a PNG");
    assert_eq!((layer.width(), layer.height()), (w, h));
    let ours: &Pixmap = &it.gfx().pixmap;
    let mut worst = 0;
    let mut opaque = 0;
    for y in 0..h {
        for x in 0..w {
            let l = layer.pixel(x, y).expect("in the layer");
            let p = ours.pixel(xy[0] + x, xy[1] + y).expect("on the page");
            // Premultiplied over grey 128.
            let over = |c: u8| u32::from(c) + 128 * (255 - u32::from(l.alpha())) / 255;
            let want = [over(l.red()), over(l.green()), over(l.blue())];
            let got = [p.red(), p.green(), p.blue()];
            for k in 0..3 {
                worst = worst.max(want[k].abs_diff(u32::from(got[k])));
            }
            opaque += u32::from(l.alpha() == 255);
        }
    }
    assert!(opaque > 5000, "{opaque} opaque pixels");
    assert!(
        worst <= 2,
        "the layer over grey differs from the PNG by {worst}"
    );
}

fn base64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let bytes: Vec<u8> = s.bytes().filter(|&c| c != b'=').map(val).collect();
    bytes
        .chunks(4)
        .flat_map(|q| {
            let n = q
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, &v)| n | u32::from(v) << (18 - 6 * i));
            let out = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            out.into_iter().take(q.len() - 1)
        })
        .collect()
}

/// A step limit stops a large one before it paints anything.
#[test]
fn a_step_limit_stops_it_before_it_paints() {
    let mut it = Interp::with_page(2000, 2000).expect("page");
    it.set_limits(pscat::Limits::none().with_max_steps(1000))
        .expect("limits");
    let r = it.run_str(&format!(
        "<< /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [2000 0 0 2000 0 0] /Function {S0} >> shfill"
    ));
    assert!(r.is_err(), "{r:?}");
    assert!(
        it.gfx()
            .pixmap
            .data()
            .chunks(4)
            .all(|p| p == [255, 255, 255, 255])
    );
}
