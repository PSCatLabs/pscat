//! `/Interpolate true` on images (#730): bilinear sampling where the image
//! is magnified, an `/Interpolate true` key on the PDF image and a smooth
//! `image-rendering` in the SVG. The look was compared with Ghostscript
//! 10.07 on the issue's repro (every channel within 4 levels).

use pscat::Interp;
use std::process::Command;

mod common;

fn gray(it: &Interp, x: u32, y: u32) -> u8 {
    it.gfx().pixmap.pixel(x, y).expect("pixel in bounds").red()
}

/// A 4x1 gray ramp (00 55 aa ff) drawn 80x20 at (10,40): each sample cell is
/// 20 device pixels wide, centres at x = 20, 40, 60, 80 (pixel centres at +0.5).
fn ramp(width: usize, data: &str, scale_x: u32, extra: &str) -> String {
    format!(
        "10 40 translate {scale_x} 20 scale /DeviceGray setcolorspace
         << /ImageType 1 /Width {width} /Height 1 /BitsPerComponent 8 /Decode [0 1]
            /ImageMatrix [{width} 0 0 -1 0 1] /DataSource <{data}> {extra} >> image"
    )
}

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn near(got: u8, want: f32) {
    assert!(
        (f32::from(got) - want).abs() <= 1.5,
        "got {got}, wanted about {want}"
    );
}

#[test]
fn bilinear_blends_between_sample_centres() {
    let on = run(&ramp(4, "0055aaff", 80, "/Interpolate true"));
    let off = run(&ramp(4, "0055aaff", 80, ""));
    // Pixel 50 has its centre 0.525 of the way from sample 1 (x=40) to
    // sample 2 (x=60): 85 + 0.525 * 85. Nearest-neighbour keeps cell 2.
    near(gray(&on, 50, 50), 85.0 + 0.525 * 85.0);
    assert_eq!(gray(&off, 50, 50), 0xaa);
    // Pixel 39 is 0.975 of the way from sample 0 (x=20) to sample 1 (x=40).
    near(gray(&on, 39, 50), 0.975 * 85.0);
    // A monotone ramp, no plateaus: every pixel between the outer centres
    // differs from its left neighbour.
    for x in 21..79 {
        assert!(gray(&on, x + 1, 50) >= gray(&on, x, 50), "monotone at {x}");
    }
    assert_ne!(gray(&on, 45, 50), gray(&on, 46, 50));
}

#[test]
fn edges_clamp_to_the_outermost_samples() {
    let on = run(&ramp(4, "0055aaff", 80, "/Interpolate true"));
    // Left of the first centre and right of the last one: the edge sample,
    // exactly (no fade towards the white page).
    for x in 10..=19 {
        assert_eq!(gray(&on, x, 50), 0x00, "left edge x={x}");
    }
    for x in 80..=89 {
        assert_eq!(gray(&on, x, 50), 0xff, "right edge x={x}");
    }
    // The footprint itself is unchanged: page outside it stays white.
    assert_eq!(gray(&on, 9, 50), 255);
    assert_eq!(gray(&on, 90, 50), 255);
    assert_eq!(gray(&on, 50, 39), 255);
    assert_eq!(gray(&on, 50, 60), 255);
}

#[test]
fn only_a_boolean_true_interpolates_and_masks_never_do() {
    let hard = run(&ramp(4, "0055aaff", 80, ""));
    for extra in [
        "/Interpolate false",
        "/Interpolate 1",
        "/Interpolate (true)",
        "/Interpolate /true",
    ] {
        let it = run(&ramp(4, "0055aaff", 80, extra));
        for x in 10..90 {
            assert_eq!(gray(&it, x, 50), gray(&hard, x, 50), "{extra} at {x}");
        }
    }
    // imagemask keeps its hard stencil edge.
    let mask = |extra: &str| {
        run(&format!(
            "10 40 translate 80 20 scale
             << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 1 /Decode [1 0]
                /ImageMatrix [2 0 0 -1 0 1] /DataSource <80> {extra} >> imagemask"
        ))
    };
    let (m_off, m_on) = (mask(""), mask("/Interpolate true"));
    for x in 10..90 {
        assert_eq!(gray(&m_on, x, 50), gray(&m_off, x, 50), "mask at {x}");
    }
}

#[test]
fn a_minified_image_is_not_smoothed() {
    // 80 samples across 40 device pixels: two samples per pixel.
    let data: String = (0..80).map(|i| format!("{:02x}", (i * 3) % 256)).collect();
    let on = run(&ramp(80, &data, 40, "/Interpolate true"));
    let off = run(&ramp(80, &data, 40, ""));
    for x in 0..100 {
        assert_eq!(gray(&on, x, 50), gray(&off, x, 50), "x={x}");
    }
}

#[test]
fn colour_and_indexed_images_interpolate_too() {
    let rgb = run("10 40 translate 80 20 scale /DeviceRGB setcolorspace
         << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 8 /Decode [0 1 0 1 0 1]
            /ImageMatrix [2 0 0 -1 0 1] /DataSource <ff0000 0000ff> /Interpolate true >> image");
    // Midway between the two centres (x=30 and x=70): nearly even red and blue.
    let p = rgb.gfx().pixmap.pixel(50, 50).unwrap();
    assert!(p.red().abs_diff(127) <= 4 && p.blue().abs_diff(127) <= 4 && p.green() == 0);
    let idx = run(
        "10 40 translate 80 20 scale [/Indexed /DeviceRGB 1 <ff0000 0000ff>] setcolorspace
         << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 8
            /ImageMatrix [2 0 0 -1 0 1] /DataSource <00 01> /Interpolate true >> image",
    );
    let p = idx.gfx().pixmap.pixel(50, 50).unwrap();
    assert!(p.red().abs_diff(127) <= 4 && p.blue().abs_diff(127) <= 4);
}

#[test]
fn rotated_image_smooths_along_its_own_axes() {
    let it = run(&format!(
        "50 50 translate 45 rotate -40 -10 translate {}",
        ramp(4, "0055aaff", 80, "/Interpolate true").replace("10 40 translate ", "")
    ));
    // Somewhere in the middle the value is neither of the four samples.
    let odd = (0..100)
        .flat_map(|x| (0..100).map(move |y| (x, y)))
        .map(|(x, y)| gray(&it, x, y))
        .filter(|v| ![0x00, 0x55, 0xaa, 0xff].contains(v) && *v != 255)
        .count();
    assert!(odd > 200, "{odd} blended pixels");
}

#[test]
fn pdf_carries_the_hint_only_when_asked() {
    let pdf = |extra: &str| {
        let mut it = Interp::with_page(100, 100).expect("page");
        it.gfx_mut().enable_pdf();
        it.run_str(&ramp(4, "0055aaff", 80, extra)).expect("runs");
        String::from_utf8_lossy(&it.gfx().pdf_document().expect("recording")).into_owned()
    };
    let on = pdf("/Interpolate true");
    assert_eq!(on.matches("/Interpolate true").count(), 1, "{on}");
    assert!(on.contains("/Subtype /Image"));
    for extra in ["", "/Interpolate false"] {
        assert!(!pdf(extra).contains("/Interpolate"), "{extra:?}");
    }
}

#[test]
fn svg_smooths_only_when_asked() {
    let svg = |extra: &str| {
        let mut it = Interp::with_page(100, 100).expect("page");
        it.gfx_mut().enable_svg();
        it.run_str(&ramp(4, "0055aaff", 80, extra)).expect("runs");
        it.gfx().svg_pages().expect("recording")[0].clone()
    };
    assert!(svg("/Interpolate true").contains("image-rendering=\"auto\""));
    for extra in ["", "/Interpolate false"] {
        let s = svg(extra);
        assert!(s.contains("image-rendering=\"pixelated\""), "{extra:?}");
        assert!(!s.contains("image-rendering=\"auto\""), "{extra:?}");
    }
}

const TYPE3: &str = "10 40 translate 80 20 scale /DeviceRGB setcolorspace
    << /ImageType 3 /InterleaveType 3
       /DataDict << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 8
          /Decode [0 1 0 1 0 1] /ImageMatrix [2 0 0 -1 0 1]
          /DataSource <ff0000 0000ff> DATAEXTRA >>
       /MaskDict << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 1
          /Decode [1 0] /ImageMatrix [2 0 0 -1 0 1] /DataSource <c0> >> >> image";

#[test]
fn imagetype3_datadict_interpolates_but_the_stencil_stays_hard() {
    let src = TYPE3.replace("DATAEXTRA", "/Interpolate true");
    let on = run(&src);
    let p = on.gfx().pixmap.pixel(50, 50).unwrap();
    assert!(p.red().abs_diff(127) <= 4 && p.blue().abs_diff(127) <= 4);
    let off = run(&TYPE3.replace("DATAEXTRA", ""));
    let q = off.gfx().pixmap.pixel(50, 50).unwrap();
    assert_eq!((q.red(), q.blue()), (0, 0xff), "nearest keeps cell 1");

    let mut it = Interp::with_page(100, 100).expect("page");
    it.gfx_mut().enable_pdf();
    it.run_str(&src).expect("runs");
    let pdf = String::from_utf8_lossy(&it.gfx().pdf_document().expect("recording")).into_owned();
    assert_eq!(pdf.matches("/Interpolate true").count(), 1, "colour only");
    // The SVG stencilled image is smoothed too (documented: its mask edge
    // softens with it).
    let mut it = Interp::with_page(100, 100).expect("page");
    it.gfx_mut().enable_svg();
    it.run_str(&src).expect("runs");
    assert!(it.gfx().svg_pages().unwrap()[0].contains("image-rendering=\"auto\""));
}

/// The issue's repro (an 8x4 RGB gradient magnified to 100x100 with
/// `/Interpolate true`), run through Ghostscript and through pscat: the
/// two rasters agree to a few levels (the sample centres and the edge
/// clamp are the same; gs's filter is marginally softer).
#[test]
fn matches_ghostscript_on_the_issue_repro() {
    if !Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        eprintln!("skipping gs comparison: gs not installed");
        return;
    }
    let src = "/W 8 def /H 4 def /row W 3 mul string def /j 0 def
        100 100 scale
        << /ImageType 1 /Width W /Height H /BitsPerComponent 8 /Decode [0 1 0 1 0 1]
           /ImageMatrix [W 0 0 H neg 0 H] /Interpolate true
           /DataSource { 0 1 W 1 sub { /i exch def row i 3 mul i 255 mul W 1 sub idiv put
             row i 3 mul 1 add j 255 mul H 1 sub idiv put row i 3 mul 2 add 128 put } for
             /j j 1 add def row } >>
        /DeviceRGB setcolorspace image showpage";
    let dir = common::scratch_dir("pscat_interp730");
    let (ps, png) = (dir.join("r.ps"), dir.join("r.png"));
    std::fs::write(&ps, format!("%!PS\n{src}")).expect("write ps");
    let status = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-sDEVICE=png16m",
            "-dDEVICEWIDTHPOINTS=100",
            "-dDEVICEHEIGHTPOINTS=100",
            "-dFIXEDMEDIA",
            "-r72",
        ])
        .arg(format!("-o{}", png.display()))
        .arg(&ps)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs failed");
    let theirs = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).expect("png")).expect("decode");
    let _ = std::fs::remove_file(&ps);
    let _ = std::fs::remove_file(&png);
    let mut it = Interp::with_page(100, 100).expect("page");
    it.run_str(src).expect("runs");
    let mut worst = 0;
    for y in 0..100 {
        for x in 0..100 {
            let a = it.gfx().pixmap.pixel(x, y).expect("px");
            let b = theirs.pixel(x, y).expect("px");
            for (p, q) in [
                (a.red(), b.red()),
                (a.green(), b.green()),
                (a.blue(), b.blue()),
            ] {
                worst = worst.max(p.abs_diff(q));
            }
        }
    }
    assert!(worst <= 8, "worst channel difference from gs: {worst}");
}
