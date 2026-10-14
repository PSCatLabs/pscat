//! drycharcoal (issue #241; design `docs/DRYMEDIA.md` 3.4): charcoal
//! presets and techniques on drykit's `dkstroke`. Guards and "no draws
//! from the caller's stream" are `%%SelfTest` blocks in
//! `lib/drycharcoal.ps`; these tests check what the renders do.

mod common;

use common::within_cpu_budget;
use pscat::Interp;
use std::time::Duration;

const LIBS: [&str; 5] = [
    "lib/artkit.ps",
    "lib/paintkit.ps",
    "lib/drykit.ps",
    "lib/hatchkit.ps",
    "lib/drycharcoal.ps",
];

/// A `w` x `h` point page at `scale` device pixels per point, on drawing
/// paper (seed 3, drykit's defaults), in charcoal, with `src` run.
fn render(w: u32, h: u32, scale: f32, src: &str) -> Interp {
    let mut it = Interp::with_page_scaled(w, h, scale).expect("page");
    for path in LIBS {
        let lib = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&lib)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
    it.run_str(&format!(
        "<< /Surface /drawing /Seed 3 >> dksurface \
         userdict /dkGround get aload pop setrgbcolor clippath fill \
         0.11 0.1 0.1 setrgbcolor {src}"
    ))
    .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    it
}

fn lumas(it: &Interp) -> Vec<f32> {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .map(|p| 0.3 * p.red() as f32 + 0.59 * p.green() as f32 + 0.11 * p.blue() as f32)
        .collect()
}

fn data(it: &Interp) -> Vec<u8> {
    it.gfx().pixmap.data().to_vec()
}

/// Mean luma over the box (x0, y0)-(x1, y1) in points, y up, of a page
/// `h` points tall at `scale`.
fn mean(it: &Interp, h: u32, scale: f32, b: (f32, f32, f32, f32)) -> f32 {
    let l = lumas(it);
    let w = it.gfx().pixmap.width() as usize;
    let (x0, y0, x1, y1) = b;
    let px = |v: f32| (v * scale) as usize;
    let py = |v: f32| ((h as f32 - v) * scale) as usize;
    let (mut s, mut n) = (0f32, 0f32);
    for y in py(y1)..py(y0) {
        for x in px(x0)..px(x1) {
            s += l[y * w + x];
            n += 1.0;
        }
    }
    s / n
}

fn cell(x: u32, y: u32, s: u32) -> String {
    format!("newpath {x} {y} moveto {s} 0 rlineto 0 {s} rlineto -{s} 0 rlineto closepath")
}

/// The hatching value scale, as the specimen draws it (soft pencil, two
/// deposit levels): every step darker than the last, from paper at 0 to
/// near the pigment at 1.
#[test]
fn hatching_builds_a_value_scale() {
    let mut src = String::new();
    for i in 0..6 {
        src.push_str(&format!(
            "{} << /Value {} /Seed {i} /Medium /pencil /Grade 1 /Layers 4 /Levels 2 >> dhhatch ",
            cell(4 + i * 34, 4, 30),
            f64::from(i) / 5.0
        ));
    }
    let it = render(210, 38, 2.0, &src);
    let steps: Vec<f32> = (0..6)
        .map(|i| {
            let x = 6.0 + i as f32 * 34.0;
            mean(&it, 38, 2.0, (x, 6.0, x + 26.0, 32.0))
        })
        .collect();
    eprintln!("hatching scale: {steps:?}");
    for w in steps.windows(2) {
        assert!(w[1] < w[0] - 5.0, "the scale is not monotone: {steps:?}");
    }
    assert!(steps[5] < 70.0, "the darkest step is not dark: {steps:?}");
}

/// The smudging scale (a light hatch, then a stump to each value), as in
/// the specimen: monotone and reaching near-black.
#[test]
fn smudging_builds_a_value_scale() {
    let mut src = String::new();
    for i in 0..6 {
        let x = 4 + i * 34;
        src.push_str(&format!(
            "{c} << /Value {v} 0.4 mul /Seed {i} /Medium /vine /Layers 1 >> dhhatch \
             gsave {c} clip newpath {x0} 0 moveto 0 1 2 {{ pop 38 7 rlineto -38 5 rlineto }} for \
             << /Value {v} /Width 14 /Seed {i} >> dhsmudge grestore ",
            c = cell(x, 4, 30),
            v = f64::from(i) / 5.0,
            x0 = x - 4
        ));
    }
    let it = render(210, 38, 2.0, &src);
    let steps: Vec<f32> = (0..6)
        .map(|i| {
            let x = 6.0 + i as f32 * 34.0;
            mean(&it, 38, 2.0, (x, 6.0, x + 26.0, 32.0))
        })
        .collect();
    eprintln!("smudging scale: {steps:?}");
    for w in steps.windows(2) {
        assert!(w[1] < w[0] - 5.0, "the scale is not monotone: {steps:?}");
    }
    assert!(steps[5] < 80.0, "the darkest step is not dark: {steps:?}");
}

/// A /Value procedure is read where each line passes: a left-to-right
/// ramp hatches darker on the right.
#[test]
fn a_value_procedure_shades_across_the_region() {
    let it = render(
        124,
        44,
        2.0,
        &format!(
            "{} << /Value {{ pop 2 sub 120 div }} /Seed 4 >> dhhatch",
            cell(2, 2, 120).replace("120 rlineto -120", "40 rlineto -120")
        ),
    );
    let left = mean(&it, 44, 2.0, (4.0, 6.0, 40.0, 38.0));
    let right = mean(&it, 44, 2.0, (84.0, 6.0, 120.0, 38.0));
    eprintln!("ramp: left {left:.1}, right {right:.1}");
    assert!(
        right < left - 40.0,
        "the ramp did not shade: {left} / {right}"
    );
}

/// DRYMEDIA.md 3.3 for hatching: layers hatched in one call share one
/// dkbatch, so at two deposit levels the second layer's faint level never
/// lands over the first layer's cores. Two separate calls do let it, which
/// greys the crossings: the one-call region is darker.
#[test]
fn one_call_keeps_crossings_dark() {
    let one = render(
        64,
        64,
        3.0,
        &format!(
            "{} << /Value 1 /Layers 2 /Levels 2 /Halo 1.5 /Seed 2 >> dhhatch",
            cell(2, 2, 60)
        ),
    );
    let two = render(
        64,
        64,
        3.0,
        &format!(
            "{c} << /Value 1 /Layers 1 /Angle 45 /Levels 2 /Halo 1.5 /Seed 2 >> dhhatch \
             {c} << /Value 1 /Layers 1 /Angle 135 /Levels 2 /Halo 1.5 /Seed 2 >> dhhatch",
            c = cell(2, 2, 60)
        ),
    );
    let a = mean(&one, 64, 3.0, (6.0, 6.0, 58.0, 58.0));
    let b = mean(&two, 64, 3.0, (6.0, 6.0, 58.0, 58.0));
    eprintln!("crossings: one call {a:.1}, two calls {b:.1}");
    assert!(
        a < b - 2.0,
        "one call ({a:.1}) is not darker than two ({b:.1})"
    );
}

/// #312: dhhatch joins a caller's dkbatch, each call keeping its own region
/// (drykit's dkclip). Two overlapping regions hatched in one batch, at two
/// deposit levels, keep the overlap's crossings dark, where two separate
/// calls let the second region's faint level grey the first one's cores.
/// Away from the overlap the one-batch render is the two calls' render,
/// pixel for pixel: each region is clipped as it was.
#[test]
fn overlapping_regions_in_one_batch_keep_crossings_dark() {
    const A: &str = "/Value 1 /Layers 1 /Angle 45 /Levels 2 /Halo 1.5 /Seed 2";
    const B: &str = "/Value 1 /Layers 1 /Angle 135 /Levels 2 /Halo 1.5 /Seed 3";
    let calls = format!(
        "{} << {A} >> dhhatch {} << {B} >> dhhatch",
        cell(2, 2, 40),
        cell(22, 22, 40)
    );
    let two = render(64, 64, 3.0, &calls);
    let one = render(64, 64, 3.0, &format!("{{ {calls} }} dkbatch"));
    let a = mean(&one, 64, 3.0, (25.0, 25.0, 39.0, 39.0));
    let b = mean(&two, 64, 3.0, (25.0, 25.0, 39.0, 39.0));
    eprintln!("overlap: one batch {a:.1}, two calls {b:.1}");
    assert!(
        a < b - 2.0,
        "one batch ({a:.1}) is not darker than two calls ({b:.1}) where the regions overlap"
    );
    // Away from the overlap: A's corner (x, y 2..20 pt) and B's (44..62 pt).
    let (lo, la) = (lumas(&one), lumas(&two));
    let w = 192;
    let mut differ = 0;
    for (x0, y0) in [(2.0f32, 2.0f32), (44.0, 44.0)] {
        for y in ((64.0 - y0 - 18.0) * 3.0) as usize..((64.0 - y0) * 3.0) as usize {
            for x in (x0 * 3.0) as usize..((x0 + 18.0) * 3.0) as usize {
                if lo[y * w + x] != la[y * w + x] {
                    differ += 1;
                }
            }
        }
    }
    assert_eq!(
        differ, 0,
        "one batch changed {differ} pixels away from the overlap"
    );
    assert!(
        mean(&one, 64, 3.0, (4.0, 4.0, 18.0, 18.0)) < 200.0,
        "nothing hatched in A's corner"
    );
}

/// A kneaded lift is lighter than the charcoal it lifts but never back to
/// paper; a hard eraser lifts more along its line.
#[test]
fn a_lift_is_grey_and_a_hard_eraser_cuts_lighter() {
    const MASS: &str =
        "newpath 10 10 moveto 150 10 lineto 150 50 lineto 10 50 lineto closepath fill";
    let base = render(160, 60, 2.0, MASS);
    let kneaded = render(
        160,
        60,
        2.0,
        &format!("{MASS} newpath 20 30 moveto 140 30 lineto << /Width 12 /Seed 1 >> dhlift"),
    );
    let hard = render(
        160,
        60,
        2.0,
        &format!(
            "{MASS} newpath 20 30 moveto 140 30 lineto << /Eraser /hard /Width 12 /Seed 1 >> dhlift"
        ),
    );
    let paper = mean(&base, 60, 2.0, (2.0, 54.0, 158.0, 58.0));
    let band = (40.0, 27.0, 120.0, 33.0);
    let m = mean(&base, 60, 2.0, band);
    let k = mean(&kneaded, 60, 2.0, band);
    let h = mean(&hard, 60, 2.0, band);
    eprintln!("mass {m:.1}, kneaded {k:.1}, hard {h:.1}, paper {paper:.1}");
    assert!(k > m + 40.0, "the kneaded lift lifted too little");
    assert!(k < paper - 30.0, "the kneaded lift went back to paper");
    assert!(h > k, "the hard eraser is not lighter along its line");
}

/// Grades: a softer grade of each medium inks more.
#[test]
fn a_softer_grade_inks_more() {
    for medium in ["vine", "compressed", "pencil"] {
        let ink = |grade: f64| {
            let it = render(
                120,
                40,
                2.0,
                &format!(
                    "newpath 10 20 moveto 110 24 lineto << /Medium /{medium} /Grade {grade} /Seed 3 >> dhstroke"
                ),
            );
            mean(&it, 40, 2.0, (2.0, 2.0, 118.0, 38.0))
        };
        let (hard, soft) = (ink(0.0), ink(1.0));
        eprintln!("{medium}: grade 0 {hard:.1}, grade 1 {soft:.1}");
        assert!(
            soft < hard - 1.0,
            "{medium}: grade 1 is not darker than grade 0"
        );
    }
}

/// The construction sketch: each extra pass lays down more line.
#[test]
fn more_sketch_passes_lay_more_line() {
    let ink = |passes: u32| {
        let it = render(
            120,
            60,
            2.0,
            &format!(
                "newpath 10 10 moveto 60 50 lineto 110 12 lineto << /Passes {passes} /Shift 2 /Seed 4 >> dhsketch"
            ),
        );
        mean(&it, 60, 2.0, (2.0, 2.0, 118.0, 58.0))
    };
    let (one, three) = (ink(1), ink(3));
    eprintln!("sketch: 1 pass {one:.2}, 3 passes {three:.2}");
    assert!(three < one - 0.5, "three passes inked no more than one");
}

/// The text between a procedure's `@summary` and its definition.
fn advertised(name: &str, summary_start: &str) -> Vec<String> {
    let src = std::fs::read_to_string("lib/drycharcoal.ps").expect("read drycharcoal");
    let start = src.find(summary_start).expect("summary");
    let end = src[start..]
        .find(&format!("\n/{name} {{"))
        .unwrap_or_else(|| panic!("{name} def"))
        + start;
    let mut keys: Vec<String> = src[start..end]
        .lines()
        .filter_map(|l| l.strip_prefix("% @param: /"))
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .collect();
    keys.sort_unstable();
    keys
}

/// Every documented key of every procedure changes the render, and the
/// advertised and tested sets are equal (the pkbroad pattern).
#[test]
fn every_documented_parameter_changes_the_render() {
    type Case = (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static [(&'static str, &'static str)],
    );
    const HATCH_REGION: &str = "newpath 4 4 moveto 76 4 lineto 76 46 lineto 4 46 lineto closepath";
    const LINE: &str = "newpath 10 20 moveto 40 34 lineto 70 18 lineto";
    const MASS: &str = "newpath 4 4 moveto 76 4 lineto 76 46 lineto 4 46 lineto closepath fill ";
    let cases: [Case; 5] = [
        (
            "dhstroke",
            "% @summary: Draws the current path in charcoal",
            LINE,
            "/Seed 2",
            &[("Medium", "/Medium /compressed"), ("Grade", "/Grade 0.1")],
        ),
        (
            "dhhatch",
            "% @summary: Hatches the region",
            HATCH_REGION,
            "/Seed 2 /Value 0.6",
            &[
                ("Medium", "/Medium /vine"),
                ("Grade", "/Grade 0.1"),
                ("Value", "/Value 0.3"),
                ("Layers", "/Layers 1"),
                ("Angle", "/Angle 10"),
                ("Spacing", "/Spacing 3"),
                ("Length", "/Length 8"),
                ("Width", "/Width 1.4"),
                ("Touch", "/Touch 0.95"),
                ("Seed", "/Seed 3"),
            ],
        ),
        (
            "dhsmudge",
            "% @summary: Smudges along the current path",
            LINE,
            "/Seed 2",
            &[
                ("Value", "/Value 0.8"),
                ("Width", "/Width 6"),
                ("Touch", "/Touch 0.95"),
                ("Under", "/Under [0.5 0.5 0.5]"),
            ],
        ),
        (
            "dhlift",
            "% @summary: Lifts charcoal along the current path",
            LINE,
            "/Seed 2",
            &[
                ("Eraser", "/Eraser /hard"),
                ("Ground", "/Ground [1 0.9 0.9]"),
                ("Under", "/Under [0.4 0.4 0.4]"),
                ("Width", "/Width 4"),
                ("Touch", "/Touch 0.9"),
            ],
        ),
        (
            "dhsketch",
            "% @summary: Draws the current path as faint construction lines",
            LINE,
            "/Seed 2",
            &[
                ("Passes", "/Passes 4"),
                ("Shift", "/Shift 4"),
                ("Touch", "/Touch 0.8"),
                ("Strength", "/Strength 0.9"),
                ("Seed", "/Seed 3"),
            ],
        ),
    ];
    for (name, summary, path, base, varied) in cases {
        let pre = if name == "dhlift" { MASS } else { "" };
        let draw = |extra: &str| {
            data(&render(
                80,
                50,
                1.5,
                &format!("{pre}{path} << {base} {extra} >> {name}"),
            ))
        };
        let baseline = draw("");
        for (_, extra) in varied {
            assert_ne!(
                draw(extra),
                baseline,
                "{name} {extra} rendered the same as the baseline: a no-op"
            );
        }
        let mut tested: Vec<String> = varied.iter().map(|(k, _)| k.to_string()).collect();
        tested.sort_unstable();
        assert_eq!(
            advertised(name, summary),
            tested,
            "{name}: advertised and tested keys differ"
        );
    }
}

/// DRYMEDIA.md 6.2: a test-sized hatch, every layer, under 5 s under
/// `--profile ci` (60 x 60 pt measured 2.4 s wall time with the file in
/// parallel). Thread CPU time, best of 3 (issue #329), so load from other
/// processes cannot fail it. Printed so drift is visible before it fails.
#[test]
fn a_hatched_region_renders_within_the_ci_budget() {
    let it = within_cpu_budget("40 x 40 pt, four layers", Duration::from_secs(5), 3, || {
        render(
            44,
            44,
            2.0,
            &format!(
                "{} << /Value 1 /Layers 4 /Seed 1 >> dhhatch",
                cell(2, 2, 40)
            ),
        )
    });
    assert!(
        mean(&it, 44, 2.0, (6.0, 6.0, 38.0, 38.0)) < 200.0,
        "nothing drawn"
    );
}

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The libraries concatenated, so gs runs them under its default -dSAFER.
fn libs_source() -> String {
    let mut src = String::new();
    for path in LIBS {
        src.push_str(&std::fs::read_to_string(path).expect("read lib"));
        src.push('\n');
    }
    src
}

/// Under `true setpacking` Ghostscript types a procedure literal
/// `packedarraytype`; dhhatch's /Value guard must accept it (as hatchkit's
/// /Mark guard does, #271). This interpreter never packs, so only real gs
/// can show it. Runs locally only: CI has no gs.
#[test]
fn ghostscript_accepts_a_packed_value_procedure() {
    if !gs_available() {
        eprintln!("skipping gs packing check: gs not installed");
        return;
    }
    let dir = std::env::temp_dir().join(format!("pscat-drycharcoal-pack-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("packed.ps");
    std::fs::write(
        &ps,
        format!(
            "{}\n<< /Surface /drawing >> dksurface 0 setgray\n\
             true setpacking\n\
             /v {{ pop 50 div }} def\n\
             /v load type /packedarraytype ne {{ /not_packed undefined }} if\n\
             {} << /Value /v load /Layers 2 /Seed 999999 >> dhhatch\n\
             newpath 0 0 moveto 50 50 lineto << /Seed -999999 >> dhsketch\n\
             false setpacking showpage\n",
            libs_source(),
            cell(0, 0, 50)
        ),
    )
    .unwrap();
    let out = std::process::Command::new("gs")
        .args(["-dNOPAUSE", "-dBATCH", "-q", "-sDEVICE=nullpage"])
        .arg(&ps)
        .output()
        .expect("run gs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "gs rejected a packed /Value procedure: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// gs agrees with pscat on a hatched region, block for block (the
/// `tests/drykit.rs` test 6 shape): every hand variation comes from
/// drykit's private generator and hatch takes no rand draws, so the same
/// strokes land in both. A coarse weave puts the pattern at block scale;
/// another seed is the negative control. Runs locally only.
#[test]
fn ghostscript_agrees_on_a_hatched_region() {
    if !gs_available() {
        eprintln!("skipping gs hatch check: gs not installed");
        return;
    }
    const SCALE: usize = 3;
    const PX: usize = 120 * SCALE;
    let body = |seed: u32| {
        format!(
            "<< /Surface /drawing /Seed 4 /Weave 3 /Mottle 20 >> dksurface \
             1 setgray clippath fill 0 setgray \
             newpath 10 10 moveto 110 10 lineto 110 110 lineto 10 110 lineto closepath \
             << /Value {{ pop 10 sub 100 div }} /Spacing 6 /Width 3 /Length 30 /Layers 3 \
                /Under [1 1 1] /Seed {seed} >> dhhatch"
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
        let mut it = Interp::with_page_scaled(120, 120, SCALE as f32).expect("page");
        it.run_str(&libs_source()).expect("libs");
        it.run_str(&body(seed)).expect("body");
        let rgb: Vec<[u8; 3]> = it
            .gfx()
            .pixmap
            .pixels()
            .iter()
            .map(|p| [p.red(), p.green(), p.blue()])
            .collect();
        blocks(&rgb)
    };
    let dir = std::env::temp_dir().join(format!("pscat-drycharcoal-gs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("hatch.ps");
    let png = dir.join("hatch.png");
    std::fs::write(&file, format!("{}\n{}\nshowpage\n", libs_source(), body(5))).expect("write");
    let status = std::process::Command::new("gs")
        .args(["-dNOPAUSE", "-dBATCH", "-q", "-sDEVICE=png16m"])
        .arg(format!("-g{PX}x{PX}"))
        .arg(format!("-r{}", 72 * SCALE))
        .arg(format!("-o{}", png.display()))
        .arg(&file)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected the dhhatch driver");
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
    let same = diff(&pscat(5), &gs);
    let other = diff(&pscat(6), &gs);
    eprintln!("pscat vs gs hatch: mean block diff {same:.2}, another seed {other:.2}");
    assert!(same < 6.0, "pscat and gs disagree on a hatch: {same:.2}");
    assert!(
        other > same * 2.0,
        "another seed is as close ({other:.2}): the check is too weak"
    );
}
