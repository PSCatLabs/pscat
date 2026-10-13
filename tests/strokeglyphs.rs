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

    let probe = "(lib/handscript.ps) run (lib/strokeglyphs.ps) run \
                 newpath (Aσ≤∑x) 10 20 << /Seed 9 /Slant 8 >> sgpath \
                 40 60 moveto 50 20 << /Symbol /braceleft /Seed 3 >> sggrow pop \
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
        "newpath (Aσ≤∑x) 10 20 << /Seed 9 /Slant 8 >> sgpath \
         40 60 moveto 50 20 << /Symbol /braceleft /Seed 3 >> sggrow pop \
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
