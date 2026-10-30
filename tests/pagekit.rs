//! Parameterized page templates (issue #18, `lib/pagekit.ps`): five
//! `x y w h dict pgNAME` procs built on artkit's paragraph flow and
//! shapes. Two failure modes drove these tests directly: artkit's
//! `fitfont` scales unconditionally, which would blow a short string
//! (a two-letter awardee name, a short headline) up to fill its
//! target width instead of leaving it alone, so `pgzfitmax` clamps
//! the ratio at 1.0 -- tested directly, not just through a template.
//! And artkit's eight mood palettes are documented only as "five
//! colors," not as ordered by lightness -- `/parchment` runs light to
//! dark and `/carnival` is a hue wheel, so pagekit registers its own
//! `/vellum`/`/marigold`, each checked here for a monotonic dark-to-
//! light luminance ramp rather than trusted by inspection.

use pscat::Interp;

mod common;

fn load(it: &mut Interp) {
    for path in ["lib/artkit.ps", "lib/pagekit.ps"] {
        let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
}

// Luminance, not a per-channel threshold: pagekit's background tints are
// deliberately light overall but can still dip a single channel low (e.g.
// /ember's tint has a blue channel under 128 despite reading as pale
// yellow), which a per-channel OR check misclassifies as "ink" -- found
// by this exact test failing against pgposter before the switch.
fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| {
            let lum = 0.3 * p.red() as f32 + 0.59 * p.green() as f32 + 0.11 * p.blue() as f32;
            lum < 180.0
        })
        .count()
}

fn fresh(w: u32, h: u32) -> Interp {
    let mut it = Interp::with_page(w, h).expect("page");
    load(&mut it);
    it.run_str("1 1 1 setrgbcolor clippath fill")
        .expect("white background");
    it
}

const TEMPLATES: [(&str, &str); 5] = [
    (
        "pgcard",
        "10 10 180 180 << /Title (Hi) /Body (a quick note) /Signoff (- me) >> pgcard",
    ),
    (
        "pgletter",
        "10 10 180 180 << /Date (Jan 1) /Salutation (Dear Sam,) \
         /Body (thanks for everything) /Signoff (Best, J) >> pgletter",
    ),
    (
        "pgcertificate",
        "10 10 180 180 << /Awardee (A. Lovelace) /Body (for excellence) \
         /Presenter (C. Babbage) /Date (1843) >> pgcertificate",
    ),
    (
        "pginvitation",
        "10 10 180 180 << /Title (Party) /When (Sat) /Where (Here) \
         /Host (Us) /Body (come by) >> pginvitation",
    ),
    (
        "pgposter",
        "10 10 180 180 << /Title (BIG) /Tagline (small) \
         /Body (poster body copy) /Footer (fine print) >> pgposter",
    ),
];

#[test]
fn loads_clean_and_registers_palettes() {
    let mut it = Interp::with_page(50, 50).expect("page");
    load(&mut it);
    assert_eq!(ink_count(&it), 0, "pagekit drew on load");
    it.run_str("Palettes length").expect("palettes");
    let got: Vec<_> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(got, ["10"], "pagekit should add 2 palettes to artkit's 8");
}

#[test]
fn every_template_draws_its_content() {
    // A light background tint alone can clear a loose ink threshold, so
    // this compares each template's filled-content render against the
    // same template on an empty dict -- proving text/labels actually
    // land, not just that a background got painted.
    for (name, call) in TEMPLATES {
        let mut it_empty = fresh(200, 200);
        it_empty
            .run_str(&format!("10 10 180 180 << >> {name} pop"))
            .unwrap_or_else(|e| {
                panic!("{name} on empty dict failed: {}", it_empty.error_report(&e))
            });
        let empty_ink = ink_count(&it_empty);

        let mut it_filled = fresh(200, 200);
        it_filled
            .run_str(&format!("{call} pop"))
            .unwrap_or_else(|e| panic!("{name} failed: {}", it_filled.error_report(&e)));
        let filled_ink = ink_count(&it_filled);

        assert!(
            filled_ink > empty_ink,
            "{name}: filled content ({filled_ink} dark px) should paint more ink than an \
             empty dict ({empty_ink} dark px) -- otherwise text/labels aren't landing"
        );
    }
}

#[test]
fn empty_dict_does_not_error() {
    for (name, _) in TEMPLATES {
        let mut it = fresh(200, 200);
        it.run_str(&format!("10 10 180 180 << >> {name} pop"))
            .unwrap_or_else(|e| panic!("{name} on an empty dict failed: {}", it.error_report(&e)));
    }
}

#[test]
fn leftover_contract_matches_tfblock() {
    // A short body fits comfortably: leftover is the empty string.
    let mut it = fresh(200, 200);
    it.run_str(
        "10 10 180 180 << /Awardee (Al) /Body (short) \
         /Presenter (P) /Date (D) >> pgcertificate",
    )
    .unwrap_or_else(|e| panic!("pgcertificate failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    assert_eq!(stack.len(), 1, "expected exactly one leftover string");
    assert_eq!(stack[0].repr(), "()", "short body should leave no leftover");

    // Body/box font size and citation-band size both scale with the box's
    // own height, so a *smaller* box doesn't force overflow (the line
    // capacity is roughly self-similar) -- only *more text* than any
    // reasonable citation band can hold does.
    let mut it = fresh(200, 200);
    let sentence = "This citation is one of many identical repeated sentences. ";
    let long_body = format!("({})", sentence.repeat(40));
    it.run_str(&format!(
        "10 10 180 180 << /Awardee (Al) /Body {long_body} \
         /Presenter (P) /Date (D) >> pgcertificate"
    ))
    .unwrap_or_else(|e| panic!("pgcertificate failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    assert_eq!(stack.len(), 1, "expected exactly one leftover string");
    assert_ne!(
        stack[0].repr(),
        "()",
        "an oversized body in a tiny box must report leftover text"
    );
}

#[test]
fn pgzfitmax_never_enlarges_only_shrinks() {
    let mut it = fresh(200, 200);
    it.run_str(
        "/Helvetica findfont 20 scalefont setfont \
         (Al) stringwidth pop \
         (Al) dup 500 pgzfitmax stringwidth pop \
         eq",
    )
    .unwrap_or_else(|e| panic!("pgzfitmax (enlarge case) failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    assert_eq!(
        stack.last().unwrap().repr(),
        "true",
        "a short string given a huge target width must not be scaled up"
    );

    let mut it = fresh(200, 200);
    it.run_str(
        "/Helvetica findfont 20 scalefont setfont \
         (a string long enough to overflow a narrow target width) stringwidth pop \
         (a string long enough to overflow a narrow target width) dup 50 pgzfitmax \
         stringwidth pop \
         gt",
    )
    .unwrap_or_else(|e| panic!("pgzfitmax (shrink case) failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    assert_eq!(
        stack.last().unwrap().repr(),
        "true",
        "a string that overflows the target width must be shrunk (original width > \
         post-fitmax width)"
    );
}

#[test]
fn new_palettes_are_ordered_dark_to_light() {
    let mut it = Interp::new();
    load(&mut it);
    it.run_str(
        "/pglum { aload pop 0.11 mul 3 1 roll 0.59 mul 3 1 roll 0.3 mul add add } def \
         /checkmono { \
           pal /arr exch def \
           arr 0 get pglum arr 1 get pglum lt \
           arr 1 get pglum arr 2 get pglum lt and \
           arr 2 get pglum arr 3 get pglum lt and \
           arr 3 get pglum arr 4 get pglum lt and \
         } def \
         /vellum checkmono /marigold checkmono",
    )
    .unwrap_or_else(|e| panic!("palette monotonicity check failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    let got: Vec<_> = stack.iter().map(|o| o.repr()).collect();
    assert_eq!(
        got,
        ["true", "true"],
        "vellum and marigold must both run dark to light (luminance strictly increasing \
         index 0 to 4), or role-indexed ink/background picks break"
    );
}

#[test]
fn ghostscript_accepts_pagekit() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let artkit = std::fs::read_to_string("lib/artkit.ps").expect("artkit");
    let pagekit = std::fs::read_to_string("lib/pagekit.ps").expect("pagekit");
    let driver = TEMPLATES
        .iter()
        .map(|(_, call)| format!("{call} pop"))
        .collect::<Vec<_>>()
        .join(" ");
    let dir = common::scratch_dir("pscat-pagekit");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let combined = dir.join("pagekit_gs.ps");
    std::fs::write(
        &combined,
        format!("{artkit}\n{pagekit}\n{driver}\nshowpage\n"),
    )
    .expect("write");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g400x400",
            "-r72",
            "-o/dev/null",
        ])
        .arg(&combined)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected pagekit");
}

// --- pgcertificate layout (#954) -------------------------------------------
//
// The certificate's spacing failed a perceptual review: the title nearly
// touched the inner rule, the signer names floated far above their rules
// while the captions under them touched, and a short citation left one
// large empty band above the signatures. These tests pin each of those
// by scanning rows of ink. The certificate is drawn at 20,20 with a
// 500 pt height on a 72 dpi page, so 1 px = 1 pt and every 0.01h is 5 px.

const CERT_W: u32 = 532;
const CERT_H: u32 = 500;
const MARGIN: u32 = 20;

fn certificate(dict: &str) -> Interp {
    let mut it = fresh(CERT_W + 2 * MARGIN, CERT_H + 2 * MARGIN);
    it.run_str(&format!(
        "{MARGIN} {MARGIN} {CERT_W} {CERT_H} << {dict} >> pgcertificate pop"
    ))
    .unwrap_or_else(|e| panic!("pgcertificate failed: {}", it.error_report(&e)));
    it
}

fn dark(it: &Interp, x: u32, y: u32) -> bool {
    let p = it.gfx().pixmap.pixel(x, y).expect("pixel in range");
    0.3 * p.red() as f32 + 0.59 * p.green() as f32 + 0.11 * (p.blue() as f32) < 180.0
}

/// Dark pixels in device row `y` between columns `x0..x1`.
fn row_ink(it: &Interp, y: u32, x0: u32, x1: u32) -> u32 {
    (x0..x1).filter(|&x| dark(it, x, y)).count() as u32
}

/// Device x of a fraction of the certificate's width.
fn cx(frac: f32) -> u32 {
    MARGIN + (CERT_W as f32 * frac) as u32
}

/// Inner rule rows: the frame inset is 0.05 * min(w, h) = 25 pt. Rows are
/// counted down from the top of the page.
fn inner_top_row() -> u32 {
    MARGIN + 25
}
fn inner_bottom_row() -> u32 {
    MARGIN + CERT_H - 25
}

/// Rows strictly inside the inner rule, scanned over `x0..x1`: true where
/// the row holds ink. The rule's own 2 px stroke is skipped at each end.
fn ink_rows(it: &Interp, x0: u32, x1: u32) -> Vec<(u32, bool)> {
    (inner_top_row() + 3..inner_bottom_row() - 2)
        .map(|y| (y, row_ink(it, y, x0, x1) > 0))
        .collect()
}

/// Lengths of the blank runs between inked rows (the runs touching the
/// inner rule at either end are the frame padding, returned separately).
fn blank_runs(rows: &[(u32, bool)]) -> (u32, Vec<u32>, u32) {
    let first = rows.iter().position(|r| r.1).expect("some ink");
    let last = rows.iter().rposition(|r| r.1).expect("some ink");
    let mut runs = Vec::new();
    let mut run = 0;
    for r in &rows[first..=last] {
        if r.1 {
            if run > 0 {
                runs.push(run);
            }
            run = 0;
        } else {
            run += 1;
        }
    }
    (first as u32, runs, (rows.len() - 1 - last) as u32)
}

const SPECIMEN: &str = "/Title (Certificate of Achievement) /Awardee (Ada Lovelace) \
     /Body (In recognition of outstanding contributions to the analytical engine and \
     the founding of computer programming as a discipline.) \
     /Presenter (C. Babbage) /Date (1843-10-12)";

#[test]
fn certificate_title_clears_the_inner_rule() {
    // Was 0.02h (10 px); the fix sets 0.07h. Ask for at least 0.05h.
    for dict in [
        SPECIMEN,
        "",
        "/Title (A Very Long Title That Has To Shrink To Fit The Width Of The Certificate)",
    ] {
        let it = certificate(dict);
        let (top_pad, _, _) = blank_runs(&ink_rows(&it, cx(0.15), cx(0.85)));
        assert!(
            top_pad >= 25,
            "title ink starts {top_pad} px under the inner rule, want >= 25 (0.05h) for {dict:?}"
        );
    }
}

/// The left signature rule's row: the row in the rule's column span that
/// is almost fully dark.
fn left_rule_row(it: &Interp) -> u32 {
    let (x0, x1) = (cx(0.12) + 2, cx(0.40) - 2);
    (inner_top_row() + 3..inner_bottom_row() - 2)
        .rev()
        .find(|&y| row_ink(it, y, x0, x1) * 10 >= (x1 - x0) * 9)
        .expect("a signature rule")
}

#[test]
fn certificate_names_sit_on_their_rules_and_captions_clear_them() {
    // "C. Babbage" has a descender, the hardest case for the name side.
    let it = certificate(SPECIMEN);
    let rule = left_rule_row(&it);
    let (x0, x1) = (cx(0.12), cx(0.40));
    let ink = |y: u32| row_ink(&it, y, x0, x1) > 0;
    // The rule is 1-2 px thick; step past it.
    let first_above = (2..60)
        .find(|&d| ink(rule - d))
        .expect("name ink above the rule");
    let first_below = (2..60)
        .find(|&d| ink(rule + d))
        .expect("caption ink below the rule");
    assert!(
        (3..=12).contains(&first_above),
        "name ink should sit 3-12 px above its rule (close, not touching); got {first_above}"
    );
    assert!(
        first_below >= 7,
        "the caption under a rule must leave a clear gap; its ink starts {first_below} px below"
    );
}

#[test]
fn certificate_gaps_are_balanced() {
    // The three gaps (title / name cluster / citation / signatures) share
    // the free space, so no one blank band dwarfs the rest: before #954 a
    // two-line citation left ~150 px above the signatures against ~45 px
    // for the next biggest gap. Holds with a short, a long-ish and no
    // citation.
    for body in [
        "/Body (For excellence.)",
        "/Body (In recognition of outstanding contributions to the analytical engine \
         and the founding of computer programming as a discipline. With thanks.)",
        "",
    ] {
        let it = certificate(&format!(
            "/Awardee (Ada Lovelace) /Presenter (C. Babbage) /Date (1843) {body}"
        ));
        // Across the signature columns too, so the band above the names
        // counts; the small rule-to-caption gaps rank below the big three.
        let (_, mut runs, _) = blank_runs(&ink_rows(&it, cx(0.15), cx(0.85)));
        runs.sort_unstable_by(|a, b| b.cmp(a));
        assert!(
            runs.len() >= 2,
            "expected several blank bands, got {runs:?}"
        );
        assert!(
            runs[0] * 10 <= runs[1] * 16,
            "largest blank band {} px vs next {} px: unbalanced for {body:?} ({runs:?})",
            runs[0],
            runs[1]
        );
    }
}

#[test]
fn certificate_long_content_stays_inside_and_clear_of_the_title() {
    let sentence = "This citation is one of many identical repeated sentences. ";
    let it = certificate(&format!(
        "/Title (Certificate of Distinguished Service and Extraordinary Merit in Computing) \
         /Awardee (Augusta Ada King, Countess of Lovelace and Baroness Wentworth) \
         /Body ({}\\nA second paragraph after a newline.) \
         /Presenter (Charles Babbage, Lucasian Professor of Mathematics, Cambridge) \
         /Date (Thursday the twelfth of October, eighteen forty-three)",
        sentence.repeat(20)
    ));
    // Nothing outside the certificate's own rectangle, give or take the
    // outer rule's own stroke, which is centred on the edge (pagekit does
    // not clip; see its header).
    let (pw, ph) = (CERT_W + 2 * MARGIN, CERT_H + 2 * MARGIN);
    let (lo, hi_x, hi_y) = (MARGIN - 2, MARGIN + CERT_W + 2, MARGIN + CERT_H + 2);
    for y in 0..ph {
        for x in 0..pw {
            let inside = (lo..hi_x).contains(&x) && (lo..hi_y).contains(&y);
            assert!(
                inside || !dark(&it, x, y),
                "ink outside the certificate at {x},{y}"
            );
        }
    }
    // Nothing between the two rules (the 25 px border band), in the middle
    // columns where only text could stray.
    for y in MARGIN + 3..inner_top_row() - 2 {
        assert_eq!(
            row_ink(&it, y, cx(0.1), cx(0.9)),
            0,
            "ink in the top border band, row {y}"
        );
    }
    for y in inner_bottom_row() + 3..MARGIN + CERT_H - 3 {
        assert_eq!(
            row_ink(&it, y, cx(0.1), cx(0.9)),
            0,
            "ink in the bottom border band, row {y}"
        );
    }
    // An overflowing citation holds every gap at its minimum (0.05h = 25 px
    // from baseline to cap top, so well over 15 px of blank ink-to-ink):
    // the awardee cluster is never pushed up into the title.
    let (_, runs, _) = blank_runs(&ink_rows(&it, cx(0.15), cx(0.85)));
    assert!(
        runs[0] >= 15,
        "title and caption are {} px apart with an overflowing citation; want >= 15 ({runs:?})",
        runs[0]
    );
}
