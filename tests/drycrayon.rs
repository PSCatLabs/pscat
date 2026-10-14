//! drycrayon (issue #239; design `docs/DRYMEDIA.md`): the crayon preset
//! `dycrayon`, crayon paper `dypaper`, and the child-drawing motions
//! (`dywobble`, `dyscribble`, `dyloop`, `dyfigure`, `dyrays`, `dygrass`).
//!
//! Guards, the crayon box, "no draws from the caller's stream" and the
//! drykit contract dycrayon reads are `%%SelfTest` blocks in
//! `lib/drycrayon.ps`. These tests cover what needs pixels or paths: every
//! documented key is live, the crayon's mechanisms move ink the way the
//! header says, the motions are deterministic and keep their draws per
//! slot, Ghostscript agrees, and a render fits the CI budget.

use pscat::Interp;

const LIBS: [&str; 4] = [
    "lib/artkit.ps",
    "lib/paintkit.ps",
    "lib/drykit.ps",
    "lib/drycrayon.ps",
];

fn load(it: &mut Interp) {
    for path in LIBS {
        let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
}

/// A `w` x `h` point page at `scale` pixels per point: crayon paper
/// (seed 4), its ground painted, `src` run on it.
fn render(w: u32, h: u32, scale: f32, src: &str) -> Interp {
    let mut it = Interp::with_page_scaled(w, h, scale).expect("page");
    load(&mut it);
    it.run_str(&format!(
        "<< /Seed 4 >> dypaper 0.97 0.96 0.93 setrgbcolor clippath fill 0 0 0 setrgbcolor 3 srand {src}"
    ))
    .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    it
}

fn data(it: &Interp) -> Vec<u8> {
    it.gfx().pixmap.data().to_vec()
}

fn rgb(it: &Interp) -> Vec<[u8; 3]> {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .map(|p| [p.red(), p.green(), p.blue()])
        .collect()
}

/// Pixels darker than the paper by a clear margin (paper is about 245).
fn inked(it: &Interp) -> Vec<bool> {
    rgb(it)
        .iter()
        .map(|p| (u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) < 3 * 200)
        .collect()
}

/// Fraction of inked pixels inside the pixel box [x0, x1) x [y0, y1).
fn ink_in(it: &Interp, w: usize, (x0, y0, x1, y1): (usize, usize, usize, usize)) -> f64 {
    let ink = inked(it);
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if ink[y * w + x] {
                n += 1;
            }
        }
    }
    n as f64 / ((x1 - x0) * (y1 - y0)) as f64
}

/// The operand stack as strings, after running `src` with the libraries
/// loaded (no page needed).
fn eval(src: &str) -> Vec<String> {
    let mut it = Interp::with_page(100, 100).expect("page");
    load(&mut it);
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

/// `src` builds a path; this returns it as coordinates rounded to 0.001,
/// each segment followed by its kind (`/m`, `/l`, `/c`, `/h`).
fn path_of(src: &str) -> Vec<String> {
    eval(&format!(
        "/r3 {{ 1000 mul round 1000 div }} def \
         {src} \
         {{ exch r3 exch r3 /m }} \
         {{ exch r3 exch r3 /l }} \
         {{ 6 {{ r3 6 1 roll }} repeat /c }} \
         {{ /h }} pathforall"
    ))
}

/// The `@param` keys advertised between a procedure's `@summary` and its
/// definition, sorted.
fn advertised(name: &str) -> Vec<String> {
    let src = std::fs::read_to_string("lib/drycrayon.ps").expect("read drycrayon");
    let def = src.find(&format!("\n/{name} {{")).expect("definition");
    let start = src[..def].rfind("% @summary:").expect("summary");
    let mut keys: Vec<String> = src[start..def]
        .lines()
        .filter_map(|l| l.strip_prefix("% @param: /"))
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .collect();
    keys.sort_unstable();
    keys
}

fn assert_tested(name: &str, varied: &[(&str, &str)]) {
    let mut tested: Vec<String> = varied.iter().map(|(k, _)| k.to_string()).collect();
    tested.sort_unstable();
    assert_eq!(
        advertised(name),
        tested,
        "{name}: advertised and tested keys differ"
    );
}

#[test]
fn dycrayon_every_documented_parameter_changes_the_render() {
    let stroke = |extra: &str| {
        data(&render(
            140,
            60,
            2.0,
            &format!(
                "0.2 0.3 0.8 setrgbcolor newpath 10 30 moveto 60 45 lineto 20 40 lineto 130 20 lineto \
                 << /Width 6 /Touch 0.7 {extra} >> dycrayon"
            ),
        ))
    };
    let baseline = stroke("");
    let varied = [
        ("Width", "/Width 9"),
        ("Touch", "/Touch { 0.3 mul 0.5 add }"),
        ("Landing", "/Landing 0.5"),
        ("Fade", "/Fade 0.5"),
        ("Uneven", "/Uneven 0.4"),
        ("Buildup", "/Buildup 0.8"),
        ("Worn", "/Worn 1"),
        ("Wear", "/Wear 1.5"),
        ("Pass", "/Pass 4"),
        ("Over", "/Over [0.9 0.8 0.1]"),
        ("Streak", "/Streak 3.5"),
        ("Strength", "/Strength 0.5"),
        ("Seed", "/Seed 12"),
    ];
    for (_, extra) in varied {
        assert_ne!(
            stroke(extra),
            baseline,
            "{extra} rendered the same as the baseline: a no-op"
        );
    }
    assert_tested("dycrayon", &varied);
}

#[test]
fn dycrayon_is_deterministic_and_seeded_by_position() {
    let body = "0.6 0.1 0.2 setrgbcolor newpath 10 10 moveto 90 50 lineto << >> dycrayon";
    let a = data(&render(100, 60, 2.0, body));
    assert_eq!(
        a,
        data(&render(100, 60, 2.0, body)),
        "same input, other pixels"
    );
    // A stroke added elsewhere doesn't re-roll this one.
    let more = format!("{body} newpath 10 55 moveto 30 55 lineto << >> dycrayon");
    let b = render(100, 60, 2.0, &more);
    let (pa, pb) = (rgb(&render(100, 60, 2.0, body)), rgb(&b));
    // rows 20..120 (pixels) hold the first stroke only
    let w = 200;
    for y in 20..110 {
        for x in 0..w {
            assert_eq!(pa[y * w + x], pb[y * w + x], "pixel {x},{y} moved");
        }
    }
}

/// Wax builds up where the stroke turns: on a serpentine of straight legs
/// and U-turns, more ink in the U-turns at /Buildup 0.6 than at 0; along
/// the straight middles, about the same.
#[test]
fn buildup_presses_harder_at_the_turns() {
    let serpentine = |b: &str| {
        render(
            200,
            72,
            3.0,
            &format!(
                "0.1 0.1 0.5 setrgbcolor newpath 20 8 moveto 180 8 lineto 180 22 14 -90 90 arc \
                 20 36 lineto 20 50 14 270 90 arcn 180 64 lineto \
                 << /Touch 0.5 /Buildup {b} /Seed 2 /Uneven 0 /Landing 0 /Fade 0 >> dycrayon"
            ),
        )
    };
    let (flat, built) = (serpentine("0"), serpentine("0.6"));
    let w = 600;
    // the U-turns lie beyond x = 180 and before x = 20; middles: 80..120pt
    let turns = |it: &Interp| ink_in(it, w, (543, 0, 600, 216)) + ink_in(it, w, (0, 0, 57, 216));
    let middle = (240, 0, 360, 216);
    eprintln!(
        "turns {:.3} -> {:.3}, middle {:.3} -> {:.3}",
        turns(&flat),
        turns(&built),
        ink_in(&flat, w, middle),
        ink_in(&built, w, middle)
    );
    assert!(
        turns(&built) > turns(&flat) * 1.15,
        "build-up added too little ink at the turns"
    );
    assert!(
        (ink_in(&built, w, middle) - ink_in(&flat, w, middle)).abs() < 0.02,
        "build-up changed the straight middle"
    );
}

/// Coverage builds with each declared pass. Every crayon level is one
/// drykit level in the stroke's full colour, so passes of one colour need
/// no dkbatch: inside one, or as separate calls, they render nearly the
/// same. Measured: 245 of 38400 pixels differ by more than 60, 211 of them
/// on an edge (anti-aliasing) and the rest where passes cross (a batch
/// shares one catch among runs whose touches coincide); coverage within
/// 0.3%.
#[test]
fn passes_build_coverage_with_or_without_a_batch() {
    let fill = |passes: u32, batch: bool| {
        let mut body = String::new();
        for p in 1..=passes {
            body.push_str(&format!(
                "4 srand 10 10 100 60 screct << /Spacing 6 /Direction {} >> dyscribble \
                 << /Touch 0.55 /Pass {p} >> dycrayon ",
                (p - 1) * 20
            ));
        }
        let body = if batch {
            format!("0.2 0.4 0.8 setrgbcolor {{ {body} }} dkbatch")
        } else {
            format!("0.2 0.4 0.8 setrgbcolor {body}")
        };
        render(120, 80, 2.0, &body)
    };
    let cover = |it: &Interp| ink_in(it, 240, (40, 40, 200, 120));
    let c: Vec<f64> = (1..=3).map(|n| cover(&fill(n, true))).collect();
    eprintln!("coverage over passes 1, 2, 3: {c:?}");
    assert!(
        c[1] > c[0] + 0.03 && c[2] > c[1] + 0.02,
        "coverage did not build: {c:?}"
    );
    // Same colour: only edges and crossings can differ (see above).
    let (a, b) = (rgb(&fill(3, true)), rgb(&fill(3, false)));
    let differ = a
        .iter()
        .zip(&b)
        .filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > 60))
        .count();
    eprintln!(
        "batch vs separate calls: {differ} of {} pixels differ by > 60",
        a.len()
    );
    assert!(
        differ * 50 < a.len(),
        "passes in a batch and as separate calls differ in {differ} pixels"
    );
}

/// A colour laid /Over another lets it show through: in the overlap both
/// the yellow beneath and the blue on top are present, and more yellow
/// shows than when the blue is laid without /Over.
#[test]
fn over_lets_the_colour_beneath_show_through() {
    let layer = |over: &str| {
        render(
            120,
            90,
            2.0,
            &format!(
                "0.99 0.87 0.22 setrgbcolor 10 10 100 70 screct << /Spacing 5 >> dyscribble \
                 << /Touch 0.8 >> dycrayon \
                 0.16 0.41 0.79 setrgbcolor 10 10 100 70 screct << /Spacing 5 /Direction 90 >> dyscribble \
                 << /Touch 0.7 {over} >> dycrayon"
            ),
        )
    };
    let count = |it: &Interp| {
        let px = rgb(it);
        let w = 240;
        let (mut yellow, mut blue, mut n) = (0, 0, 0);
        for y in 50..130 {
            for x in 50..190 {
                let p = px[y * w + x];
                n += 1;
                if p[0] > 200 && p[1] > 170 && p[2] < 120 {
                    yellow += 1;
                }
                if p[2] > 150 && p[0] < 100 {
                    blue += 1;
                }
            }
        }
        (yellow as f64 / n as f64, blue as f64 / n as f64)
    };
    let (y_over, b_over) = count(&layer("/Over [0.99 0.87 0.22]"));
    let (y_plain, _) = count(&layer(""));
    eprintln!("with /Over: yellow {y_over:.3}, blue {b_over:.3}; without: yellow {y_plain:.3}");
    assert!(y_over > 0.08 && b_over > 0.3, "both colours should show");
    assert!(y_over > y_plain, "/Over should let more of the yellow show");
}

/// A worn flat draws a wider line than a new point.
#[test]
fn a_worn_crayon_draws_wider() {
    let line = |worn: &str| {
        render(
            100,
            40,
            4.0,
            &format!("newpath 10 20 moveto 90 20 lineto << /Touch 0.95 /Worn {worn} >> dycrayon"),
        )
    };
    let rows = |it: &Interp| {
        let ink = inked(it);
        (0..160)
            .filter(|y| ink[y * 400 + 200] || ink[y * 400 + 240])
            .count()
    };
    let (new, worn) = (rows(&line("0")), rows(&line("1")));
    eprintln!("inked rows: new {new}, worn {worn}");
    assert!(worn > new + 4, "a worn crayon should draw wider");
}

/// Every motion: the same srand gives the same path, another srand a
/// different one, and the guide (or the old path) is replaced.
#[test]
fn motions_are_deterministic_and_replace_the_path() {
    let calls = [
        "newpath 10 10 moveto 90 20 lineto << >> dywobble",
        "10 10 80 40 screct << >> dyscribble",
        "50 50 20 << >> dyloop",
        "50 10 80 << /Hair 5 >> dyfigure",
        "50 50 15 << >> dyrays",
        "newpath 10 10 moveto 90 12 lineto << >> dygrass",
    ];
    for call in calls {
        let a = path_of(&format!("newpath 0 0 moveto 1 1 lineto 7 srand {call}"));
        let b = path_of(&format!("newpath 7 srand {call}"));
        let c = path_of(&format!("newpath 8 srand {call}"));
        assert!(a.len() > 6, "{call}: an empty path");
        assert_eq!(
            a, b,
            "{call}: the old path survived, or the same srand gave another path"
        );
        assert_ne!(a, c, "{call}: another srand gave the same path");
    }
}

/// Every documented key of every motion changes its path.
#[test]
fn every_motion_parameter_changes_the_path() {
    type Case<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)]);
    let cases: [Case; 6] = [
        (
            "dywobble",
            "newpath 10 10 moveto 90 20 lineto 90 60 lineto << {} >> dywobble",
            &[
                ("Wobble", "/Wobble 4"),
                ("Wavelength", "/Wavelength 15"),
                ("Tremor", "/Tremor 2"),
                ("Overshoot", "/Overshoot 15"),
            ],
        ),
        (
            "dyscribble",
            "newpath 10 10 moveto 90 15 lineto 50 80 lineto closepath scpath << {} >> dyscribble",
            &[
                ("Spacing", "/Spacing 7"),
                ("Direction", "/Direction 30"),
                ("Overrun", "/Overrun 9"),
                ("Miss", "/Miss 0.9"),
                ("Scatter", "/Scatter 4"),
            ],
        ),
        (
            "dyloop",
            "50 50 20 << {} >> dyloop",
            &[
                ("Gap", "/Gap 80"),
                ("Squash", "/Squash 0.5"),
                ("Lean", "/Lean 60"),
                ("Wobble", "/Wobble 4"),
                ("Clockwise", "/Clockwise true"),
            ],
        ),
        (
            "dyfigure",
            "50 10 80 << /Hair 3 {} >> dyfigure",
            &[
                ("Tadpole", "/Tadpole true"),
                ("Arms", "/Arms -30"),
                ("Hair", "/Hair 6"),
                ("Wobble", "/Wobble 5"),
                ("Overshoot", "/Overshoot 10"),
            ],
        ),
        (
            "dyrays",
            "50 50 15 << {} >> dyrays",
            &[
                ("Count", "/Count 7"),
                ("Length", "/Length 30"),
                ("LengthJitter", "/LengthJitter 0.9"),
                ("Offset", "/Offset 9"),
                ("Lean", "/Lean 30"),
                ("Wander", "/Wander 0.9"),
            ],
        ),
        (
            "dygrass",
            "newpath 10 10 moveto 90 12 lineto << {} >> dygrass",
            &[
                ("Spacing", "/Spacing 9"),
                ("Length", "/Length 30"),
                ("LengthJitter", "/LengthJitter 0.9"),
                ("Lean", "/Lean 40"),
                ("Direction", "/Direction 60"),
                ("Wander", "/Wander 0.9"),
                ("Zigzag", "/Zigzag true"),
            ],
        ),
    ];
    for (name, template, varied) in cases {
        let run = |extra: &str| path_of(&format!("7 srand {}", template.replace("{}", extra)));
        let baseline = run("");
        for (_, extra) in varied {
            assert_ne!(
                run(extra),
                baseline,
                "{name} {extra}: the path did not change"
            );
        }
        assert_tested(name, varied);
    }
}

/// The #184 contract: a knob that reshapes a slot doesn't re-roll where
/// the later slots land. Ray starts don't move when /Length changes, and
/// a second wobbly line doesn't move when the first one's overshoot does.
#[test]
fn reshaping_a_slot_does_not_reroll_the_others() {
    let starts = |len: &str| -> Vec<String> {
        let p = path_of(&format!("7 srand 50 50 15 << /Length {len} >> dyrays"));
        p.chunks(3)
            .filter(|c| c[2] == "/m")
            .flat_map(|c| [c[0].clone(), c[1].clone()])
            .collect()
    };
    assert_eq!(starts("10"), starts("40"), "ray starts moved with /Length");
    let second = |over: &str| -> Vec<String> {
        let p = path_of(&format!(
            "7 srand newpath 0 0 moveto 50 0 lineto 0 40 moveto 50 40 lineto << /Overshoot {over} >> dywobble"
        ));
        // the second subpath starts at the second moveto
        let ms: Vec<usize> = p
            .chunks(3)
            .enumerate()
            .filter(|(_, c)| c[2] == "/m")
            .map(|(i, _)| i)
            .collect();
        p.chunks(3)
            .skip(ms[1])
            .take(20)
            .flat_map(|c| c.to_vec())
            .collect()
    };
    // The overshoot changes the lead-in point, which is the moveto, so
    // compare from the first lineto of the second line on.
    let (a, b) = (second("2"), second("12"));
    assert_eq!(
        a[3..],
        b[3..],
        "the second line moved with the first's overshoot"
    );
}

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn gs_libs() -> String {
    let mut src = String::new();
    for path in LIBS {
        src.push_str(&std::fs::read_to_string(path).expect("read lib"));
        src.push('\n');
    }
    src
}

/// Ghostscript agrees with pscat on crayon strokes, block for block, with
/// /Seed pinned and fixed paths (the motions use `rand`, which differs
/// between the interpreters). A coarse weave puts the tooth at block
/// scale; another seed is the negative control. Local only: CI has no gs.
#[test]
fn ghostscript_agrees_on_crayon() {
    if !gs_available() {
        eprintln!("skipping gs crayon check: gs not installed");
        return;
    }
    const SCALE: usize = 4;
    const PX: usize = 200 * SCALE;
    let body = |seed: u32| {
        format!(
            "<< /Seed 4 /Weave 6 /Mottle 30 >> dypaper 0.97 0.96 0.93 setrgbcolor clippath fill \
             0.2 0.3 0.8 setrgbcolor newpath 10 40 moveto 100 170 lineto 190 60 lineto 20 120 moveto 180 130 lineto \
             << /Width 30 /Touch 0.6 /Seed {seed} >> dycrayon"
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
        let mut it = Interp::with_page_scaled(200, 200, SCALE as f32).expect("page");
        load(&mut it);
        it.run_str(&body(seed))
            .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        blocks(&rgb(&it))
    };
    let dir = std::env::temp_dir().join(format!("pscat-drycrayon-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("crayon.ps");
    let png = dir.join("crayon.png");
    std::fs::write(&file, format!("{}\n{}\nshowpage\n", gs_libs(), body(11))).expect("write");
    let status = std::process::Command::new("gs")
        .args(["-dNOPAUSE", "-dBATCH", "-q", "-sDEVICE=png16m"])
        .arg(format!("-g{PX}x{PX}"))
        .arg(format!("-r{}", 72 * SCALE))
        .arg(format!("-o{}", png.display()))
        .arg(&file)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected the dycrayon driver");
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
    eprintln!("pscat vs gs crayon: mean block diff {same:.2}, another seed {other:.2}");
    assert!(same < 6.0, "pscat and gs disagree on crayon: {same:.2}");
    assert!(
        other > same * 2.0,
        "another seed is as close ({other:.2}): the check is too weak"
    );
}

/// In real Ghostscript, whose procedures can be packed arrays: a packed
/// /Touch procedure is taken as a procedure, a packed procedure given for
/// a number is rejected by its guard, and every motion runs. Local only.
#[test]
fn ghostscript_takes_packed_procedures_and_runs_every_motion() {
    if !gs_available() {
        eprintln!("skipping gs packed-procedure check: gs not installed");
        return;
    }
    let driver = "<< >> dypaper 0 0 1 setrgbcolor \
        true setpacking \
        /pk { pop 0.7 } def \
        /pw { 2 } def \
        false setpacking \
        (packed: ) print /pk load type == \
        newpath 10 10 moveto 90 40 lineto << /Touch /pk load >> dycrayon (touch-ok) = \
        { newpath 0 0 moveto 50 0 lineto << /Wobble /pw load >> dywobble } stopped \
          { $error /errorname get == $error /command get == } { (no-error) = } ifelse \
        3 srand newpath 10 10 moveto 90 20 lineto << >> dywobble \
        10 10 80 40 screct << >> dyscribble 50 50 20 << >> dyloop \
        50 10 80 << /Tadpole true /Hair 4 >> dyfigure 50 50 15 << >> dyrays \
        newpath 10 10 moveto 90 12 lineto << /Zigzag true >> dygrass (motions-ok) = \
        showpage";
    let dir = std::env::temp_dir().join(format!("pscat-drycrayon-pk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("packed.ps");
    std::fs::write(&file, format!("{}\n{driver}\n", gs_libs())).expect("write");
    let out = std::process::Command::new("gs")
        .args(["-dNOPAUSE", "-dBATCH", "-q", "-sDEVICE=nullpage"])
        .arg(&file)
        .output()
        .expect("run gs");
    let _ = std::fs::remove_dir_all(&dir);
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    eprintln!("{text}");
    assert!(
        text.contains("packed: packedarraytype") || text.contains("packed: /packedarraytype"),
        "not packed: {text}"
    );
    assert!(
        text.contains("touch-ok"),
        "a packed /Touch was refused: {text}"
    );
    assert!(
        text.contains("/undefined") && text.contains("dywobble-wobble-must-be-non-negative"),
        "a packed /Wobble was not refused by its guard: {text}"
    );
    assert!(text.contains("motions-ok"), "a motion failed in gs: {text}");
}

/// The CI budget (DRYMEDIA.md 6.2): a small crayon scene -- a scribble
/// fill, a figure, rays and grass -- renders in under 2 s under
/// `--profile ci`. Printed so drift is visible before it fails.
#[test]
fn a_crayon_scene_renders_within_the_ci_budget() {
    let t0 = std::time::Instant::now();
    let it = render(
        200,
        150,
        1.0,
        "0.2 0.4 0.8 setrgbcolor 10 120 180 20 screct << >> dyscribble << >> dycrayon \
         0.1 0.1 0.1 setrgbcolor 60 20 70 << >> dyfigure << /Width 3 >> dycrayon \
         0.98 0.6 0.1 setrgbcolor 160 80 12 << >> dyrays << /Width 3 >> dycrayon \
         0.2 0.6 0.3 setrgbcolor newpath 5 15 moveto 195 18 lineto << >> dygrass << /Width 3 >> dycrayon",
    );
    let secs = t0.elapsed().as_secs_f64();
    eprintln!("crayon scene: {secs:.2}s");
    assert!(inked(&it).iter().any(|&i| i), "nothing drawn");
    assert!(
        secs < 2.0,
        "the scene took {secs:.2}s, over the 2s CI budget"
    );
}
