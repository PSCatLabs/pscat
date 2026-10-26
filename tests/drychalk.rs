//! drychalk (issue #235; design `docs/dev/design/DRYMEDIA.md`): the chalk preset
//! `dcchalk`, the chalk colours and the board dressing `dcboard`, over
//! drykit's `dkstroke`.
//!
//! Guards, "no draws from the caller's stream" and the packed-procedure
//! /Ghost are `%%SelfTest` blocks in `lib/drychalk.ps`. These tests pin
//! what the reference board says chalk does (REFERENCES.md C1-C9): a
//! landing brighter than the body, a lift that thins, a side-of-chalk
//! mark with brighter edges than its face, ghost writing far fainter than
//! fresh chalk, wipes darker than the haze and dust thickest at the ledge.

mod common;

use common::within_cpu_budget;
use pscat::Interp;
use std::time::Duration;

const LIBS: [&str; 6] = [
    "lib/artkit.ps",
    "lib/paintkit.ps",
    "lib/drykit.ps",
    "lib/handscript.ps",
    "lib/strokeglyphs.ps",
    "lib/drychalk.ps",
];

fn render(w: u32, h: u32, scale: f32, src: &str) -> Interp {
    let mut it = Interp::with_page_scaled(w, h, scale).expect("page");
    for path in LIBS {
        let text = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&text)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    it
}

/// A bare board (no haze features) filling the page, then `body`.
fn on_board(w: u32, h: u32, scale: f32, body: &str) -> Interp {
    render(
        w,
        h,
        scale,
        &format!(
            "0 0 {w} {h} << /Surface /slate /Seed 3 /Haze 0 /Swipes 0 /Wipes 0 \
             /Smudges 0 /Ledge 0 >> dcboard /white dcchalkcolor {body}"
        ),
    )
}

fn data(it: &Interp) -> Vec<u8> {
    it.gfx().pixmap.data().to_vec()
}

fn luma(p: tiny_skia::PremultipliedColorU8) -> f64 {
    0.3 * f64::from(p.red()) + 0.59 * f64::from(p.green()) + 0.11 * f64::from(p.blue())
}

/// Mean luma over the device-pixel rectangle [x0, x1) x [y0, y1).
fn mean(it: &Interp, x0: u32, y0: u32, x1: u32, y1: u32) -> f64 {
    let pm = &it.gfx().pixmap;
    let w = pm.width();
    let px = pm.pixels();
    let mut sum = 0.0;
    let mut n = 0.0;
    for y in y0..y1 {
        for x in x0..x1 {
            sum += luma(px[(y * w + x) as usize]);
            n += 1.0;
        }
    }
    sum / n
}

/// Pixels in column `x` brighter than `above`.
fn lit_in_column(it: &Interp, x: u32, above: f64) -> usize {
    let pm = &it.gfx().pixmap;
    let w = pm.width();
    (0..pm.height())
        .filter(|&y| luma(pm.pixels()[(y * w + x) as usize]) > above)
        .count()
}

fn advertised(proc_summary: &str, def: &str) -> Vec<String> {
    let src = std::fs::read_to_string("lib/drychalk.ps").expect("read drychalk");
    let start = src.find(proc_summary).expect("tags");
    let end = src[start..].find(def).expect("def") + start;
    let mut keys: Vec<String> = src[start..end]
        .lines()
        .filter_map(|l| l.strip_prefix("% @param: /"))
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .collect();
    keys.sort_unstable();
    keys
}

#[test]
fn dcchalk_every_documented_parameter_changes_the_render() {
    const BASE: &str = "/Width 3 /Seed 5";
    let stroke = |extra: &str| {
        data(&on_board(
            140,
            60,
            2.0,
            &format!(
                "newpath 10 30 moveto 70 45 lineto 130 25 lineto 20 12 moveto 120 14 lineto \
                 << {BASE} {extra} >> dcchalk"
            ),
        ))
    };
    let baseline = stroke("");
    let varied = [
        ("Width", "/Width 5"),
        ("Touch", "/Touch 0.4"),
        ("Landing", "/Landing 1"),
        ("Lift", "/Lift 1"),
        ("Firm", "/Firm 1"),
        ("Vary", "/Vary 0"),
        ("Dust", "/Dust 1"),
        ("Tilt", "/Tilt 80"),
        ("Side", "/Side 4 /Tilt 80"),
        ("Strength", "/Strength 0.9"),
        ("Streak", "/Streak 3"),
        ("Under", "/Under [0.5 0.1 0.1]"),
        ("Seed", "/Seed 6"),
    ];
    for (_, extra) in varied {
        assert_ne!(
            stroke(extra),
            baseline,
            "{extra} rendered the same as the baseline: a no-op"
        );
    }
    // /Side only acts on a tilted stick: check it against a tilted baseline.
    assert_ne!(
        stroke("/Side 4 /Tilt 80"),
        stroke("/Tilt 80"),
        "/Side is a no-op"
    );
    let mut tested: Vec<String> = varied.iter().map(|(k, _)| k.to_string()).collect();
    tested.sort_unstable();
    assert_eq!(
        advertised(
            "% @summary: Strokes the current path as chalk",
            "\n/dcchalk "
        ),
        tested,
        "advertised and tested keys differ"
    );
}

#[test]
fn dcboard_every_documented_parameter_changes_the_render() {
    let board = |extra: &str| {
        data(&render(
            160,
            100,
            1.0,
            &format!(
                "0 20 160 80 << /Seed 4 /Ghost {{ 20 60 moveto 140 64 lineto }} {extra} >> dcboard"
            ),
        ))
    };
    let baseline = board("");
    let varied = [
        ("Surface", "/Surface /slate"),
        ("Ground", "/Ground [0.3 0.1 0.1]"),
        ("Seed", "/Seed 5"),
        ("Weave", "/Weave 1.5"),
        ("Haze", "/Haze 1"),
        ("Ghost", "/Ghost { 20 30 moveto 140 34 lineto }"),
        ("Erased", "/Erased 0.1"),
        ("Swipes", "/Swipes 0"),
        ("Wipes", "/Wipes 6"),
        ("Smudges", "/Smudges 0"),
        ("Ledge", "/Ledge 0"),
        ("Tray", "/Tray 10"),
    ];
    for (_, extra) in varied {
        assert_ne!(
            board(extra),
            baseline,
            "{extra} rendered the same as the baseline: a no-op"
        );
    }
    let mut tested: Vec<String> = varied.iter().map(|(k, _)| k.to_string()).collect();
    tested.sort_unstable();
    assert_eq!(
        advertised("% @summary: Dresses a chalkboard", "\n/dcboard "),
        tested,
        "advertised and tested keys differ"
    );
}

/// Same input, same pixels; and a stroke's texture is seeded from its own
/// position, so writing a second word elsewhere re-rolls nothing.
#[test]
fn dcchalk_is_deterministic_and_position_seeded() {
    let word = "newpath (chalk) 10 20 << /Size 30 /Seed 2 >> sgpath << /Width 2 >> dcchalk";
    let a = on_board(200, 100, 2.0, word);
    let b = on_board(200, 100, 2.0, word);
    assert_eq!(data(&a), data(&b), "the same chalk rendered differently");
    let c = on_board(
        200,
        100,
        2.0,
        &format!(
            "{word} newpath (board) 100 65 << /Size 30 /Seed 3 >> sgpath << /Width 2 >> dcchalk"
        ),
    );
    // the first word's box, clear of the second word and its dust
    let (pa, pc) = (&a.gfx().pixmap, &c.gfx().pixmap);
    let w = pa.width();
    let mut differ = 0;
    for y in 110..170 {
        for x in 10..180 {
            let i = (y * w + x) as usize;
            if pa.pixels()[i] != pc.pixels()[i] {
                differ += 1;
            }
        }
    }
    assert_eq!(differ, 0, "a word written elsewhere changed this one");
}

/// C2/C3: the stroke lands heavier than its tired middle, and lifts thin.
/// Measured on many strokes, so one stroke's tooth can't decide it.
#[test]
fn a_stroke_lands_heavy_and_lifts_thin() {
    let it = on_board(
        240,
        200,
        2.0,
        "0 1 9 { /k exch def newpath 20 k 18 mul 15 add moveto 220 k 18 mul 15 add lineto \
         << /Width 4 /Dust 0 /Vary 0 /Landing 1 /Lift 1 /Seed k 100 mul >> dcchalk } for",
    );
    // device pixels: x = 2 * pt; strokes span x 40..440
    let land = mean(&it, 44, 0, 60, 400);
    let body = mean(&it, 200, 0, 280, 400);
    eprintln!("landing {land:.1}, body {body:.1}");
    assert!(
        land > body + 2.0,
        "the landing ({land:.1}) is not brighter than the body ({body:.1})"
    );
    let ground = mean(&it, 0, 0, 20, 400);
    let thr = ground + 20.0;
    let mid_w = lit_in_column(&it, 240, thr);
    let end_w = lit_in_column(&it, 432, thr);
    eprintln!("lit pixels: middle {mid_w}, near the lift {end_w}");
    assert!(
        (end_w as f64) < mid_w as f64 * 0.85,
        "the lift ({end_w}) is not thinner than the middle ({mid_w})"
    );
    assert!(end_w > 0, "the lift ends in nothing: a point, not a floor");
}

/// #713: /Firm keeps a long stroke solid to its end and its body, where the
/// default lift thins the last quarter of it and the middle tires. Many
/// strokes, so one stroke's tooth can't decide it. The page is a
/// transparent-style surface at a light touch, the case /Firm is for.
#[test]
fn firm_holds_a_long_stroke_to_its_end() {
    let strokes = |extra: &str| {
        on_board(
            240,
            200,
            2.0,
            &format!(
                "0 1 9 {{ /k exch def newpath 20 k 18 mul 15 add moveto 220 k 18 mul 15 add \
                 lineto << /Width 4 /Dust 0 /Vary 0 /Touch 0.34 /Seed k 100 mul {extra} >> \
                 dcchalk }} for"
            ),
        )
    };
    let (plain, firm) = (strokes(""), strokes("/Firm 1"));
    // the lift: device x 340..400 is 75%..90% along the stroke
    let ground = mean(&plain, 0, 0, 20, 400);
    let lift_plain = mean(&plain, 340, 0, 400, 400) - ground;
    let lift_firm = mean(&firm, 340, 0, 400, 400) - ground;
    eprintln!("lift above the ground: plain {lift_plain:.2}, firm {lift_firm:.2}");
    assert!(
        lift_firm > lift_plain * 1.05,
        "/Firm's lift ({lift_firm:.2}) is no brighter than the default's ({lift_plain:.2})"
    );
    let body_plain = mean(&plain, 200, 0, 280, 400) - ground;
    let body_firm = mean(&firm, 200, 0, 280, 400) - ground;
    eprintln!("body above the ground: plain {body_plain:.2}, firm {body_firm:.2}");
    assert!(
        body_firm > body_plain * 1.1,
        "/Firm's body ({body_firm:.2}) is no brighter than the default's ({body_plain:.2})"
    );
}

/// C3: a worn stick on its side presses at the edges of its contact, so a
/// broad mark has brighter edges than its granular face.
#[test]
fn a_side_stroke_is_brighter_at_its_edges() {
    let it = on_board(
        240,
        60,
        4.0,
        "newpath 20 30 moveto 220 30 lineto << /Tilt 90 /Side 12 /Dust 0 /Vary 0 /Seed 9 >> dcchalk",
    );
    // the mark spans y 24..36 pt = 96..144 device pixels (y down: 240-144..)
    let pm = &it.gfx().pixmap;
    let h = pm.height();
    let row = |pt: f64| {
        let y = (f64::from(h) - pt * 4.0) as u32;
        mean(&it, 200, y, 760, y + 4)
    };
    let edges = (row(34.5) + row(26.5)) / 2.0;
    let face = row(30.5);
    eprintln!("edges {edges:.1}, face {face:.1}");
    assert!(
        edges > face + 3.0,
        "the edges ({edges:.1}) are not brighter than the face ({face:.1})"
    );
}

/// C5: erased writing survives, but far fainter than fresh chalk.
#[test]
fn ghost_writing_is_faint_and_fresh_chalk_is_not() {
    let it = render(
        240,
        80,
        2.0,
        "0 0 240 80 << /Surface /greenboard /Seed 2 /Haze 0 /Swipes 0 /Wipes 0 /Smudges 0 \
         /Ledge 0 /Ghost { 20 55 moveto 220 55 lineto } >> dcboard \
         /white dcchalkcolor newpath 20 20 moveto 220 20 lineto << /Width 2.4 /Dust 0 >> dcchalk",
    );
    let pm = &it.gfx().pixmap;
    let h = pm.height();
    let band = |pt: f64| {
        let y = (f64::from(h) - pt * 2.0) as u32;
        mean(&it, 80, y - 2, 400, y + 2)
    };
    let ground = band(40.0);
    let ghost = band(55.0) - ground;
    let fresh = band(20.0) - ground;
    eprintln!("ghost +{ghost:.1}, fresh +{fresh:.1} over the board");
    assert!(ghost > 1.0, "the ghost writing vanished (+{ghost:.1})");
    assert!(
        ghost < fresh * 0.35,
        "the ghost (+{ghost:.1}) is not much fainter than fresh chalk (+{fresh:.1})"
    );
}

/// C9: dust settles thickest along the ledge.
#[test]
fn dust_is_thickest_at_the_ledge() {
    let ledge = |n: u32| {
        render(
            200,
            120,
            1.0,
            &format!(
                "0 0 200 120 << /Seed 2 /Haze 0.3 /Swipes 0 /Wipes 0 /Smudges 0 /Ledge {n} >> dcboard"
            ),
        )
    };
    let with = ledge(18);
    let without = ledge(0);
    let bottom = |it: &Interp| mean(it, 0, 112, 200, 120);
    let (b1, b0) = (bottom(&with), bottom(&without));
    eprintln!("bottom band: ledge {b1:.1}, none {b0:.1}");
    assert!(
        b1 > b0 + 4.0,
        "the ledge band ({b1:.1}) is no brighter ({b0:.1})"
    );
}

/// C5: a wipe takes the dust film off, so it reads darker than the haze.
#[test]
fn a_wipe_is_darker_than_the_haze() {
    let board = |wipes: u32| {
        render(
            200,
            120,
            1.0,
            &format!(
                "0 0 200 120 << /Seed 7 /Haze 1 /Swipes 0 /Wipes {wipes} /Smudges 0 /Ledge 0 >> dcboard"
            ),
        )
    };
    let all = |it: &Interp| mean(it, 0, 0, 200, 120);
    let (w, n) = (all(&board(8)), all(&board(0)));
    eprintln!("mean with wipes {w:.2}, without {n:.2}");
    assert!(
        w < n - 0.3,
        "wipes ({w:.2}) did not darken the board ({n:.2})"
    );
}

/// The CI budget (DRYMEDIA.md 6.2): a dressed board with a ghost and a
/// line of chalk renders in under 5 s under `--profile ci`. Thread CPU
/// time, best of 3 (issue #329), so load from other processes cannot fail it.
#[test]
fn a_small_board_renders_within_the_ci_budget() {
    let it = within_cpu_budget("small board", Duration::from_secs(5), 3, || {
        render(
            240,
            120,
            1.0,
            "0 0 240 120 << /Seed 1 /Ghost { (old notes) 20 70 << /Size 24 /Seed 1 >> sgpath } >> dcboard \
             /white dcchalkcolor newpath (chalk) 20 30 << /Size 28 /Seed 2 >> sgpath << >> dcchalk",
        )
    });
    assert!(!data(&it).is_empty());
}

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// One file holding every library and `body`, rendered by Ghostscript under
/// its default -dSAFER (so no `run` of library files), as RGB rows.
fn gs_rgb(body: &str, w: u32, h: u32, scale: u32, tag: &str) -> Vec<[u8; 3]> {
    let mut src = String::new();
    for path in LIBS {
        src.push_str(&std::fs::read_to_string(path).expect("read lib"));
        src.push('\n');
    }
    let dir = common::scratch_dir(&format!("pscat-drychalk-{tag}"));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("chalk.ps");
    let png = dir.join("chalk.png");
    std::fs::write(&file, format!("{src}\n{body}\nshowpage\n")).expect("write");
    let out = std::process::Command::new("gs")
        .args(["-dNOPAUSE", "-dBATCH", "-q", "-sDEVICE=png16m"])
        .arg(format!("-g{}x{}", w * scale, h * scale))
        .arg(format!("-r{}", 72 * scale))
        .arg(format!("-o{}", png.display()))
        .arg(&file)
        .output()
        .expect("run gs");
    assert!(
        out.status.success(),
        "gs rejected the drychalk driver: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let img = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).expect("png")).expect("decode");
    let _ = std::fs::remove_dir_all(&dir);
    img.pixels()
        .iter()
        .map(|p| [p.red(), p.green(), p.blue()])
        .collect()
}

/// Ghostscript agrees with pscat on a dressed board with chalk, block for
/// block, with every seed pinned; another seed is the negative control.
/// Runs locally only: CI has no gs.
#[test]
fn ghostscript_agrees_on_a_board() {
    if !gs_available() {
        eprintln!("skipping gs board check: gs not installed");
        return;
    }
    const SCALE: u32 = 2;
    let body = |seed: u32| {
        format!(
            "0 0 200 120 << /Seed {seed} /Tray 8 /Ghost {{ 20 80 moveto 180 84 lineto }} >> dcboard \
             /white dcchalkcolor newpath 20 40 moveto 180 44 lineto << /Width 3 /Seed 7 >> dcchalk \
             newpath 20 20 moveto 180 22 lineto << /Tilt 85 /Side 8 /Seed 8 >> dcchalk"
        )
    };
    let blocks = |rgb: &[[u8; 3]]| {
        let b = (10 * SCALE) as usize;
        let (pw, ph) = ((200 * SCALE) as usize, (120 * SCALE) as usize);
        let (bw, bh) = (pw / b, ph / b);
        let mut cells = vec![0f64; bw * bh];
        for (i, p) in rgb.iter().enumerate() {
            let (x, y) = (i % pw, i / pw);
            cells[(y / b) * bw + x / b] +=
                (f64::from(p[0]) + f64::from(p[1]) + f64::from(p[2])) / 3.0 / (b * b) as f64;
        }
        cells
    };
    let pscat = |seed: u32| {
        let it = render(200, 120, SCALE as f32, &body(seed));
        let rgb: Vec<[u8; 3]> = it
            .gfx()
            .pixmap
            .pixels()
            .iter()
            .map(|p| [p.red(), p.green(), p.blue()])
            .collect();
        blocks(&rgb)
    };
    let gs = blocks(&gs_rgb(&body(11), 200, 120, SCALE, "agree"));
    let diff = |a: &[f64], b: &[f64]| {
        a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f64>() / a.len() as f64
    };
    let same = diff(&pscat(11), &gs);
    let other = diff(&pscat(12), &gs);
    eprintln!("pscat vs gs board: mean block diff {same:.2}, another seed {other:.2}");
    assert!(same < 1.5, "pscat and gs disagree on a board: {same:.2}");
    assert!(
        other > same * 2.0,
        "another seed is as close ({other:.2}): the check is too weak"
    );
}

/// Every procedure-valued key accepts a packed procedure in real
/// Ghostscript too (`dcisproc` checks `packedarraytype`): /Ghost built
/// under `true setpacking`.
#[test]
fn ghostscript_accepts_a_packed_ghost_procedure() {
    if !gs_available() {
        eprintln!("skipping gs packed-procedure check: gs not installed");
        return;
    }
    let body = "true setpacking /pk { 20 60 moveto 180 64 lineto } def false setpacking \
                /pk load type /packedarraytype ne { pk-is-not-packed } if \
                0 0 200 120 << /Seed 3 /Ghost /pk load >> dcboard";
    let rgb = gs_rgb(body, 200, 120, 1, "packed");
    assert!(!rgb.is_empty());
    // pscat has no packed arrays (setpacking is accepted, and the procedure
    // stays an ordinary one), so it only shows the same driver runs here
    let it = render(
        200,
        120,
        1.0,
        &body.replace(
            "/pk load type /packedarraytype ne { pk-is-not-packed } if",
            "",
        ),
    );
    assert!(!data(&it).is_empty());
}

/// The tray sits below the board: the board's own bottom rows are the same
/// with and without it (Codex review of PR #310).
#[test]
fn the_tray_is_drawn_below_the_board() {
    let board = |tray: u32| {
        render(
            100,
            100,
            2.0,
            &format!(
                "0 30 100 70 << /Seed 2 /Tray {tray} /Ledge 10 /Swipes 0 /Wipes 0 /Smudges 0 >> dcboard"
            ),
        )
    };
    let (a, b) = (board(0), board(20));
    let w = a.gfx().pixmap.width();
    // device rows 0..140 are the board (y 30..100pt at 2x, y down)
    let rows = |it: &Interp| it.gfx().pixmap.pixels()[..(140 * w) as usize].to_vec();
    assert_eq!(rows(&a), rows(&b), "the tray painted over the board");
    assert_ne!(data(&a), data(&b), "the tray drew nothing");
}

/// /Landing presses a dot harder (Codex review of PR #310): a page of dots
/// is brighter at /Landing 1 than at /Landing 0.
#[test]
fn landing_presses_dots_harder() {
    let dots = |landing: &str| {
        on_board(
            200,
            60,
            2.0,
            &format!(
                "newpath 0 1 19 {{ 9 mul 10 add 30 moveto 0.5 0 rlineto }} for \
                 << /Width 3 /Dust 0 /Vary 0 /Landing {landing} /Seed 4 >> dcchalk"
            ),
        )
    };
    let lit = |it: &Interp| mean(it, 0, 40, 400, 80);
    let (hard, soft) = (lit(&dots("1")), lit(&dots("0")));
    eprintln!("dots: landing 1 {hard:.2}, landing 0 {soft:.2}");
    assert!(
        hard > soft + 0.2,
        "/Landing did not press the dots ({hard:.2} vs {soft:.2})"
    );
}
