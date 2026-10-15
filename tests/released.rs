//! The released-document look (issue #214, `lib/released.ps`, and the
//! typewriter's `/Copies` and `<r>` redactions).
//!
//! The option guards are `%%SelfTest` blocks in `lib/released.ps`. These
//! tests need pixels: that every effect is off at strength 0 and shows when
//! on, that a seed fixes the page, that copies darken the type and whiten the
//! paper, that a redaction bar covers its whole word, that the caller's rand
//! stream and the dictionary stack come out untouched, and (locally, with gs
//! installed) that Ghostscript draws the specimen.

mod common;

use pscat::Interp;

const LIBS: [&str; 6] = [
    "lib/artkit.ps",
    "lib/surfacekit.ps",
    "lib/handscript.ps",
    "lib/typewriter_outlines.ps",
    "lib/typewriter.ps",
    "lib/released.ps",
];

const W: u32 = 300;
const H: u32 = 200;
/// A short typed page: the content most tests wrap.
const TYPED: &str = "{ << /Text (Dear Sir, the file is closed.) /Width 300 /Height 200 /Margin 30 \
                     /Machine 4 /Seed 3 >> tw-type }";
const SHEET: &str = "/Width 300 /Height 200 /Seed 7";

fn load(it: &mut Interp) {
    for path in LIBS {
        let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
}

fn render(src: &str) -> Interp {
    let mut it = Interp::with_page(W, H).expect("page");
    load(&mut it);
    it.run_str(&format!("1 setgray clippath fill 0 setgray {src}"))
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    it
}

/// Luminance, 0 (black) to 255.
fn lum(it: &Interp) -> Vec<u8> {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .map(|p| ((u32::from(p.red()) + u32::from(p.green()) + u32::from(p.blue())) / 3) as u8)
        .collect()
}

fn page(src: &str) -> Vec<u8> {
    lum(&render(src))
}

fn differing(a: &[u8], b: &[u8]) -> usize {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

/// Pixels that differ by more than a rounding step: the ribbon ink drawn at
/// another device origin rounds a clip edge one grey level differently (#423).
/// For the "is off" checks, which compare the same type through two routes.
fn differing_visibly(a: &[u8], b: &[u8]) -> usize {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).filter(|(x, y)| x.abs_diff(**y) > 2).count()
}

fn mean(p: &[u8]) -> f64 {
    p.iter().map(|&v| f64::from(v)).sum::<f64>() / p.len() as f64
}

fn rel_page(content: &str, opts: &str) -> Vec<u8> {
    page(&format!("{content} << {SHEET} {opts} >> rel-page"))
}

#[test]
fn rel_page_with_no_options_runs_the_content_and_draws_nothing_else() {
    let plain = page(&format!("{TYPED} exec"));
    assert_eq!(
        differing_visibly(&plain, &rel_page(TYPED, "")),
        0,
        "with no effect on, rel-page is the content alone"
    );
    assert!(mean(&plain) < 254.0, "the content did draw something");
}

#[test]
fn every_effect_is_off_at_zero_and_shows_when_on() {
    let plain = page(&format!("{TYPED} exec"));
    for (key, on) in [
        ("Age", "0.8"),
        ("EdgeDark", "0.8"),
        ("Folds", "0.8"),
        ("Lamp", "0.8"),
        ("Skew", "1.5"),
        ("Generation", "1"),
        ("Punch", "2"),
        ("Staple", "1"),
        ("Clip", "1"),
    ] {
        let off = rel_page(TYPED, &format!("/{key} 0"));
        assert_eq!(differing_visibly(&plain, &off), 0, "/{key} 0 is not off");
        let shown = rel_page(TYPED, &format!("/{key} {on}"));
        assert!(
            differing(&plain, &shown) > 100,
            "/{key} {on} changed only {} pixels",
            differing(&plain, &shown)
        );
    }
    // the marks that take dicts
    let stamp = "/Stamps [ << /Lines [ (SEEN) ] /X 150 /Y 60 /Size 24 >> ]";
    assert_eq!(
        differing_visibly(&plain, &rel_page(TYPED, "/Stamps [ ]")),
        0
    );
    assert!(differing(&plain, &rel_page(TYPED, stamp)) > 200, "a stamp");
}

#[test]
fn a_seed_fixes_the_page_and_another_seed_moves_the_marks() {
    let opts = "/Age 0.7 /Folds 0.5 /Generation 2 /Punch 2";
    let a = rel_page(TYPED, opts);
    let b = rel_page(TYPED, opts);
    assert_eq!(differing(&a, &b), 0, "the same seed draws the same page");
    let c = page(&format!(
        "{TYPED} << /Width 300 /Height 200 /Seed 8 {opts} >> rel-page"
    ));
    assert!(differing(&a, &c) > 200, "another seed moves the specks");
}

#[test]
fn a_copy_whitens_the_paper_and_the_type_gets_fatter() {
    // the same typing on the same aged paper, through 0 and 3 copies
    let aged = rel_page(TYPED, "/Age 0.9");
    let copy = rel_page(TYPED, "/Age 0.9 /Generation 3 /Speckle 0");
    assert!(
        mean(&copy) > mean(&aged) + 15.0,
        "a copy turns the ivory paper white: {} against {}",
        mean(&copy),
        mean(&aged)
    );
    // ink is measured on the type alone, same paper (none)
    let typed = |copies: &str| {
        page(&format!(
            "<< /Text (Dear Sir, the file is closed. a e o) /Width 300 /Height 200 /Margin 30 \
             /Machine 4 /Seed 3 {copies} >> tw-type"
        ))
    };
    let (c0, c3) = (typed(""), typed("/Copies 3"));
    let dark = |p: &[u8]| p.iter().filter(|&&v| v < 128).count() as f64;
    assert!(
        dark(&c3) > dark(&c0) * 1.1,
        "three copies put down more ink: {} against {} dark pixels",
        dark(&c3),
        dark(&c0)
    );
    // /Copies 0 is the plain page, pixel for pixel
    assert_eq!(differing(&c0, &typed("/Copies 0")), 0);
    // rel-page sets the type's copies for its content
    // (/Dropout 0: the holes of #449 take ink away from the type, which this
    // comparison, of the copy's spread and fill, is not about)
    let through = rel_page(TYPED, "/Generation 3 /Speckle 0 /Haze 0 /Dropout 0");
    let by_hand = page(
        "<< /Text (Dear Sir, the file is closed.) /Width 300 /Height 200 /Margin 30 \
         /Machine 4 /Seed 3 /Copies 3 >> tw-type",
    );
    // the copy's paper and chain of skews differ, but the ink counts agree
    // closely: both are the same fat type
    let ink = |p: &[u8]| p.iter().filter(|&&v| v < 100).count() as f64;
    assert!(
        (ink(&through) / ink(&by_hand) - 1.0).abs() < 0.12,
        "{} against {} dark pixels",
        ink(&through),
        ink(&by_hand)
    );
}

/// The paper of a copy: the median of a page with no type on it.
fn median(p: &[u8]) -> u8 {
    let mut v = p.to_vec();
    v.sort_unstable();
    v[v.len() / 2]
}

#[test]
fn a_copy_keeps_a_grey_haze_and_a_grain_unless_haze_is_zero() {
    // issue #466: a copy's paper is not flat white
    let blank = |opts: &str| rel_page("{ }", opts);
    let white = blank("/Generation 1 /Haze 0");
    assert!(median(&white) >= 250, "/Haze 0 is the plain white copy");
    let (mid, high) = (blank("/Generation 1"), blank("/Generation 1 /Haze 1"));
    // the default is T14's tone, about 0.9
    assert!(
        (215..=240).contains(&median(&mid)),
        "the default copy paper is a light grey: {}",
        median(&mid)
    );
    assert!(mean(&white) > mean(&mid) + 8.0 && mean(&mid) > mean(&high) + 3.0);
    // it has a grain and a mottle: the paper is not one value
    let m = median(&mid);
    let lo = mid.iter().filter(|&&v| v < m - 2).count();
    let hi = mid.iter().filter(|&&v| v > m + 2).count();
    assert!(
        lo > 500 && hi > 500,
        "the grey varies: {lo} darker, {hi} lighter"
    );
    // an original has no haze: the key does nothing without a generation
    assert_eq!(differing(&blank("/Haze 1"), &blank("")), 0);
    // another seed moves it
    let other = page("{ } << /Width 300 /Height 200 /Seed 8 /Generation 1 >> rel-page");
    assert!(differing(&mid, &other) > 500);
}

#[test]
fn the_haze_grain_does_not_cover_a_punch_hole() {
    // one hole at (34, 100) on the 300 x 200 sheet, radius 8.5 (the copy
    // chain may move it by up to 3.2pt): the lid shows through it, near black on a copy (0.1), and the grain must stay off it.
    // Rendered at 4 pixels to the point: at one the flecks average away.
    let mut it = Interp::with_page_scaled(W, H, 4.0).expect("page");
    load(&mut it);
    it.run_str(&format!(
        "1 setgray clippath fill {{ }} << {SHEET} /Generation 1 /Punch 1 >> rel-page"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let p = lum(&it);
    let w = (W * 4) as usize;
    let mut brightest = 0u8;
    for dy in -20i32..=20 {
        for dx in -20i32..=20 {
            if dx * dx + dy * dy <= 20 * 20 {
                let (x, y) = ((34 * 4 + dx) as usize, (100 * 4 + dy) as usize);
                brightest = brightest.max(p[y * w + x]);
            }
        }
    }
    assert!(
        brightest < 60,
        "a fleck of paper grey inside the hole: {brightest}"
    );
}

#[test]
fn a_redaction_bar_covers_its_whole_word() {
    // the hook gets x0 x1 baseline size of each run: <r>bcd</r> is 3 cells
    // of the 7.2pt pica advance, and a run that wraps comes as two
    let mut it = Interp::with_page(W, H).expect("page");
    load(&mut it);
    it.run_str(
        "<< /Text (a <r>bcd</r> e) /Width 300 /Height 200 /Margin 30
            /Redactor { 4 array astore /rdLog exch def } >> tw-type
         rdLog aload pop",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let st: Vec<f64> = it
        .operand_stack()
        .iter()
        .rev()
        .take(4)
        .map(|o| o.repr().parse().expect("a number"))
        .collect();
    let (size, y, x1, x0) = (st[0], st[1], st[2], st[3]);
    assert!((x1 - x0 - 21.6).abs() < 1e-3, "3 cells of 7.2pt: {x0} {x1}");
    assert!((x0 - (30.0 + 2.0 * 7.2)).abs() < 1e-3, "starts at column 2");
    assert!((size - 12.5).abs() < 1e-9 && (y - (200.0 - 30.0 - 0.75 * 12.5)).abs() < 1e-6);

    // the plain bar (no /Redactor) and rel-redact both cover every cell of
    // the word: the middle rows are dark from x0 to x1
    for redactor in ["", "/Redactor { rel-redact }"] {
        let p = page(&format!(
            "<< /Text (a <r>bcd</r> e) /Width 300 /Height 200 /Margin 30 /Machine 4 /Seed 3 \
             {redactor} >> tw-type"
        ));
        let row = |py: u32| &p[(py * W) as usize..((py + 1) * W) as usize];
        let top = (200.0 - y) as u32; // the baseline, in pixel rows from the top
        for py in [top - 4, top - 2, top, top + 1] {
            let r = row(py);
            let (lo, hi) = (x0.ceil() as usize + 1, x1.floor() as usize - 1);
            assert!(
                r[lo..hi].iter().all(|&v| v < 60),
                "{redactor:?}: a gap in the bar at row {py}"
            );
        }
        // and the word does not show through above or below
        let above = row(top - 14);
        assert!(above[(x0 as usize)..(x1 as usize)].iter().all(|&v| v > 200));
    }
}

/// Runs of pixels darker than `below` along one row of `p` between two columns.
fn dark_runs(p: &[u8], row: usize, cols: std::ops::Range<usize>, below: u8) -> usize {
    let r = &p[row * W as usize..(row + 1) * W as usize];
    let mut runs = 0;
    let mut inside = false;
    for &v in &r[cols] {
        if v < below && !inside {
            runs += 1;
        }
        inside = v < below;
    }
    runs
}

#[test]
fn a_clip_is_a_gem_wire_at_the_top_edge_with_the_inner_loop_only_a_dent() {
    // issue #451: it was three nested ellipses. Now the outer loop is wire on
    // the sheet, hung from the top edge, and the inner loop is behind the
    // sheet: the paper shows its dent, not its metal.
    let blank = rel_page("{ }", "");
    let one = rel_page("{ }", "/Clip 1");
    let cols = 70..115;
    let w = W as usize;
    for (i, (a, b)) in blank.iter().zip(&one).enumerate() {
        if a != b {
            let (x, y) = (i % w, i / w);
            assert!(
                y < 56 && cols.contains(&x),
                "the clip drew outside its box, at {x},{y}"
            );
        }
    }
    let wire = one.iter().zip(&blank).filter(|(o, b)| **o < 200 && **b > 250).count();
    assert!(wire > 40, "no wire: {wire} dark pixels");
    // the dent and the rust are faint: changed, but nowhere near the wire's dark
    let faint = one
        .iter()
        .zip(&blank)
        .filter(|(o, b)| **o >= 200 && b.abs_diff(**o) > 2)
        .count();
    assert!(faint > 60, "no dent or shadow: {faint} faint pixels");
    // across the middle of the clip the one loop's two sides are two wires;
    // an ellipse stack would cross three or more
    for row in [20, 28] {
        assert_eq!(dark_runs(&one, row, cols.clone(), 170), 2, "row {row}");
    }
    // a second clip is another one, not a heavier one
    let two = rel_page("{ }", "/Clip 2");
    assert!(differing(&one, &two) > 100, "the second clip drew nothing");
    // a clip on a copy is still there
    let cb = rel_page("{ }", "/Generation 1");
    let c1 = rel_page("{ }", "/Generation 1 /Clip 1");
    assert!(differing(&cb, &c1) > 100, "a copy lost the clip");
}

#[test]
fn the_callers_rand_stream_and_dictionary_stack_are_untouched() {
    let sheet = format!(
        "<< {SHEET} /Age 0.5 /Generation 2 /Punch 3 /Staple 1 /Clip 1 \
         /Stamps [ << /Lines [ (OK) ] /X 100 /Y 100 >> ] \
         /Notes [ << /Text (see me) /X 40 /Y 60 >> ] >>"
    );
    let drawn = |run: &str| -> (String, usize) {
        let mut it = Interp::with_page(W, H).expect("page");
        load(&mut it);
        let depth = it.dict_stack_len();
        it.run_str(&format!("11 srand {run} rand"))
            .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        assert_eq!(it.dict_stack_len(), depth, "rel-page left a dict behind");
        (it.operand_stack().last().expect("rand").repr(), depth)
    };
    let plain = drawn("");
    let used = drawn(&format!("{TYPED} {sheet} rel-page"));
    assert_eq!(
        plain.0, used.0,
        "rel-page drew from the caller's rand stream"
    );
}

#[test]
fn a_rejected_call_leaves_the_dictionary_stack_and_names_its_key() {
    for bad in [
        "{ } << /Age 2 >> rel-page",
        "{ } << /Aged 1 >> rel-page",
        "{ } << /Generation 9 >> rel-page",
        "<< /Lines 3 >> rel-stamp",
        "<< /Text (x) /Strength 7 >> rel-stamp",
        "<< /Text (x) /Font 5 >> rel-stamp",
        "<< >> rel-note",
        "<< /Text (x) /X (a) >> rel-stamp",
        "<< /Text (x) /Seed 1.5 >> rel-stamp",
        "<< /Text (x) /Y /up >> rel-note",
        "(a) 1 2 3 rel-redact",
    ] {
        let mut it = Interp::with_page(W, H).expect("page");
        load(&mut it);
        let before = it.dict_stack_len();
        assert!(it.run_str(bad).is_err(), "{bad} should fail");
        assert_eq!(
            it.dict_stack_len(),
            before,
            "{bad} left a dict on the stack"
        );
    }
}

#[test]
fn rel_paper_inside_a_page_keeps_the_pages_options() {
    // the content paints paper with its own options; the page's generations
    // and stamps must still be drawn after it
    let page_opts = "/Generation 2 /Stamps [ << /Lines [ (SEEN) ] /X 150 /Y 100 /Size 24 >> ]";
    let dark = |p: &[u8]| p.iter().filter(|&&v| v < 100).count() as f64;
    let plain = rel_page("{ }", page_opts);
    let nested = rel_page(
        "{ << /Age 0.5 /Width 120 /Height 80 >> rel-paper }",
        page_opts,
    );
    assert!(dark(&plain) > 500.0);
    assert!(
        (dark(&nested) / dark(&plain) - 1.0).abs() < 0.03,
        "{} against {} dark pixels",
        dark(&nested),
        dark(&plain)
    );
}

#[test]
fn a_redaction_inside_an_overtype_still_covers() {
    let p = page(
        "<< /Text (a <o>old|<r>secret</r></o> b) /Width 300 /Height 200 /Margin 30 >> tw-type",
    );
    // the bar's rows are dark right across the 6 cells of the replacement
    let row = |py: u32| &p[(py * W) as usize..((py + 1) * W) as usize];
    let top = (30.0 + 0.75 * 12.5) as u32;
    assert!(
        row(top - 2)[(30.0 + 2.0 * 7.2) as usize + 2..(30.0 + 8.0 * 7.2) as usize - 2]
            .iter()
            .all(|&v| v < 60)
    );
}

#[test]
fn the_effects_leave_the_contents_graphics_state_and_paths_alone() {
    // the content sees the caller's colour and line width, whatever paper,
    // backdrop and copy sheets were painted first
    let mut it = Interp::with_page(W, H).expect("page");
    load(&mut it);
    it.run_str(
        "0 setgray 1 setlinewidth \
         { currentgray currentlinewidth } \
         << /Width 300 /Height 200 /Folds 0.5 /Punch 2 /Generation 1 /Skew 1 /Age 0.5 >> rel-page",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let st = it.operand_stack();
    let n = st.len();
    let num = |i: usize| st[n - i].repr().parse::<f64>().expect("a number");
    assert_eq!((num(2), num(1)), (0.0, 1.0));
    // a stamp and a bar are drawn on a fresh path: a path the caller has
    // begun neither joins their ink nor their clip
    let alone = |src: &str| page(src);
    let with_path = |src: &str| {
        page(&format!(
            "10 10 moveto 250 150 lineto 100 20 lineto {src} newpath"
        ))
    };
    for src in [
        "<< /Lines [ (SEEN) ] /X 150 /Y 100 /Size 24 /Ink [0 0 0] >> rel-stamp",
        "50 100 120 14 rel-redact",
    ] {
        assert_eq!(differing(&alone(src), &with_path(src)), 0, "{src}");
    }
}

#[test]
fn the_callers_current_point_survives_the_wrapper_and_a_rejected_stamp_changes_nothing() {
    for opts in ["", "/Age 0.5 /Generation 2 /Skew 1"] {
        let mut it = Interp::with_page(W, H).expect("page");
        load(&mut it);
        it.run_str(&format!(
            "10 20 moveto 60 20 lineto {{ currentpoint }} << /Width 300 /Height 200 {opts} >> rel-page"
        ))
        .unwrap_or_else(|e| panic!("{opts}: {}", it.error_report(&e)));
        let st = it.operand_stack();
        let n = st.len();
        let num = |i: usize| st[n - i].repr().parse::<f64>().expect("a number");
        // the point keeps its place on the page; the content's own frame is
        // moved a few points by the skew and the copies
        assert!(
            (num(2) - 60.0).abs() < 6.0 && (num(1) - 20.0).abs() < 6.0,
            "{opts}: {} {}",
            num(2),
            num(1)
        );
    }
    let mut it = Interp::with_page(W, H).expect("page");
    load(&mut it);
    it.run_str(
        "/Courier findfont 12 scalefont setfont \
         { << /Lines [ 5 ] >> rel-stamp } stopped pop \
         { << /Lines [ (a) ] /Font /NoSuchFace-Bold >> rel-stamp } stopped pop \
         currentfont /FontName get",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(it.operand_stack().last().unwrap().repr(), "/Courier");
}

#[test]
fn a_zero_strength_stamp_and_a_huge_sheet_are_handled() {
    let blank = page("");
    assert_eq!(
        differing(
            &blank,
            &page("<< /Text (RECEIVED) /Strength 0 /Size 30 >> rel-stamp")
        ),
        0,
        "a stamp at strength 0 is off"
    );
    // the paper grid sizes itself from the sheet: 6000pt is more than 600 edges
    let _ = page("<< /Width 6000 /Height 200 /EdgeDark 1 /Age 1 >> rel-paper");
}

#[test]
fn a_stamp_is_uneven_ink_through_which_the_page_shows() {
    let stamp = |strength: &str| {
        page(&format!(
            "<< /Lines [ (RECEIVED) ] /X 150 /Y 100 /Size 30 /Ink [0 0 0] /Strength {strength} /Seed 2 >> rel-stamp"
        ))
    };
    let (full, light) = (stamp("1"), stamp("0.2"));
    assert!(
        mean(&light) > mean(&full) + 0.5,
        "less ink on a lighter stamp: {} against {}",
        mean(&light),
        mean(&full)
    );
    // typed text under a stamp stays in sight where the stamp's ink is light
    let under = page(
        "<< /Text (WWWWWWWWWWWWWWWWWWWW) /Width 300 /Height 200 /Margin 30 >> tw-type \
         << /Lines [ (X) ] /X 150 /Y 170 /Size 12 /Ink [1 0 0] /Frame /none /Strength 0.5 >> rel-stamp",
    );
    let typed_only =
        page("<< /Text (WWWWWWWWWWWWWWWWWWWW) /Width 300 /Height 200 /Margin 30 >> tw-type");
    assert!(differing(&under, &typed_only) > 0);
}

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Ghostscript draws the specimen (a correctness check only; CI has no gs):
/// its three sheets have the paper, the copies' white and the redaction bars
/// where pscat has them.
#[test]
fn ghostscript_draws_the_specimen_the_way_pscat_does() {
    if !gs_available() {
        eprintln!("skipping gs released check: gs not installed");
        return;
    }
    let dir = common::scratch_dir("pscat-released");
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("gs.png");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-r30",
            "-g782x217",
            "-dGraphicsAlphaBits=4",
            "-dTextAlphaBits=4",
        ])
        .arg(format!("-o{}", png.display()))
        .arg("examples/released_specimen.ps")
        .stdin(std::process::Stdio::null())
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected the specimen");
    let gs = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).unwrap()).unwrap();
    let mut it = Interp::with_page_scaled(1876, 520, 30.0 / 72.0).expect("page");
    let src = std::fs::read_to_string("examples/released_specimen.ps").unwrap();
    for path in LIBS {
        it.run_source(&std::fs::read(path).unwrap()).unwrap();
    }
    it.run_str(&src)
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let ours = lum(&it);
    let theirs: Vec<u8> = gs
        .pixels()
        .iter()
        .map(|p| ((u32::from(p.red()) + u32::from(p.green()) + u32::from(p.blue())) / 3) as u8)
        .collect();
    assert_eq!(ours.len(), theirs.len(), "the same page size");
    let (w, h) = (gs.width() as usize, gs.height() as usize);
    // mean luminance of each third of the page: the ivory original, the white
    // copies. The two interpreters' sheets agree to a few levels.
    for third in 0..3 {
        let (a, b) = (third * w / 3, (third + 1) * w / 3);
        let m = |p: &[u8]| -> f64 {
            let mut s = 0.0;
            for y in 0..h {
                for x in a..b {
                    s += f64::from(p[y * w + x]);
                }
            }
            s / ((b - a) * h) as f64
        };
        assert!(
            (m(&ours) - m(&theirs)).abs() < 6.0,
            "sheet {third}: pscat {} gs {}",
            m(&ours),
            m(&theirs)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// RMS over 6 x 6 pixel blocks of the within-block deviation, for the 48 pixel
/// windows of bare paper (nothing darker than 150 of 255): the same measure as
/// docs/dev/design/typewriter/tools/grain.py, in luminance 0..1.
fn grain_rms(p: &[u8], w: usize, h: usize) -> f64 {
    let (mut sum, mut n) = (0.0, 0u32);
    for wy in (0..h.saturating_sub(47)).step_by(48) {
        for wx in (0..w.saturating_sub(47)).step_by(48) {
            let at = |x: usize, y: usize| f64::from(p[(wy + y) * w + wx + x]) / 255.0;
            if (0..48).any(|y| (0..48).any(|x| at(x, y) < 0.6)) {
                continue;
            }
            for by in (0..48).step_by(6) {
                for bx in (0..48).step_by(6) {
                    let v: Vec<f64> = (0..36).map(|i| at(bx + i % 6, by + i / 6)).collect();
                    let m = v.iter().sum::<f64>() / 36.0;
                    sum += v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / 36.0;
                    n += 1;
                }
            }
        }
    }
    (sum / f64::from(n.max(1))).sqrt()
}

/// The standard deviation of the 48 pixel windows' means over the bare-paper
/// windows (nothing darker than 150 of 255): the mottle measure of
/// docs/dev/design/typewriter/tools/grain.py, in luminance 0..1.
fn mottle_std(p: &[u8], w: usize, h: usize) -> f64 {
    let mut means = Vec::new();
    for wy in (0..h.saturating_sub(47)).step_by(48) {
        for wx in (0..w.saturating_sub(47)).step_by(48) {
            let v: Vec<f64> = (0..48 * 48)
                .map(|i| f64::from(p[(wy + i / 48) * w + wx + i % 48]) / 255.0)
                .collect();
            if v.iter().any(|&x| x < 0.6) {
                continue;
            }
            means.push(v.iter().sum::<f64>() / v.len() as f64);
        }
    }
    let m = means.iter().sum::<f64>() / means.len().max(1) as f64;
    (means.iter().map(|x| (x - m).powi(2)).sum::<f64>() / means.len().max(1) as f64).sqrt()
}

#[test]
fn a_copys_mottle_is_at_the_scans_strength_and_has_creases() {
    // issue #528: T14's mottle is 0.0087 at 48 pixels (300 dpi); the crumpled-paper
    // wave is octaves of tilted noise and a few creases, not soft axis-aligned blocks
    let sheet = |pre: &str| {
        let mut it = Interp::with_page_scaled(W, H, 4.0).expect("page");
        load(&mut it);
        it.run_str(&format!(
            "{pre} 1 setgray clippath fill {{ }} << {SHEET} /Generation 1 >> rel-page"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        lum(&it)
    };
    let (on, off) = (sheet(""), sheet("RelState /rlCrD 0 put"));
    let (w, h) = ((W * 4) as usize, (H * 4) as usize);
    let std = mottle_std(&on, w, h);
    assert!(
        (0.005..0.014).contains(&std),
        "a copy's mottle is at the scans' strength: {std:.4} (T14 0.0087)"
    );
    // the creases show, and leave the paper's tone alone
    assert!(
        differing(&on, &off) > 3000,
        "creases drew {} pixels",
        differing(&on, &off)
    );
    assert!(
        (mean(&on) - mean(&off)).abs() < 1.0,
        "creases move the tone: {} against {}",
        mean(&on),
        mean(&off)
    );
}

#[test]
fn the_haze_grain_is_at_the_strength_of_the_scans() {
    // issue #474: T2, T9 and T14 are 0.007 to 0.009 at 300 dpi; at 4 pixels to
    // the point (288 dpi) a hazed copy must be near that, an original and a
    // /Haze 0 copy only what the lid and the edge give (under 0.005), and the grain scales with /Haze
    let on = |opts: &str| {
        let mut it = Interp::with_page_scaled(W, H, 4.0).expect("page");
        load(&mut it);
        it.run_str(&format!(
            "1 setgray clippath fill {{ }} << {SHEET} {opts} >> rel-page"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        grain_rms(&lum(&it), (W * 4) as usize, (H * 4) as usize)
    };
    let copy = on("/Generation 1");
    let (white, orig) = (on("/Generation 1 /Haze 0"), on(""));
    assert!(
        (0.006..0.013).contains(&copy),
        "a copy's grain is at the scans' strength: {copy:.4} (white copy {white:.4})"
    );
    assert!(white < 0.005, "/Haze 0 has no grain: {white:.4}");
    assert!(orig < 0.005, "an original has no haze grain: {orig:.4}");
    assert!(
        on("/Generation 1 /Haze 0.3") < copy * 0.75,
        "it scales with /Haze"
    );
}

/// A page of strokes at 4 pixels to the point: 8 hairlines (0.7pt) over 6
/// bars (3.2pt), each 260pt long, through `opts`. Returns the luminance.
fn strokes(opts: &str, tweak: &str) -> Vec<u8> {
    let mut it = Interp::with_page_scaled(W, H, 4.0).expect("page");
    load(&mut it);
    it.run_str(&format!(
        "{tweak} 1 setgray clippath fill \
         {{ 0 setgray 0.7 setlinewidth 0 1 7 {{ 12 mul 14 add 20 exch moveto 260 0 rlineto stroke }} for \
            3.2 setlinewidth 0 1 5 {{ 12 mul 112 add 20 exch moveto 260 0 rlineto stroke }} for }} \
         << {SHEET} /Speckle 0 /Haze 0 /CopyEdge 0 {opts} >> rel-page"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    lum(&it)
}

/// 4-connected dark (under 128) components in the pixel rows `rows` and
/// columns `cols`.
fn dark_components(p: &[u8], rows: std::ops::Range<usize>, cols: std::ops::Range<usize>) -> usize {
    let w = (W * 4) as usize;
    let mut seen = std::collections::HashSet::new();
    let mut n = 0;
    for y in rows.clone() {
        for x in cols.clone() {
            if p[y * w + x] >= 128 || !seen.insert((x, y)) {
                continue;
            }
            n += 1;
            let mut todo = vec![(x, y)];
            while let Some((cx, cy)) = todo.pop() {
                for (nx, ny) in [
                    (cx + 1, cy),
                    (cx.wrapping_sub(1), cy),
                    (cx, cy + 1),
                    (cx, cy.wrapping_sub(1)),
                ] {
                    if rows.contains(&ny)
                        && cols.contains(&nx)
                        && p[ny * w + nx] < 128
                        && seen.insert((nx, ny))
                    {
                        todo.push((nx, ny));
                    }
                }
            }
        }
    }
    n
}

#[test]
fn a_copy_breaks_its_thin_strokes_and_keeps_its_thick_ones() {
    // issue #449: T15's copy has lost its hairlines and kept its stems. The page
    // cannot read its strokes, but a small hole cuts a hairline in two and only
    // notches a bar, so after dropout the hairlines are in many pieces and the
    // bars are in few. 4 pixels to the point; the hairlines lie at y 14..98 and the bars at
    // y 112..172 (pixel rows from the top are 4 x (200 - y))
    let rows = |lo: usize, hi: usize| (4 * (200 - hi))..(4 * (200 - lo));
    let (thin, thick) = (rows(10, 104), rows(108, 176));
    let cols = 4 * 24..4 * 276;
    let count = |p: &[u8]| {
        (
            dark_components(p, thin.clone(), cols.clone()),
            dark_components(p, thick.clone(), cols.clone()),
        )
    };
    let (t0, k0) = count(&strokes("/Generation 3 /Dropout 0", ""));
    let (t1, k1) = count(&strokes("/Generation 3", ""));
    let (g1t, _) = count(&strokes("/Generation 1", ""));
    assert_eq!((t0, k0), (8, 6), "without dropout every stroke is whole");
    assert!(
        t1 >= t0 * 3,
        "hairlines break into pieces: {t0} strokes in {t1} pieces"
    );
    assert!(
        k1 * 2 <= t1,
        "bars break less than hairlines: {k1} against {t1} pieces"
    );
    assert!(
        g1t > t0 && g1t < t1,
        "a first copy breaks fewer: {g1t} against {t1}"
    );
}

#[test]
fn dropout_leaves_an_original_alone_and_scales_with_its_strength() {
    // an original has no copy loss: /Dropout is not a visible key at generation 0
    let orig = rel_page(TYPED, "/Age 0.6");
    assert_eq!(differing(&orig, &rel_page(TYPED, "/Age 0.6 /Dropout 1")), 0);
    // a copy's holes come and go with the strength: 0 is the copy without them
    let broken = |d: &str| {
        let p = strokes(&format!("/Generation 3 /Dropout {d}"), "");
        dark_components(&p, 4 * 30..4 * 190, 4 * 24..4 * 276)
    };
    let (none, half, full) = (broken("0"), broken("0.5"), broken("1"));
    assert!(
        none < half && half < full,
        "{none} < {half} < {full} pieces"
    );
}

#[test]
fn a_redaction_bar_stays_solid_under_dropout() {
    // the holes cut the content, and the bar is content: it is drawn again whole
    // afterwards (rlBarsAgain). Without that, a bar of 21.6pt x 12pt takes a dozen holes.
    let text = "{ << /Text (a <r>bcdefg</r> h) /Width 300 /Height 200 /Margin 30 /Machine 4 /Seed 3 \
                 /Redactor { rel-redact } >> tw-type }";
    let bar_gaps = |tweak: &str| {
        let mut it = Interp::with_page_scaled(W, H, 4.0).expect("page");
        load(&mut it);
        it.run_str(&format!(
            "{tweak} 1 setgray clippath fill {text} << {SHEET} /Generation 3 /Speckle 0 /Haze 0 /CopyEdge 0 >> rel-page"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        let p = lum(&it);
        let w = (W * 4) as usize;
        // the bar covers x 44.4..87.6 (cells 2 to 7 of 7.2pt from the margin 30) and
        // y 29..43 from the top; the copy's shift is up to 3pt, so take its middle
        let (x0, x1) = (4 * 50, 4 * 82);
        let y = 4 * 36;
        (y - 8..y + 8)
            .map(|py| {
                p[py * w + x0..py * w + x1]
                    .iter()
                    .filter(|&&v| v > 100)
                    .count()
            })
            .sum::<usize>()
    };
    assert_eq!(bar_gaps(""), 0, "the bar is solid under dropout");
    assert!(
        bar_gaps("RelState /rlBarsAgain { } put") > 20,
        "with the bar not drawn again, holes show in it"
    );
}

#[test]
fn a_content_that_errors_does_not_leave_the_bar_recorder_on() {
    // the content of a copy raised an error while its bars were being recorded;
    // the next page starts with the recorder off
    let mut it = Interp::with_page(W, H).expect("page");
    load(&mut it);
    it.run_str(&format!(
        "mark {{ {{ 1 0 div }} << {SHEET} /Generation 1 >> rel-page }} stopped cleartomark \
         {{ }} << {SHEET} >> rel-page RelState /rlRecOn get"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(
        it.operand_stack().last().map(|o| o.repr()),
        Some("false".to_string())
    );
}

#[test]
fn a_redaction_bar_drawn_under_the_contents_own_clip_stays_inside_it() {
    // review of #449: the bar is drawn again after the holes' clip is dropped, and
    // must keep the clip the content had put on it
    let p = {
        let mut it = Interp::with_page_scaled(W, H, 4.0).expect("page");
        load(&mut it);
        it.run_str(&format!(
            "1 setgray clippath fill {{ 100 0 100 200 rectclip 20 280 100 12 rel-redact }} \
             << {SHEET} /Generation 3 /Speckle 0 /Haze 0 /CopyEdge 0 >> rel-page"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        lum(&it)
    };
    let w = (W * 4) as usize;
    let dark = |x0: usize, x1: usize| {
        (0..(H * 4) as usize)
            .map(|y| {
                p[y * w + 4 * x0..y * w + 4 * x1]
                    .iter()
                    .filter(|&&v| v < 100)
                    .count()
            })
            .sum::<usize>()
    };
    assert!(dark(110, 190) > 300, "the bar shows inside the clip");
    assert_eq!(dark(0, 90), 0, "and not outside it");
}

// ---- #450: the copier's softening of the marks -----------------------------

/// A page drawn at 4 pixels to the point, luminance.
fn page4(src: &str) -> Vec<u8> {
    let mut it = Interp::with_page_scaled(W, H, 4.0).expect("page");
    load(&mut it);
    it.run_str(&format!("1 setgray clippath fill 0 setgray {src}"))
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    lum(&it)
}

/// Pixels strictly between ink and paper (30..225) along one row.
fn grey_in_row(p: &[u8], y: usize) -> usize {
    let w = (W * 4) as usize;
    p[y * w..(y + 1) * w]
        .iter()
        .filter(|&&v| (30..225).contains(&v))
        .count()
}

#[test]
fn a_softened_stamp_fades_over_its_edge_and_a_crisp_one_does_not() {
    // a black "I" at 60pt: its stem is two vertical edges
    let stamp = |soften: &str| {
        page4(&format!(
            "<< /Lines [ (I) ] /X 150 /Y 90 /Size 60 /Ink [0 0 0] /Frame /none /Strength 1 \
             /Seed 2 /Soften {soften} >> rel-stamp"
        ))
    };
    let (crisp, soft) = (stamp("0"), stamp("1"));
    // the middle of the stem: y = 200 - 90 - 10 = 100pt from the top
    let y = 4 * 100;
    let (a, b) = (grey_in_row(&crisp, y), grey_in_row(&soft, y));
    assert!(
        a <= 4,
        "a crisp stem's two edges are an antialiased pixel each: {a}"
    );
    assert!(
        b >= 3 * a.max(4),
        "a softened edge is several pixels of grey: {b} against {a}"
    );
    // and the soft stamp still has its solid core
    let w = (W * 4) as usize;
    assert!(soft[y * w + 4 * 150] < 30, "the stem's centre is still ink");
}

#[test]
fn softening_follows_the_copy_and_leaves_an_original_alone() {
    let marks = "/Stamps [ << /Lines [ (SEEN) ] /X 150 /Y 60 /Size 24 /Seed 4 >> ] \
                 /Notes [ << /Text (see me) /X 40 /Y 120 /Size 16 >> ]";
    let draw = |opts: &str| page4(&format!("{{ }} << {SHEET} {marks} {opts} >> rel-page"));
    // an original is not blurred: the page /Soften has nothing to act on
    assert_eq!(draw("/Soften 1"), draw("/Soften 0"));
    assert_eq!(draw(""), draw("/Soften 0"));
    // a copy is, and /Soften 0 turns it off
    let soft = draw("/Generation 1 /Speckle 0 /Haze 0 /CopyEdge 0");
    let crisp = draw("/Generation 1 /Speckle 0 /Haze 0 /CopyEdge 0 /Soften 0");
    assert_ne!(soft, crisp, "a copy's marks are softened");
    let grey = |p: &[u8]| p.iter().filter(|&&v| (30..225).contains(&v)).count();
    assert!(
        grey(&soft) > grey(&crisp) + 500,
        "more grey pixels with the fringe: {} against {}",
        grey(&soft),
        grey(&crisp)
    );
    // more copies, more blur
    let g3 = draw("/Generation 3 /Speckle 0 /Haze 0 /CopyEdge 0");
    let g3c = draw("/Generation 3 /Speckle 0 /Haze 0 /CopyEdge 0 /Soften 0");
    assert!(grey(&g3) - grey(&g3c) > grey(&soft) - grey(&crisp));
}

#[test]
fn a_softened_redaction_bar_fades_and_stays_under_its_clip() {
    let bar = |opts: &str| {
        page4(&format!(
            "{{ 100 0 200 200 rectclip 120 280 100 14 rel-redact }} \
             << {SHEET} /Generation 2 /Speckle 0 /Haze 0 /CopyEdge 0 /Dropout 0 {opts} >> rel-page"
        ))
    };
    let (soft, crisp) = (bar(""), bar("/Soften 0"));
    // a column through the bar, 60pt along it: the top edge is a short ramp
    let w = (W * 4) as usize;
    let ramp = |p: &[u8]| {
        (0..(H * 4) as usize)
            .filter(|&y| (30..225).contains(&p[y * w + 4 * 200]))
            .count()
    };
    assert!(
        ramp(&soft) >= ramp(&crisp) + 3,
        "a softened bar's edge is wider: {} against {}",
        ramp(&soft),
        ramp(&crisp)
    );
    // nothing leaks to the left of the clip the bar was drawn under
    let left_dark = (0..(H * 4) as usize)
        .map(|y| {
            soft[y * w..y * w + 4 * 95]
                .iter()
                .filter(|&&v| v < 200)
                .count()
        })
        .sum::<usize>();
    assert_eq!(left_dark, 0, "the fringe stays inside the clip");
}

#[test]
fn softening_does_not_touch_the_callers_rand_stream_or_the_dict_stack() {
    let mut it = Interp::with_page(W, H).expect("page");
    load(&mut it);
    let depth = it.dict_stack_len();
    let run = |it: &mut Interp, opts: &str| {
        it.run_str(&format!(
            "11 srand {{ 20 100 140 12 rel-redact }} << {SHEET} /Generation 2 {opts} \
             /Stamps [ << /Lines [ (OK) ] /X 100 /Y 100 >> ] \
             /Notes [ << /Text (see me) /X 40 /Y 60 >> ] >> rel-page rand"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.operand_stack().last().expect("rand").repr()
    };
    let soft = run(&mut it, "");
    let crisp = run(&mut it, "/Soften 0");
    assert_eq!(
        soft, crisp,
        "the fringe draws no random numbers the caller sees"
    );
    assert_eq!(it.dict_stack_len(), depth);
}

#[test]
fn a_note_with_a_soften_out_of_range_is_refused_and_leaves_the_dict_stack_alone() {
    let mut it = Interp::with_page(W, H).expect("page");
    load(&mut it);
    let depth = it.dict_stack_len();
    let err = it
        .run_str("<< /Text (a) /Soften 2 >> rel-note")
        .expect_err("a soften of 2 is refused");
    assert!(
        it.error_report(&err)
            .contains("rel-note-soften-must-be-0-to-1"),
        "{}",
        it.error_report(&err)
    );
    assert_eq!(it.dict_stack_len(), depth);
}
