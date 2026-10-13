//! drykit (issue #234, slice D1; design `docs/DRYMEDIA.md`): `dksurface`
//! and the swept-band oriented catch `dkcatch`, driven directly the way
//! D2's `dkstroke` will call it -- a catch, then the stroke's ink through it.
//!
//! The numbered tests are DRYMEDIA.md section 8's: 1 (shared surface),
//! 2 (nesting) and 3 (reach). D2 extends test 3's matrix with the keys it
//! adds (`/Tilt`, `/Wear`, `/Pressure`, `/Fallout`) and adds tests 4-8.
//! Guards, the private generator, "no draws from the caller's stream" and
//! the paintkit contract are `%%SelfTest` blocks in `lib/drykit.ps`.

use pscat::Interp;

fn load(it: &mut Interp) {
    for path in ["lib/artkit.ps", "lib/paintkit.ps", "lib/drykit.ps"] {
        let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
}

/// A `w` x `h` point page at `scale` device pixels per point, white, with
/// drykit loaded and `src` run on it.
fn render(w: u32, h: u32, scale: f32, src: &str) -> Interp {
    let mut it = Interp::with_page_scaled(w, h, scale).expect("page");
    load(&mut it);
    it.run_str(&format!(
        "1 1 1 setrgbcolor clippath fill 0 0 0 setrgbcolor {src}"
    ))
    .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    it
}

fn luma(p: tiny_skia::PremultipliedColorU8) -> f32 {
    0.3 * p.red() as f32 + 0.59 * p.green() as f32 + 0.11 * p.blue() as f32
}

fn lumas(it: &Interp) -> Vec<f32> {
    it.gfx().pixmap.pixels().iter().map(|&p| luma(p)).collect()
}

fn inked(it: &Interp) -> Vec<bool> {
    lumas(it).into_iter().map(|l| l < 128.0).collect()
}

/// The phi (Matthews) coefficient between two binary images over `mask`.
fn phi(a: &[bool], b: &[bool], mask: &[bool]) -> f64 {
    let (mut n11, mut n10, mut n01, mut n00) = (0f64, 0f64, 0f64, 0f64);
    for i in 0..a.len() {
        if !mask[i] {
            continue;
        }
        match (a[i], b[i]) {
            (true, true) => n11 += 1.0,
            (true, false) => n10 += 1.0,
            (false, true) => n01 += 1.0,
            (false, false) => n00 += 1.0,
        }
    }
    let den = ((n11 + n10) * (n01 + n00) * (n11 + n01) * (n10 + n00)).sqrt();
    if den == 0.0 {
        0.0
    } else {
        (n11 * n00 - n10 * n01) / den
    }
}

/// A 12pt-wide, 60pt-long band through the middle of a 40pt page at
/// `angle`, filled through `dkcatch` at `touch` (1 = no catch). The surface
/// is the one DRYMEDIA.md 2.2's numbers were measured on (pkcanvas's
/// default /Relief 0.7 and /Mottle 8 W), at drykit's fine /Weave 0.4.
fn band(seed: u32, angle: u32, touch: &str) -> Interp {
    render(
        40,
        40,
        10.0,
        &format!(
            "<< /Seed {seed} /Weave 0.4 /Mottle 3.2 /Relief 0.7 >> pkcanvas \
             gsave 20 20 translate {angle} rotate \
             [ [ [-30 0 6] [30 0 6] ] ] {touch} 1.8 dkcatch \
             -30 -6 60 12 rectfill grestore"
        ),
    )
}

/// Test 1 (DRYMEDIA.md 2.2, Q4): two crossing strokes break up on the same
/// tooth. Bands at 0 and 60 degrees, each rendered alone, compared over
/// their overlap, at 10 device pixels per point -- 4 per cell at /Weave 0.4,
/// so the comparison sees knuckles rather than anti-aliasing. The control
/// is the second band on another seed's surface.
///
/// Measured: mean 0.51 here (control 0.03). Phi falls with /Relief: the
/// /slate starting value 0.55 gives a mean of 0.38, under Q4's bar, because
/// each knuckle's own height then outweighs the shared mottle. D2's specimen
/// re-tunes the surfaces and must re-check this per surface.
#[test]
fn crossing_strokes_share_the_tooth() {
    let overlap: Vec<bool> = {
        let a = inked(&band(3, 0, "1"));
        let b = inked(&band(3, 60, "1"));
        a.iter().zip(&b).map(|(x, y)| *x && *y).collect()
    };
    assert!(
        overlap.iter().filter(|&&o| o).count() > 10_000,
        "the bands should overlap"
    );
    let mut shared = Vec::new();
    let mut control = Vec::new();
    for seed in [3, 7, 11, 19, 23] {
        let a = inked(&band(seed, 0, "0.6"));
        let b = inked(&band(seed, 60, "0.6"));
        let c = inked(&band(seed + 100, 60, "0.6"));
        shared.push(phi(&a, &b, &overlap));
        control.push(phi(&a, &c, &overlap));
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    eprintln!("phi shared {shared:.2?} control {control:.2?}");
    assert!(
        mean(&shared) >= 0.4,
        "mean phi {:.3} < 0.4: {shared:.2?}",
        mean(&shared)
    );
    assert!(
        mean(&shared) >= mean(&control) + 0.25,
        "shared mean {:.3} not 0.25 above the control's {:.3}",
        mean(&shared),
        mean(&control)
    );
}

/// A bent four-segment centerline, as stops `[x y hw]`.
fn zigzag(hw: f64) -> String {
    format!("[ [10 30 {hw}] [40 70 {hw}] [70 25 {hw}] [100 65 {hw}] [130 35 {hw}] ]")
}

/// The same centerline as a round-capped, round-joined stroke `width` wide.
const ZIG_INK: &str = "newpath 10 30 moveto 40 70 lineto 70 25 lineto 100 65 lineto \
                       130 35 lineto 1 setlinecap 1 setlinejoin stroke";

/// Test 2: a catch at touch t1 is inside the catch at t2 > t1, including
/// when the second is cut for a wider stroke (D2's dim level is both higher
/// and wider). Each knuckle takes its orientation from its nearest segment,
/// whatever the reach, so the bent run's joints can't re-orient a knuckle
/// between the two. Compared under the narrow stroke's ink.
#[test]
fn a_lighter_touch_catches_inside_a_heavier_one() {
    let mark = |touch: &str, hw: f64| {
        render(
            140,
            100,
            4.0,
            &format!(
                "<< /Seed 4 /Weave 0.6 /Mottle 6 /Relief 0.65 >> pkcanvas \
                 gsave [ {} ] {touch} 1.8 dkcatch 4 setlinewidth {ZIG_INK} grestore",
                zigzag(hw)
            ),
        )
    };
    let pairs = [
        ("0.5", 2.0, "0.62", 2.0),
        ("0.6", 2.0, "0.72", 3.5),
        ("0.75", 2.0, "0.95", 3.0),
    ];
    for (t1, w1, t2, w2) in pairs {
        let lo = lumas(&mark(t1, w1));
        let hi = lumas(&mark(t2, w2));
        let ink_lo = lo.iter().filter(|&&l| l < 128.0).count();
        let ink_hi = hi.iter().filter(|&&l| l < 128.0).count();
        assert!(ink_lo > 500, "touch {t1} caught almost nothing");
        assert!(ink_hi > ink_lo, "touch {t2} caught no more than {t1}");
        // Coverage only grows: every pixel is at least as dark at t2.
        let outside = lo.iter().zip(&hi).filter(|(l, h)| **h > **l + 1.0).count();
        assert_eq!(
            outside, 0,
            "{outside} pixels inked at touch {t1} are lighter at touch {t2}"
        );
    }
}

/// Pixels whose luma differs by more than anti-aliasing noise. A whole-page
/// clip path has far larger bounds than the band's, and the rasterizer's
/// fixed-point precision follows the bounds, so the same edge pixel can
/// come out a few levels apart (measured up to 16 of 255, in both
/// directions -- a pixel *lighter* with more clip, which no missing knuckle
/// can explain). A missed knuckle leaves whole pixels uninked.
fn missed(a: &Interp, b: &Interp) -> usize {
    lumas(a)
        .iter()
        .zip(lumas(b))
        .filter(|(p, q)| (**p - q).abs() > 40.0)
        .count()
}

/// Test 3 (CS 6 test 7): the swept band misses no knuckle that touches the
/// ink. Each case renders once with the band and once with
/// `userdict /pjWholePage` set, where every other cell under the clip joins
/// as a disc as long as the longest knuckle (so it reaches as far as any
/// orientation could); the two must match pixel for pixel, up to
/// anti-aliasing noise (`missed`). `/dkslack 1.5`
/// makes "touches" include sharing an anti-aliased edge pixel.
#[test]
fn the_swept_band_misses_no_knuckle_that_touches_the_ink() {
    // (name, space set before the canvas, space set after it, runs, touch,
    // streak, ink). Ink is drawn per segment at the wider stop's width, the
    // reach dkcatch promises.
    let cases: [(&str, &str, &str, String, &str, &str, &str); 6] = [
        (
            "longest knuckles: streak 4 at the top of the touch range",
            "",
            "",
            format!("[ {} ]", zigzag(2.0)),
            "0.99",
            "4",
            &format!("4 setlinewidth {ZIG_INK}"),
        ),
        (
            "a sheared, non-uniform space after the canvas",
            "",
            "20 10 translate 1.6 0.6 scale 25 rotate",
            format!("[ {} ]", zigzag(1.5)),
            "0.9",
            "3",
            &format!("3 setlinewidth {ZIG_INK}"),
        ),
        (
            "a canvas set up in a rotated, scaled space",
            "70 50 translate 30 rotate 1.5 1.5 scale -70 -50 translate",
            "",
            format!("[ {} ]", zigzag(2.0)),
            "0.9",
            "2.5",
            &format!("4 setlinewidth {ZIG_INK}"),
        ),
        (
            "a one-stop run (a dot) and a zero-length run",
            "",
            "",
            "[ [ [40 50 5] ] [ [100 50 3] [100 50 3] ] ]".to_string(),
            "0.95",
            "4",
            "newpath 40 50 5 0 360 arc fill newpath 100 50 3 0 360 arc fill",
        ),
        (
            "half-widths that change along the run",
            "",
            "",
            "[ [ [10 30 1] [40 70 6] [70 25 2] [100 65 0] [130 35 4] ] ]".to_string(),
            "0.9",
            "3",
            "1 setlinecap \
             12 setlinewidth newpath 10 30 moveto 40 70 lineto stroke \
             12 setlinewidth newpath 40 70 moveto 70 25 lineto stroke \
             4 setlinewidth newpath 70 25 moveto 100 65 lineto stroke \
             8 setlinewidth newpath 100 65 moveto 130 35 lineto stroke",
        ),
        (
            "two runs that cross",
            "",
            "",
            "[ [ [10 20 2] [130 80 2] ] [ [10 80 2] [130 20 2] ] ]".to_string(),
            "0.9",
            "4",
            "1 setlinecap 4 setlinewidth \
             newpath 10 20 moveto 130 80 lineto stroke \
             newpath 10 80 moveto 130 20 lineto stroke",
        ),
    ];
    for (name, pre, post, runs, touch, streak, ink) in &cases {
        let src = |whole: bool| {
            format!(
                "/dkslack 1.5 def {} {pre} \
                 << /Seed 6 /Weave 0.6 /Mottle 6 /Relief 0.65 >> pkcanvas \
                 initmatrix {post} \
                 gsave {runs} {touch} {streak} dkcatch {ink} grestore",
                if whole {
                    "userdict /pjWholePage true put"
                } else {
                    ""
                }
            )
        };
        let band = render(140, 100, 2.0, &src(false));
        let whole = render(140, 100, 2.0, &src(true));
        let ink = inked(&band).iter().filter(|&&i| i).count();
        assert!(ink > 100, "{name}: the band caught almost nothing");
        let diff = missed(&band, &whole);
        assert_eq!(
            diff, 0,
            "{name}: {diff} pixels differ from the whole-page catch"
        );
    }
}

/// The reach test above must be able to fail: with the band cut short (a
/// half-width of 0 for a stroke that is 4pt wide), it does.
#[test]
fn the_reach_test_detects_a_short_band() {
    let src = |whole: bool| {
        format!(
            "/dkslack 1.5 def {} \
             << /Seed 6 /Weave 0.6 >> pkcanvas \
             gsave [ {} ] 0.9 1.8 dkcatch 4 setlinewidth {ZIG_INK} grestore",
            if whole {
                "userdict /pjWholePage true put"
            } else {
                ""
            },
            zigzag(0.0)
        )
    };
    let a = render(140, 100, 2.0, &src(false));
    let b = render(140, 100, 2.0, &src(true));
    let diff = missed(&a, &b);
    assert!(
        diff > 50,
        "a band cut for zero width differs in only {diff} pixels: the reach test proves nothing"
    );
}

/// The catch is fixed in canvas space: the same stroke drawn after the
/// space moves catches on the same knuckles where it overlaps, and a run
/// drawn in a scaled space reaches as far as its ink in that space.
#[test]
fn the_catch_is_anchored_to_the_canvas() {
    let mark = |pre: &str, post: &str| {
        inked(&render(
            140,
            100,
            2.0,
            &format!(
                "{pre} << /Seed 2 /Weave 0.6 >> pkcanvas {post} \
                 gsave [ [ [0 50 6] [140 50 6] ] ] 0.6 1.8 dkcatch \
                 0 44 140 12 rectfill grestore"
            ),
        ))
    };
    let still = mark("", "");
    let moved = mark("", "6 0 translate");
    let shifted = mark("6 0 translate", "");
    let w = 280;
    let diff = |a: &[bool], b: &[bool]| {
        (0..a.len())
            .filter(|&i| {
                let x = i % w;
                (40..240).contains(&x) && a[i] != b[i]
            })
            .count()
    };
    assert_eq!(
        diff(&still, &moved),
        0,
        "a stroke moved after the canvas caught on a different tooth"
    );
    assert!(
        diff(&still, &shifted) > 500,
        "a canvas set up in a moved space should be a moved canvas"
    );
}

/// Every key `dksurface` advertises changes the surface (the pkbroad
/// pattern), and spelling out a surface's documented values changes nothing.
#[test]
fn dksurface_every_documented_parameter_changes_the_render() {
    let catch = |opts: &str| {
        render(
            100,
            60,
            4.0,
            &format!(
                "<< {opts} >> dksurface userdict /dkGround get aload pop setrgbcolor \
                 clippath fill 1 1 1 setrgbcolor \
                 gsave [ [ [5 30 15] [95 30 15] ] ] 0.6 1.4 dkcatch \
                 5 15 90 30 rectfill grestore"
            ),
        )
        .gfx()
        .pixmap
        .data()
        .to_vec()
    };
    let baseline = catch("");
    for (param, value) in [
        ("Surface", "/slate"),
        ("Seed", "2"),
        ("Weave", "0.9"),
        ("Mottle", "30"),
        ("Relief", "0.1"),
        ("Ground", "[0.2 0.3 0.4]"),
    ] {
        assert_ne!(
            catch(&format!("/{param} {value}")),
            baseline,
            "/{param} {value} rendered the same as the default: a no-op"
        );
    }
    assert_eq!(
        catch(
            "/Surface /drawing /Seed 1 /Weave 0.6 /Mottle 6 /Relief 0.65 /Ground [0.95 0.94 0.90]"
        ),
        baseline,
        "the documented defaults are not the defaults"
    );
}

#[test]
fn dksurface_advertises_exactly_the_parameters_it_implements() {
    let src = std::fs::read_to_string("lib/drykit.ps").expect("read drykit");
    let start = src
        .find("% @example: << /Surface /slate /Seed 3 >> dksurface")
        .expect("dksurface tags");
    let end = src[start..].find("\n/dksurface ").expect("dksurface def") + start;
    let mut advertised: Vec<&str> = src[start..end]
        .lines()
        .filter_map(|l| l.strip_prefix("% @param: /"))
        .map(|l| l.split_whitespace().next().unwrap_or(""))
        .collect();
    advertised.sort_unstable();
    assert_eq!(
        advertised,
        ["Ground", "Mottle", "Relief", "Seed", "Surface", "Weave"]
    );
}

/// The `dkcatch` path is built in first-seen order, never dict order, so
/// the same catch exported twice is the same file (#258 was an SVG that
/// changed between runs).
#[test]
fn the_catch_exports_the_same_svg_every_time() {
    let svg = || {
        let mut it = Interp::with_page(60, 40).expect("page");
        it.gfx_mut().enable_svg();
        load(&mut it);
        it.run_str(
            "<< /Seed 3 /Weave 0.9 >> pkcanvas 0 0 0 setrgbcolor \
             gsave [ [ [5 20 4] [30 30 4] [55 15 4] ] ] 0.6 1.8 dkcatch \
             5 10 50 25 rectfill grestore showpage",
        )
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.gfx().svg_pages().expect("recording enabled").concat()
    };
    let first = svg();
    assert!(
        first.contains("clipPath"),
        "the catch should export as a clip"
    );
    for _ in 0..3 {
        assert_eq!(
            svg(),
            first,
            "the SVG of the same catch changed between runs"
        );
    }
}

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Fraction of inked pixels inside the band, rendered by Ghostscript.
fn gs_band_ink(prelude: &str, body: &str, px: (usize, usize), dpi: u32) -> f64 {
    let mut src = String::new();
    for path in ["lib/artkit.ps", "lib/paintkit.ps", "lib/drykit.ps"] {
        src.push_str(&std::fs::read_to_string(path).expect("read lib"));
        src.push('\n');
    }
    let dir = std::env::temp_dir().join(format!("pscat-drykit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join(format!("gs-{}.ps", prelude.len()));
    let png = dir.join(format!("gs-{}.png", prelude.len()));
    std::fs::write(
        &file,
        format!("{prelude}\n{src}\n1 setgray clippath fill 0 setgray {body}\nshowpage\n"),
    )
    .expect("write");
    let status = std::process::Command::new("gs")
        .args(["-dNOPAUSE", "-dBATCH", "-q", "-sDEVICE=pnggray"])
        .arg(format!("-g{}x{}", px.0, px.1))
        .arg(format!("-r{dpi}"))
        .arg(format!("-o{}", png.display()))
        .arg(&file)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected the drykit driver");
    let data = std::fs::read(&png).expect("read gs png");
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_file(&png);
    let img = tiny_skia::Pixmap::decode_png(&data).expect("decode gs png");
    let ink = img.pixels().iter().filter(|p| luma(**p) < 128.0).count();
    ink as f64 / img.pixels().len() as f64
}

/// DRYMEDIA.md 2.4 and test 6's first half: Ghostscript renders a fine
/// catch as texture, not as a solid stroke, because dkcatch switches its
/// fill adjust off. Ink coverage of a band filled through a /Weave 0.6
/// catch at 216 dpi must be close to pscat's; with the guard defeated
/// (a no-op `.setfilladjust2` found first) gs closes the gaps, which proves
/// the comparison can tell. Runs locally only: CI has no gs.
#[test]
fn ghostscript_keeps_the_tooth_open() {
    if !gs_available() {
        eprintln!("skipping gs fill-adjust check: gs not installed");
        return;
    }
    let body = "<< /Seed 5 /Weave 0.6 >> pkcanvas \
                gsave [ [ [0 20 20] [100 20 20] ] ] 0.6 1.6 dkcatch 0 0 100 40 rectfill grestore";
    let pscat = {
        let it = render(100, 40, 3.0, body);
        let ink = inked(&it);
        ink.iter().filter(|&&i| i).count() as f64 / ink.len() as f64
    };
    let gs = gs_band_ink("", body, (300, 120), 216);
    let gs_unguarded = gs_band_ink(
        "userdict /.setfilladjust2 { pop pop } put",
        body,
        (300, 120),
        216,
    );
    eprintln!("ink: pscat {pscat:.3}, gs {gs:.3}, gs without the guard {gs_unguarded:.3}");
    assert!(
        pscat > 0.2 && pscat < 0.8,
        "pscat ink {pscat:.3}: not a broken catch"
    );
    assert!(
        (gs - pscat).abs() < 0.1,
        "gs ink {gs:.3} is far from pscat's {pscat:.3}"
    );
    assert!(
        gs_unguarded > gs + 0.1,
        "without the guard gs inked {gs_unguarded:.3}, no more than with it ({gs:.3}): \
         the check cannot see the fill adjust"
    );
}
