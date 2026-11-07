//! Skeleton glyphs for board math (issue #236, `lib/strokeglyphs.ps`).
//! The library's own `%%SelfTest` blocks pin its guards and invariants;
//! these tests cover what needs Rust or a second interpreter: the
//! specimen renders, handscript's output is unchanged by loading this
//! file, every glyph's path survives a real path consumer (paintkit's
//! `pkribbon`), and Ghostscript builds the same path from the same seed.

use pscat::Interp;

fn load(it: &mut Interp, files: &[&str]) {
    for f in files {
        let src = std::fs::read(f).unwrap_or_else(|_| panic!("{f} present"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{f} failed: {}", it.error_report(&e)));
    }
}

fn run(it: &mut Interp, src: &str) {
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
}

fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 128)
        .count()
}

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn the_specimen_renders_with_ink_and_a_clean_stack() {
    let src = std::fs::read("examples/strokeglyphs.ps").expect("specimen present");
    let mut it = Interp::with_page(900, 1600).expect("page");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("specimen failed: {}", it.error_report(&e)));
    assert!(it.operand_stack().is_empty(), "specimen leaked operands");
    assert!(ink_count(&it) > 20_000, "specimen drew too little ink");
}

/// Issue #339's specimen: the new board-math glyphs in sgpath text and in
/// mathkit formulas, on one page, ending with a clean stack.
#[test]
fn the_board_math_specimen_renders_with_ink_and_a_clean_stack() {
    let src = std::fs::read("examples/strokeglyphs_board_math.ps").expect("specimen present");
    let mut it = Interp::with_page(900, 1100).expect("page");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("specimen failed: {}", it.error_report(&e)));
    assert!(it.operand_stack().is_empty(), "specimen leaked operands");
    assert!(ink_count(&it) > 15_000, "specimen drew too little ink");
}

/// Issue #366's specimen: subscripts and variant Greek in sgpath text and
/// in mathkit formulas, on one page, ending with a clean stack.
#[test]
fn the_subscripts_and_greek_specimen_renders_with_ink_and_a_clean_stack() {
    let src = std::fs::read("examples/strokeglyphs_subscripts_greek.ps").expect("specimen present");
    let mut it = Interp::with_page(900, 600).expect("page");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("specimen failed: {}", it.error_report(&e)));
    assert!(it.operand_stack().is_empty(), "specimen leaked operands");
    assert!(ink_count(&it) > 8_000, "specimen drew too little ink");
}

/// #339: every code point the issue and #238's agent asked for draws,
/// where it used to raise `sg-missing-glyph:<char>` (#350).
#[test]
fn the_requested_board_math_characters_draw() {
    let mut it = Interp::with_page(300, 300).expect("page");
    load(&mut it, &["lib/handscript.ps", "lib/strokeglyphs.ps"]);
    for ch in "≡∼≃∘∗⋆∖⊆⊃⊇↦⊥∥∝∴∵⟨⟩‖ℏℓ∬∭∮ιυΥο²³¹⁴⁵⁶⁷⁸⁹⁰⁺⁻ⁿ".chars()
    {
        // currentpoint raises nocurrentpoint when the glyph added no path
        run(
            &mut it,
            &format!("newpath ({ch}) 100 100 << /Size 80 /Jitter 0 >> sgpath currentpoint clear"),
        );
        assert!(
            it.operand_stack().is_empty(),
            "{ch} left operands on the stack"
        );
    }
}

/// #366: the subscript digits and letters and the variant Greek letters
/// draw, where they used to raise `sg-missing-glyph:<char>`.
#[test]
fn the_subscripts_and_variant_greek_draw() {
    let mut it = Interp::with_page(300, 300).expect("page");
    load(&mut it, &["lib/handscript.ps", "lib/strokeglyphs.ps"]);
    for ch in "₀₁₂₃₄₅₆₇₈₉₊₋₌ₐₑₒₓₙᵢⱼϕϑϵϰϱϖς".chars() {
        run(
            &mut it,
            &format!("newpath ({ch}) 100 100 << /Size 80 /Jitter 0 >> sgpath currentpoint clear"),
        );
        assert!(
            it.operand_stack().is_empty(),
            "{ch} left operands on the stack"
        );
    }
}

/// #236's compatibility criterion: existing handscript output is
/// unchanged. strokeglyphs copies handscript's glyphs at load, so the
/// risk is mutating the shared arrays or the rand stream; render the
/// same note with and without it loaded (and after an sgpath call).
#[test]
fn handscript_output_is_unchanged_by_loading_strokeglyphs() {
    let note = "<< /Text (the quick brown fox, 1985!) /Size 30 /Width 400 /Height 200 \
                /Margin 20 /Seed 42 >> hs-write";
    let mut plain = Interp::with_page(400, 200).expect("page");
    load(&mut plain, &["lib/handscript.ps"]);
    run(&mut plain, note);

    let mut with = Interp::with_page(400, 200).expect("page");
    load(&mut with, &["lib/handscript.ps", "lib/strokeglyphs.ps"]);
    run(
        &mut with,
        "gsave newpath (abc) 0 0 << >> sgpath newpath grestore",
    );
    run(&mut with, note);

    let marked = plain
        .gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 230)
        .count();
    assert!(
        marked > 500,
        "the note drew nothing ({marked} marked pixels)"
    );
    assert!(
        plain.gfx().pixmap.data() == with.gfx().pixmap.data(),
        "hs-write rendered differently once strokeglyphs.ps was loaded"
    );
}

/// The design's point: the glyphs are paths a tool can follow. Feed
/// every glyph (exact skeleton, so doubled corner points and dots are
/// present unjittered) and every growing symbol through `pkribbon`,
/// which walks the path and computes directions along it.
#[test]
fn every_glyph_path_survives_a_paintkit_stroke() {
    let mut it = Interp::with_page(300, 300).expect("page");
    load(
        &mut it,
        &[
            "lib/artkit.ps",
            "lib/paintkit.ps",
            "lib/handscript.ps",
            "lib/strokeglyphs.ps",
        ],
    );
    run(
        &mut it,
        "sgtab { length 2 gt { /sgt-n exch def \
           newpath [sgt-n] 40 100 << /Size 120 /Jitter 0 >> sgpath \
           << /Width 4 >> pkribbon } { pop } ifelse } forall \
         sggrownames { /sgt-n exch def \
           newpath 40 100 moveto 120 40 << /Symbol sgt-n /Size 30 /Jitter 0 /Span 60 >> \
           sggrow pop << /Width 4 >> pkribbon } forall",
    );
    assert!(it.operand_stack().is_empty(), "a stroke leaked operands");
    assert!(ink_count(&it) > 1000, "pkribbon drew nothing");
}

/// Ghostscript must accept the specimen and build the very same path
/// from the same seed: the generator, the UTF-8 decode and the
/// integer-keyed code-point map are all plain integer PostScript.
#[test]
fn ghostscript_builds_the_same_path_from_the_same_seed() {
    if !gs_available() {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let status = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g900x1600",
            "-r72",
            "-o/dev/null",
            "examples/strokeglyphs.ps",
        ])
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected examples/strokeglyphs.ps");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g900x1100",
            "-r72",
            "-o/dev/null",
            "examples/strokeglyphs_board_math.ps",
        ])
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected examples/strokeglyphs_board_math.ps"
    );
    let status = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g900x600",
            "-r72",
            "-o/dev/null",
            "examples/strokeglyphs_subscripts_greek.ps",
        ])
        .stdin(std::process::Stdio::null())
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected examples/strokeglyphs_subscripts_greek.ps"
    );

    let probe = "(lib/handscript.ps) run (lib/strokeglyphs.ps) run \
                 newpath (Aσ≤∑x∮ℏ²⟨₂ϑ) 10 20 << /Seed 9 /Slant 8 >> sgpath \
                 40 60 moveto 50 20 << /Symbol /braceleft /Seed 3 >> sggrow pop \
                 80 60 moveto 50 20 << /Symbol /oint /Seed 4 >> sggrow pop \
                 { } { } { } { } pathforall \
                 count { 1000 mul round cvi = } repeat";
    let gs = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNODISPLAY",
            "-dBATCH",
            "-dNOPAUSE",
            "-q",
            "-c",
        ])
        .arg(probe)
        .output()
        .expect("run gs");
    assert!(gs.status.success(), "gs probe failed");
    let gs_nums: Vec<i64> = String::from_utf8_lossy(&gs.stdout)
        .lines()
        .map(|l| l.trim().parse().expect("an integer"))
        .collect();

    let mut it = Interp::new();
    load(&mut it, &["lib/handscript.ps", "lib/strokeglyphs.ps"]);
    run(
        &mut it,
        "newpath (Aσ≤∑x∮ℏ²⟨₂ϑ) 10 20 << /Seed 9 /Slant 8 >> sgpath \
         40 60 moveto 50 20 << /Symbol /braceleft /Seed 3 >> sggrow pop \
         80 60 moveto 50 20 << /Symbol /oint /Seed 4 >> sggrow pop \
         { } { } { } { } pathforall \
         count array astore { 1000 mul round cvi } forall",
    );
    let mut ours: Vec<i64> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().parse().expect("an integer"))
        .collect();
    // gs printed top of stack first
    ours.reverse();
    assert!(ours.len() > 100, "the probe path is suspiciously short");
    assert_eq!(ours.len(), gs_nums.len(), "path lengths differ");
    for (i, (a, b)) in ours.iter().zip(&gs_nums).enumerate() {
        assert!(
            (a - b).abs() <= 2,
            "coordinate {i} differs: pscat {a} vs gs {b} (thousandths)"
        );
    }
}

/// Issue #1051's specimen: the accented Latin letters in test words, from
/// Latin-1 byte strings, on a chalk slate, and every new letter once.
#[test]
fn the_accents_specimen_renders_with_ink_and_a_clean_stack() {
    let src = std::fs::read("examples/strokeglyphs_accents.ps").expect("specimen present");
    let mut it = Interp::with_page(900, 1180).expect("page");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("specimen failed: {}", it.error_report(&e)));
    assert!(it.operand_stack().is_empty(), "specimen leaked operands");
    assert!(ink_count(&it) > 20_000, "specimen drew too little ink");
}

/// The code points #1051 covers: every letter of Latin-1 (U+00C0-00FF
/// but × and ÷, which were already there) and Latin Extended-A
/// (U+0100-017F), and the Romanian comma-below letters U+0218-021B.
fn latin_letters() -> Vec<u32> {
    (0xC0..=0x17F)
        .filter(|cp| *cp != 0xD7 && *cp != 0xF7)
        .chain(0x218..=0x21B)
        .collect()
}

/// #1051: every accented letter draws from a UTF-8 string, and every one
/// up to U+00FF also from its single Latin-1 byte, where each used to
/// raise `sg-missing-glyph:<char>`.
#[test]
fn every_latin_letter_draws_from_utf8_and_from_latin1() {
    let mut it = Interp::with_page(300, 300).expect("page");
    load(&mut it, &["lib/handscript.ps", "lib/strokeglyphs.ps"]);
    for cp in latin_letters() {
        let ch = char::from_u32(cp).expect("a char");
        // currentpoint raises nocurrentpoint when the glyph added no path
        run(
            &mut it,
            &format!("newpath ({ch}) 100 100 << /Size 80 /Jitter 0 >> sgpath currentpoint clear"),
        );
        if cp <= 0xFF {
            run(
                &mut it,
                &format!(
                    "newpath (a\\{cp:03o}) << /Size 80 >> sgwidth (a{ch}) << /Size 80 >> sgwidth \
                     ne {{ latin1-width-differs }} if \
                     (\\{cp:03o}) 100 100 << /Size 80 /Jitter 0 >> sgpath currentpoint clear"
                ),
            );
        }
        assert!(
            it.operand_stack().is_empty(),
            "U+{cp:04X} left operands on the stack"
        );
    }
}

/// #1051's convention: a Latin-1 byte string draws exactly the path its
/// UTF-8 twin does under the same seed, coordinate for coordinate.
#[test]
fn a_latin1_string_draws_the_same_path_as_its_utf8_twin() {
    let mut it = Interp::new();
    load(&mut it, &["lib/handscript.ps", "lib/strokeglyphs.ps"]);
    let flat = |text: &str| {
        format!(
            "newpath ({text}) 10 20 << /Seed 9 /Slant 6 >> sgpath \
             [ {{ }} {{ }} {{ }} {{ }} pathforall ]"
        )
    };
    run(
        &mut it,
        &format!(
            "{} {} 2 copy length exch length eq \
             {{ true 0 1 3 index length 1 sub {{ 3 index 1 index get 3 index 3 -1 roll get \
                eq and }} for }} {{ false }} ifelse 3 1 roll pop pop",
            flat("crème brûlée, naïve Zoë"),
            flat("cr\\350me br\\373l\\351e, na\\357ve Zo\\353"),
        ),
    );
    let stack = it.operand_stack();
    assert_eq!(stack.len(), 1, "the comparison left one result");
    assert_eq!(stack[0].repr(), "true", "the two paths differ");
}

/// Ghostscript builds the same accented path from the same seed, from a
/// UTF-8 string and from a Latin-1 byte string alike.
#[test]
fn ghostscript_builds_the_same_accented_path() {
    if !gs_available() {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let body = "newpath (Łódź Ångström ĳ ď) 10 20 << /Seed 9 /Slant 8 >> sgpath \
                (caf\\351 \\337) 10 60 << /Seed 4 >> sgpath \
                { } { } { } { } pathforall";
    let probe = format!(
        "(lib/handscript.ps) run (lib/strokeglyphs.ps) run {body} \
         count {{ 1000 mul round cvi = }} repeat"
    );
    let gs = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNODISPLAY",
            "-dBATCH",
            "-dNOPAUSE",
            "-q",
            "-c",
        ])
        .arg(&probe)
        .output()
        .expect("run gs");
    assert!(gs.status.success(), "gs probe failed");
    let gs_nums: Vec<i64> = String::from_utf8_lossy(&gs.stdout)
        .lines()
        .map(|l| l.trim().parse().expect("an integer"))
        .collect();

    let mut it = Interp::new();
    load(&mut it, &["lib/handscript.ps", "lib/strokeglyphs.ps"]);
    run(
        &mut it,
        &format!("{body} count array astore {{ 1000 mul round cvi }} forall"),
    );
    let mut ours: Vec<i64> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().parse().expect("an integer"))
        .collect();
    ours.reverse();
    assert!(ours.len() > 100, "the probe path is suspiciously short");
    assert_eq!(ours.len(), gs_nums.len(), "path lengths differ");
    for (i, (a, b)) in ours.iter().zip(&gs_nums).enumerate() {
        assert!(
            (a - b).abs() <= 2,
            "coordinate {i} differs: pscat {a} vs gs {b} (thousandths)"
        );
    }
}

/// #1051 left mathkit's input UTF-8 only. mathkit slices a character's
/// bytes by its lead byte and asks `sgmetrics` for the name, so a Latin-1
/// byte must still be refused there, not read as é while the two bytes
/// after it vanish. A UTF-8 accented letter now lays out in `\text{}`.
#[test]
fn mathkit_refuses_latin1_bytes_and_draws_utf8_accents() {
    let mut it = Interp::new();
    load(
        &mut it,
        &["lib/handscript.ps", "lib/strokeglyphs.ps", "lib/mathkit.ps"],
    );
    run(
        &mut it,
        "newpath { (\\351ab) 0 0 << >> mkpath } stopped \
         { $error /command get 40 string cvs (mk-unsupported-character:) anchorsearch \
           { pop pop true } { pop false } ifelse } { false } ifelse \
         newpath (\\\\text{caf\u{e9}}) 0 0 << /Seed 1 >> mkpath currentpoint pop pop",
    );
    let stack = it.operand_stack();
    assert_eq!(stack.len(), 1, "the probe left one result");
    assert_eq!(stack[0].repr(), "true", "mathkit accepted a Latin-1 byte");
}
