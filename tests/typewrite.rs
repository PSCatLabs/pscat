//! The typewriter's layout (issue #213): `tw-type`, `tw-linecount`,
//! `tw-pageheight` and `pscat typewrite` (the subcommand `scripts/typewrite.sh`
//! now wraps; the wrapper itself is smoke-tested in `tests/subcommands.rs`).
//!
//! The option guards and a few line counts are `%%SelfTest` blocks in
//! `lib/typewriter.ps`. These tests need pixels or a process: wrapping,
//! tab stops, markup, spacing, pagination, determinism, `tw-linecount`
//! agreeing with `tw-type`, and the subcommand, with a real `gs` run of its
//! driver when `gs` is installed.
//!
//! `tw-type` types each glyph at a cell of the grid its frame sets, and
//! every random draw is keyed by that cell. So a page it types must equal
//! the same characters typed by hand with `moveto` and `show` at those cells
//! in the same frame: that is how the placement tests compare.

use pscat::Interp;

mod common;

const LIBS: [&str; 2] = ["lib/typewriter_outlines.ps", "lib/typewriter.ps"];

/// 300 dpi: a pica cell (7.2pt) is 30 px and a 12pt line 50 px.
const SCALE: f32 = 300.0 / 72.0;
const W: u32 = 288;
const H: u32 = 120;
/// With a 26.625 top margin the first baseline (0.75 x 12.5 = 9.375 below
/// it) is at y = 84, and a 36 left margin puts column 0 at x = 36: the
/// frame the hand-typed comparisons use.
const OPTS: &str = "/Width 288 /Height 120 /MarginTop 26.625 /MarginLeft 36 /MarginRight 36 \
                    /MarginBottom 20 /Seed 5 /Machine 3";
const FRAME: &str = "36 84 moveto";

fn load(it: &mut Interp) {
    for path in LIBS {
        let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
}

fn render(src: &str) -> Interp {
    let mut it = Interp::with_page_scaled(W, H, SCALE).expect("page");
    load(&mut it);
    it.run_str(&format!("1 setgray clippath fill {src}"))
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    it
}

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

/// The page `tw-type` types for `opts` and `text`.
fn typed(opts: &str, text: &str) -> Vec<u8> {
    ink(&render(&format!(
        "<< /Text ({text}) {OPTS} {opts} >> tw-type"
    )))
}

/// The page typed by hand: `body` runs after `tw-setup` in the same frame.
fn by_hand(opts: &str, body: &str) -> Vec<u8> {
    ink(&render(&format!("{FRAME} << {opts} >> tw-setup {body}")))
}

/// Pixels that differ. The ribbon's clip has hundreds of edges per glyph, so
/// the same strike typed by `show` (advances added up) and by `moveto`
/// can round a pixel or two apart; a different strike moves hundreds.
fn differing(a: &[u8], b: &[u8]) -> usize {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

fn same_page(a: &[u8], b: &[u8], what: &str) {
    let d = differing(a, b);
    assert!(d <= 40, "{what}: {d} pixels differ");
}

fn linecount(opts: &str) -> i64 {
    let mut it = Interp::with_page(100, 100).expect("page");
    load(&mut it);
    it.run_str(&format!("<< {opts} >> tw-linecount"))
        .unwrap_or_else(|e| panic!("{opts}: {}", it.error_report(&e)));
    it.operand_stack()
        .last()
        .expect("a count")
        .repr()
        .parse()
        .expect("an integer")
}

#[test]
fn text_wraps_at_the_column_count_the_page_gives() {
    // 288 - 72 = 216pt: 30 pica columns, 36 elite ones.
    let words = "(aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj)";
    assert_eq!(
        linecount(&format!("/Text {words} /Width 288 /Margin 36")),
        2,
        "50 columns of text in 30"
    );
    assert_eq!(
        linecount(&format!("/Text {words} /Width 288 /Margin 36 /Pitch 12")),
        2,
        "elite fits 36"
    );
    assert_eq!(
        linecount(&format!("/Text {words} /Width 372 /Margin 36 /Pitch 12")),
        1,
        "elite in 300pt fits all 49 columns"
    );
    // a line holds exactly its column count, and one more wraps
    let thirty = "a".repeat(30);
    assert_eq!(
        linecount(&format!("/Text ({thirty}) /Width 288 /Margin 36")),
        1
    );
    assert_eq!(
        linecount(&format!("/Text ({thirty}a) /Width 288 /Margin 36")),
        2
    );
    // a word wider than a line is split, not left to run off the page
    let long = "b".repeat(95);
    assert_eq!(
        linecount(&format!("/Text ({long}) /Width 288 /Margin 36")),
        4
    );
    // an indent narrows the first line only
    assert_eq!(
        linecount(&format!("/Text ({thirty}) /Width 288 /Margin 36 /Indent 3")),
        2
    );
    // a hanging memo field wraps under its text
    let memo = "/Memo << /To (x) /Subject (one two three four five six seven eight nine ten eleven) >> /Text (t) /Width 288 /Margin 36";
    // title, blank, TO, SUBJECT (55 characters in the 20 columns right of the
    // label: 3 lines), rule, blank, t
    assert_eq!(linecount(memo), 9, "a memo's long subject hangs and wraps");
}

#[test]
fn a_typed_page_is_the_same_characters_typed_by_hand() {
    // Wrapped lines land on the grid: line 2 starts at column 0, 12pt down.
    let a = typed("", "aaaa bbbb cccc dddd eeee ffff gggg hhhh");
    let b = by_hand(
        "/Seed 5 /Machine 3",
        "36 84 moveto (aaaa bbbb cccc dddd eeee ffff) show \
         36 72 moveto (gggg hhhh) show",
    );
    same_page(&a, &b, "the second line");
    // the same text at spacing 2 sits 24pt down
    let c = typed("/Spacing 2", "one\ntwo");
    let d = by_hand(
        "/Seed 5 /Machine 3",
        "36 84 moveto (one) show 36 60 moveto (two) show",
    );
    same_page(&c, &d, "double spacing");
    let e = typed("/Spacing 1.5", "one\ntwo");
    let f = by_hand(
        "/Seed 5 /Machine 3",
        "36 84 moveto (one) show 36 66 moveto (two) show",
    );
    same_page(&e, &f, "one-and-a-half spacing");
}

#[test]
fn tab_stops_move_to_the_next_stop() {
    let by = |opts: &str, text: &str| typed(opts, text);
    // every 5 columns by default: a at column 0, the tab to column 5
    let hand = by_hand(
        "/Seed 5 /Machine 3",
        "36 84 moveto (a) show 72 84 moveto (b) show",
    );
    same_page(&by("", "a\\tb"), &hand, "the default stop");
    // a tab at column 5 goes to 10, not nowhere
    let hand = by_hand(
        "/Seed 5 /Machine 3",
        "36 84 moveto (abcde) show 108 84 moveto (f) show",
    );
    same_page(&by("", "abcde\\tf"), &hand, "from a stop to the next");
    // explicit stops, and past the last one a tab is one column
    let hand = by_hand(
        "/Seed 5 /Machine 3",
        "36 84 moveto (a) show 79.2 84 moveto (b) show 187.2 84 moveto (c) show",
    );
    same_page(
        &by("/Tabs [6 20]", "a\\tb\\t\\tc"),
        &hand,
        "explicit tab stops",
    );
    let hand = by_hand(
        "/Seed 5 /Machine 3",
        "36 84 moveto (a) show 79.2 84 moveto (b) show 93.6 84 moveto (c) show",
    );
    same_page(
        &by("/Tabs [6]", "a\\tb\\tc"),
        &hand,
        "a tab past the last stop is one column",
    );
}

#[test]
fn markup_strikes_over_and_underlines_with_its_own_strike_indices() {
    let plain = typed("", "ab");
    // <x> types x over each letter, at strike index 1
    let hand = by_hand(
        "/Seed 5 /Machine 3",
        "36 84 moveto (ab) show << /Strike 1 >> tw-set 36 84 moveto (xx) show",
    );
    same_page(&typed("", "<x>ab</x>"), &hand, "an x-out");
    // <u> backspaces and types underscores, at strike index 2
    let hand = by_hand(
        "/Seed 5 /Machine 3",
        "36 84 moveto (ab) show << /Strike 2 >> tw-set 36 84 moveto (__) show",
    );
    same_page(&typed("", "<u>ab</u>"), &hand, "an underline");
    // <o>old|new</o> types new over old
    let hand = by_hand(
        "/Seed 5 /Machine 3",
        "36 84 moveto (a) show << /Strike 1 >> tw-set 36 84 moveto (b) show",
    );
    same_page(&typed("", "<o>a|b</o>"), &hand, "an overtype");
    assert!(
        differing(&plain, &typed("", "<x>ab</x>")) > 200,
        "the cross-out left no ink"
    );
    // a second strike at one cell with the same index would repeat the first
    // exactly; the markup's own indices keep an x struck over an x unlike it
    let xx = typed("", "<x>x</x>");
    let once = typed("", "x");
    assert!(differing(&xx, &once) > 100, "x over x is one strike");
    // tags are not typed; another < is
    assert_eq!(
        differing(
            &typed("", "a<b"),
            &by_hand("/Seed 5 /Machine 3", "36 84 moveto (a<b) show")
        ),
        0,
        "a lone < is a literal"
    );
    // an underline carries across a space, and a break drops its tail
    let wide = typed("", "<u>ab cd</u>");
    let gap = typed("", "<u>ab</u> <u>cd</u>");
    assert!(
        differing(&wide, &gap) > 50,
        "the space between is underlined"
    );
}

#[test]
fn an_overtype_is_one_unbreakable_token() {
    // `<o>` with a space in it still wraps as one word, as wide as the
    // longer side: 25 columns of text then a 6-wide token cannot share a line
    let n = linecount(&format!(
        "/Text ({} <o>ab cd|xy z</o>) /Width 288 /Margin 36",
        "a".repeat(25)
    ));
    assert_eq!(n, 2, "the token moved to the second line whole");
    let n = linecount("/Text (<o>a|bcdef</o> <x>gh</x>) /Width 288 /Margin 36");
    assert_eq!(n, 1);
}

#[test]
fn the_same_options_type_the_same_page_and_the_callers_rand_is_untouched() {
    let text = "Dear Sir,\\n\\nI enclose the <u>specimens</u>.";
    let a = typed("", text);
    assert_eq!(a, typed("", text), "the same seeds typed two pages");
    assert_ne!(a, typed("/Seed 6", text), "a new /Seed changed nothing");
    assert_ne!(
        a,
        typed("/Machine 4", text),
        "a new /Machine changed nothing"
    );
    let draws = |lead: &str| {
        let mut it = Interp::with_page_scaled(W, H, SCALE).expect("page");
        load(&mut it);
        it.run_str(&format!(
            "7 srand {lead} rand 1000000 mod rand 1000000 mod rand 1000000 mod"
        ))
        .unwrap();
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>()
    };
    let typing = format!(
        "<< /Text (a longer memo\\nin two lines) {OPTS} >> tw-type << /Text (x) >> tw-linecount pop"
    );
    assert_eq!(
        draws(""),
        draws(&typing),
        "typing consumed the caller's rand"
    );
}

#[test]
fn tw_linecount_agrees_with_what_tw_type_types() {
    // After typing, the carriage is at the start of the line after the last
    // one: y = first baseline - lines x step, on one tall page.
    let texts = [
        "short",
        "aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll",
        "a\\n\\n\\nb\\n",
        "\\tindented word and a very long unbroken run xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "<o>ab cd|xy z</o> and <u>an underlined phrase that wraps around</u> end",
    ];
    for (i, text) in texts.iter().enumerate() {
        for extra in [
            "",
            "/Spacing 2 /Indent 4",
            "/Pitch 12 /Tabs [3 9]",
            "/Memo << /To (me) /Subject (a subject that is long enough to hang and wrap onto a line) >>",
        ] {
            let opts = format!("/Width 288 /Height 4000 /Margin 36 {extra}");
            let mut it = Interp::with_page(100, 100).expect("page");
            load(&mut it);
            it.run_str(&format!(
                "<< /Text ({text}) {opts} >> tw-linecount \
                 << /Text ({text}) {opts} >> tw-type currentpoint exch pop"
            ))
            .unwrap_or_else(|e| panic!("{text}: {}", it.error_report(&e)));
            let st = it.operand_stack();
            let n: f64 = st[st.len() - 2].repr().parse().expect("count");
            let y: f64 = st[st.len() - 1].repr().parse().expect("y");
            let step = if extra.contains("/Spacing 2") {
                24.0
            } else {
                12.0
            };
            let y0 = 4000.0 - 36.0 - 0.75 * 12.5;
            let lines = ((y0 - y) / step).round();
            assert!(
                (lines - n).abs() < 1e-9 && ((y0 - y) / step - lines).abs() < 1e-6,
                "text {i} with {extra:?}: tw-linecount {n}, tw-type typed {lines}"
            );
        }
    }
}

#[test]
fn pages_overflow_onto_the_next_with_a_page_start_and_numbers() {
    // 120pt tall, 36 margins, 12pt lines: the first baseline is 74.625 and
    // the last that stays above the 36 margin is at 38.6, so 4 lines a page
    let mut it = Interp::with_page_scaled(W, H, SCALE).expect("page");
    load(&mut it);
    let text = "(1\\n2\\n3\\n4\\n5\\n6\\n7\\n8\\n9)";
    it.run_str(&format!(
        "/starts 0 def \
         << /Text {text} /Width 288 /Height 120 /Margin 36 /PageNumbers /bottom \
            /PageStart {{ /starts starts 1 add def 1 setgray clippath fill }} >> tw-type"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(
        it.gfx().pages().len(),
        2,
        "9 lines at 4 a page: 3 pages, 2 showpages"
    );
    it.run_str("starts").unwrap();
    assert_eq!(
        it.operand_stack().last().unwrap().repr(),
        "3",
        "PageStart ran before each of the three pages"
    );
    // the page numbers differ from page to page and the first page has one
    let first = ink(&Interp::with_page_scaled(W, H, SCALE).map(|mut i| {
        load(&mut i);
        i.run_str(
            "1 setgray clippath fill << /Text (1) /Width 288 /Height 120 /Margin 36 /PageNumbers /bottom >> tw-type",
        )
        .unwrap();
        i
    }).unwrap());
    let bare = ink(&Interp::with_page_scaled(W, H, SCALE)
        .map(|mut i| {
            load(&mut i);
            i.run_str(
                "1 setgray clippath fill << /Text (1) /Width 288 /Height 120 /Margin 36 >> tw-type",
            )
            .unwrap();
            i
        })
        .unwrap());
    assert!(differing(&first, &bare) > 100, "no page number on page 1");
}

#[test]
fn a_page_start_cannot_leak_state_and_the_strike_index_is_restored() {
    // a paper proc that moves the origin and clips, and paints nothing, must
    // leave the typing as it was
    let plain = typed("", "Dear Sir");
    let with = typed(
        "/PageStart { 50 50 translate 10 rotate 0 0 5 5 rectclip newpath 3 3 moveto }",
        "Dear Sir",
    );
    assert_eq!(plain, with, "PageStart leaked its graphics state");
    // /Strike is back to the caller's value, also after a markup-heavy page
    let mut it = Interp::with_page(100, 100).expect("page");
    load(&mut it);
    it.run_str(
        "<< /Strike 4 >> tw-setup << /Text (<u>a</u> <x>b</x>) >> tw-type \
         TWState /tStrike get",
    )
    .unwrap();
    assert_eq!(it.operand_stack().last().unwrap().repr(), "4");
    // a memo field is one paragraph even with a newline in it
    assert_eq!(
        linecount("/Memo << /To (a\nb) >> /Text (t) /Width 288 /Margin 36"),
        linecount("/Memo << /To (a b) >> /Text (t) /Width 288 /Margin 36")
    );
}

#[test]
fn tw_pageheight_holds_exactly_the_text() {
    let mut it = Interp::with_page(100, 100).expect("page");
    load(&mut it);
    // 3 lines at 12pt: 72 + 9.375 + 24 + 72 = 177.375 -> 178
    it.run_str("<< /Text (a\\nb\\nc) >> tw-pageheight").unwrap();
    assert_eq!(it.operand_stack().last().unwrap().repr(), "178");
    // and a page that high takes the 3 lines on one page
    let mut it = Interp::with_page_scaled(612, 178, 1.0).expect("page");
    load(&mut it);
    it.run_str("<< /Text (a\\nb\\nc) /Height 178 >> tw-type")
        .unwrap();
    assert_eq!(it.gfx().pages().len(), 0, "it spilled onto a second page");
}

#[test]
fn tw_linecount_applies_no_option() {
    let mut it = Interp::with_page(100, 100).expect("page");
    load(&mut it);
    it.run_str(
        "<< /Pitch 10 /Size 12.5 >> tw-setup \
         << /Pitch 12 /Size 20 /Machine 9 /Text (hello) >> tw-linecount pop \
         << /Pitch 12 /Size 20 /Text (hello) >> tw-pageheight pop \
         (x) stringwidth pop",
    )
    .unwrap();
    let w: f64 = it.operand_stack().last().unwrap().repr().parse().unwrap();
    assert!(
        (w - 7.2).abs() < 1e-3,
        "the measuring calls changed the font: {w}"
    );
}

// ---- pscat typewrite ---------------------------------------------------

fn script(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .arg("typewrite")
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run typewrite.sh")
}

fn scratch(tag: &str) -> std::path::PathBuf {
    common::scratch_dir(&format!("pscat-typewrite-{tag}"))
}

const MEMO: &str = "1. Effective immediately every letter is typed in an original and two \
carbon copies. The <u>original</u> goes to the addressee.\n\n2. Corrections are made by \
<o>strikeover|strikeouts</o> or <x>erasure</x>, never by cutting the page. \u{201c}Quoted\u{201d} \
and it\u{2019}s fine \u{2014} caf\u{e9}.\n\n\tThank you.\n";

#[test]
fn the_script_types_a_memo_to_png_and_pdf_and_sizes_a_page_to_fit() {
    let dir = scratch("png");
    let txt = dir.join("memo.txt");
    std::fs::write(&txt, MEMO).unwrap();
    let png = dir.join("memo.png");
    let out = script(
        &[
            "--memo-to",
            "All Staff",
            "--memo-from",
            "Records",
            "--memo-subject",
            "Carbon (copies) \\ rules",
            "--memo-date",
            "October 14, 1962",
            "--seed",
            "4",
            "-o",
            png.to_str().unwrap(),
            txt.to_str().unwrap(),
        ],
        &dir,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let img = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).unwrap()).unwrap();
    assert_eq!((img.width(), img.height()), (612, 792), "a letter page");
    let dark = img.pixels().iter().filter(|p| p.red() < 128).count();
    assert!(dark > 2000, "the memo left only {dark} dark pixels");

    // --height fit: shorter than a letter page for a short text
    let fit = dir.join("fit.png");
    let out = script(
        &[
            "--height",
            "fit",
            "--text",
            "A short note.",
            "-o",
            fit.to_str().unwrap(),
        ],
        &dir,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let img = tiny_skia::Pixmap::decode_png(&std::fs::read(&fit).unwrap()).unwrap();
    assert_eq!(
        img.height(),
        72 + 10 + 72,
        "one line: 72 + 0.75 em + 72, rounded up"
    );

    // a PDF, and a driver the other tools can run
    let pdf = dir.join("memo.pdf");
    let out = script(
        &[
            "--spacing",
            "2",
            "-o",
            pdf.to_str().unwrap(),
            txt.to_str().unwrap(),
        ],
        &dir,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        std::fs::read(&pdf).unwrap().starts_with(b"%PDF"),
        "not a PDF"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_script_turns_typographic_characters_into_ascii_and_rejects_bad_input() {
    let dir = scratch("ascii");
    let ps = dir.join("drv.ps");
    let out = script(&["--text", MEMO, "--ps", ps.to_str().unwrap()], &dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let driver = std::fs::read(&ps).unwrap();
    let text_start = driver
        .windows(10)
        .position(|w| w == b"/TWOpts <<")
        .expect("the options dict");
    let opts = &driver[text_start..(text_start + 700).min(driver.len())];
    assert!(opts.is_ascii(), "non-ASCII bytes reached the driver");
    let opts = String::from_utf8_lossy(opts);
    assert!(opts.contains("\"Quoted\""), "curly quotes: {opts}");
    assert!(
        opts.contains("it's fine -- caf?"),
        "an apostrophe, a dash, an accent: {opts}"
    );
    // --reflow joins a paragraph's lines and keeps the blank line between
    let out = script(
        &[
            "--reflow",
            "--text",
            "one\ntwo\n\n\tthree\n four",
            "--ps",
            ps.to_str().unwrap(),
        ],
        &dir,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let driver = String::from_utf8(std::fs::read(&ps).unwrap()).unwrap();
    assert!(
        driver.contains("/Text (one two\n\n\tthree four\n)"),
        "reflowed text: {}",
        driver
            .lines()
            .find(|l| l.contains("/Text ("))
            .unwrap_or("none")
    );
    // trailing blank lines are kept (command substitution would strip them)
    let txt = dir.join("trail.txt");
    std::fs::write(&txt, "a\n\n\n").unwrap();
    let out = script(&[txt.to_str().unwrap(), "--ps", ps.to_str().unwrap()], &dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let driver = String::from_utf8(std::fs::read(&ps).unwrap()).unwrap();
    assert!(
        driver.contains("/Text (a\n\n\n)"),
        "trailing blank lines lost"
    );
    assert!(
        driver.contains("/PageSize [ 612 792 ]"),
        "the driver does not declare its page size"
    );
    for bad in [
        &["--text", "x", "--page-numbers", "middle"][..],
        &["--text", "x", "--width", "wide"][..],
        &["--text", "x", "--ink", "1,2"][..],
        &["--text", "x", "--bogus"][..],
        &[][..],
    ] {
        let out = script(bad, &dir);
        assert!(!out.status.success(), "{bad:?} should fail");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The driver the script writes runs in Ghostscript and types ink in the
/// same place as pscat (a correctness check only; CI has no gs).
#[test]
fn ghostscript_types_the_memo_the_script_writes() {
    if !gs_available() {
        eprintln!("skipping gs typewrite check: gs not installed");
        return;
    }
    let dir = scratch("gs");
    let ps = dir.join("memo.ps");
    let out = script(
        &[
            "--memo-to",
            "All Staff",
            "--memo-from",
            "Records",
            "--memo-subject",
            "Carbon copies",
            "--memo-date",
            "October 14, 1962",
            "--text",
            MEMO,
            "--ps",
            ps.to_str().unwrap(),
        ],
        &dir,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let png = dir.join("gs.png");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g612x792",
            "-r72",
        ])
        .arg(format!("-o{}", png.display()))
        .arg(&ps)
        .stdin(std::process::Stdio::null())
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected the typewrite driver");
    let img = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).unwrap()).unwrap();
    assert_eq!(
        (img.width(), img.height()),
        (612, 792),
        "the driver sets its own page size"
    );
    let ours = dir.join("ours.png");
    let out = script(
        &[
            "--memo-to",
            "All Staff",
            "--memo-from",
            "Records",
            "--memo-subject",
            "Carbon copies",
            "--memo-date",
            "October 14, 1962",
            "--text",
            MEMO,
            "-o",
            ours.to_str().unwrap(),
        ],
        &dir,
    );
    assert!(out.status.success());
    let rows = |p: &std::path::Path| -> Vec<u32> {
        let img = tiny_skia::Pixmap::decode_png(&std::fs::read(p).unwrap()).unwrap();
        let w = img.width() as usize;
        let mut rows = vec![0u32; img.height() as usize];
        for (i, px) in img.pixels().iter().enumerate() {
            // any visible ink: at 72 dpi the lighter ribbon ink (#423) leaves
            // few pixels under 128 in pscat, where gs's strokes are darker
            if px.red() < 200 {
                rows[i / w] += 1;
            }
        }
        rows
    };
    let (g, o) = (rows(&png), rows(&ours));
    let inked = |r: &[u32]| r.iter().filter(|&&n| n > 0).count();
    assert!(g.iter().sum::<u32>() > 2000, "gs drew almost nothing");
    // the same lines of text: the rows with ink match to within a pixel
    let (gi, oi) = (inked(&g), inked(&o));
    assert!(
        gi.abs_diff(oi) <= gi / 10,
        "gs inks {gi} rows, pscat {oi}: the layouts differ"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_page_end_runs_after_each_page_the_last_included_and_cannot_leak_state() {
    // 9 lines at 4 a page: 3 pages. /PageEnd paints a block in the bottom-left
    // corner, outside the margins, and must run on each of them.
    let mut it = Interp::with_page_scaled(W, H, SCALE).expect("page");
    load(&mut it);
    let text = "(1\\n2\\n3\\n4\\n5\\n6\\n7\\n8\\n9)";
    it.run_str(&format!(
        "/ends 0 def 1 setgray clippath fill \
         << /Text {text} /Width 288 /Height 120 /Margin 36 \
            /PageEnd {{ /ends ends 1 add def 0 setgray 0 0 12 12 rectfill }} >> tw-type"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it.run_str("ends").unwrap();
    assert_eq!(
        it.operand_stack().last().unwrap().repr(),
        "3",
        "PageEnd ran after each of the three pages"
    );
    let corner = |p: &tiny_skia::Pixmap| {
        let y = p.height() - 5;
        p.pixel(5, y).map_or(255, |c| c.red())
    };
    assert_eq!(it.gfx().pages().len(), 2);
    for (i, p) in it.gfx().pages().iter().enumerate() {
        assert!(corner(p) < 50, "page {} has no PageEnd block", i + 1);
    }
    assert!(corner(&it.gfx().pixmap) < 50, "the last page has none");

    // a hook that moves the origin and clips, and paints nothing, leaves the
    // typing of the next page and the caller's state as they were
    let plain = typed("", "Dear Sir");
    let with = typed(
        "/PageEnd { 50 50 translate 10 rotate 0 0 5 5 rectclip newpath 3 3 moveto }",
        "Dear Sir",
    );
    assert_eq!(plain, with, "PageEnd leaked its graphics state");

    // the guard
    let mut it = Interp::with_page(100, 100).expect("page");
    load(&mut it);
    let depth = it.dict_stack_len();
    let err = it
        .run_str("<< /PageEnd 5 >> tw-type")
        .expect_err("a number for PageEnd is refused");
    assert!(
        it.error_report(&err)
            .contains("tw-pageend-must-be-a-procedure"),
        "{}",
        it.error_report(&err)
    );
    assert_eq!(it.dict_stack_len(), depth);
}
