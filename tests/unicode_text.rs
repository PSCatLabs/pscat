//! UTF-8 text, the fallback chain and colour emoji (issue #1425):
//! `/PSCatUnicode true` on an outline font makes `show`, `stringwidth` and
//! `charpath` read UTF-8, fall back across the compiled-in faces and draw
//! emoji from the bundled Twemoji face. `docs/dev/FONTS.md`, "Unicode
//! text, fallback faces and colour emoji".

mod common;

use common::CommandDeadline as _;
use pscat::{Interp, Limits, Value};

/// Define `/U` as Helvetica with `/PSCatUnicode true`, `size` points, set.
fn ufont(size: u32) -> String {
    format!(
        "/Helvetica findfont dup length dict copy dup /PSCatUnicode true put \
         /Helvetica-U exch definefont {size} scalefont setfont "
    )
}

fn run_on(w: u32, h: u32, src: &str) -> Interp {
    let mut it = Interp::with_page(w, h).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run of {src:?} failed: {e}"));
    it
}

fn run(src: &str) -> Interp {
    run_on(200, 120, src)
}

fn num(it: &mut Interp) -> f64 {
    match it.pop().expect("operand").value {
        Value::Integer(i) => i as f64,
        Value::Real(r) => r,
        ref v => panic!("expected a number, got {:?}", pscat::Object::lit(v.clone())),
    }
}

fn width(src_font: &str, text: &str) -> f64 {
    let mut it = run(&format!("{src_font} ({text}) stringwidth pop"));
    num(&mut it)
}

/// Pixels whose colour is "emoji yellow" (Twemoji's face is #FFCC4D).
fn yellow(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() > 200 && p.green() > 150 && p.green() < 230 && p.blue() < 120)
        .count()
}

/// Strongly red pixels (the red heart is #DD2E44).
fn red(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() > 180 && p.green() < 90 && p.blue() < 110)
        .count()
}

fn dark(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 100 && p.green() < 100 && p.blue() < 100)
        .count()
}

#[test]
fn utf8_accents_greek_and_cyrillic_draw_without_warnings() {
    let it = run(&format!(
        "{} 5 40 moveto (Café Zoë Ωμέγα Жук) show",
        ufont(16)
    ));
    assert!(dark(&it) > 200, "expected text ink");
    assert_eq!(it.font_warnings(), "", "every character has a face");
}

#[test]
fn utf8_width_matches_the_latin1_byte_mode_width() {
    let latin1 = "/Helvetica findfont dup length dict copy dup /Encoding \
                  ISOLatin1Encoding put /H1 exch definefont 20 scalefont setfont";
    let byte_mode = width(latin1, "caf\\351");
    let utf8 = width(&ufont(20), "café");
    assert!((byte_mode - utf8).abs() < 1e-9, "{byte_mode} vs {utf8}");
}

#[test]
fn byte_mode_fonts_still_show_bytes() {
    // Without the key, U+00E9's two UTF-8 bytes are two StandardEncoding
    // glyphs (circumflex 333, quotesingle 191), as in Ghostscript.
    let w = width("/Helvetica findfont 100 scalefont setfont", "é");
    assert!((w - 52.4).abs() < 0.1, "{w}");
}

#[test]
fn the_key_works_on_a_scaled_font_too() {
    let w = width(
        "/Helvetica findfont 20 scalefont dup /PSCatUnicode true put setfont",
        "é",
    );
    let e = width("/Helvetica findfont 20 scalefont setfont", "e");
    assert!((w - e).abs() < 1e-9, "é {w} should be e's width {e}");
}

#[test]
fn emoji_draw_in_colour_and_follow_the_text_colour_back() {
    let mut it = run(&format!(
        "{} 1 0 0 setrgbcolor 20 30 moveto (\u{1F600}) show currentrgbcolor",
        ufont(60)
    ));
    let b = num(&mut it);
    let g = num(&mut it);
    let r = num(&mut it);
    assert_eq!((r, g, b), (1.0, 0.0, 0.0), "the current colour is put back");
    assert!(
        yellow(&it) > 500,
        "expected a yellow face, got {}",
        yellow(&it)
    );
}

#[test]
fn emoji_leave_a_cmyk_colour_as_it_was() {
    let mut it = run(&format!(
        "{} 0 1 1 0 setcmykcolor 20 30 moveto (\u{1F600}) show currentcmykcolor",
        ufont(40)
    ));
    let k = num(&mut it);
    let y = num(&mut it);
    let m = num(&mut it);
    let c = num(&mut it);
    assert_eq!((c, m, y, k), (0.0, 1.0, 1.0, 0.0));
}

#[test]
fn heart_is_text_unless_variation_selector_16_asks_for_emoji() {
    // U+2764 has Emoji_Presentation=No: plain, it is the black dingbat
    // heart; with U+FE0F it is the red emoji.
    let text = run(&format!("{} 20 30 moveto (\u{2764}) show", ufont(60)));
    assert!(
        dark(&text) > 300 && red(&text) == 0,
        "a black dingbat heart"
    );
    let emoji = run(&format!(
        "{} 20 30 moveto (\u{2764}\u{FE0F}) show",
        ufont(60)
    ));
    assert!(red(&emoji) > 300, "a red emoji heart");
    assert_eq!(emoji.font_warnings(), "");
}

#[test]
fn sequences_join_into_one_glyph() {
    let em = width(&ufont(100), "\u{1F600}");
    assert!((em - 100.0).abs() < 1e-6, "an emoji is one em: {em}");
    for seq in [
        "\u{1F1FA}\u{1F1F8}",                          // flag
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}", // family (ZWJ)
        "\u{1F44D}\u{1F3FD}",                          // skin tone
        "#\u{FE0F}\u{20E3}",                           // keycap
    ] {
        let w = width(&ufont(100), seq);
        assert!((w - em).abs() < 1e-6, "{seq:?} should be one glyph, is {w}");
    }
}

#[test]
fn show_advance_matches_stringwidth() {
    let s = "Hi \u{1F44B}\u{1F3FD} caf\u{e9} \u{1F1EF}\u{1F1F5}\u{2764}\u{FE0F} \u{3b1}\u{2192}\u{2713}";
    let mut it = run(&format!(
        "{} 0 0 moveto ({s}) show currentpoint pop ({s}) stringwidth pop",
        ufont(12)
    ));
    let sw = num(&mut it);
    let shown = num(&mut it);
    assert!(
        (sw - shown).abs() < 1e-4,
        "stringwidth {sw} vs show {shown}"
    );
}

#[test]
fn charpath_takes_the_emoji_outline() {
    // Twemoji draws from -1/8 em to 7/8 em, one em wide.
    let mut it = run(&format!(
        "{} newpath 0 0 moveto (\u{1F600}) false charpath pathbbox",
        ufont(80)
    ));
    let ury = num(&mut it);
    let urx = num(&mut it);
    let lly = num(&mut it);
    let llx = num(&mut it);
    assert!(llx > -1.0 && llx < 10.0, "llx {llx}");
    assert!(urx > 70.0 && urx < 81.0, "urx {urx}");
    assert!(lly > -11.0 && lly < 0.0, "lly {lly}");
    assert!(ury > 60.0 && ury < 71.0, "ury {ury}");
}

#[test]
fn ignorables_and_controls_draw_nothing_and_warn_nothing() {
    let font = ufont(20);
    let ab = width(&font, "ab");
    assert!(
        (width(&font, "a\u{200D}b") - ab).abs() < 1e-9,
        "ZWJ is zero width"
    );
    assert!(
        (width(&font, "a\u{FE0F}b") - ab).abs() < 1e-9,
        "VS16 is zero width"
    );
    // A control takes the missing-glyph advance, as byte mode gives it.
    let byte_mode = width("/Helvetica findfont 20 scalefont setfont", "a\\nb");
    let unicode = width(&font, "a\\nb");
    assert!(
        (byte_mode - unicode).abs() < 1e-9,
        "{byte_mode} vs {unicode}"
    );
    let it = run(&format!("{font} 5 40 moveto (a\\n\u{200D}\\tb) show"));
    assert_eq!(it.font_warnings(), "");
}

#[test]
fn a_character_nothing_has_draws_notdef_and_warns_once() {
    let it = run(&format!(
        "{} 5 40 moveto (\u{AC00}) show (\u{AC00}) stringwidth pop pop",
        ufont(30)
    ));
    let w = it.font_warnings();
    assert_eq!(w.lines().count(), 1, "{w}");
    assert!(w.contains("U+AC00") && w.contains("NotoSansKR"), "{w}");
    assert!(dark(&it) > 20, "the .notdef box is drawn");
}

#[test]
fn missing_glyph_warnings_are_capped() {
    let many: String = (0xAC00u32..0xAC00 + 40)
        .filter_map(char::from_u32)
        .collect();
    let mut it = Interp::with_page(200, 120).expect("test page");
    it.quiet_stderr();
    it.run_str(&format!("{} ({many}) stringwidth pop pop", ufont(10)))
        .expect("run");
    let w = it.font_warnings();
    assert_eq!(w.lines().filter(|l| l.contains("no glyph for")).count(), 32);
    assert_eq!(w.lines().filter(|l| l.contains("not reported")).count(), 1);
}

#[test]
fn cshow_kshow_and_xshow_see_one_step_per_scalar() {
    // A flag is two scalars and one glyph: cshow runs twice (the second
    // with no width), kshow once between them, xshow needs two numbers.
    let flag = "\u{1F1FA}\u{1F1F8}";
    let mut it = run(&format!(
        "{} /n 0 def {{ pop pop pop /n n 1 add def }} ({flag}) cshow n",
        ufont(10)
    ));
    assert_eq!(num(&mut it), 2.0);
    let mut it = run(&format!(
        "{} /n 0 def 0 0 moveto {{ pop pop /n n 1 add def }} ({flag}) kshow n",
        ufont(10)
    ));
    assert_eq!(num(&mut it), 1.0);
    let it = run(&format!("{} 0 0 moveto ({flag}) [10 10] xshow", ufont(10)));
    assert_eq!(it.font_warnings(), "");
    let mut short = Interp::with_page(50, 50).expect("page");
    let err = short
        .run_str(&format!("{} 0 0 moveto ({flag}) [10] xshow", ufont(10)))
        .expect_err("one number for two scalars");
    assert!(err.to_string().contains("rangecheck"), "{err}");
}

#[test]
fn emoji_work_is_charged_to_the_step_limit() {
    // The work is held against --max-steps at a step per KiB, like a text
    // glyph's raster work (#1332): the colour layers' rasterising and the
    // ligature search (a base like U+1F468 starts 300-odd ligatures).
    let steps = |size: u32, text: &str| {
        let mut it = Interp::with_page(1600, 1600).expect("page");
        it.set_limits(Limits::none().with_max_steps(1 << 40))
            .expect("limits");
        it.run_str(&format!("{} 10 300 moveto ({text}) show", ufont(size)))
            .expect("show");
        it.steps_used()
    };
    let big_space = steps(1500, " ");
    let big_emoji = steps(1500, "\u{1F600}");
    assert!(big_emoji > big_space, "layers: {big_emoji} vs {big_space}");
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    let five_spaces = steps(4, "     ");
    let joined = steps(4, family);
    assert!(
        joined > five_spaces,
        "ligature search: {joined} vs {five_spaces}"
    );
}

#[test]
fn svg_and_pdf_get_the_emoji_as_coloured_paths() {
    let dir = common::scratch_dir("unicode-emoji");
    let ps = dir.join("e.ps");
    std::fs::write(
        &ps,
        format!(
            "%!PS\n{} 10 20 moveto (A\u{1F600}) show showpage\n",
            ufont(40)
        ),
    )
    .expect("write");
    let svg = dir.join("e.svg");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .arg("--svg")
        .arg(&svg)
        .arg(&ps)
        .output_deadline()
        .expect("pscat --svg");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let svg = std::fs::read_to_string(&svg)
        .expect("svg")
        .to_ascii_lowercase();
    assert!(
        svg.contains("#ffcc4d") || svg.contains("rgb(255,204,77)"),
        "the face's yellow is a fill in the SVG"
    );
    let pdf = dir.join("e.pdf");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .arg("--pdf")
        .arg(&pdf)
        .arg(&ps)
        .output_deadline()
        .expect("pscat --pdf");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(std::fs::metadata(&pdf).expect("pdf").len() > 1000);
}

#[test]
fn ashow_spacing_skips_the_invisible_parts_of_a_joined_emoji() {
    // Letter-spaced credits: the extra goes after each visible glyph, not
    // after the ZWJs and code points a family emoji took in.
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    let mut it = run(&format!(
        "{} 0 0 moveto 5 0 ({family}A\u{200D}B) ashow currentpoint pop \
         ({family}AB) stringwidth pop",
        ufont(20)
    ));
    let plain = num(&mut it);
    let spaced = num(&mut it);
    assert!(
        (spaced - (plain + 3.0 * 5.0)).abs() < 1e-6,
        "{spaced} vs {plain} + 15"
    );
}
