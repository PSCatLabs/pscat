//! The /Typewriter font (issue #211; design `docs/dev/design/TYPEWRITER.md`).
//!
//! The option guards and presets are `%%SelfTest` blocks in
//! `lib/typewriter.ps`. These tests need pixels or a real `gs`: the same
//! seeds give the same page, repeated strikes differ while a machine's
//! quirks hold, a second strike at one cell differs, `stringwidth` and
//! `charpath` change nothing, the caller's `rand` stream is untouched, zero
//! variation is the plain Courier Prime face, Ghostscript types the same
//! page without caching a glyph, and a strip fits the CI budget.

mod common;

use common::within_cpu_budget;
use pscat::Interp;
use std::time::Duration;

const LIBS: [&str; 2] = ["lib/typewriter_outlines.ps", "lib/typewriter.ps"];

/// 300 dpi: a pica cell (7.2pt) is exactly 30 px and a line (12pt) 50 px,
/// so cells that start on the frame's grid start on whole pixels and two
/// strikes with the same variation rasterize the same.
const SCALE: f32 = 300.0 / 72.0;
const CELL: usize = 30;
const LINE: usize = 50;
/// The page, in points, and where typing starts: (36, 84) is (150, 150) px
/// from the top left.
const W: u32 = 288;
const H: u32 = 120;
const X0: usize = 150;
const Y0: usize = 150;

fn load(it: &mut Interp) {
    for path in LIBS {
        let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
}

/// Runs `src` on a white page with the libraries loaded.
fn render(src: &str) -> Interp {
    let mut it = Interp::with_page_scaled(W, H, SCALE).expect("page");
    load(&mut it);
    it.run_str(&format!("1 setgray clippath fill {src}"))
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    it
}

/// Darkness (255 - luminance) per pixel.
fn ink(it: &Interp) -> Vec<u8> {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .map(|p| {
            255 - ((u32::from(p.red()) + u32::from(p.green()) + u32::from(p.blue())) / 3) as u8
        })
        .collect()
}

fn width_px() -> usize {
    (W as f32 * SCALE).round() as usize
}

/// The pixels of the two-cell box around cell (`line`, `col`): one cell to
/// either side of the glyph's advance, from an ascender to a descender.
fn cell(img: &[u8], line: usize, col: usize) -> Vec<u8> {
    let w = width_px();
    let (x0, y0) = (X0 + col * CELL - CELL / 2, Y0 + line * LINE - 40);
    let mut out = Vec::with_capacity(2 * CELL * 52);
    for y in y0..y0 + 52 {
        out.extend_from_slice(&img[y * w + x0..y * w + x0 + 2 * CELL]);
    }
    out
}

/// Types `text` on three lines at the default grid, with `opts`.
fn three_lines(opts: &str, text: &str) -> Vec<u8> {
    ink(&render(&format!(
        "36 84 moveto << {opts} >> tw-setup ({text}) show \
         36 72 moveto ({text}) show 36 60 moveto ({text}) show"
    )))
}

fn mean_diff(a: &[u8], b: &[u8]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (f64::from(*x) - f64::from(*y)).abs())
        .sum::<f64>()
        / a.len() as f64
}

/// The ribbon ink's clip has hundreds of edges per glyph, so the same
/// strike drawn at another cell (another device origin) can round a pixel
/// or two differently. Two renders are "the same strike" when at most
/// this many pixels differ; a different strike moves hundreds.
const ROUNDING_PX: usize = 12;

fn differing(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

fn nearly_same(a: &[u8], b: &[u8]) -> bool {
    differing(a, b) <= ROUNDING_PX
}

const PARAGRAPH: &str = "Dear Sir, In reply to your letter of June 22";

#[test]
fn the_same_seeds_type_the_same_page_byte_for_byte() {
    let page = |opts: &str| three_lines(opts, PARAGRAPH);
    let a = page("/Machine 7 /Seed 3");
    assert_eq!(a, page("/Machine 7 /Seed 3"), "same seeds, another page");
    assert_ne!(a, page("/Machine 7 /Seed 4"), "a new /Seed changed nothing");
    assert_ne!(
        a,
        page("/Machine 8 /Seed 3"),
        "a new /Machine changed nothing"
    );
    // The caller's graphics state is not the font's: a dash and a cap set
    // before typing leave the ink as it was.
    let dashed = ink(&render(&format!(
        "[3 2] 0 setdash 2 setlinecap \
         36 84 moveto << /Machine 7 /Seed 3 >> tw-setup ({PARAGRAPH}) show \
         36 72 moveto ({PARAGRAPH}) show 36 60 moveto ({PARAGRAPH}) show"
    )));
    assert_eq!(a, dashed, "the caller's dash or cap changed the ink");
    // Plain use, without tw-setup: the load-time defaults and frame.
    let plain = || {
        ink(&render(&format!(
            "/Typewriter findfont 12.5 scalefont setfont 36 84 moveto ({PARAGRAPH}) show"
        )))
    };
    assert_eq!(plain(), plain(), "plain findfont/show is not deterministic");
}

#[test]
fn repeated_strikes_differ_but_a_machines_quirks_hold() {
    // Every other cell, so a box holds one glyph and its neighbours' ink
    // stays out of it.
    let text = "e e e e e e e";
    let cells = |img: &[u8]| -> Vec<Vec<u8>> {
        (0..3)
            .flat_map(|line| (0..7).map(move |k| (line, 2 * k)))
            .map(|(line, col)| cell(img, line, col))
            .collect()
    };
    let full = cells(&three_lines("/Machine 2 /Seed 5", text));
    for i in 0..full.len() {
        for j in i + 1..full.len() {
            assert!(
                !nearly_same(&full[i], &full[j]),
                "strikes {i} and {j} of `e` are identical"
            );
        }
    }
    // Machine quirks only: every `e` is the same `e`, on every line and in
    // every document, and another machine's `e` is a different one. The
    // ink's weave is keyed by the key, not the strike, when strikes do not
    // vary, so this holds with the ink model on (up to rounding).
    let quirks = "/Machine 2 /LineWear 0 /StrikeWear 0";
    let only = cells(&three_lines(&format!("{quirks} /Seed 5"), text));
    assert!(
        only.iter().all(|c| nearly_same(c, &only[0])),
        "a machine quirk moved within a page"
    );
    assert!(only[0].iter().any(|&p| p > 128), "the box missed the glyph");
    let other_doc = cells(&three_lines(&format!("{quirks} /Seed 6"), text));
    assert!(
        nearly_same(&only[0], &other_doc[0]),
        "a new /Seed re-rolled the machine's quirks"
    );
    let other_machine = cells(&three_lines(
        "/Machine 3 /LineWear 0 /StrikeWear 0 /Seed 5",
        text,
    ));
    assert!(
        !nearly_same(&only[0], &other_machine[0]),
        "two machines type the same `e`"
    );
}

#[test]
fn a_second_strike_at_a_cell_differs_from_the_first() {
    let strike = |n: i32| {
        cell(
            &ink(&render(&format!(
                "36 84 moveto << /Machine 1 /Seed 1 /Strike {n} >> tw-setup (a) show"
            ))),
            0,
            0,
        )
    };
    assert_eq!(strike(0), strike(0));
    assert_ne!(
        strike(0),
        strike(1),
        "strike index 1 retyped strike 0 exactly"
    );
    assert_ne!(strike(1), strike(2));
}

/// The line layer is memoized per line, already scaled by the wear. A
/// change of options mid-line must drop it, or a strike would depend on
/// what was typed before it on the line.
#[test]
fn an_option_change_mid_line_types_as_a_fresh_block_would() {
    let after = |img: &[u8]| -> Vec<u8> {
        let w = width_px();
        // columns 3 onwards; the space at column 2 keeps `b`'s ink out
        let x0 = X0 + 3 * CELL - CELL / 2;
        (Y0 - 40..Y0 + 12)
            .flat_map(|y| img[y * w + x0..y * w + x0 + 3 * CELL].to_vec())
            .collect()
    };
    let changed = ink(&render(
        "36 84 moveto << /Machine 3 /Seed 8 /Wear 2 >> tw-setup (ab ) show \
         << /Wear 0.5 >> tw-set (def) show",
    ));
    let fresh = ink(&render(
        "36 84 moveto << /Machine 3 /Seed 8 /Wear 0.5 >> tw-setup 57.6 84 moveto (def) show",
    ));
    assert!(
        after(&fresh).iter().any(|&p| p > 128),
        "the box missed the glyphs"
    );
    // Up to rounding: `show` reaches column 3 by adding advances, the
    // fresh block by a moveto.
    let n = differing(&after(&changed), &after(&fresh));
    assert!(
        n <= ROUNDING_PX,
        "a mid-line /Wear change kept the line's old variation ({n} px differ)"
    );
}

#[test]
fn stringwidth_and_charpath_change_nothing_a_later_show_draws() {
    let setup = "36 84 moveto << /Machine 4 /Seed 2 >> tw-setup";
    let plain = ink(&render(&format!("{setup} ({PARAGRAPH}) show")));
    let probed = ink(&render(&format!(
        "{setup} ({PARAGRAPH}) stringwidth pop pop \
         gsave newpath 36 84 moveto ({PARAGRAPH}) true charpath newpath grestore \
         36 84 moveto ({PARAGRAPH}) show"
    )));
    assert_eq!(plain, probed, "stringwidth or charpath changed the page");
    let blank = ink(&render(&format!(
        "{setup} ({PARAGRAPH}) stringwidth pop pop newpath 36 84 moveto ({PARAGRAPH}) false charpath newpath"
    )));
    assert!(
        blank.iter().all(|&p| p == 0),
        "stringwidth or charpath painted"
    );
    // The advance is the pitch, whatever the glyph's variation.
    let mut it = render(&format!("{setup} (abc) stringwidth"));
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(stack.len(), 2);
    let wx: f64 = stack[0].parse().expect("a number");
    assert!((wx - 21.6).abs() < 1e-6, "three pica advances: {wx}");
    it.run_str("clear").expect("clear");
}

#[test]
fn the_callers_rand_stream_is_untouched() {
    let draws = |src: &str| -> Vec<String> {
        let mut it = Interp::with_page_scaled(W, H, SCALE).expect("page");
        it.run_str("42 srand").expect("srand");
        if !src.is_empty() {
            load(&mut it);
            it.run_str(src)
                .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
        }
        it.run_str("rand rand rand").expect("rand");
        it.operand_stack().iter().map(|o| o.repr()).collect()
    };
    let bare = draws("");
    let typed = draws(&format!(
        "36 84 moveto << /Machine 5 /Seed 5 /Wear 3 >> tw-setup ({PARAGRAPH}) show \
         << /Machine 6 /Strike 1 >> tw-set ({PARAGRAPH}) stringwidth pop pop"
    ));
    assert_eq!(bare, typed, "loading or typing drew from rand");
}

#[test]
fn zero_variation_is_the_plain_courier_prime_face() {
    // /Wear 0 removes every variation, /InkModel false the ribbon's weave
    // and /Ribbon 0 the ink spread. What is
    // left must be Courier Prime itself, drawn here from the TrueType file
    // through pscat's own font machinery, squeezed to pica as the font is.
    // Each character is placed on the pica grid: the TTF's own advance is
    // 1228/2048 em, 0.07% short of Courier's 600 units, which a whole line
    // of `show` would accumulate.
    let zero = ink(&render(&format!(
        "36 84 moveto << /Wear 0 /Ribbon 0 /InkModel false >> tw-setup ({PARAGRAPH}) show"
    )));
    let ttf = ink(&render(&format!(
        "0.13 0.13 0.15 setrgbcolor \
         /CourierPrime-Regular findfont [12.5 0.96 mul 0 0 12.5 0 0] makefont setfont \
         /i 0 def ({PARAGRAPH}) {{ 36 i 7.2 mul add 84 moveto ( ) dup 0 4 -1 roll put show /i i 1 add def }} forall"
    )));
    let worn = ink(&render(&format!(
        "36 84 moveto << /Wear 0 /InkModel false >> tw-setup ({PARAGRAPH}) show"
    )));
    let same = mean_diff(&zero, &ttf);
    let spread = mean_diff(&worn, &ttf);
    eprintln!("zero variation vs the TTF: {same:.4}; with the ink spread: {spread:.4}");
    assert!(ttf.iter().any(|&p| p > 128), "the TTF did not render");
    assert!(
        same < 0.05,
        "zero variation is not the plain face: {same:.4}"
    );
    assert!(
        spread > 20.0 * same.max(0.01),
        "the comparison cannot tell a spread stroke"
    );
    // And with no variation, the seeds have nothing to vary.
    let seeded = ink(&render(&format!(
        "36 84 moveto << /Wear 0 /Ribbon 0 /InkModel false /Machine 9 /Seed 9 >> tw-setup ({PARAGRAPH}) show"
    )));
    assert_eq!(zero, seeded, "seeds moved a glyph at /Wear 0");
}

/// Total darkness of a page.
fn mass(img: &[u8]) -> f64 {
    img.iter().map(|&p| f64::from(p)).sum()
}

/// The ribbon ink (#212) leaves the weave's voids open inside the type
/// face, where flat ink is solid, and a drier ribbon leaves more of them.
#[test]
fn the_weave_opens_voids_and_a_dry_ribbon_opens_more() {
    let line = |opts: &str| {
        ink(&render(&format!(
            "36 84 moveto << /Machine 1 /Seed 1 {opts} >> tw-setup ({PARAGRAPH}) show"
        )))
    };
    // the share of the bare face's solid pixels (no spread: /Ribbon 0, flat
    // ink) that the weave leaves light, so the thinner edge of the ink (#423)
    // is not counted as a void
    let flat = line("/Ribbon 0 /InkModel false");
    let voids = |ribbon: f64| {
        let inked = line(&format!("/Ribbon {ribbon}"));
        let solid = flat.iter().filter(|&&p| p >= 200).count();
        let open = flat
            .iter()
            .zip(&inked)
            .filter(|&(&f, &i)| f >= 200 && i < 128)
            .count();
        open as f64 / solid as f64
    };
    let wet = voids(0.7);
    let dry = voids(0.15);
    eprintln!("voids in the type face: wet ribbon {wet:.3}, dry ribbon {dry:.3}");
    assert!(
        (0.01..0.25).contains(&wet),
        "a wet ribbon left {wet:.3} of the face open"
    );
    assert!(
        dry > 1.5 * wet,
        "a dry ribbon ({dry:.3}) is not patchier than a wet one ({wet:.3})"
    );
}

/// Elite is inked a little lighter than pica (#511): its weave opens more
/// holes. Pica, and a photocopy of elite, are not touched, to the byte.
#[test]
fn elite_takes_more_holes_than_pica_and_pica_is_untouched() {
    let line = |opts: &str, bump: f64| {
        ink(&render(&format!(
            "TWState /tEliteHole {bump} put \
             36 84 moveto << /Machine 1 /Seed 1 {opts} >> tw-setup ({PARAGRAPH}) show"
        )))
    };
    let (pica, pica_off) = (line("/Pitch 10", 0.18), line("/Pitch 10", 0.0));
    assert_eq!(pica, pica_off, "the elite constant moved a pica page");
    let (elite, elite_off) = (line("/Pitch 12", 0.18), line("/Pitch 12", 0.0));
    let (m, m0) = (mass(&elite), mass(&elite_off));
    eprintln!(
        "elite mass with the bump {m:.0}, without {m0:.0} ({:.3})",
        m / m0
    );
    assert!(
        m < 0.99 * m0,
        "elite is not lighter ({m:.0} against {m0:.0})"
    );
    assert!(
        m > 0.8 * m0,
        "elite lost too much ink ({m:.0} against {m0:.0})"
    );
    let copy = |bump: f64| line("/Pitch 12 /Copies 1", bump);
    assert_eq!(
        copy(0.18),
        copy(0.0),
        "the elite constant moved a photocopy"
    );
}

/// A clogged key's counter fills (TYPEWRITER.md section 6 step 4).
#[test]
fn a_clogged_counter_fills_with_ink() {
    // Machine 10's `o` is clogged (clog fraction 1.0); machine 1's is not.
    // The counter's middle, at the cell's centre and half the x-height up.
    let middle = |machine: i32| -> f64 {
        let img = ink(&render(&format!(
            "36 84 moveto << /Machine {machine} /Seed 1 /LineWear 0 /StrikeWear 0 >> tw-setup (o) show"
        )));
        let w = width_px();
        let (cx, cy) = (X0 + CELL / 2, Y0 - 13);
        let mut sum = 0.0;
        for y in cy - 2..=cy + 2 {
            for x in cx - 2..=cx + 2 {
                sum += f64::from(img[y * w + x]);
            }
        }
        sum / 25.0
    };
    let clogged = middle(10);
    let clean = middle(1);
    eprintln!("counter darkness: clogged {clogged:.1}, clean {clean:.1}");
    assert!(
        clogged > 100.0,
        "machine 10's `o` did not fill: {clogged:.1}"
    );
    assert!(
        clean < 20.0,
        "machine 1's clean `o` has ink in its counter: {clean:.1}"
    );
    // With the ink model off the counter stays open, as #211 drew it.
    let flat = mass(&ink(&render(
        "36 84 moveto << /Machine 10 /Seed 1 /LineWear 0 /StrikeWear 0 /InkModel false >> tw-setup (o) show",
    )));
    let inked = mass(&ink(&render(
        "36 84 moveto << /Machine 10 /Seed 1 /LineWear 0 /StrikeWear 0 >> tw-setup (o) show",
    )));
    assert!(inked > flat, "the clogged counter added no ink");
}

/// (red, green, blue) per pixel.
fn rgb(it: &Interp) -> Vec<(u8, u8, u8)> {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .map(|p| (p.red(), p.green(), p.blue()))
        .collect()
}

/// Rows (device pixels) of a glyph-unit height at `/Size 40`: one unit is
/// 0.04pt, so 1/6 px at this file's scale; the baseline is row `Y0`.
fn row_of(units: i32) -> usize {
    (Y0 as i32 - units / 6) as usize
}

/// The ribbon's extras (#422) are opt-in: a page with both keys at 0 is the
/// page without them, to the byte, and with them on it is still a function
/// of the seeds and leaves the caller's rand stream alone.
#[test]
fn the_ribbon_extras_are_off_by_default_and_deterministic() {
    let page = |opts: &str| {
        ink(&render(&format!(
            "36 84 moveto << /Machine 7 /Seed 3 {opts} >> tw-setup ({PARAGRAPH}) show \
             36 72 moveto ({PARAGRAPH}) show"
        )))
    };
    let plain = page("");
    assert_eq!(
        plain,
        page("/WornBand 0 /Bleed 0"),
        "the keys at 0 moved ink"
    );
    let on = "/WornBand 0.8 /Bleed 0.8";
    assert_ne!(plain, page(on), "the keys did nothing");
    assert_eq!(page(on), page(on), "the extras are not deterministic");
    // each key moves only its own effect: the worn band's page is not the bleed's
    assert_ne!(page("/WornBand 0.8"), page("/Bleed 0.8"));
    let draws = |src: &str| -> Vec<String> {
        let mut it = Interp::with_page_scaled(W, H, SCALE).expect("page");
        it.run_str("42 srand").expect("srand");
        if !src.is_empty() {
            load(&mut it);
            it.run_str(src)
                .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
        }
        it.run_str("rand rand rand").expect("rand");
        it.operand_stack().iter().map(|o| o.repr()).collect()
    };
    assert_eq!(
        draws(""),
        draws(&format!(
            "36 84 moveto << /Machine 5 /Seed 5 {on} /BleedInk [1 0 0] /BleedSide /above >> tw-setup \
             ({PARAGRAPH}) show"
        )),
        "the extras drew from rand"
    );
}

/// The worn band (#422) thins the ink in the lane every x-height strike
/// hits (glyph heights 20 to 410) and leaves the ascender (480 to 640)
/// alone; /InkModel false has no weave to wear.
#[test]
fn the_worn_band_lightens_the_lane_and_not_the_ascender() {
    let page = |opts: &str| {
        ink(&render(&format!(
            "36 84 moveto << /Machine 1 /Seed 1 /Wear 0 /Size 40 /Pitch 6 {opts} >> tw-setup (lkl) show"
        )))
    };
    let band_mass = |img: &[u8], lo: i32, hi: i32| -> f64 {
        let w = width_px();
        (row_of(hi)..row_of(lo))
            .map(|y| {
                img[y * w..(y + 1) * w]
                    .iter()
                    .map(|&p| f64::from(p))
                    .sum::<f64>()
            })
            .sum()
    };
    let (off, on) = (page("/WornBand 0"), page("/WornBand 1"));
    let lane = band_mass(&on, 20, 410) / band_mass(&off, 20, 410);
    let top = band_mass(&on, 500, 640) / band_mass(&off, 500, 640);
    eprintln!("worn band: lane keeps {lane:.3} of its ink, the ascender {top:.3}");
    assert!(lane < 0.9, "the lane kept {lane:.3} of its ink");
    assert!(top > 0.95, "the ascender lost ink: kept {top:.3}");
    // no seam: with the lane no sparser than its surround the zones draw the
    // same ink as one pass would (the three nest)
    let flat = |opts: &str| {
        ink(&render(&format!(
            "36 84 moveto << /Machine 1 /Seed 1 /Wear 0 /Size 40 /Pitch 6 /InkModel false {opts} >> tw-setup (lkl) show"
        )))
    };
    assert_eq!(
        flat("/WornBand 0"),
        flat("/WornBand 1"),
        "flat ink was worn"
    );
}

/// Two-colour bleed (#422): black text prints the ribbon's other colour in
/// the part of a letter past the boundary, below the line on /below. A
/// bleed too small to reach a letter leaves it as it was.
#[test]
fn bleed_prints_the_other_colour_below_the_boundary() {
    let red_rows = |opts: &str, text: &str| -> Vec<usize> {
        let it = render(&format!(
            "36 84 moveto << /Machine 1 /Seed 2 /Size 40 /Pitch 3.5 {opts} >> tw-setup ({text}) show"
        ));
        let w = width_px();
        rgb(&it)
            .iter()
            .enumerate()
            .filter(|&(_, &(r, g, b))| r > g.saturating_add(50) && r > b.saturating_add(50))
            .map(|(i, _)| i / w)
            .collect()
    };
    let text = "Hxdlbkpgyqjpy";
    let ink_red = "/BleedInk [0.9 0.1 0.1]";
    assert!(
        red_rows(&format!("/Bleed 0 {ink_red}"), text).is_empty(),
        "red with no bleed"
    );
    assert!(
        red_rows(&format!("/Bleed 1 {ink_red}"), "Hxdlbk").len() > 20,
        "no red from a full bleed"
    );
    let rows = red_rows(&format!("/Bleed 1 {ink_red}"), text);
    assert!(
        rows.len() > 100,
        "a full bleed coloured {} pixels",
        rows.len()
    );
    // the boundary starts below the descender and reaches up at most to the
    // x-height's middle: nothing red above 300 units
    let top = rows.iter().min().copied().unwrap();
    assert!(top >= row_of(300), "red reached row {top}, above 300 units");
    // /above colours the other end: black text's red head
    let above = red_rows(&format!("/BleedSide /above /Bleed 1 {ink_red}"), text);
    assert!(above.len() > 100, "no red head: {}", above.len());
    assert!(
        above.iter().max().copied().unwrap() <= row_of(150),
        "a head reached below 150 units"
    );
    // a bleed too small to reach a letter (the boundary rests outside the
    // face, past every descender and ascender) is the page without a bleed,
    // to the byte: on descenders, ascenders and the baseline, on both sides
    let page = |opts: &str| {
        ink(&render(&format!(
            "36 84 moveto << /Machine 1 /Seed 2 {opts} >> tw-setup (Hxdlbkpgyqj Hl) show"
        )))
    };
    for side in ["/below", "/above"] {
        assert_eq!(
            page(&format!("/Bleed 0 /BleedSide {side}")),
            page(&format!("/Bleed 0.0001 /BleedSide {side}")),
            "a bleed too small to reach a letter moved ink on {side}"
        );
    }
}

/// The extras compose (#422, Codex round 1): where the bleed's other colour
/// lies in the worn lane it wears too, so a worn lane lightens the red foot.
#[test]
fn the_worn_lane_lightens_the_bleed_colour_too() {
    // the lane is moved down over the descenders, where this bleed's red lies
    let red_mass = |opts: &str| -> f64 {
        let it = render(&format!(
            "TWState /tWornLo -500 put TWState /tWornHi 100 put \
             36 84 moveto << /Machine 1 /Seed 2 /Wear 0 /Size 40 /Pitch 3.5 /Bleed 1 \
             /BleedInk [0.9 0.1 0.1] {opts} >> tw-setup (Hxdlbkpgyqjpy) show"
        ));
        rgb(&it)
            .iter()
            .map(|&(r, g, b)| f64::from(r.saturating_sub(g.max(b))))
            .sum()
    };
    let (plain, worn) = (red_mass("/WornBand 0"), red_mass("/WornBand 1"));
    eprintln!("red mass: lane intact {plain:.0}, worn {worn:.0}");
    assert!(plain > 0.0, "no red at the foot");
    assert!(worn < 0.95 * plain, "the worn lane left the red foot alone");
}

/// The pin stripe (#538): `/BleedGap` cuts a strip of no ink, centred on the
/// colour boundary, from both inks; a gap of 0, or one with no bleed to reach a
/// letter, is the page without it, to the byte.
#[test]
fn bleed_gap_leaves_a_white_stripe_at_the_boundary() {
    // /Wear 0 and one key repeated: every strike has the same boundary. At
    // /Size 40 a glyph unit is 1/6 px, so one yarn (0.48pt) is 12 units (2 px)
    let page = |opts: &str| {
        render(&format!(
            "36 84 moveto << /Machine 1 /Seed 2 /Wear 0 /Size 40 /Pitch 3.5 \
             /BleedInk [0.9 0.1 0.1] {opts} >> tw-setup (pppppp) show"
        ))
    };
    let plain = page("/Bleed 1");
    // the default, and a gap that has no bleed to reach a letter, move nothing
    assert_eq!(
        ink(&plain),
        ink(&page("/Bleed 1 /BleedGap 0")),
        "a gap of 0"
    );
    assert_eq!(
        ink(&page("/Bleed 0")),
        ink(&page("/Bleed 0 /BleedGap 6")),
        "a gap with no bleed"
    );
    // rows (across the whole line) that hold ink without the gap and none with it
    let rows_cut = |gap: &str| -> usize {
        let (w, base) = (width_px(), ink(&plain));
        let cut = ink(&page(&format!("/Bleed 1 /BleedGap {gap}")));
        (row_of(500)..row_of(-600))
            .filter(|&y| {
                let row = |img: &[u8]| img[y * w..(y + 1) * w].iter().any(|&v| v > 60);
                row(&base) && !row(&cut)
            })
            .count()
    };
    let (one, three) = (rows_cut("1"), rows_cut("3"));
    eprintln!("rows cut: 1 yarn {one}, 3 yarns {three}");
    // one yarn is 2 px, and the strip is bounded by the transition's weave
    assert!(three >= 4, "a 3-yarn gap cut {three} rows");
    assert!(
        three > one,
        "a wider gap cut no more rows ({one} vs {three})"
    );
    assert!(
        ink(&plain) != ink(&page("/Bleed 1 /BleedGap 1")),
        "no stripe at 1"
    );
    // and on a red run (/above) the stripe is cut there too
    let head = |gap: &str| {
        ink(&render(&format!(
            "36 84 moveto << /Machine 1 /Seed 2 /Wear 0 /Size 40 /Pitch 3.5 /Bleed 1 \
             /BleedSide /above /BleedGap {gap} >> tw-setup (dlbkdlbk) show"
        )))
    };
    assert!(head("0") != head("3"), "no stripe on /above");
}

/// One strike of machine 4's clogged `o` (fill 1 at `/Wear 1.5`), `/Bleed` poked
/// past its 0..1 so the boundary can cross the counter (62..390 units): at
/// `/Bleed 1` it can too, but none of seeds 1..40 gets below about 354. The
/// stack is left with the boundary's height in the cell frame, or -9999 if the
/// strike's boundary misses the letter.
fn clogged_o(seed: u32, opts: &str) -> Interp {
    render(&format!(
        "36 84 moveto << /Machine 4 /Seed {seed} /Wear 1.5 /Size 40 /Pitch 3.5 \
         /BleedInk [0.9 0.1 0.1] /BleedSide /above {opts} >> tw-setup \
         TWState /tBleed 1.6 put (o) show \
         TWState /tZFar get null eq {{ -9999 }} \
         {{ TWState /zbY get TWState /tYoff get add }} ifelse"
    ))
}

/// The clogged counter is ribbon ink like the body (#539): where the bleed's
/// boundary puts part of a letter in the other colour, the counter's fill there
/// is the other colour too, not the main ink.
#[test]
fn a_clogged_counter_takes_the_bleed_colour() {
    let boundary = |it: &Interp| -> f64 {
        let st: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
        st.last().and_then(|v| v.parse().ok()).expect("a number")
    };
    let (mut checked, mut red_seen) = (0, 0usize);
    for seed in 1..=40 {
        let it = clogged_o(seed, "/Bleed 1");
        let b = boundary(&it);
        // a boundary through the counter: its upper part is past the blend
        if !(150.0..=300.0).contains(&b) {
            continue;
        }
        checked += 1;
        // rows past the blend (60 + 20 units each side) and above the glyph's top
        let (y0, y1) = (row_of(600), row_of(b as i32 + 120));
        let (w, px) = (width_px(), rgb(&it));
        // main ink is dark and neutral; the other colour is red
        let dark = px[y0 * w..y1 * w]
            .iter()
            .filter(|&&(r, g, bl)| r.max(g).max(bl) < 110 && r.abs_diff(g) < 40)
            .count();
        assert_eq!(dark, 0, "seed {seed}: main ink past the boundary ({b})");
        red_seen += px[y0 * w..y1 * w]
            .iter()
            .filter(|&&(r, g, bl)| r > g.saturating_add(50) && r > bl.saturating_add(50))
            .count();
    }
    assert!(checked >= 3, "only {checked} strikes crossed the counter");
    assert!(red_seen > 200, "the counter showed no red ({red_seen})");
}

/// And the worn lane lightens a clogged counter (#539): the counter holds
/// nothing but the clog, so a patch in its middle loses ink.
#[test]
fn the_worn_lane_lightens_a_clogged_counter() {
    let patch = |opts: &str| -> u64 {
        let it = clogged_o(3, opts);
        let (w, img) = (width_px(), ink(&it));
        (row_of(300)..row_of(150))
            .flat_map(|y| img[y * w + 185..y * w + 201].iter().map(|&v| u64::from(v)))
            .sum()
    };
    let (plain, worn) = (patch("/Bleed 0"), patch("/Bleed 0 /WornBand 1"));
    eprintln!("counter patch: lane intact {plain}, worn {worn}");
    assert!(plain > 0, "no clog in the patch");
    assert!(worn * 20 < plain * 19, "the worn lane left the clog alone");
}

/// The mis-strike keys (#554): the defaults are the measured 1% at 4x/2x, to
/// the byte, whether omitted or given; a rate of 0 or 1 is never or always; the
/// size matters only where a letter is mis-struck.
#[test]
fn mis_strike_keys_default_to_the_measured_board() {
    let page = |opts: &str| {
        ink(&render(&format!(
            "36 84 moveto << /Machine 3 /Seed 9 {opts} >> tw-setup ({PARAGRAPH}) show"
        )))
    };
    let plain = page("");
    assert_eq!(
        plain,
        page("/MisStrike 0.01 /MisStrikeScale 1"),
        "the defaults, given"
    );
    assert!(plain != page("/MisStrike 1"), "every strike mis-struck");
    assert!(plain != page("/MisStrike 0.2"), "a loose machine");
    // with none mis-struck the size has nothing to move
    assert_eq!(
        page("/MisStrike 0"),
        page("/MisStrike 0 /MisStrikeScale 5"),
        "size with no mis-strikes"
    );
    // and a larger jump is a different page where there are some
    assert!(
        page("/MisStrike 1 /MisStrikeScale 1") != page("/MisStrike 1 /MisStrikeScale 2"),
        "size with mis-strikes"
    );
    // a mis-strike still scales with /Wear: a perfect machine has none
    assert_eq!(
        page("/Wear 0"),
        page("/Wear 0 /MisStrike 1 /MisStrikeScale 3"),
        "no wear, no mis-strike"
    );
    // a preset does not reset them: tw-set carries them across it
    let held = ink(&render(&format!(
        "36 84 moveto << /MisStrike 0.2 /Machine 3 /Seed 9 >> tw-setup \
         << /Preset /manual >> tw-set ({PARAGRAPH}) show"
    )));
    assert_eq!(held, page("/MisStrike 0.2"), "a preset reset the rate");
}

#[test]
fn a_strip_types_within_the_cpu_budget() {
    let src = format!(
        "36 84 moveto << /Preset /worn /Machine 2 >> tw-setup ({PARAGRAPH} {PARAGRAPH}) show \
         36 72 moveto ({PARAGRAPH} {PARAGRAPH}) show 36 60 moveto ({PARAGRAPH} {PARAGRAPH}) show"
    );
    within_cpu_budget("typewriter strip", Duration::from_secs(2), 3, || {
        render(&src);
    });
}

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Renders `body` in Ghostscript at this file's page and scale; darkness
/// per pixel.
fn gs_ink(tag: &str, body: &str) -> Vec<u8> {
    let mut src = String::new();
    for path in LIBS {
        src.push_str(&std::fs::read_to_string(path).expect("read lib"));
        src.push('\n');
    }
    src.push_str(&format!("1 setgray clippath fill {body}\nshowpage\n"));
    let dir = common::scratch_dir(&format!("pscat-typewriter-{tag}"));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("tw.ps");
    let png = dir.join("tw.png");
    std::fs::write(&file, src).expect("write");
    let px = width_px();
    let status = std::process::Command::new("gs")
        .args(["-dNOPAUSE", "-dBATCH", "-q", "-sDEVICE=png16m"])
        .arg(format!("-g{px}x{}", (H as f32 * SCALE).round() as usize))
        .arg("-r300")
        .arg(format!("-o{}", png.display()))
        .arg(&file)
        .stdin(std::process::Stdio::null())
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected the typewriter driver");
    let img = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).expect("png")).expect("decode");
    let _ = std::fs::remove_dir_all(&dir);
    img.pixels()
        .iter()
        .map(|p| {
            255 - ((u32::from(p.red()) + u32::from(p.green()) + u32::from(p.blue())) / 3) as u8
        })
        .collect()
}

/// Ghostscript: no strike is a cached replay of another (the reason the
/// font uses setcharwidth), a machine's quirks hold, and the page is the
/// one pscat types, cell for cell, with another seed as the negative
/// control. Local only: CI has no gs.
#[test]
fn ghostscript_types_the_same_page_and_caches_no_glyph() {
    if !gs_available() {
        eprintln!("skipping gs typewriter check: gs not installed");
        return;
    }
    let text = "e e e e e e e";
    let driver = |opts: &str| {
        format!(
            "36 84 moveto << {opts} >> tw-setup ({text}) show \
             36 72 moveto ({text}) show 36 60 moveto ({text}) show"
        )
    };
    let cells = |img: &[u8]| -> Vec<Vec<u8>> {
        (0..3)
            .flat_map(|line| (0..7).map(move |k| (line, 2 * k)))
            .map(|(line, col)| cell(img, line, col))
            .collect()
    };
    let gs_full = gs_ink("full", &driver("/Machine 2 /Seed 5 /Wear 2"));
    let strikes = cells(&gs_full);
    for i in 0..strikes.len() {
        for j in i + 1..strikes.len() {
            assert!(
                !nearly_same(&strikes[i], &strikes[j]),
                "gs replayed strike {i} as strike {j}"
            );
        }
    }
    let quirks = cells(&gs_ink(
        "quirks",
        &driver("/Machine 2 /Seed 5 /LineWear 0 /StrikeWear 0"),
    ));
    assert!(
        quirks.iter().all(|c| nearly_same(c, &quirks[0])),
        "gs moved a machine quirk"
    );

    // The same draws as pscat: compare cell by cell, by ink mass and
    // centroid, which do not depend on the two rasterizers' edges.
    let summary = |img: &[u8]| -> Vec<(f64, f64, f64)> {
        cells(img)
            .iter()
            .map(|c| {
                let (mut m, mut sx, mut sy) = (0.0, 0.0, 0.0);
                for (i, &p) in c.iter().enumerate() {
                    let p = f64::from(p);
                    m += p;
                    sx += p * (i % (2 * CELL)) as f64;
                    sy += p * (i / (2 * CELL)) as f64;
                }
                (m, sx / m, sy / m)
            })
            .collect()
    };
    let centroid_gap = |a: &[(f64, f64, f64)], b: &[(f64, f64, f64)]| {
        a.iter()
            .zip(b)
            .map(|(p, q)| ((p.1 - q.1).powi(2) + (p.2 - q.2).powi(2)).sqrt())
            .sum::<f64>()
            / a.len() as f64
    };
    let gs = summary(&gs_full);
    let ours = summary(&three_lines("/Machine 2 /Seed 5 /Wear 2", text));
    let other = summary(&three_lines("/Machine 2 /Seed 6 /Wear 2", text));
    let same = centroid_gap(&gs, &ours);
    let differ = centroid_gap(&gs, &other);
    eprintln!("gs vs pscat: mean centroid gap {same:.3} px; another seed {differ:.3} px");
    assert!(
        same < 0.25,
        "gs and pscat place the strikes differently: {same:.3} px"
    );
    assert!(
        differ > 2.0 * same,
        "another seed is as close ({differ:.3}): the check is too weak"
    );
}
