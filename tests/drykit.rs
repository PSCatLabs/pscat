//! drykit (issue #234, slice D1; design `docs/DRYMEDIA.md`): `dksurface`
//! and the swept-band oriented catch `dkcatch`, driven directly the way
//! D2's `dkstroke` will call it -- a catch, then the stroke's ink through it.
//!
//! The numbered tests are DRYMEDIA.md section 8's: 1 (shared surface),
//! 2 (nesting) and 3 (reach). D2 extends test 3's matrix with the keys it
//! adds (`/Tilt`, `/Wear`, `/Pressure`, `/Fallout`) and adds tests 4-8.
//! Guards, the private generator, "no draws from the caller's stream" and
//! the paintkit contract are `%%SelfTest` blocks in `lib/drykit.ps`.

mod common;

use common::within_cpu_budget;
use pscat::Interp;
use std::time::Duration;

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
/// `angle`, filled through `dkcatch` at `touch` (1 = no catch) on the
/// surface `surface` (a `dksurface` dict body) at `streak`.
fn band(surface: &str, seed: u32, angle: u32, touch: &str, streak: &str) -> Interp {
    render(
        40,
        40,
        10.0,
        &format!(
            "<< {surface} /Seed {seed} >> dksurface \
             gsave 20 20 translate {angle} rotate \
             [ [ [-30 0 6] [30 0 6] ] ] {touch} {streak} dkcatch \
             -30 -6 60 12 rectfill grestore"
        ),
    )
}

/// Test 1 (DRYMEDIA.md 2.2, Q4): two crossing strokes break up on the same
/// tooth. Bands at 0 and 60 degrees, each rendered alone, compared over
/// their overlap, at 10 device pixels per point -- 4 or more per cell, so
/// the comparison sees knuckles rather than anti-aliasing. The control is
/// the second band on another seed's surface. Q4's bar: mean phi over five
/// seeds >= 0.4, and >= 0.25 above the control's mean.
///
/// Every shipped surface is checked at the streak its medium uses
/// (DRYMEDIA.md 3.4: chalk on the boards and newsprint, charcoal on drawing
/// paper at the default 1.4; crayon on construction paper at about 1.8),
/// plus the surface the design measured on (pkcanvas's Relief 0.7).
/// Measured means: boards and newsprint 0.42, drawing 0.44 (0.46 at its
/// old /Mottle 6; /Relief 0.35 gave 0.34), construction
/// 0.42, the design's surface 0.51; controls -0.16 to 0.03. Phi falls as
/// /Relief falls or /Streak rises: the boards at /Streak 1.8 give 0.38.
#[test]
fn crossing_strokes_share_the_tooth() {
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    for (surface, streak) in [
        ("/Surface /slate", "1.4"),
        ("/Surface /greenboard", "1.4"),
        ("/Surface /newsprint", "1.4"),
        ("/Surface /drawing", "1.4"),
        ("/Surface /construction", "1.8"),
        ("/Weave 0.4 /Mottle 3.2 /Relief 0.7", "1.8"),
    ] {
        let overlap: Vec<bool> = {
            let a = inked(&band(surface, 3, 0, "1", streak));
            let b = inked(&band(surface, 3, 60, "1", streak));
            a.iter().zip(&b).map(|(x, y)| *x && *y).collect()
        };
        assert!(
            overlap.iter().filter(|&&o| o).count() > 10_000,
            "the bands should overlap"
        );
        let mut shared = Vec::new();
        let mut control = Vec::new();
        for seed in [3, 7, 11, 19, 23] {
            let a = inked(&band(surface, seed, 0, "0.6", streak));
            let b = inked(&band(surface, seed, 60, "0.6", streak));
            let c = inked(&band(surface, seed + 100, 60, "0.6", streak));
            shared.push(phi(&a, &b, &overlap));
            control.push(phi(&a, &c, &overlap));
        }
        eprintln!("{surface} streak {streak}: phi {shared:.2?} control {control:.2?}");
        assert!(
            mean(&shared) >= 0.4,
            "{surface}: mean phi {:.3} < 0.4: {shared:.2?}",
            mean(&shared)
        );
        assert!(
            mean(&shared) >= mean(&control) + 0.25,
            "{surface}: shared mean {:.3} not 0.25 above the control's {:.3}",
            mean(&shared),
            mean(&control)
        );
    }
}

/// The spread of ink coverage across 3pt blocks of a 60pt square filled
/// through one `dkcatch` at touch 0.6 on `surface`, as a standard deviation
/// of per-block coverage (0..1).
fn block_spread(surface: &str, seed: u32) -> f64 {
    let it = render(
        60,
        60,
        6.0,
        &format!(
            "<< {surface} /Seed {seed} >> dksurface \
             [ [ [0 30 32] [60 30 32] ] ] 0.6 1.4 dkcatch 0 0 60 60 rectfill"
        ),
    );
    let ink = inked(&it);
    let (side, blocks) = (18, 20); // 3pt at 6 px/pt, over 360 px
    let mut cover = Vec::new();
    for by in 0..blocks {
        for bx in 0..blocks {
            let mut n = 0;
            for y in 0..side {
                for x in 0..side {
                    n += usize::from(ink[(by * side + y) * 360 + bx * side + x]);
                }
            }
            cover.push(n as f64 / (side * side) as f64);
        }
    }
    let mean = cover.iter().sum::<f64>() / cover.len() as f64;
    (cover.iter().map(|c| (c - mean).powi(2)).sum::<f64>() / cover.len() as f64).sqrt()
}

/// Issue #313: on drykit's default drawing paper, a flat mid-touch fill
/// breaks up as specks, not in patches several points across. At the old
/// `/Mottle 6` the slow mottle decided which tooth stood proud over whole
/// patches, and fine hatching showed white islands the references do not.
/// The patchiness is the spread of ink coverage between 3pt blocks: the
/// coarse mottle's is about 1.8 times the default's, over five seeds.
#[test]
fn drawing_paper_breaks_up_in_specks_not_patches() {
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let seeds = [1, 3, 7, 11, 19];
    let fine: Vec<f64> = seeds
        .iter()
        .map(|&s| block_spread("/Surface /drawing", s))
        .collect();
    let coarse: Vec<f64> = seeds
        .iter()
        .map(|&s| block_spread("/Surface /drawing /Mottle 6", s))
        .collect();
    eprintln!("block spread: default {fine:.3?} /Mottle 6 {coarse:.3?}");
    assert!(
        mean(&fine) < 0.7 * mean(&coarse),
        "default drawing paper spread {:.3} is not well below /Mottle 6's {:.3}",
        mean(&fine),
        mean(&coarse)
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
/// come out a few levels apart (measured up to 16 of 255 in both directions
/// on /drawing's old coarse mottle -- a pixel *lighter* with more clip, which
/// no missing knuckle can explain). The finer mottle of #313 adds a rarer
/// case: a knuckle grazing the ink's edge by under a pixel, which the band's
/// reach drops and the whole page keeps. It costs one pixel a fifth lighter
/// (50 of 255; it is gone at `dkslack` 3, which no case needs). A missed
/// knuckle at the ink leaves whole pixels uninked (a band one cell short
/// differs in hundreds of them, below), so the bar is 60.
fn missed(a: &Interp, b: &Interp) -> usize {
    lumas(a)
        .iter()
        .zip(lumas(b))
        .filter(|(p, q)| (**p - q).abs() > 60.0)
        .count()
}

/// Device pixels per point for the reach renders: 4.2 pixels per cell at
/// /Weave 0.6, so a knuckle covers whole pixels and a missed one can't hide
/// inside anti-aliasing noise (the design's 4-pixels-per-cell rule).
const REACH_SCALE: f32 = 7.0;

/// One case rendered with the band and with the whole page, with `slack`
/// device pixels of extra reach (`dkslack`).
fn reach_pair(
    slack: f64,
    pre: &str,
    post: &str,
    runs: &str,
    touch: &str,
    streak: &str,
    ink: &str,
) -> (Interp, Interp) {
    let src = |whole: bool| {
        format!(
            "/dkslack {slack} def {} {pre} \
             << /Seed 6 /Weave 0.6 /Mottle 6 /Relief 0.65 >> pkcanvas \
             initmatrix 0 15 140 70 rectclip {post} \
             gsave {runs} {touch} {streak} dkcatch {ink} grestore",
            if whole {
                "userdict /pjWholePage true put"
            } else {
                ""
            }
        )
    };
    (
        render(140, 100, REACH_SCALE, &src(false)),
        render(140, 100, REACH_SCALE, &src(true)),
    )
}

/// Test 3 (CS 6 test 7): the swept band misses no knuckle that touches the
/// ink. Each case renders once with the band and once with
/// `userdict /pjWholePage` set, where every other cell under the clip joins
/// as a disc as long as the longest knuckle (so it reaches as far as any
/// orientation could); the two must match pixel for pixel, up to
/// anti-aliasing noise (`missed`). `/dkslack 1.5` makes "touches" include
/// sharing an anti-aliased edge pixel.
#[test]
fn the_swept_band_misses_no_knuckle_that_touches_the_ink() {
    // (name, space set before the canvas, space set after it, runs, touch,
    // streak, ink). Ink is drawn per segment at the wider stop's width, the
    // reach dkcatch promises.
    let cases: [(&str, &str, &str, String, &str, &str, &str); 7] = [
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
            "a sparse catch, where a missed knuckle shows",
            "",
            "",
            format!("[ {} ]", zigzag(2.0)),
            "0.6",
            "1.8",
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
        let (band, whole) = reach_pair(1.5, pre, post, runs, touch, streak, ink);
        let ink = inked(&band).iter().filter(|&&i| i).count();
        assert!(ink > 2000, "{name}: the band caught almost nothing");
        let diff = missed(&band, &whole);
        assert_eq!(
            diff, 0,
            "{name}: {diff} pixels differ from the whole-page catch"
        );
    }
}

/// The reach test above must be able to fail on a near miss: with the
/// band's reach one cell short (a negative `dkslack` of one cell's worth of
/// pixels), the knuckles it drops at the ink's edge are seen. It needs a
/// broken catch (touch 0.6, measured 294 pixels on the current drawing paper, 486 before #313): near touch 1 the catch is
/// all but solid and a dropped edge knuckle covers ink that is already
/// covered (0 pixels at touch 0.99). So the matrix above has a sparse case.
#[test]
fn the_reach_test_detects_a_band_one_cell_short() {
    let cell = -0.6 * f64::from(REACH_SCALE);
    let (band, whole) = reach_pair(
        cell,
        "",
        "",
        &format!("[ {} ]", zigzag(2.0)),
        "0.6",
        "1.8",
        &format!("4 setlinewidth {ZIG_INK}"),
    );
    let diff = missed(&band, &whole);
    eprintln!("a band one cell short: {diff} pixels differ");
    assert!(
        diff > 100,
        "a band one cell short differs in only {diff} pixels: the reach test proves nothing"
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
            "/Surface /drawing /Seed 1 /Weave 0.6 /Mottle 1.5 /Relief 0.65 /Ground [0.95 0.94 0.90]"
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
    // One name per call: the gs tests run on parallel threads of one
    // process, and two of them pass the same (empty) prelude.
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let file = dir.join(format!("gs-{n}.ps"));
    let png = dir.join(format!("gs-{n}.png"));
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

// ---------------------------------------------------------------------
// Slice D2: `dkstroke` and `dkbatch` (DRYMEDIA.md 3.1, 3.3; tests 3-8).
// ---------------------------------------------------------------------

/// A greenboard page `w` x `h` at `scale`, chalk colour set, running `body`.
fn board(w: u32, h: u32, scale: f32, body: &str) -> Interp {
    render(
        w,
        h,
        scale,
        &format!(
            "<< /Surface /greenboard /Seed 3 >> dksurface \
             userdict /dkGround get aload pop setrgbcolor clippath fill \
             0.93 0.94 0.90 setrgbcolor {body}"
        ),
    )
}

fn data(it: &Interp) -> Vec<u8> {
    it.gfx().pixmap.data().to_vec()
}

/// Test 3, extended to `dkstroke`'s own reach: every key that moves ink away
/// from the centerline, at its extreme, against the whole-page catch. A
/// sparse touch keeps a missed knuckle visible. /Fallout is drawn on top,
/// not through a catch, so its row only shows that fallout and /Drift
/// leave the catch alone.
#[test]
fn dkstroke_reaches_every_knuckle_its_ink_touches() {
    const PATH: &str = "newpath 6 12 moveto 30 40 lineto 64 18 lineto";
    let cases = [
        ("streak 4", "/Width 3 /Touch 0.6 /Streak 4"),
        (
            "tilt 90 on a 10pt side",
            "/Width 3 /Touch 0.6 /Tilt 90 /Side 10",
        ),
        ("wear 1", "/Width 3 /Touch 0.6 /Wear 1"),
        (
            "pressure above 1",
            "/Width 3 /Touch 0.6 /Pressure { pop 1.8 }",
        ),
        (
            "lanes with dropout",
            "/Width 4 /Touch 0.6 /Lanes 4 /Dropout 0.5 /Load 0.5",
        ),
        (
            "a touch profile over bands",
            "/Width 3 /Touch { 0.4 mul 0.5 add } /Bands 3",
        ),
        (
            "fallout with drift",
            "/Width 3 /Touch 0.6 /Fallout 2 /Drift 4",
        ),
        // A budget this small splits the run into pieces that share an end
        // stop, each its own catch, and the pieces must still meet.
        (
            "a run split under a small budget",
            "/Width 3 /Touch 0.6 /SPLIT true",
        ),
    ];
    for (name, opts) in cases {
        let budget = if opts.contains("/SPLIT") {
            "/dkbudget 1500 def"
        } else {
            ""
        };
        let src = |whole: bool| {
            format!(
                "/dkslack 1.5 def {budget} {} {PATH} << {opts} >> dkstroke \
                 newpath 50 44 moveto << /Width 4 /Touch 0.6 >> dkstroke",
                if whole {
                    "userdict /pjWholePage true put"
                } else {
                    ""
                }
            )
        };
        let t0 = common::thread_cpu_time();
        let paper = |src: &str| {
            render(
                70,
                50,
                REACH_SCALE,
                &format!("<< /Surface /drawing /Seed 3 >> dksurface 0 0 0 setrgbcolor {src}"),
            )
        };
        let band = paper(&src(false));
        let whole = paper(&src(true));
        eprintln!(
            "{name}: {:.2}s cpu",
            (common::thread_cpu_time() - t0).as_secs_f64()
        );
        let diff = missed(&band, &whole);
        assert_eq!(
            diff, 0,
            "{name}: {diff} pixels differ from the whole-page catch"
        );
    }
}

/// Test 4: determinism. The tool-side randomness (lanes with a contact
/// chain, fallout) is on in every case, or identity would prove nothing.
#[test]
fn dkstroke_is_deterministic_and_leaves_the_callers_marks_alone() {
    const OPTS: &str = "/Width 5 /Touch 0.7 /Lanes 3 /Dropout 0.4 /Load 0.5 /Fallout 1 /Drift 3";
    const A: &str = "newpath 10 30 moveto 60 70 lineto 110 40 lineto";
    // Same input, same pixels.
    let one = data(&board(140, 100, 2.0, &format!("{A} << {OPTS} >> dkstroke")));
    let two = data(&board(140, 100, 2.0, &format!("{A} << {OPTS} >> dkstroke")));
    assert_eq!(one, two, "the same stroke rendered differently twice");

    // A mark placed from the caller's stream after the stroke lands in
    // the same place whatever drykit was asked to do.
    let dot = |opts: &str| {
        let it = board(
            140,
            100,
            2.0,
            &format!(
                "5 srand {A} << {opts} >> dkstroke \
                 1 0 0 setrgbcolor rand 120 mod 10 add rand 80 mod 10 add 3 0 360 arc fill"
            ),
        );
        it.gfx()
            .pixmap
            .pixels()
            .iter()
            .map(|p| p.red() > 200 && p.green() < 60)
            .collect::<Vec<bool>>()
    };
    let base = dot(OPTS);
    assert!(base.iter().any(|&r| r), "the caller's mark is missing");
    for opts in [
        "/Width 9 /Lanes 5 /Dropout 0.7 /Fallout 4",
        "/Tilt 60 /Levels 3 /Touch { 0.8 mul } /Bands 5",
        "/Seed 99 /Pressure { 1 add } /Wear 2",
    ] {
        assert_eq!(
            dot(opts),
            base,
            "{opts} moved a mark drawn after the stroke"
        );
    }

    // Adding a stroke elsewhere re-rolls nothing in this one: its seed is
    // its own first point's, not a count of strokes drawn before it.
    let alone = board(140, 100, 2.0, &format!("{A} << {OPTS} >> dkstroke"));
    let with = board(
        140,
        100,
        2.0,
        &format!(
            "newpath 10 92 moveto 130 92 lineto << {OPTS} >> dkstroke \
             {A} << {OPTS} >> dkstroke"
        ),
    );
    let w = 280;
    let rows = |it: &Interp| -> Vec<u8> {
        // Rows of the page below y = 75pt (pixels 50.. from the top) hold
        // only stroke A and its fallout.
        let d = it.gfx().pixmap.data();
        d[56 * w * 4..].to_vec()
    };
    assert_eq!(
        rows(&alone),
        rows(&with),
        "a stroke elsewhere re-rolled this one"
    );
}

/// Test 5: level-major order. Two crossing strokes drawn as two subpaths of
/// one call and as two calls inside a `dkbatch` are pixel-identical (each
/// subpath is seeded by its own first point), and at the crossing no pixel
/// of either stroke's bright core is covered by the other's dim level. Two
/// separate calls, the control, do cover it: that is what the batch is for.
#[test]
fn dkbatch_draws_level_major_like_one_call() {
    const OPTS: &str = "/Width 6 /Touch 0.75 /Halo 1.5 /Dropout 0.3 /Load 0.6 /Lanes 2";
    const A: &str = "newpath 10 20 moveto 130 80 lineto";
    const B: &str = "newpath 10 80 moveto 130 20 lineto";
    let one_call = data(&board(
        140,
        100,
        3.0,
        &format!("{A} 10 80 moveto 130 20 lineto << {OPTS} >> dkstroke"),
    ));
    let batch = data(&board(
        140,
        100,
        3.0,
        &format!("{{ {A} << {OPTS} >> dkstroke {B} << {OPTS} >> dkstroke }} dkbatch"),
    ));
    assert_eq!(
        one_call, batch,
        "a batch of two calls differs from one call"
    );

    // The bright level: the stroke alone at a touch of 1 for its core.
    let core_of = |path: &str| -> Vec<bool> {
        let it = board(140, 100, 3.0, &format!("{path} << {OPTS} >> dkstroke"));
        let bright = it
            .gfx()
            .pixmap
            .pixels()
            .iter()
            .map(|p| luma(*p))
            .collect::<Vec<f32>>();
        let max = bright.iter().cloned().fold(0f32, f32::max);
        bright.iter().map(|&l| l > max - 4.0).collect()
    };
    let core_a = core_of(A);
    let separate = board(
        140,
        100,
        3.0,
        &format!("{A} << {OPTS} >> dkstroke {B} << {OPTS} >> dkstroke"),
    );
    let batched = board(
        140,
        100,
        3.0,
        &format!("{{ {A} << {OPTS} >> dkstroke {B} << {OPTS} >> dkstroke }} dkbatch"),
    );
    let max = lumas(&batched).iter().cloned().fold(0f32, f32::max);
    let dimmed = |it: &Interp| {
        lumas(it)
            .iter()
            .zip(&core_a)
            .filter(|(l, c)| **c && **l < max - 30.0)
            .count()
    };
    assert_eq!(
        dimmed(&batched),
        0,
        "the batch let B's dim level cover A's core"
    );
    assert!(
        dimmed(&separate) > 20,
        "separate calls did not dim A's core ({} pixels): the check proves nothing",
        dimmed(&separate)
    );
}

/// A batch keeps each stroke in the space it was given in: strokes drawn
/// inside their own `gsave ... rotate ... grestore` land where the same
/// strokes land unbatched. They don't cross, so order can't matter.
#[test]
fn dkbatch_keeps_each_strokes_own_space() {
    const STROKES: &str = "gsave 70 50 translate 30 rotate 1.5 0.8 scale \
                           newpath -30 0 moveto 30 0 lineto << /Width 3 /Dropout 0.3 >> dkstroke grestore \
                           gsave 15 75 translate -40 rotate \
                           newpath 0 0 moveto 0 20 lineto << /Width 2 /Lanes 3 >> dkstroke grestore";
    let plain = data(&board(140, 100, 2.0, STROKES));
    let batched = data(&board(140, 100, 2.0, &format!("{{ {STROKES} }} dkbatch")));
    let diff = plain
        .chunks(4)
        .zip(batched.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        diff, 0,
        "a batch moved or re-shaped its strokes: {diff} pixels differ"
    );
}

/// #312: a stroke made inside `dkclip` in a batch renders exactly as the
/// same stroke drawn at once under the same clip: the region, captured in
/// device space (here an ellipse, so curves, under a rotated and scaled
/// space), is replayed for its record and for its fallout. A stroke made
/// outside the dkclip, in the same batch, is not clipped. Two controls: the
/// region moved by half a point differs (the comparison can see the clip),
/// and a plain `clip` inside the batch is not kept (what dkclip is for).
#[test]
fn dkclip_keeps_a_strokes_region_in_a_batch() {
    const REGION: &str = "matrix currentmatrix 70 50 translate 20 rotate 1.4 1 scale \
                          newpath 0 0 25 0 360 arc closepath setmatrix";
    const MOVED: &str = "matrix currentmatrix 70.5 50 translate 20 rotate 1.4 1 scale \
                         newpath 0 0 25 0 360 arc closepath setmatrix";
    const CLIPPED: &str = "newpath 10 30 moveto 130 70 lineto 20 75 moveto 120 20 lineto \
                           << /Width 5 /Levels 2 /Halo 1 /Lanes 2 /Fallout 0.6 /Drift 3 >> dkstroke";
    const FREE: &str = "newpath 10 92 moveto 130 92 lineto << /Width 4 >> dkstroke";
    let at_once = |region: &str| {
        data(&board(
            140,
            100,
            2.0,
            &format!("gsave {region} clip newpath {CLIPPED} grestore {FREE}"),
        ))
    };
    let batched = data(&board(
        140,
        100,
        2.0,
        &format!("{{ {REGION} {{ {CLIPPED} }} dkclip {FREE} }} dkbatch"),
    ));
    let differ = |a: &[u8], b: &[u8]| a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count();
    assert_eq!(
        differ(&at_once(REGION), &batched),
        0,
        "a dkclip region in a batch did not render as the same clip at once"
    );
    let moved = differ(&at_once(MOVED), &batched);
    assert!(
        moved > 20,
        "moving the region half a point changed only {moved} pixels: the check cannot see the clip"
    );
    let plain = data(&board(
        140,
        100,
        2.0,
        &format!("{{ gsave {REGION} clip newpath {CLIPPED} grestore {FREE} }} dkbatch"),
    ));
    assert!(
        differ(&at_once(REGION), &plain) > 200,
        "a plain clip inside a batch was kept: the batch already did what dkclip is for"
    );
}

/// #312, the layering problem: two crossing strokes, each inside its own
/// dkclip region (the regions overlap where they cross), in one batch,
/// stay level-major: no pixel of A's bright core inside the overlap is
/// covered by B's dim level. The same regions in two batches, the
/// control, do cover it.
#[test]
fn dkclip_regions_in_one_batch_stay_level_major() {
    const OPTS: &str = "/Width 6 /Touch 0.75 /Halo 1.5 /Dropout 0.3 /Load 0.6 /Lanes 2";
    const A: &str = "newpath 10 20 moveto 130 80 lineto";
    const B: &str = "newpath 10 80 moveto 130 20 lineto";
    const RA: &str = "newpath 0 0 moveto 100 0 lineto 100 100 lineto 0 100 lineto closepath";
    const RB: &str = "newpath 40 0 moveto 140 0 lineto 140 100 lineto 40 100 lineto closepath";
    let core_a = {
        let it = board(140, 100, 3.0, &format!("{A} << {OPTS} >> dkstroke"));
        let l = lumas(&it);
        let max = l.iter().cloned().fold(0f32, f32::max);
        // A's core, inside the overlap (x 40..100 pt, 3 px per pt)
        l.iter()
            .enumerate()
            .map(|(i, &v)| {
                let x = i % 420;
                v > max - 4.0 && (126..294).contains(&x)
            })
            .collect::<Vec<bool>>()
    };
    let a = format!("{RA} {{ {A} << {OPTS} >> dkstroke }} dkclip");
    let b = format!("{RB} {{ {B} << {OPTS} >> dkstroke }} dkclip");
    let one = board(140, 100, 3.0, &format!("{{ {a} {b} }} dkbatch"));
    let two = board(
        140,
        100,
        3.0,
        &format!("{{ {a} }} dkbatch {{ {b} }} dkbatch"),
    );
    let max = lumas(&one).iter().cloned().fold(0f32, f32::max);
    let dimmed = |it: &Interp| {
        lumas(it)
            .iter()
            .zip(&core_a)
            .filter(|(l, c)| **c && **l < max - 30.0)
            .count()
    };
    assert_eq!(
        dimmed(&one),
        0,
        "one batch let B's dim level cover A's core across regions"
    );
    assert!(
        dimmed(&two) > 20,
        "two batches did not dim A's core ({} pixels): the check proves nothing",
        dimmed(&two)
    );
}

/// Test 2 for strokes, with half-widths that vary (/Pressure): a lighter
/// touch inside a heavier one. Not guaranteed by construction (a heavier
/// touch reaches further, which can bring a nearer, thinner segment into a
/// cell's reach and turn its knuckle), so this measures it: none at these
/// settings.
#[test]
fn dkstroke_nests_under_a_pressure_profile() {
    let mark = |touch: &str| {
        lumas(&render(
            140,
            100,
            4.0,
            &format!(
                "<< /Surface /drawing /Seed 4 >> dksurface 0 0 0 setrgbcolor \
                 newpath 10 30 moveto 40 70 lineto 70 25 lineto 100 65 lineto 130 35 lineto \
                 << /Width 4 /Levels 1 /Strength 1 /Under [1 1 1] /Touch {touch} \
                    /Pressure {{ 6.28 mul 3 mul sin 0.6 mul 1 add }} >> dkstroke"
            ),
        ))
    };
    for (t1, t2) in [("0.55", "0.65"), ("0.7", "0.9")] {
        let lo = mark(t1);
        let hi = mark(t2);
        let lighter = lo.iter().zip(&hi).filter(|(l, h)| **h > **l + 1.0).count();
        eprintln!("touch {t1} -> {t2}: {lighter} pixels lighter at the heavier touch");
        assert_eq!(
            lighter, 0,
            "touch {t1} inked {lighter} pixels that touch {t2} did not"
        );
    }
}

/// Test 7 for `dkstroke`: every parameter it advertises changes the
/// render. The baseline turns every feature on, so each key is live:
/// a contact chain (/Seed, /Load, /Dropout), fallout (/Drift), a tilt
/// (/Side), a touch profile (/Bands) and two levels (/Halo, /Under).
#[test]
fn dkstroke_every_documented_parameter_changes_the_render() {
    const BASE: &str = "/Width 4 /Touch { 0.3 mul 0.55 add } /Bands 4 /Levels 2 \
                        /Tilt 30 /Side 8 /Dropout 0.3 /Load 0.5 /Fallout 1 /Drift 3 /Seed 5";
    let stroke = |extra: &str| {
        data(&board(
            140,
            60,
            2.0,
            &format!(
                "newpath 10 30 moveto 70 45 lineto 130 25 lineto << {BASE} {extra} >> dkstroke"
            ),
        ))
    };
    let baseline = stroke("");
    let varied = [
        ("Width", "/Width 7"),
        ("Touch", "/Touch { 0.3 mul 0.4 add }"),
        ("Bands", "/Bands 2"),
        ("Pressure", "/Pressure { 1 add }"),
        ("Wear", "/Wear 1"),
        ("Tilt", "/Tilt 70"),
        ("Side", "/Side 14"),
        ("Streak", "/Streak 2.5"),
        ("Levels", "/Levels 3"),
        ("Strength", "/Strength 0.5"),
        ("Under", "/Under [0.5 0.1 0.1]"),
        ("Halo", "/Halo 1"),
        ("Load", "/Load 0.2"),
        ("Dropout", "/Dropout 0.7"),
        ("Lanes", "/Lanes 6"),
        ("Fallout", "/Fallout 4"),
        ("Drift", "/Drift 9"),
        ("Seed", "/Seed 6"),
    ];
    for (_, extra) in varied {
        assert_ne!(
            stroke(extra),
            baseline,
            "{extra} rendered the same as the baseline: a no-op"
        );
    }
    // Every advertised key is in the list above, and nothing else is.
    let src = std::fs::read_to_string("lib/drykit.ps").expect("read drykit");
    let start = src
        .find("% @summary: Draws the current path as a dry stick")
        .expect("dkstroke tags");
    let end = src[start..].find("\n/dkstroke ").expect("dkstroke def") + start;
    let mut advertised: Vec<&str> = src[start..end]
        .lines()
        .filter_map(|l| l.strip_prefix("% @param: /"))
        .map(|l| l.split_whitespace().next().unwrap_or(""))
        .collect();
    advertised.sort_unstable();
    let mut tested: Vec<&str> = varied.iter().map(|(k, _)| *k).collect();
    tested.sort_unstable();
    assert_eq!(advertised, tested, "advertised and tested keys differ");
}

/// Test 8, the CI budget smoke test: a 100-stroke dkstroke render stays
/// under 5 s under `--profile ci` (DRYMEDIA.md 6.2). Thread CPU time,
/// best of 3 (issue #329), so load from other processes cannot fail it.
/// Printed so drift is visible before it fails. One level, so the bar has room on a slow CI
/// runner: two levels measured 1.2 s here with the rest of this file
/// running in parallel.
#[test]
fn a_hundred_strokes_render_within_the_ci_budget() {
    let it = within_cpu_budget("100 strokes", Duration::from_secs(5), 3, || {
        render(
            200,
            200,
            1.0,
            "<< /Surface /drawing /Seed 2 >> dksurface 0.1 0.1 0.12 setrgbcolor \
             { 0 1 99 { /i exch def newpath i 10 mod 18 mul 12 add i 10 idiv 18 mul 12 add moveto \
               12 6 rlineto << /Width 1.2 /Touch 0.7 /Levels 1 >> dkstroke } for } dkbatch",
        )
    });
    assert!(inked(&it).iter().any(|&i| i), "nothing drawn");
}

/// Test 6: Ghostscript agrees with pscat on a stroke, block for block (the
/// `tests/paintkit.rs` surface check's shape), with /Seed pinned (the
/// position seed can round differently, DRYMEDIA.md 3.2) and no fallout
/// (scatter draws from `rand`, which differs). A coarse weave puts the
/// pattern at block scale, and a different seed is the negative control.
#[test]
fn ghostscript_agrees_on_a_stroke() {
    if !gs_available() {
        eprintln!("skipping gs stroke check: gs not installed");
        return;
    }
    const SCALE: usize = 4;
    const PX: usize = 200 * SCALE;
    let body = |seed: u32| {
        format!(
            "<< /Surface /drawing /Seed {seed} /Weave 6 /Mottle 30 >> dksurface \
             0 0 0 setrgbcolor \
             newpath 10 40 moveto 100 170 lineto 190 60 lineto 20 120 moveto 180 130 lineto \
             << /Width 26 /Touch 0.6 /Seed 7 /Lanes 3 /Dropout 0.2 /Load 0.6 /Under [1 1 1] >> dkstroke"
        )
    };
    let blocks = |rgb: &[[u8; 3]]| {
        let b = 10 * SCALE;
        let bw = PX / b;
        let mut cells = vec![0f64; bw * bw];
        for (i, p) in rgb.iter().enumerate() {
            let (x, y) = (i % PX, i / PX);
            cells[(y / b) * bw + x / b] +=
                (f64::from(p[0]) + f64::from(p[1]) + f64::from(p[2])) / 3.0 / (b * b) as f64;
        }
        cells
    };
    let pscat = |seed: u32| {
        let it = render(200, 200, SCALE as f32, &body(seed));
        let rgb: Vec<[u8; 3]> = it
            .gfx()
            .pixmap
            .pixels()
            .iter()
            .map(|p| [p.red(), p.green(), p.blue()])
            .collect();
        blocks(&rgb)
    };
    let mut src = String::new();
    for path in ["lib/artkit.ps", "lib/paintkit.ps", "lib/drykit.ps"] {
        src.push_str(&std::fs::read_to_string(path).expect("read lib"));
        src.push('\n');
    }
    let dir = std::env::temp_dir().join(format!("pscat-drykit-stroke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("stroke.ps");
    let png = dir.join("stroke.png");
    std::fs::write(
        &file,
        format!("{src}\n1 setgray clippath fill {}\nshowpage\n", body(11)),
    )
    .expect("write");
    let status = std::process::Command::new("gs")
        .args(["-dNOPAUSE", "-dBATCH", "-q", "-sDEVICE=png16m"])
        .arg(format!("-g{PX}x{PX}"))
        .arg(format!("-r{}", 72 * SCALE))
        .arg(format!("-o{}", png.display()))
        .arg(&file)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected the dkstroke driver");
    let img = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).expect("png")).expect("decode");
    let _ = std::fs::remove_dir_all(&dir);
    let gs_rgb: Vec<[u8; 3]> = img
        .pixels()
        .iter()
        .map(|p| [p.red(), p.green(), p.blue()])
        .collect();
    let gs = blocks(&gs_rgb);
    let diff = |a: &[f64], b: &[f64]| {
        a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f64>() / a.len() as f64
    };
    let same = diff(&pscat(11), &gs);
    let other = diff(&pscat(12), &gs);
    eprintln!("pscat vs gs stroke: mean block diff {same:.2}, another seed {other:.2}");
    assert!(same < 6.0, "pscat and gs disagree on a stroke: {same:.2}");
    assert!(
        other > same * 2.0,
        "another seed is as close ({other:.2}): the check is too weak"
    );
}

/// #312 under Ghostscript: dkclip captures its region with `pathforall` in
/// device space and the batch replays it there. A stroke across the page,
/// made inside a dkclip of its middle third in a batch, inks about a third
/// of what it inks unclipped, in gs as in pscat. Runs locally only.
#[test]
fn ghostscript_keeps_a_dkclip_region_in_a_batch() {
    if !gs_available() {
        eprintln!("skipping gs dkclip check: gs not installed");
        return;
    }
    const STROKE: &str = "newpath 0 20 moveto 120 20 lineto \
                          << /Width 16 /Touch 0.9 /Levels 1 /Seed 7 >> dkstroke";
    let clipped = format!(
        "{{ newpath 40 0 moveto 80 0 lineto 80 40 lineto 40 40 lineto closepath \
         {{ {STROKE} }} dkclip }} dkbatch"
    );
    let surface = "<< /Surface /drawing /Seed 2 >> dksurface ";
    let pscat = |body: &str| {
        let ink = inked(&render(120, 40, 2.0, &format!("{surface}{body}")));
        ink.iter().filter(|&&i| i).count() as f64 / ink.len() as f64
    };
    let gs_ink = |body: &str| gs_band_ink("", &format!("{surface}{body}"), (240, 80), 144);
    let (p_full, p_clip) = (pscat(&format!("{{ {STROKE} }} dkbatch")), pscat(&clipped));
    let (g_full, g_clip) = (gs_ink(&format!("{{ {STROKE} }} dkbatch")), gs_ink(&clipped));
    eprintln!("ink: pscat {p_clip:.3} of {p_full:.3}, gs {g_clip:.3} of {g_full:.3}");
    for (what, clip, full) in [("pscat", p_clip, p_full), ("gs", g_clip, g_full)] {
        let r = clip / full;
        assert!(
            r > 0.25 && r < 0.42,
            "{what}: the dkclip kept {r:.2} of the stroke's ink, not about a third"
        );
    }
}

/// Chunking estimates a run's cells before `dkZthin` grows the thinned
/// stops' reach by the chord deviation, so a curvy run's catch is larger
/// than estimated. On glyph-radius curves at the longest knuckles and a
/// small budget, the chunks still fit: the stroke renders without raising.
#[test]
fn a_curvy_run_at_long_knuckles_fits_its_chunks() {
    let it = render(
        240,
        110,
        2.0,
        "/dkbudget 800 def << /Surface /slate >> dksurface 0 0 0 setrgbcolor \
         newpath 20 50 moveto 20 90 120 90 120 50 curveto 120 10 220 10 220 50 curveto \
         220 70 200 80 190 70 curveto << /Width 3 /Touch 0.9 /Streak 4 /Under [1 1 1] >> dkstroke \
         newpath 30 20 15 0 350 arc << /Width 2 /Streak 4 /Touch 0.95 /Under [1 1 1] >> dkstroke",
    );
    assert!(
        inked(&it).iter().filter(|&&i| i).count() > 1000,
        "the curvy run drew almost nothing"
    );
}
