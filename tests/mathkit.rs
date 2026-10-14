//! mathkit (issue #237, `lib/mathkit.ps`): TeX-subset math laid out as
//! strokeglyphs skeletons with board-writing irregularity.
//!
//! The library's own `%%SelfTest` blocks pin its guards (every
//! unsupported construct fails with a name that says what it was), its
//! TeX layout rules, determinism and the stacks after an error. These
//! tests cover what needs Rust or a second interpreter: the fixed test set
//! laid out in a headless pass with its ink inside its measured box, the
//! error text a user actually sees, the catalog examples, the specimen,
//! and Ghostscript building the same layout and path.

use pscat::Interp;

const LIBS: [&str; 3] = ["lib/handscript.ps", "lib/strokeglyphs.ps", "lib/mathkit.ps"];

/// The fixed test set (issue #237's acceptance), as the specimen draws it.
/// Each TeX backslash is doubled for the PostScript string.
const TEST_SET: [(&str, &str); 14] = [
    ("quadratic formula", r"x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}"),
    ("Euler's identity", r"e^{i\\pi} + 1 = 0"),
    (
        "Gaussian integral",
        r"\\int_{-\\infty}^{\\infty} e^{-x^2}\\,dx = \\sqrt{\\pi}",
    ),
    (
        "Gauss's law",
        r"\\nabla \\cdot \\vec{E} = \\frac{\\rho}{\\varepsilon_0}",
    ),
    (
        "Faraday's law",
        r"\\nabla \\times \\vec{E} = -\\frac{\\partial \\vec{B}}{\\partial t}",
    ),
    (
        "Ampere-Maxwell law",
        r"\\nabla \\times \\vec{B} = \\mu_0 \\vec{J} + \\mu_0 \\varepsilon_0 \\frac{\\partial \\vec{E}}{\\partial t}",
    ),
    (
        "Taylor series",
        r"f(x) = \\sum_{n=0}^{\\infty} \\frac{f^{(n)}(a)}{n!}\\,(x-a)^n",
    ),
    (
        "2x2 inverse",
        r"\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}^{-1} = \\frac{1}{ad - bc} \\begin{pmatrix} d & -b \\\\ -c & a \\end{pmatrix}",
    ),
    (
        "derivative",
        r"f'(x) = \\lim_{h \\to 0} \\frac{f(x+h) - f(x)}{h}",
    ),
    (
        "Wallis product",
        r"\\frac{\\pi}{2} = \\prod_{n=1}^{\\infty} \\frac{4n^2}{4n^2 - 1}",
    ),
    (
        "Cauchy-Schwarz",
        r"\\left( \\sum_{i=1}^n a_i b_i \\right)^2 \\le \\left( \\sum_{i=1}^n a_i^2 \\right) \\left( \\sum_{i=1}^n b_i^2 \\right)",
    ),
    (
        "Fourier transform",
        r"\\hat{f}(\\xi) = \\int_{-\\infty}^{\\infty} f(x)\\, e^{-2\\pi i x \\xi}\\,dx",
    ),
    (
        "roots and cases",
        r"\\sqrt[3]{x^3} = x \\qquad |x| = \\begin{cases} x & x \\ge 0 \\\\ -x & x < 0 \\end{cases}",
    ),
    (
        "Stirling",
        r"n! \\approx \\sqrt{2\\pi n} \\left( \\frac{n}{e} \\right)^n",
    ),
];

fn load(it: &mut Interp, files: &[&str]) {
    for f in files {
        let src = std::fs::read(f).unwrap_or_else(|_| panic!("{f} present"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{f} failed: {}", it.error_report(&e)));
    }
}

fn with_libs() -> Interp {
    let mut it = Interp::new();
    load(&mut it, &LIBS);
    it
}

fn run(it: &mut Interp, src: &str) {
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
}

/// Pops the numbers `count array astore` left on the stack.
fn numbers(it: &mut Interp) -> Vec<f64> {
    let arr = it.pop().expect("an array");
    let repr = arr.repr();
    repr.trim_matches(|c| c == '[' || c == ']')
        .split_whitespace()
        .map(|t| t.parse::<f64>().unwrap_or_else(|_| panic!("not a number: {t}")))
        .collect()
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

/// The acceptance's "layout boxes are computed in a headless pass": every
/// formula of the test set measures to a sane box with no page at all, and
/// the path mkpath builds at /Irregular 0 lies inside that box (to the
/// glyph jitter and the frame overshoot, a tenth of an em).
#[test]
fn the_fixed_test_set_lays_out_in_a_headless_pass() {
    let mut it = with_libs();
    for (name, tex) in TEST_SET {
        run(
            &mut it,
            &format!(
                "({tex}) << /Size 30 /Seed 1 /Irregular 0 >> mkmetrics \
                 dup /Width get exch dup /Height get exch /Depth get 3 array astore"
            ),
        );
        let m = numbers(&mut it);
        let (w, h, d) = (m[0], m[1], m[2]);
        assert!(w > 40.0 && w < 900.0, "{name}: width {w}");
        assert!(h > 10.0 && h < 120.0, "{name}: height {h}");
        assert!((0.0..120.0).contains(&d), "{name}: depth {d}");

        run(
            &mut it,
            &format!(
                "newpath ({tex}) 100 300 << /Size 30 /Seed 1 /Irregular 0 >> mkpath \
                 pathbbox 4 array astore newpath"
            ),
        );
        let b = numbers(&mut it);
        let tol = 3.0;
        assert!(b[0] >= 100.0 - tol, "{name}: ink starts left of the box: {b:?}");
        assert!(b[2] <= 100.0 + w + tol, "{name}: ink ends past the box: {b:?} w={w}");
        assert!(b[1] >= 300.0 - d - tol, "{name}: ink below the box: {b:?} d={d}");
        assert!(b[3] <= 300.0 + h + tol, "{name}: ink above the box: {b:?} h={h}");
    }
    assert!(it.operand_stack().is_empty(), "the test set leaked operands");
}

#[test]
fn the_layout_is_deterministic_and_seeded() {
    let path = |seed: i64, irregular: f64| {
        let mut it = with_libs();
        run(
            &mut it,
            &format!(
                "newpath ({}) 50 200 << /Seed {seed} /Irregular {irregular} >> mkpath \
                 [ {{ }} {{ }} {{ }} {{ }} pathforall ]",
                TEST_SET[0].1
            ),
        );
        numbers(&mut it)
    };
    let a = path(5, 1.0);
    assert!(a.len() > 200, "the quadratic formula's path is too short");
    assert_eq!(a, path(5, 1.0), "the same seed must give the same path");
    assert_ne!(a, path(6, 1.0), "another seed must give another path");

    // At /Irregular 0 only the glyph jitter depends on the seed; the
    // layout (what mkmetrics measures) does not.
    let width = |seed: i64| {
        let mut it = with_libs();
        run(
            &mut it,
            &format!(
                "({}) << /Seed {seed} /Irregular 0 >> mkmetrics /Width get 1 array astore",
                TEST_SET[6].1
            ),
        );
        numbers(&mut it)[0]
    };
    assert_eq!(width(1), width(99));
}

/// What a user sees when the TeX is outside the subset: an error naming
/// the construct, never a silent gap.
#[test]
fn unsupported_tex_fails_with_an_error_that_names_it() {
    for (src, expect) in [
        (r"(\\iint f) 0 0 << >> mkpath", r"mk-unsupported-command:\iint"),
        (r"(a \\star b) 0 0 << >> mkpath", r"mk-unsupported-command:\star"),
        ("(a * b) 0 0 << >> mkpath", "mk-unsupported-character:*"),
        (
            r"(\\begin{array} x \\end{array}) 0 0 << >> mkpath",
            "mk-unsupported-environment:array",
        ),
        // the commonest mistake: one backslash, eaten by the string
        (r"(\sqrt{2}) 0 0 << >> mkpath", "mk-bare-word-needs-a-backslash:sqrt"),
        (r"(\nabla f) 0 0 << >> mkpath", "mk-control-character"),
        (r"(\\frac{1}) 0 0 << >> mkpath", r"mk-missing-argument:\frac"),
    ] {
        let mut it = with_libs();
        let err = it
            .run_str(src)
            .expect_err(&format!("{src} should have failed"));
        let report = it.error_report(&err);
        assert!(
            report.contains(expect),
            "{src}: expected {expect:?} in the error, got {report}"
        );
    }
}

/// The `% @example:` lines are what `--capabilities` hands an agent to try
/// first; each must run clean.
#[test]
fn every_example_tag_runs_clean() {
    let src = std::fs::read_to_string("lib/mathkit.ps").expect("mathkit present");
    let examples: Vec<&str> = src
        .lines()
        .filter_map(|l| l.strip_prefix("% @example:"))
        .map(str::trim)
        .collect();
    assert_eq!(examples.len(), 6, "one example per public procedure");
    for example in examples {
        let mut it = Interp::with_page(300, 200).expect("page");
        it.run_str(example)
            .unwrap_or_else(|e| panic!("@example {example:?} failed: {}", it.error_report(&e)));
        assert!(ink_count(&it) > 0 || example.contains("mkmetrics"), "{example} drew nothing");
    }
}

/// The specimen draws the whole test set, an /Irregular row and a chalk
/// board through mkalign's /Ink. A guard, not a benchmark: it takes about
/// 0.7 s in a release build, and the dry-media budget for a specimen is 10 s.
#[test]
fn the_specimen_renders_with_ink_and_a_clean_stack() {
    let src = std::fs::read("examples/mathkit_specimen.ps").expect("specimen present");
    let t0 = std::time::Instant::now();
    let mut it = Interp::with_page(1000, 1960).expect("page");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("specimen failed: {}", it.error_report(&e)));
    let secs = t0.elapsed().as_secs_f64();
    assert!(it.operand_stack().is_empty(), "specimen leaked operands");
    assert!(ink_count(&it) > 30_000, "specimen drew too little ink");
    assert!(secs < 10.0, "the specimen took {secs:.1}s, over the 10s budget");
}

/// mkalign aligns its lines on `&` (or before the first relation): with no
/// irregularity, the = signs of every line start at the same x.
#[test]
fn mkalign_lines_up_the_relations() {
    let mut it = with_libs();
    run(
        &mut it,
        r"newpath [ (y &= (x+1)^2) (&= x^2 + 2x + 1) (= (x + 1)(x + 1)) ] 200 400
          << /Size 30 /Irregular 0 /Seed 2 >> mkalign
          [ exch { dup /Origin get 0 get exch /Align get add } forall ] newpath",
    );
    let xs = numbers(&mut it);
    assert_eq!(xs.len(), 3);
    for x in &xs {
        assert!((x - 200.0).abs() < 1e-6, "an alignment point moved: {xs:?}");
    }
}

/// #237's compatibility criterion: Ghostscript runs the specimen, and lays
/// out the same formula to the same box and the same path from the same
/// seed (gs computes in single precision, hence the tolerance).
#[test]
fn ghostscript_builds_the_same_layout_and_path() {
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
            "-g1000x1960",
            "-r72",
            "-o/dev/null",
            "examples/mathkit_specimen.ps",
        ])
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected examples/mathkit_specimen.ps");

    let body = format!(
        "({tex}) << /Size 30 /Seed 3 >> mkmetrics \
         dup /Width get exch dup /Height get exch /Depth get \
         newpath ({tex}) 40 200 << /Size 30 /Seed 3 >> mkpath \
         {{ }} {{ }} {{ }} {{ }} pathforall",
        tex = TEST_SET[6].1
    );
    let probe = format!(
        "(lib/handscript.ps) run (lib/strokeglyphs.ps) run (lib/mathkit.ps) run {body} \
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
    assert!(
        gs.status.success(),
        "gs probe failed: {}",
        String::from_utf8_lossy(&gs.stdout)
    );
    let gs_nums: Vec<i64> = String::from_utf8_lossy(&gs.stdout)
        .lines()
        .map(|l| l.trim().parse().expect("an integer"))
        .collect();

    let mut it = with_libs();
    run(
        &mut it,
        &format!("{body} count array astore {{ 1000 mul round cvi }} forall"),
    );
    let mut ours: Vec<i64> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().parse().expect("an integer"))
        .collect();
    // gs printed top of stack first
    ours.reverse();
    assert!(ours.len() > 300, "the probe path is suspiciously short");
    assert_eq!(ours.len(), gs_nums.len(), "path lengths differ");
    for (i, (a, b)) in ours.iter().zip(&gs_nums).enumerate() {
        assert!(
            (a - b).abs() <= 20,
            "value {i} differs: pscat {a} vs gs {b} (thousandths of a point)"
        );
    }
}
