//! `--svg-text` (#1119): text set in the standard fonts as SVG `<text>`
//! instead of glyph outlines. Off by default, and then the output is the
//! outline SVG it always was; on, each run of glyphs on a baseline is one
//! `<text>` with every glyph at pscat's own x. The raster and PDF never
//! change. Browser checks of the look and of selection are in
//! `notes/1119-svg-text.md`.

use pscat::{Interp, SvgText};

mod common;
use common::CommandDeadline as _;

fn interp(mode: Option<SvgText>) -> Interp {
    let mut it = Interp::with_page(300, 200).expect("test page");
    if let Some(m) = mode {
        it.set_svg_text(m);
    }
    it.enable_svg();
    it
}

fn svg_with(mode: Option<SvgText>, src: &str) -> Vec<String> {
    let mut it = interp(mode);
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it.svg_pages().expect("recording enabled")
}

fn text_svg(src: &str) -> String {
    let pages = svg_with(Some(SvgText::Text), src);
    assert_eq!(pages.len(), 1, "{pages:?}");
    pages.into_iter().next().unwrap_or_default()
}

/// The `<text ...>...</text>` elements of a page, in order.
fn texts(svg: &str) -> Vec<&str> {
    svg.match_indices("<text ")
        .map(|(i, _)| {
            let end = svg[i..].find("</text>").expect("closed text") + i + "</text>".len();
            &svg[i..end]
        })
        .collect()
}

fn content(text: &str) -> &str {
    let open = text.find('>').expect("open tag") + 1;
    &text[open..text.len() - "</text>".len()]
}

fn attr<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let i = text.find(&key)? + key.len();
    Some(&text[i..i + text[i..].find('"')?])
}

const HELLO: &str = "/Helvetica findfont 20 scalefont setfont 10 50 moveto (Hello world) show";

#[test]
fn the_default_is_outlines_and_unchanged() {
    let default = svg_with(None, HELLO);
    let paths = svg_with(Some(SvgText::Paths), HELLO);
    assert_eq!(default, paths);
    assert!(!default[0].contains("<text"), "{}", default[0]);
    assert!(default[0].contains("<path "), "{}", default[0]);
}

#[test]
fn a_show_becomes_one_text_with_a_position_per_glyph() {
    let svg = text_svg(HELLO);
    assert!(!svg.contains("<path "), "no outlines left: {svg}");
    let t = texts(&svg);
    assert_eq!(t.len(), 1, "{svg}");
    // The space is kept as a character, so copied text keeps its words.
    assert_eq!(content(t[0]), "Hello world");
    assert_eq!(attr(t[0], "xml:space"), Some("preserve"));
    let xs: Vec<f64> = attr(t[0], "x")
        .expect("x list")
        .split(' ')
        .map(|v| v.parse().expect("number"))
        .collect();
    assert_eq!(xs.len(), "Hello world".len());
    assert_eq!(xs[0], 10.0);
    assert!(xs.windows(2).all(|w| w[1] > w[0]), "{xs:?}");
    // Device y is down: the 50 pt baseline on a 200 pt page is at y 150.
    assert_eq!(attr(t[0], "y"), Some("150"));
    assert_eq!(attr(t[0], "font-size"), Some("20"));
    assert!(attr(t[0], "font-family").is_some_and(|f| f.starts_with("Helvetica,Arial")));
    assert_eq!(attr(t[0], "fill"), Some("#000000"));
    assert!(
        !t[0].contains("transform="),
        "upright text needs no transform"
    );
}

#[test]
fn glyph_positions_are_pscats_advances() {
    // Courier is Liberation Mono: every advance is 1229/2048 em, so 12 pt
    // glyphs sit 7.201 apart (printed to three decimals).
    let svg = text_svg("/Courier findfont 12 scalefont setfont 0 20 moveto (abcd) show");
    let t = texts(&svg);
    assert_eq!(attr(t[0], "x"), Some("0 7.201 14.402 21.604"), "{svg}");
}

#[test]
fn styles_map_to_weight_and_style() {
    let svg = text_svg(
        "/Times-BoldItalic findfont 10 scalefont setfont 10 20 moveto (a) show \
         /Courier-Oblique findfont 10 scalefont setfont 10 40 moveto (b) show \
         /Helvetica-Bold findfont 10 scalefont setfont 10 60 moveto (c) show",
    );
    let t = texts(&svg);
    assert_eq!(t.len(), 3, "{svg}");
    assert!(
        attr(t[0], "font-family").is_some_and(|f| f.ends_with("serif") && f.starts_with("Times"))
    );
    assert_eq!(attr(t[0], "font-weight"), Some("bold"));
    assert_eq!(attr(t[0], "font-style"), Some("italic"));
    assert!(attr(t[1], "font-family").is_some_and(|f| f.ends_with("monospace")));
    assert_eq!(attr(t[1], "font-weight"), None);
    assert_eq!(attr(t[1], "font-style"), Some("italic"));
    assert_eq!(attr(t[2], "font-weight"), Some("bold"));
    assert_eq!(attr(t[2], "font-style"), None);
}

#[test]
fn markup_characters_are_escaped() {
    let svg = text_svg("/Helvetica findfont 10 scalefont setfont 10 20 moveto (a<b & c>d) show");
    assert_eq!(content(texts(&svg)[0]), "a&lt;b &amp; c&gt;d");
}

#[test]
fn a_reencoded_font_gives_the_encoded_characters() {
    let svg = text_svg(
        "/Helvetica findfont dup length dict begin { 1 index /FID ne { def } { pop pop } ifelse } forall \
         /Encoding ISOLatin1Encoding def currentdict end /H1 exch definefont pop \
         /H1 findfont 10 scalefont setfont 10 20 moveto (caf\\351) show",
    );
    assert_eq!(content(texts(&svg)[0]), "café");
}

#[test]
fn shows_on_one_baseline_share_a_run_and_a_new_line_starts_another() {
    let svg = text_svg(
        "/Helvetica findfont 10 scalefont setfont \
         10 100 moveto (one ) show (two) show \
         10 80 moveto (three) show",
    );
    let t = texts(&svg);
    assert_eq!(t.len(), 2, "{svg}");
    assert_eq!(content(t[0]), "one two");
    assert_eq!(content(t[1]), "three");
}

#[test]
fn a_change_of_colour_size_or_face_starts_a_new_run() {
    let svg = text_svg(
        "/Helvetica findfont 10 scalefont setfont 10 100 moveto (a) show \
         1 0 0 setrgbcolor (b) show \
         /Helvetica findfont 12 scalefont setfont (c) show \
         /Times-Roman findfont 12 scalefont setfont (d) show",
    );
    let t = texts(&svg);
    assert_eq!(
        t.iter().map(|t| content(t)).collect::<Vec<_>>(),
        ["a", "b", "c", "d"]
    );
    assert_eq!(attr(t[1], "fill"), Some("#ff0000"));
}

#[test]
fn rotated_text_carries_a_unit_frame_and_lands_where_the_outline_did() {
    let svg = text_svg(
        "/Helvetica findfont 10 scalefont setfont 100 100 translate 90 rotate 0 0 moveto (ab) show",
    );
    let t = texts(&svg);
    assert_eq!(t.len(), 1, "{svg}");
    let m: Vec<f64> = attr(t[0], "transform")
        .and_then(|v| v.strip_prefix("matrix(")?.strip_suffix(')'))
        .expect("a transform")
        .split(' ')
        .map(|v| v.parse().expect("number"))
        .collect();
    assert_eq!(attr(t[0], "font-size"), Some("10"));
    let x0: f64 = attr(t[0], "x")
        .and_then(|x| x.split(' ').next())
        .expect("x")
        .parse()
        .expect("number");
    let y: f64 = attr(t[0], "y").expect("y").parse().expect("number");
    // The frame maps the first glyph's local origin back to the device pen:
    // (100, 100) user, (100, 100) device on a 200 pt page.
    let (dx, dy) = (m[0] * x0 + m[2] * y + m[4], m[1] * x0 + m[3] * y + m[5]);
    assert!(
        (dx - 100.0).abs() < 1e-3 && (dy - 100.0).abs() < 1e-3,
        "{dx} {dy} {svg}"
    );
}

#[test]
fn other_fonts_and_other_modes_keep_outlines() {
    for (what, src) in [
        (
            "Symbol",
            "/Symbol findfont 10 scalefont setfont 10 20 moveto (abc) show",
        ),
        (
            "a standard-35 face without a common metric match",
            "/Palatino-Roman findfont 10 scalefont setfont 10 20 moveto (abc) show",
        ),
        (
            "charpath",
            "/Helvetica findfont 10 scalefont setfont 10 20 moveto (abc) true charpath fill",
        ),
        (
            "a Type 3 font",
            "/T3 << /FontType 3 /FontMatrix [0.001 0 0 0.001 0 0] /FontBBox [0 0 1000 1000] \
             /Encoding StandardEncoding /BuildChar { pop 1000 0 0 0 1000 1000 setcachedevice \
             0 0 1000 1000 rectfill } >> definefont 10 scalefont setfont 10 20 moveto (a) show",
        ),
    ] {
        let svg = text_svg(src);
        assert!(!svg.contains("<text"), "{what}: {svg}");
        assert!(svg.contains("<path "), "{what}: {svg}");
    }
}

#[test]
fn text_keeps_paint_order_with_other_marks() {
    let svg = text_svg(
        "0 0 1 setrgbcolor 0 0 50 50 rectfill \
         0 setgray /Helvetica findfont 10 scalefont setfont 10 20 moveto (on top) show \
         1 0 0 setrgbcolor 0 0 20 20 rectfill",
    );
    let blue = svg.find("#0000ff").expect("first fill");
    let text = svg.find("<text ").expect("text");
    let red = svg.find("#ff0000").expect("last fill");
    assert!(blue < text && text < red, "{svg}");
}

#[test]
fn a_page_ending_in_text_has_it_without_a_showpage() {
    let mut it = interp(Some(SvgText::Text));
    it.run_str(HELLO).expect("runs");
    let pages = it.svg_pages().expect("recording");
    assert_eq!(pages.len(), 1);
    assert_eq!(content(texts(&pages[0])[0]), "Hello world");
    // Reading the page did not close the run: more text joins it.
    it.run_str("( again) show showpage").expect("runs");
    let pages = it.svg_pages().expect("recording");
    let t = texts(&pages[0]);
    assert_eq!(t.len(), 1, "{}", pages[0]);
    assert_eq!(content(t[0]), "Hello world again");
}

#[test]
fn text_is_clipped_and_composited_like_an_outline() {
    let svg = text_svg(
        "0 0 100 100 rectclip 0.5 setalpha /Helvetica findfont 10 scalefont setfont \
         10 20 moveto (a) show",
    );
    let t = texts(&svg);
    assert_eq!(attr(t[0], "fill-opacity"), Some("0.5"), "{svg}");
    let before = &svg[..svg.find("<text ").expect("text")];
    assert!(before.ends_with("<g clip-path=\"url(#c0)\">"), "{svg}");
}

#[test]
fn pages_split_the_same_way_and_blank_text_opens_no_page() {
    let src = "/Helvetica findfont 10 scalefont setfont 10 20 moveto (one) show showpage \
               10 20 moveto (   ) show";
    let text = svg_with(Some(SvgText::Text), src);
    let paths = svg_with(Some(SvgText::Paths), src);
    assert_eq!(text.len(), paths.len());
    assert_eq!(text.len(), 1);
}

#[test]
fn the_raster_and_the_pdf_do_not_change() {
    let src = "/Times-Roman findfont 18 scalefont setfont 20 100 moveto (Raster stays) show \
               30 rotate (and so does PDF) show showpage";
    let run = |mode| {
        let mut it = Interp::with_page(300, 200).expect("page");
        it.set_svg_text(mode);
        it.enable_svg();
        it.enable_pdf();
        it.run_str(src).expect("runs");
        let pixels: Vec<Vec<u8>> = it.pages().iter().map(pscat::Page::rgba8).collect();
        (pixels, it.pdf_document().expect("pdf"))
    };
    let (px_paths, pdf_paths) = run(SvgText::Paths);
    let (px_text, pdf_text) = run(SvgText::Text);
    assert!(px_paths == px_text, "the raster changed");
    assert!(pdf_paths == pdf_text, "the PDF changed");
}

#[test]
fn the_cli_flag_writes_text_and_needs_svg() {
    let dir = common::scratch_dir("pscat-svg-text-cli");
    let ps = dir.join("hello.ps");
    std::fs::write(&ps, HELLO).expect("write program");
    let out = dir.join("hello.svg");
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["--page", "300x200", "--svg-text", "--svg"])
        .arg(&out)
        .arg(&ps)
        .output_deadline()
        .expect("run pscat");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let svg = std::fs::read_to_string(&out).expect("svg written");
    assert_eq!(content(texts(&svg)[0]), "Hello world");

    let alone = std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["--svg-text", "--png"])
        .arg(dir.join("hello.png"))
        .arg(&ps)
        .output_deadline()
        .expect("run pscat");
    assert!(!alone.status.success());
    assert!(
        String::from_utf8_lossy(&alone.stderr).contains("--svg-text needs --svg"),
        "{}",
        String::from_utf8_lossy(&alone.stderr)
    );
}

#[test]
fn a_glyph_that_falls_back_mid_show_splits_the_run_in_paint_order() {
    // Code 128 is a combining acute (U+0301): it has an outline in the face
    // but would attach to the previous character as text, so it stays a path
    // between two text runs.
    let svg = text_svg(
        "/Helvetica findfont dup length dict begin { 1 index /FID ne { def } { pop pop } ifelse } forall \
         /Encoding StandardEncoding 256 array copy dup 128 /uni0301 put def currentdict end \
         /HA exch definefont pop /HA findfont 20 scalefont setfont 10 50 moveto (a\\200b) show",
    );
    let t = texts(&svg);
    assert_eq!(
        t.iter().map(|t| content(t)).collect::<Vec<_>>(),
        ["a", "b"],
        "{svg}"
    );
    let first = svg.find("<text ").expect("first run");
    let path = svg.find("<path ").expect("the accent's outline");
    let second = svg.rfind("<text ").expect("second run");
    assert!(first < path && path < second, "{svg}");
}
