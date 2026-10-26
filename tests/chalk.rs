//! `pscat chalk` and `pscat::chalk` (issue #555): raw LaTeX to a chalk
//! board, deterministically.
//!
//! The pure parts (row splitting, escaping, the config schema) are unit
//! tests in `src/chalk.rs`. These run the boards: pinned renders of the
//! issue's five formulas, determinism across processes, the CLI writing
//! exactly what the library returns, a clean layout at `irregular: 0`, and
//! the one-line errors for TeX outside mathkit's subset.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use pscat::Interp;
use pscat::chalk::{self, Config, Format};
use tiny_skia::Pixmap;

mod common;

const PSCAT: &str = env!("CARGO_BIN_EXE_pscat");

fn scratch(tag: &str) -> PathBuf {
    common::scratch_dir(&format!("pscat-chalk-{tag}"))
}

/// `pscat <args>` in `dir`, an empty directory outside the checkout, with
/// the environment that could point it at one removed (as
/// tests/subcommands.rs does): the libraries come from the binary.
fn pscat(dir: &Path, args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut cmd = Command::new(PSCAT);
    cmd.args(args)
        .current_dir(dir)
        .env_remove("PSCAT_HOME")
        .env_remove("PSCAT_ROOT")
        .env_remove("PSCAT_LIB")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match stdin {
        None => cmd.stdin(Stdio::null()).output().expect("run pscat"),
        Some(bytes) => {
            let mut child = cmd.stdin(Stdio::piped()).spawn().expect("spawn pscat");
            common::feed_stdin(&mut child, bytes);
            child.wait_with_output().expect("wait pscat")
        }
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn ok(out: &Output) {
    assert!(out.status.success(), "pscat failed: {}", stderr(out));
}

fn fnv1a(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// A small board, cheap enough for five renders under `--profile ci`.
fn small() -> Config {
    Config::from_json(r#"{"layout": {"width": 400, "height": 200, "dpi": 72, "size": 40}}"#)
        .unwrap()
}

/// The issue's acceptance set, as a user types it: single backslashes.
const FORMULAS: [(&str, &str, i64); 5] = [
    ("quadratic", r"x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}", 1),
    (
        "integral",
        r"\int_{-\infty}^{\infty} e^{-x^2}\,dx = \sqrt{\pi}",
        2,
    ),
    (
        "pmatrix",
        r"A^{-1} = \frac{1}{ad-bc}\begin{pmatrix} d & -b \\ -c & a \end{pmatrix}",
        3,
    ),
    (
        "cases",
        r"|x| = \begin{cases} x & x \ge 0 \\ -x & x < 0 \end{cases}",
        4,
    ),
    (
        "align",
        "\\begin{align*}\n  f(x) &= (x+1)^2 \\\\\n       &= x^2 + 2x + 1\n\\end{align*}",
        5,
    ),
];

/// Golden renders: each formula at its seed on the small board, pinned by
/// a hash of the pixels (not the PNG bytes, so an encoder change does not
/// move them). Any edit to mathkit, strokeglyphs, drychalk, drykit or the
/// driver generator that moves a pixel moves these: look at the boards
/// (`pscat chalk ... --width 400 --height 200 --dpi 72 --size 40 --seed N`),
/// and if they are right, re-pin from this test's failure message.
#[test]
fn the_five_formulas_render_as_pinned() {
    let pinned: [u64; 5] = [
        0x0ace74c7afc6678c,
        0xd956a77ea25618a3,
        0xbfbcd6c67947a409,
        // The cases board comes out a few pixels different on Windows
        // (macOS and Linux agree): its C runtime's transcendental functions
        // round differently in the last place for one glyph. Both are
        // pinned, so each platform still catches a change.
        if cfg!(windows) {
            0x947a44d3fe0c77a0
        } else {
            0xf4e60cabdd8fe447
        },
        0xd93e1e1def7fc961,
    ];
    let got: Vec<u64> = FORMULAS
        .iter()
        .map(|(name, tex, seed)| {
            let mut cfg = small();
            cfg.seed = *seed;
            let png =
                chalk::render(tex, &cfg, Format::Png).unwrap_or_else(|e| panic!("{name}: {e}"));
            let pm = Pixmap::decode_png(&png).expect("decode");
            assert_eq!((pm.width(), pm.height()), (400, 200), "{name}");
            fnv1a(pm.data())
        })
        .collect();
    assert_eq!(got, pinned, "a chalk board moved: {got:#x?}");
}

/// Same LaTeX, config and seed: the same bytes, from two separate
/// processes, and the CLI writes exactly what the library returns.
#[test]
fn boards_are_byte_identical_across_runs_and_callers() {
    let dir = scratch("determinism");
    let (_, tex, _) = FORMULAS[0];
    let args = |out: &str| {
        vec![
            "chalk", tex, "-o", out, "--width", "400", "--height", "200", "--dpi", "72", "--size",
            "40", "--seed", "1",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>()
    };
    for out in ["a.png", "b.png"] {
        let a = args(out);
        let a: Vec<&str> = a.iter().map(String::as_str).collect();
        ok(&pscat(&dir, &a, None));
    }
    let a = std::fs::read(dir.join("a.png")).unwrap();
    let b = std::fs::read(dir.join("b.png")).unwrap();
    assert!(a == b, "two runs of the same board differ");
    let mut cfg = small();
    cfg.seed = 1;
    let lib = chalk::render(tex, &cfg, Format::Png).unwrap();
    assert!(a == lib, "the CLI's board is not the library's");

    // a different seed is a different board
    let mut other = cfg.clone();
    other.seed = 2;
    assert!(chalk::render(tex, &other, Format::Png).unwrap() != lib);
}

/// The measuring half of a driver (everything before the board), then the
/// line records mkalign returns for the same lines and options: each
/// line's angle and the x of its alignment point.
fn records(tex: &str, cfg: &Config) -> Vec<(f64, f64, f64)> {
    let d = chalk::driver(tex, cfg).unwrap();
    let cut = d.source.find("(lib/artkit.ps) run").expect("board half");
    let mut src = d.source[..cut].to_string();
    src.push_str(
        "ckdict begin newpath cklines ckox ckoy ckopts mkalign end
         { dup /Angle get = dup /Origin get 0 get exch /Align get add = 0 = } forall",
    );
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(&src)
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let nums: Vec<f64> = String::from_utf8_lossy(it.captured_stdout())
        .split_whitespace()
        .map(|t| t.parse().unwrap())
        .collect();
    nums.chunks(3).map(|c| (c[0], c[1], c[2])).collect()
}

/// `irregular: 0` is TeX's clean layout: no line tilts, and the rows of an
/// align share one alignment column. The default hand tilts them.
#[test]
fn irregular_zero_is_a_clean_untilted_layout() {
    let (_, tex, _) = FORMULAS[4];
    let mut clean = small();
    clean.irregular = 0.0;
    let recs = records(tex, &clean);
    assert_eq!(recs.len(), 2);
    for (angle, _, _) in &recs {
        assert_eq!(*angle, 0.0, "a clean line tilts: {recs:?}");
    }
    assert!(
        (recs[0].1 - recs[1].1).abs() < 1e-6,
        "the = column wanders: {recs:?}"
    );
    let hand = records(tex, &small());
    assert!(
        hand.iter().any(|(a, _, _)| *a != 0.0),
        "the default hand never tilts: {hand:?}"
    );
}

/// LaTeX centres each `gather` row on its own width (issue #699); `align`
/// keeps its rows on one relation column, and its driver never asks for the
/// centring.
#[test]
fn gather_rows_are_centred_and_align_is_not() {
    let rows = r"a = b \\ ccc + dd = eeee \\ f";
    let gather = format!("\\begin{{gather*}} {rows} \\end{{gather*}}");
    let align = format!("\\begin{{align*}} {rows} \\end{{align*}}");
    let mut clean = small();
    clean.irregular = 0.0;

    let recs = records(&gather, &clean);
    assert_eq!(recs.len(), 3);
    for r in &recs {
        assert!(
            (r.1 - recs[0].1).abs() < 1e-6,
            "a gather row is off the centre line: {recs:?}"
        );
    }
    // the rows have different widths, so a shared centre means ragged edges
    let starts = records(&align, &clean);
    assert!(
        (starts[0].1 - starts[1].1).abs() < 1e-6,
        "align lost its relation column: {starts:?}"
    );

    let has = |tex: &str| {
        chalk::driver(tex, &clean)
            .unwrap()
            .source
            .contains("/LineAlign /center")
    };
    assert!(has(&gather));
    assert!(!has(&align));
    assert!(!has(rows));
    // one row is centred by the block already: its driver is the plain one
    assert!(!has(r"\begin{gather} a = b \end{gather}"));
}

/// The writing fits the board: a long formula is shrunk to stay inside
/// the margins, and a short one is centred.
#[test]
fn the_block_is_fitted_inside_the_margins() {
    let cfg = small();
    let long = r"f(x) = a_0 + a_1 x + a_2 x^2 + a_3 x^3 + a_4 x^4 + a_5 x^5 + a_6 x^6 + \cdots";
    let d = chalk::driver(long, &cfg).unwrap();
    let cut = d.source.find("(lib/artkit.ps) run").unwrap();
    let mut src = d.source[..cut].to_string();
    src.push_str("ckdict begin cksize = ckox ckx0 add = ckox ckx1 add = end");
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(&src).unwrap();
    let v: Vec<f64> = String::from_utf8_lossy(it.captured_stdout())
        .split_whitespace()
        .map(|t| t.parse().unwrap())
        .collect();
    assert!(v[0] < 40.0, "not shrunk: size {}", v[0]);
    assert!(
        v[1] >= cfg.margin - 0.5,
        "left edge {} past the margin",
        v[1]
    );
    assert!(
        v[2] <= f64::from(cfg.width) - cfg.margin + 0.5,
        "right edge {} past the margin",
        v[2]
    );
}

/// The ghost writing never touches the formula (issue #698): the driver is
/// run up to the board, which is when the ghosts are placed, and each
/// placed ghost's real path box is compared with the formula's box. The
/// board keeps a margin of 0.6 of the writing size around the maths, so a
/// faint fragment can never pass for a superscript. Many seeds on formulas
/// that fill the board and ones that do not; no render.
#[test]
fn ghost_writing_keeps_clear_of_the_formula() {
    let cases: [(&str, &str, u32, u32, f64); 5] = [
        ("x = 1", "x = 1", 800, 400, 64.0),
        ("F = ma", "F = ma", 800, 400, 64.0),
        ("f(x)", r"f(x) = \int_0^1 g(t)\,dt", 800, 400, 64.0),
        (
            "quadratic",
            r"x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}",
            600,
            300,
            64.0,
        ),
        ("small", r"e^{i\pi} + 1 = 0", 400, 200, 40.0),
    ];
    let mut placed_total = 0;
    for (name, tex, w, h, size) in cases {
        for seed in 1..=12 {
            let mut cfg = small();
            cfg.width = w;
            cfg.height = h;
            cfg.size = size;
            cfg.seed = seed;
            let d = chalk::driver(tex, &cfg).unwrap();
            let end = d.source.find("dcboard\n").expect("the board line");
            let cut = d.source[..end].rfind('\n').expect("a line before it") + 1;
            let mut src = d.source[..cut].to_string();
            src.push_str(
                "ckdict begin
                 cksize =
                 gsave newpath cklines ckox ckoy ckopts mkalign pop pathbbox grestore
                 4 { = } repeat
                 ckgplaced {
                   aload pop /ckgsd exch def /ckgsz exch def
                   gsave newpath << /Size ckgsz /Seed ckgsd /Irregular 1.3 >> mkpath
                   pathbbox grestore 4 { = } repeat
                 } forall end",
            );
            let mut it = Interp::new();
            it.capture_stdout();
            it.run_str(&src)
                .unwrap_or_else(|e| panic!("{name} {seed}: {}", it.error_report(&e)));
            let v: Vec<f64> = String::from_utf8_lossy(it.captured_stdout())
                .split_whitespace()
                .map(|t| t.parse().unwrap())
                .collect();
            // the writing's path box, printed from the top of the stack down
            let (cksize, f) = (v[0], [v[4], v[2], v[3], v[1]]);
            for g in v[5..].chunks(4) {
                // `=` printed the bbox from the top of the stack down
                let (ury, urx, lly, llx) = (g[0], g[1], g[2], g[3]);
                let gap_x = (f[0] - urx).max(llx - f[1]).max(0.0);
                let gap_y = (f[2] - ury).max(lly - f[3]).max(0.0);
                let gap = gap_x.max(gap_y);
                assert!(
                    gap >= 0.6 * cksize,
                    "{name} seed {seed}: a ghost at [{llx} {lly} {urx} {ury}] is {gap:.1} from \
                     the formula {f:?} (writing size {cksize})"
                );
                placed_total += 1;
            }
        }
    }
    // the board is still used: most boards keep some ghosts
    assert!(
        placed_total >= 40,
        "ghosts all but vanished: {placed_total}"
    );
}

/// Writing with no ink (a bare space) has no path to measure: the ghosts
/// fall back to the line boxes, and the board still renders (issue #698).
#[test]
fn a_formula_with_no_ink_still_gets_a_board() {
    for tex in [r"\quad", "~", r"\,"] {
        chalk::render(tex, &small(), Format::Png).unwrap_or_else(|e| panic!("{tex}: {e}"));
    }
}

/// TeX outside the subset fails with one line naming the command or
/// character, exits non-zero and writes no file.
#[test]
fn unsupported_input_fails_with_the_named_error() {
    let dir = scratch("errors");
    for (tex, says, code) in [
        (
            r"a \oplus b",
            r"unsupported LaTeX command \oplus",
            "unsupported_command",
        ),
        ("a @ b", "unsupported character: @", "unsupported_character"),
        (
            r"\(a\) + \(b",
            r"math delimiter \( is never closed: end it with \)",
            "unclosed_math_delimiter",
        ),
        (
            r"a \left\( b \right\)",
            "unsupported delimiter: \\(",
            "unsupported_delimiter",
        ),
        (
            r"1 + 2\)",
            r"math delimiter \) closes nothing",
            "unmatched_math_delimiter",
        ),
        (
            r"\(x\) where \(y\)",
            "text outside the math delimiters (w",
            "text_outside_math",
        ),
    ] {
        let out = pscat(&dir, &["chalk", tex, "-o", "bad.png"], None);
        assert!(!out.status.success(), "{tex} succeeded");
        let err = stderr(&out);
        assert!(err.contains(says), "{tex}: {err}");
        assert_eq!(err.trim().lines().count(), 1, "{tex}: not one line: {err}");
        assert!(
            !err.contains("%%[ Error"),
            "{tex}: a PostScript report: {err}"
        );
        assert!(!dir.join("bad.png").exists(), "{tex}: wrote a file");

        let e = chalk::render(tex, &small(), Format::Png).unwrap_err();
        assert_eq!(e.code(), code);
    }
}

/// Separate inline pairs (the issue's `\(a\) + \(b\)`, in all four delimiter
/// forms) are one formula with the delimiters gone: the same board as the
/// bare formula, not a mathkit error about a stray `\)`.
#[test]
fn separate_inline_pairs_render_as_one_formula() {
    let bare = chalk::render("a + b", &small(), Format::Png).unwrap();
    for tex in [
        r"\(a\) + \(b\)",
        r"\[a\] + \[b\]",
        "$a$ + $b$",
        "$$a$$ + $$b$$",
    ] {
        let png = chalk::render(tex, &small(), Format::Png).unwrap();
        assert_eq!(png, bare, "{tex}: not the board of `a + b`");
    }
}

/// Single-backslash LaTeX works end to end: the commands whose first
/// letter a PostScript string would eat as an escape (\n \t \b \r \f)
/// reach mathkit whole.
#[test]
fn single_backslash_latex_works_end_to_end() {
    let dir = scratch("backslash");
    let tex = r"\nabla \theta + \beta \rho = \frac{1}{2}";
    let out = pscat(
        &dir,
        &[
            "chalk", tex, "-o", "ok.png", "--width", "400", "--height", "200", "--dpi", "72",
        ],
        None,
    );
    ok(&out);
    assert!(dir.join("ok.png").exists());
    let ps = pscat(&dir, &["chalk", tex, "--print-ps"], None);
    ok(&ps);
    let src = String::from_utf8_lossy(&ps.stdout);
    assert!(
        src.contains(r"(\\nabla \\theta + \\beta \\rho = \\frac{1}{2})"),
        "{src}"
    );
}

/// The LaTeX can come from stdin (`-`) or a file, comments and all, and
/// the config from a file, overridden by flags.
#[test]
fn input_and_config_come_from_files_and_stdin() {
    let dir = scratch("inputs");
    std::fs::write(dir.join("eq.tex"), "% the identity\ne^{i\\pi} + 1 = 0\n").unwrap();
    std::fs::write(
        dir.join("c.json"),
        r#"{"layout": {"width": 300, "height": 150, "dpi": 72}, "chalk": {"color": "pink"}}"#,
    )
    .unwrap();
    let from_file = pscat(
        &dir,
        &[
            "chalk",
            "--file",
            "eq.tex",
            "--config",
            "c.json",
            "--seed",
            "3",
            "--print-ps",
        ],
        None,
    );
    ok(&from_file);
    let from_stdin = pscat(
        &dir,
        &[
            "chalk",
            "-",
            "--config",
            "c.json",
            "--seed",
            "3",
            "--print-ps",
        ],
        Some(b"% the identity\ne^{i\\pi} + 1 = 0\n"),
    );
    ok(&from_stdin);
    assert_eq!(from_file.stdout, from_stdin.stdout);
    let src = String::from_utf8_lossy(&from_file.stdout);
    assert!(src.contains("%%BoundingBox: 0 0 300 150"), "{src}");
    assert!(src.contains("/pink dcchalkcolor"), "{src}");
    assert!(src.contains("/Seed 3 "), "{src}");

    let bad = pscat(
        &dir,
        &[
            "chalk",
            "x",
            "--json",
            r#"{"chalk": {"colour": 1}}"#,
            "-o",
            "x.png",
        ],
        None,
    );
    assert!(!bad.status.success());
    assert!(
        stderr(&bad).contains("unknown key chalk.colour"),
        "{}",
        stderr(&bad)
    );
    let none = pscat(&dir, &["chalk", "-o", "x.png"], None);
    assert!(!none.status.success());
    assert!(
        stderr(&none).contains("no LaTeX given"),
        "{}",
        stderr(&none)
    );
}

/// `board: false` is chalk alone: a transparent page with the marks in the
/// stick's colour.
#[test]
fn no_board_is_chalk_on_a_transparent_page() {
    let cfg = Config::from_json(
        r#"{"board": false, "layout": {"width": 300, "height": 150, "dpi": 72, "size": 40}}"#,
    )
    .unwrap();
    let png = chalk::render("x = 1", &cfg, Format::Png).unwrap();
    let pm = Pixmap::decode_png(&png).unwrap();
    assert_eq!(pm.pixel(0, 0).unwrap().alpha(), 0, "the corner is painted");
    let inked: Vec<_> = pm.pixels().iter().filter(|p| p.alpha() > 0).collect();
    assert!(inked.len() > 200, "no chalk: {} inked pixels", inked.len());
    // every mark is the stick's own colour (white chalk: grey-free once
    // the coverage is divided out), never a dark board tone
    for p in &inked {
        let c = p.demultiply();
        assert!(c.red() > 200 && c.green() > 200, "a dark mark: {c:?}");
    }
}
