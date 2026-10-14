//! `strokepath` (issue #264): the current path becomes the outline
//! `stroke` would paint. Three kinds of check:
//!
//! * hard-coded geometry (`pathbbox` after `strokepath`), pinned against
//!   Ghostscript with `false setstrokeadjust` (gs snaps stroke edges to
//!   pixel centres by default, which shifts every edge by up to half a
//!   device pixel; pscat has no stroke adjust),
//! * a raster comparison: `strokepath fill` against pscat's own `stroke`,
//! * live Ghostscript comparisons (skipped when `gs` is missing) of both
//!   the bounding box and the filled pixels.

use std::process::Command;

use pscat::Interp;
use tiny_skia::Pixmap;

const SIZE: u32 = 400;

fn eval(src: &str) -> Vec<String> {
    let mut it = Interp::new();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {e}"));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

/// `[llx lly urx ury]` of the path `src` builds, or None for
/// `nocurrentpoint` (an empty path).
fn bbox(src: &str) -> Option<[f64; 4]> {
    let mut it = Interp::new();
    let code = format!("{src} pathbbox");
    if it.run_str(&code).is_err() {
        return None;
    }
    let v: Vec<f64> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().parse().expect("number"))
        .collect();
    Some([v[0], v[1], v[2], v[3]])
}

fn near(a: [f64; 4], b: [f64; 4], tol: f64) -> bool {
    a.iter().zip(&b).all(|(x, y)| (x - y).abs() <= tol)
}

fn gs_available() -> bool {
    Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// A scratch path unique per call: the tests run in parallel threads of
/// one process, so the pid alone would collide.
fn scratch(name: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "pscat_strokepath_{}_{n}_{name}",
        std::process::id()
    ))
}

fn gs_bbox(src: &str) -> Option<[f64; 4]> {
    let path = scratch("bbox.ps");
    std::fs::write(
        &path,
        format!(
            "false setstrokeadjust {{ {src} pathbbox 4 array astore == }} stopped {{ (err) = }} if\n"
        ),
    )
    .unwrap();
    let out = Command::new("gs")
        .args(["-q", "-dNOPAUSE", "-dBATCH", "-dNODISPLAY"])
        .arg(&path)
        .output()
        .expect("run gs");
    let _ = std::fs::remove_file(&path);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let line = text.lines().next().unwrap_or("").trim().to_string();
    if line == "err" {
        return None;
    }
    let nums: Vec<f64> = line
        .trim_matches(['[', ']'])
        .split_whitespace()
        .map(|n| n.parse().unwrap_or_else(|_| panic!("gs said {text:?}")))
        .collect();
    Some([nums[0], nums[1], nums[2], nums[3]])
}

fn render_pscat(src: &str) -> Pixmap {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{src:?}: {}", it.error_report(&e)));
    it.gfx().pixmap.clone()
}

fn render_gs(src: &str) -> Pixmap {
    let ps = scratch("raster.ps");
    let png = scratch("raster.png");
    std::fs::write(&ps, format!("false setstrokeadjust\n{src}\nshowpage\n")).unwrap();
    let status = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-sDEVICE=png16m",
            &format!("-g{SIZE}x{SIZE}"),
            "-r72",
            &format!("-o{}", png.display()),
        ])
        .arg(&ps)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs failed on {src:?}");
    let data = std::fs::read(&png).expect("gs output");
    let _ = std::fs::remove_file(&ps);
    let _ = std::fs::remove_file(&png);
    Pixmap::decode_png(&data).expect("decode gs png")
}

/// (mean absolute luma difference over the whole page, fraction of
/// pixels that differ by more than 128 of 255, ink pixels in `a`).
fn compare(a: &Pixmap, b: &Pixmap) -> (f64, f64, usize) {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    let luma = |d: &[u8]| (u32::from(d[0]) + u32::from(d[1]) + u32::from(d[2])) as f64 / 3.0;
    let (mut sum, mut hard, mut ink) = (0.0, 0usize, 0usize);
    for (pa, pb) in a.data().chunks(4).zip(b.data().chunks(4)) {
        let (la, lb) = (luma(pa), luma(pb));
        sum += (la - lb).abs();
        if (la - lb).abs() > 128.0 {
            hard += 1;
        }
        if la < 128.0 {
            ink += 1;
        }
    }
    let n = f64::from(a.width() * a.height());
    (sum / n, hard as f64 / n, ink)
}

/// Path + stroke attributes, painted by `stroke` or `strokepath fill`.
/// Everything lives well inside the 400x400 page.
const CASES: &[(&str, &str)] = &[
    ("butt line", "100 100 moveto 300 100 lineto 20 setlinewidth"),
    (
        "round cap",
        "100 100 moveto 300 100 lineto 20 setlinewidth 1 setlinecap",
    ),
    (
        "square cap",
        "100 100 moveto 300 100 lineto 20 setlinewidth 2 setlinecap",
    ),
    (
        "miter join",
        "100 100 moveto 200 200 lineto 300 100 lineto 20 setlinewidth",
    ),
    (
        "round join",
        "100 100 moveto 200 200 lineto 300 100 lineto 20 setlinewidth 1 setlinejoin",
    ),
    (
        "bevel join",
        "100 100 moveto 200 200 lineto 300 100 lineto 20 setlinewidth 2 setlinejoin",
    ),
    (
        "miter limit falls back to bevel",
        "100 100 moveto 200 130 lineto 300 100 lineto 20 setlinewidth 1.5 setmiterlimit",
    ),
    (
        "even dash",
        "100 100 moveto 300 100 lineto 20 setlinewidth [10 5] 0 setdash",
    ),
    (
        "dash with phase and round caps",
        "100 100 moveto 300 100 lineto 20 setlinewidth [10 5] 3 setdash 1 setlinecap",
    ),
    (
        "odd dash array repeats",
        "100 100 moveto 300 100 lineto 20 setlinewidth [10 5 3] 0 setdash",
    ),
    (
        "single-element dash",
        "100 100 moveto 300 100 lineto 20 setlinewidth [10] 0 setdash",
    ),
    (
        "curve",
        "100 100 moveto 100 300 300 300 300 100 curveto 12 setlinewidth",
    ),
    (
        "curve, round cap and join",
        "100 100 moveto 100 300 300 300 300 100 curveto 12 setlinewidth 1 setlinecap 1 setlinejoin",
    ),
    (
        "closed circle",
        "200 200 80 0 360 arc closepath 14 setlinewidth",
    ),
    (
        "dashed circle",
        "200 200 80 0 360 arc closepath 8 setlinewidth [12 6] 0 setdash",
    ),
    (
        "scaled CTM",
        "2 2 scale 50 50 moveto 100 50 lineto 100 100 lineto 6 setlinewidth 1 setlinejoin [8 4] 0 setdash",
    ),
    (
        "rotated CTM",
        "200 150 translate 30 rotate 0 0 moveto 120 0 lineto 120 60 lineto 12 setlinewidth [14 6] 0 setdash",
    ),
    (
        "zero-width line",
        "100 100 moveto 300 100 lineto 300 200 lineto 0 setlinewidth",
    ),
    (
        "subpaths, one closed",
        "50 50 moveto 150 50 lineto 100 120 lineto closepath 250 60 moveto 350 120 lineto 10 setlinewidth",
    ),
];

#[test]
fn butt_line_outline_is_the_stroked_rectangle() {
    // gs-pinned (stroke adjust off): a 20-wide horizontal line from
    // x=100..300 at y=100 is the rectangle 100..300 x 90..110. One
    // subpath, closed, no curves (gs writes the closing edge as the
    // close itself, pscat as a line then a close: three or four lines).
    assert_eq!(
        bbox("newpath 100 100 moveto 300 100 lineto 20 setlinewidth strokepath"),
        Some([100.0, 90.0, 300.0, 110.0])
    );
    assert_eq!(
        eval(
            "newpath 10 10 moveto 50 10 lineto 10 setlinewidth strokepath \
             /m 0 def /l 0 def /c 0 def /z 0 def \
             { pop pop /m m 1 add def } { pop pop /l l 1 add def } \
             { 6 { pop } repeat /c c 1 add def } { /z z 1 add def } pathforall m l c z"
        ),
        ["1", "4", "0", "1"]
    );
}

#[test]
fn caps_extend_the_outline() {
    // gs-pinned bboxes: projecting square and round caps grow the box by
    // half the width at both ends.
    for cap in [1, 2] {
        assert_eq!(
            bbox(&format!(
                "newpath 100 100 moveto 300 100 lineto 20 setlinewidth {cap} setlinecap strokepath"
            )),
            Some([90.0, 90.0, 310.0, 110.0]),
            "cap {cap}"
        );
    }
}

#[test]
fn joins_and_miter_limit_shape_the_corner() {
    // gs-pinned: apex y of the 90-degree corner at (200,200), width 20.
    let top = |extra: &str| {
        bbox(&format!(
            "newpath 100 100 moveto 200 200 lineto 300 100 lineto 20 setlinewidth {extra} strokepath"
        ))
        .unwrap()[3]
    };
    assert!((top("") - 214.142).abs() < 0.01, "miter {}", top(""));
    assert!((top("2 setlinejoin") - 207.071).abs() < 0.01, "bevel");
    assert!((top("1 setlinejoin") - 210.0).abs() < 0.01, "round");
    // A miter limit of 1.0 turns the miter into a bevel.
    assert!((top("1 setmiterlimit") - 207.071).abs() < 0.01, "limit");
}

#[test]
fn dashes_cut_the_outline_into_pieces() {
    let subpaths = |setdash: &str| {
        eval(&format!(
            "newpath 100 100 moveto 300 100 lineto 20 setlinewidth {setdash} strokepath \
             /m 0 def {{ pop pop /m m 1 add def }} {{ pop pop }} {{ 6 {{ pop }} repeat }} {{ }} \
             pathforall m"
        ))
    };
    // 200 long, [10 5]: 14 full dashes and a 5-long tail dash... counted
    // against gs below; here just that dashing multiplies the pieces.
    assert_eq!(subpaths("[] 0 setdash"), ["1"]);
    assert!(subpaths("[10 5] 0 setdash")[0].parse::<u32>().unwrap() >= 13);
    // Odd arrays double (PLRM): [10] is 10 on, 10 off => 10 dashes, and
    // the last one ends at 290.
    assert_eq!(subpaths("[10] 0 setdash"), ["10"]);
    let b = bbox("newpath 100 100 moveto 300 100 lineto 20 setlinewidth [10] 0 setdash strokepath")
        .unwrap();
    assert!(near(b, [100.0, 90.0, 290.0, 110.0], 1e-3), "{b:?}");
    // gs-pinned: [10 5 3] is 10 on, 5 off, 3 on, 10 off, 5 on, 3 off.
    let b =
        bbox("newpath 100 100 moveto 300 100 lineto 20 setlinewidth [10 5 3] 0 setdash strokepath")
            .unwrap();
    assert!(near(b, [100.0, 90.0, 298.0, 110.0], 1e-3), "{b:?}");
}

#[test]
fn empty_and_degenerate_paths_give_an_empty_path() {
    // gs-pinned: no error, and the path stays empty (nocurrentpoint).
    assert_eq!(bbox("newpath strokepath"), None);
    assert_eq!(
        bbox("newpath 100 100 moveto 10 setlinewidth strokepath"),
        None
    );
    // A zero-length subpath with butt caps paints nothing.
    assert_eq!(
        bbox("newpath 100 100 moveto 100 100 lineto 10 setlinewidth 0 setlinecap strokepath"),
        None
    );
    // ... but with round caps it is a dot (gs-pinned).
    let dot = bbox("newpath 100 100 moveto 100 100 lineto 10 setlinewidth 1 setlinecap strokepath")
        .unwrap();
    assert!(near(dot, [95.0, 95.0, 105.0, 105.0], 1e-3), "{dot:?}");
    // The stack is untouched either way.
    assert_eq!(eval("newpath strokepath"), Vec::<String>::new());
}

#[test]
fn zero_width_is_one_device_pixel() {
    // gs-pinned: a 0 setlinewidth line is one device pixel thick.
    let b = bbox("newpath 100 100 moveto 300 100 lineto 0 setlinewidth strokepath").unwrap();
    assert!(near(b, [100.0, 99.5, 300.0, 100.5], 1e-3), "{b:?}");
    // Under a 2x scale that pixel is half a user unit (gs-pinned).
    let b = bbox("2 2 scale newpath 50 50 moveto 150 50 lineto 0 setlinewidth strokepath").unwrap();
    assert!(near(b, [50.0, 49.75, 150.0, 50.25], 1e-3), "{b:?}");
}

#[test]
fn gsave_strokepath_pathbbox_grestore_is_the_stroked_footprint() {
    // The idiom from the drykit prototype (#233): measure the stroked
    // footprint of a centerline without disturbing the path.
    assert_eq!(
        eval(
            "newpath 100 100 moveto 300 100 lineto 20 setlinewidth \
             gsave strokepath pathbbox grestore pathbbox"
        ),
        [
            "100.0", "90.0", "300.0", "110.0", // the stroked outline
            "100.0", "100.0", "300.0", "100.0", // the centerline, restored
        ]
    );
}

#[test]
fn result_is_reported_in_current_user_space() {
    // The outline is stored in device space; pathbbox maps it back
    // through the CTM in force when it is asked, so a scale between
    // strokepath and pathbbox rescales the box.
    let b = bbox("newpath 10 10 moveto 30 10 lineto 4 setlinewidth strokepath 2 2 scale").unwrap();
    assert!(near(b, [5.0, 4.0, 15.0, 6.0], 1e-3), "{b:?}");
}

#[test]
fn strokepath_paints_nothing_and_its_result_is_an_ordinary_path() {
    let pm = render_pscat("newpath 100 100 moveto 300 100 lineto 20 setlinewidth strokepath");
    assert!(pm.data().chunks(4).all(|p| p[0] == 255 && p[1] == 255));
    // A second strokepath strokes the outline: the box grows by half the
    // line width on every side.
    let b = bbox("newpath 100 100 moveto 300 100 lineto 20 setlinewidth strokepath strokepath")
        .unwrap();
    assert!(near(b, [90.0, 80.0, 310.0, 120.0], 1e-3), "{b:?}");
}

#[test]
fn the_outline_can_be_clipped_to_and_filled() {
    // 20-wide line at y=100; clip to its stroke outline, then fill the
    // whole page red: only the outline's pixels turn red.
    let pm = render_pscat(
        "newpath 100 100 moveto 300 100 lineto 20 setlinewidth strokepath clip \
         newpath 1 0 0 setrgbcolor 0 0 400 400 rectfill",
    );
    let red = |x: u32, y: u32| {
        let i = ((y * SIZE + x) * 4) as usize;
        pm.data()[i + 1] < 40
    };
    // PostScript y=100 is device row SIZE-100.
    assert!(red(200, SIZE - 100));
    assert!(red(200, SIZE - 105));
    assert!(!red(200, SIZE - 125));
    assert!(!red(50, SIZE - 100));
}

#[test]
fn fill_of_strokepath_matches_stroke() {
    // pscat's own rasterizer, both ways. Hairline widths take a
    // different tiny-skia code path, so the zero-width case is covered
    // by the gs comparison below instead.
    for (name, path) in CASES {
        if name.contains("zero-width") {
            continue;
        }
        let stroked = render_pscat(&format!("newpath {path} 0 setgray stroke"));
        let filled = render_pscat(&format!("newpath {path} strokepath 0 setgray fill"));
        let (mean, hard, ink) = compare(&stroked, &filled);
        assert!(ink > 500, "{name}: nothing inked");
        assert!(
            mean < 0.5 && hard < 0.0005,
            "{name}: mean {mean:.3}, hard {hard:.5}"
        );
    }
}

#[test]
fn strokepath_bbox_matches_ghostscript() {
    if !gs_available() {
        eprintln!("skipping: ghostscript (gs) not installed");
        return;
    }
    for (name, path) in CASES {
        let src = format!("newpath {path} strokepath");
        match (bbox(&src), gs_bbox(&src)) {
            (Some(a), Some(b)) => {
                // Most cases agree to a few thousandths. Round joins:
                // gs reports the control-polygon extent of its Bezier
                // arcs (about 1 unit past the arc at width 20); pscat's
                // box is the tighter true one.
                let tol = if name.contains("round join") {
                    1.0
                } else {
                    0.05
                };
                assert!(near(a, b, tol), "{name}: pscat {a:?} vs gs {b:?}");
            }
            (None, None) => {}
            (a, b) => panic!("{name}: pscat {a:?} vs gs {b:?}"),
        }
    }
}

#[test]
fn fill_of_strokepath_matches_ghostscript_stroke() {
    if !gs_available() {
        eprintln!("skipping: ghostscript (gs) not installed");
        return;
    }
    for (name, path) in CASES {
        let ours = render_pscat(&format!("newpath {path} strokepath 0 setgray fill"));
        let theirs = render_gs(&format!("newpath {path} 0 setgray stroke"));
        let (mean, hard, ink) = compare(&ours, &theirs);
        assert!(ink > 100, "{name}: nothing inked");
        // gs does not anti-alias, so slanted edges differ from any AA
        // rasterizer by about a pixel's worth of grey (mean up to ~1.2
        // over the page here). What matters is that filling the outline
        // is no further from gs than pscat's own `stroke` is.
        let (smean, shard, _) = compare(
            &render_pscat(&format!("newpath {path} 0 setgray stroke")),
            &theirs,
        );
        assert!(
            mean < 1.6 && hard < 0.006 && mean <= smean + 0.1 && hard <= shard + 0.0005,
            "{name}: strokepath fill mean {mean:.3} hard {hard:.5}, stroke mean {smean:.3} hard {shard:.5}"
        );
    }
}

#[test]
fn ghostscript_accepts_strokepath() {
    if !gs_available() {
        eprintln!("skipping: ghostscript (gs) not installed");
        return;
    }
    // gs must run every case itself, so the programs are valid
    // PostScript and not just valid pscat.
    for (_, path) in CASES {
        let _ = render_gs(&format!("newpath {path} strokepath fill"));
    }
}
