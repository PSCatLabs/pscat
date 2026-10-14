//! The released-document look (issue #214, `lib/released.ps`, and the
//! typewriter's `/Copies` and `<r>` redactions).
//!
//! The option guards are `%%SelfTest` blocks in `lib/released.ps`. These
//! tests need pixels: that every effect is off at strength 0 and shows when
//! on, that a seed fixes the page, that copies darken the type and whiten the
//! paper, that a redaction bar covers its whole word, that the caller's rand
//! stream and the dictionary stack come out untouched, and (locally, with gs
//! installed) that Ghostscript draws the specimen.

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
        differing(&plain, &rel_page(TYPED, "")),
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
        assert_eq!(differing(&plain, &off), 0, "/{key} 0 is not off");
        let shown = rel_page(TYPED, &format!("/{key} {on}"));
        assert!(
            differing(&plain, &shown) > 100,
            "/{key} {on} changed only {} pixels",
            differing(&plain, &shown)
        );
    }
    // the marks that take dicts
    let stamp = "/Stamps [ << /Lines [ (SEEN) ] /X 150 /Y 60 /Size 24 >> ]";
    assert_eq!(differing(&plain, &rel_page(TYPED, "/Stamps [ ]")), 0);
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
    let through = rel_page(TYPED, "/Generation 3 /Speckle 0");
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
    let dir = std::env::temp_dir().join(format!("pscat-released-{}", std::process::id()));
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
