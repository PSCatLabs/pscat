//! CLI contract tests (Stage 14): the behaviors scripts and agents
//! rely on — `-` reads stdin, exit codes are honest, artifacts are
//! announced on stdout and errors go to stderr.

use std::process::{Command, Stdio};

use tiny_skia::Pixmap;

mod common;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

fn tmp(name: &str) -> std::path::PathBuf {
    common::scratch_dir("pscat-cli").join(name)
}

/// Run pscat with `args`, feeding `stdin`, returning (status, stdout, stderr).
fn run(args: &[&str], stdin: &str) -> (bool, String, String) {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pscat");
    common::feed_stdin(&mut child, stdin.as_bytes());
    let out = child.wait_with_output().expect("wait");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn dash_reads_the_program_from_stdin() {
    let png = tmp("stdin.png");
    let (ok, stdout, stderr) = run(
        &["--page", "50x50", "--png", png.to_str().unwrap(), "-"],
        "0 0 50 50 rectfill",
    );
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("wrote"), "artifact announced: {stdout}");
    let bytes = std::fs::read(&png).expect("png written");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn a_postscript_error_fails_the_exit_code() {
    let (ok, _out, stderr) = run(&["--headless", "-"], "1 0 div");
    assert!(!ok, "undefinedresult must exit nonzero");
    assert!(
        stderr.contains("undefinedresult"),
        "error named on stderr: {stderr}"
    );
}

#[test]
fn eval_prints_on_stdout_and_exits_clean() {
    let (ok, stdout, stderr) = run(&["-e", "3 4 add ="], "");
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains('7'), "= output on stdout: {stdout}");
}

#[test]
fn stdin_with_interactive_is_rejected() {
    let (ok, _out, stderr) = run(&["--interactive", "-"], "");
    assert!(!ok);
    assert!(stderr.contains("stdin"), "explains the conflict: {stderr}");
}

#[test]
fn lint_flags_an_unbalanced_gsave() {
    let (ok, _out, stderr) = run(&["--lint", "-"], "gsave");
    assert!(ok, "a lint finding isn't a program failure: {stderr}");
    assert!(
        stderr.contains("pscat: lint: [gsave-imbalance]"),
        "stderr: {stderr}"
    );
}

#[test]
fn lint_is_clean_when_nothing_is_wrong() {
    let png = tmp("lint-clean.png");
    let (ok, _out, stderr) = run(
        &[
            "--page",
            "40x40",
            "--png",
            png.to_str().unwrap(),
            "--lint",
            "-",
        ],
        "0 0 40 40 rectfill",
    );
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("pscat: lint: clean"), "stderr: {stderr}");
}

// --- issue #369: an errored run is never "clean" ---------------------

#[test]
fn lint_does_not_call_an_errored_run_clean() {
    // `pop` on an empty stack: the report said stackunderflow and the
    // very next line said `lint: clean`, which agents trust.
    for args in [
        vec!["--lint", "-e", "pop"],
        vec!["--lint", "-"],
        vec!["--lint-strict", "-e", "pop"],
    ] {
        let (ok, _out, stderr) = run(&args, "pop");
        assert!(!ok, "{args:?}: an errored run exits non-zero: {stderr}");
        assert!(stderr.contains("stackunderflow"), "{args:?}: {stderr}");
        assert!(
            stderr.contains("pscat: lint: [run-error]"),
            "{args:?}: {stderr}"
        );
        assert!(
            !stderr.contains("pscat: lint: clean"),
            "{args:?}: must not say clean: {stderr}"
        );
    }
}

#[test]
fn lint_strict_footer_counts_real_findings_not_the_run_error() {
    // Error only: no real finding, so no "failing because --lint-strict".
    let (ok, _out, stderr) = run(&["--lint-strict", "-e", "pop"], "");
    assert!(!ok);
    assert!(!stderr.contains("because --lint-strict"), "{stderr}");
    // Error plus a real finding (the blank page): footer says 1, not 2.
    let (ok, _out, stderr) = run(&["--lint-strict", "-"], "pop\nshowpage");
    assert!(!ok);
    assert!(stderr.contains("pscat: lint: [blank-page]"), "{stderr}");
    assert!(
        stderr.contains("pscat: lint: 1 finding(s) \u{2014} failing because --lint-strict"),
        "{stderr}"
    );
}

#[test]
fn lint_after_a_successful_run_is_still_clean_and_exits_zero() {
    let (ok, _out, stderr) = run(&["--lint", "-e", "3 4 add pop"], "");
    assert!(ok, "{stderr}");
    assert!(stderr.contains("pscat: lint: clean"), "{stderr}");
    assert!(!stderr.contains("[run-error]"), "{stderr}");
}

#[test]
fn lint_skips_the_blank_page_check_for_eval_style_usage() {
    // -e is the "leave a result on the stack, don't necessarily draw
    // anything" exception -- an untouched canvas isn't a mistake there.
    let (ok, _out, stderr) = run(&["--lint", "-e", "3 4 add pop"], "");
    assert!(ok, "stderr: {stderr}");
    assert!(!stderr.contains("blank-page"), "stderr: {stderr}");
}

#[test]
fn lint_flags_a_blank_page_on_a_plain_file_run_with_no_output_format() {
    // Regression test (Codex review round 2, PR #59): render_checks
    // used to key off whether --png/--svg/--pdf was given, so a plain
    // `pscat --lint file.ps` (a real program run, just not asked to
    // save an artifact) silently skipped blank-page/stack-leak. Only
    // -e should skip them.
    let (ok, _out, stderr) = run(&["--lint", "-"], "showpage");
    assert!(ok, "stderr: {stderr}");
    assert!(
        stderr.contains("pscat: lint: [blank-page]"),
        "stderr: {stderr}"
    );
}

#[test]
fn lint_flags_a_blank_page_for_eval_style_usage_when_an_output_format_is_also_requested() {
    // Regression test (Codex review round 7, PR #59): render_checks
    // gated purely on `eval.is_none()`, so `-e` paired with an
    // explicit --png/--svg/--pdf -- a real render request, not just a
    // "leave a result on the stack" calculator snippet -- silently
    // skipped blank-page/stack-leak too. An output format flag means
    // rendering was actually asked for, regardless of the source.
    let png = tmp("lint-eval-blank.png");
    let (ok, _out, stderr) = run(
        &[
            "--page",
            "40x40",
            "--png",
            png.to_str().unwrap(),
            "--lint",
            "-e",
            "showpage",
        ],
        "",
    );
    assert!(ok, "stderr: {stderr}");
    assert!(
        stderr.contains("pscat: lint: [blank-page]"),
        "stderr: {stderr}"
    );
}

#[test]
fn lint_flags_a_blank_page_when_an_output_format_is_requested() {
    let png = tmp("lint-blank.png");
    let (ok, _out, stderr) = run(
        &[
            "--page",
            "40x40",
            "--png",
            png.to_str().unwrap(),
            "--lint",
            "-",
        ],
        "showpage",
    );
    assert!(ok, "stderr: {stderr}");
    assert!(
        stderr.contains("pscat: lint: [blank-page]"),
        "stderr: {stderr}"
    );
}

#[test]
fn lint_is_clean_on_real_example_and_gallery_pieces() {
    // Toy programs can't catch what a real, library-loading piece
    // does — confirmed the hard way: the first version of this feature
    // passed every unit test while --lint against these exact files
    // found two real, previously undetected bugs (a `search` "not
    // found" leak in artkit's tfdrawline /justify, and a missing `and`
    // in etching.ps's et-hatch bounds check) — both fixed alongside
    // this test. Regression coverage for "the lint pass itself doesn't
    // cry wolf on ordinary library usage."
    for path in [
        "examples/paragraph_layout.ps",
        "examples/etching_demo.ps",
        "examples/sweep_demo.ps",
        "gallery/compositors_proof.ps",
        // The three committed files carrying a `%%Pages:` header, so
        // the page-count check (issue #95) is exercised against the
        // real corpus rather than only against drivers — a header that
        // drifts out of step with the pages actually rendered must
        // fail here, not surface as a mystery finding in pscat-mcp's
        // output on someone's file.
        "examples/font_catalog.ps",
        "examples/international_text.ps",
        "examples/pdf_document.ps",
    ] {
        let png = tmp(&format!("lint-corpus-{}.png", path.replace('/', "_")));
        let (ok, _out, stderr) = run(&["--png", png.to_str().unwrap(), "--lint", path], "");
        assert!(ok, "{path} stderr: {stderr}");
        assert!(
            stderr.contains("pscat: lint: clean"),
            "{path} should lint clean, stderr: {stderr}"
        );
    }
}

#[test]
fn error_report_includes_a_line_number() {
    let (ok, _out, stderr) = run(&["--headless", "-"], "1 0 div\n");
    assert!(!ok);
    assert!(stderr.contains("Line: 1"), "stderr: {stderr}");
}

// --- issue #21: sweep / contact-sheet ---------------------------------

/// The one test that catches "the override never fires": a script that
/// hardcodes its own `N srand` still produces genuinely different
/// pixels per swept seed, because `--sweep-seed` overrides `srand`'s
/// operand rather than requiring the source to cooperate.
#[test]
fn sweep_seed_produces_different_frames_from_a_hardcoded_srand() {
    let out = tmp("sweep-differ.png");
    let source = "7 srand 1 1 1 setrgbcolor 0 0 20 20 rectfill \
                  rand 255 mod 255 div rand 255 mod 255 div rand 255 mod 255 div setrgbcolor \
                  0 0 20 20 rectfill";
    let (ok, _out_text, stderr) = run(
        &[
            "--page",
            "20x20",
            "--sweep-seed",
            "1,2",
            "--png",
            out.to_str().unwrap(),
            "-",
        ],
        source,
    );
    assert!(ok, "stderr: {stderr}");
    let a = std::fs::read(out.with_file_name("sweep-differ-001.png")).unwrap();
    let b = std::fs::read(out.with_file_name("sweep-differ-002.png")).unwrap();
    assert_ne!(a, b, "different seeds must render different pixels");
}

/// The reproducible-art doctrine, through a sweep: the same seed
/// listed twice must render byte-identical frames.
#[test]
fn sweep_seed_repeated_value_is_byte_identical() {
    let out = tmp("sweep-same.png");
    let source = "7 srand rand 255 mod 255 div rand 255 mod 255 div rand 255 mod 255 div setrgbcolor \
         0 0 20 20 rectfill";
    let (ok, _out_text, stderr) = run(
        &[
            "--page",
            "20x20",
            "--sweep-seed",
            "5,5",
            "--png",
            out.to_str().unwrap(),
            "-",
        ],
        source,
    );
    assert!(ok, "stderr: {stderr}");
    let a = std::fs::read(out.with_file_name("sweep-same-001.png")).unwrap();
    let b = std::fs::read(out.with_file_name("sweep-same-002.png")).unwrap();
    assert_eq!(a, b, "same seed twice must render byte-identical frames");
}

#[test]
fn sweep_seed_warns_when_source_never_calls_srand() {
    let out = tmp("sweep-no-srand.png");
    let (ok, _out_text, stderr) = run(
        &[
            "--page",
            "20x20",
            "--sweep-seed",
            "1,2",
            "--png",
            out.to_str().unwrap(),
            "-",
        ],
        "0 0 20 20 rectfill",
    );
    assert!(ok, "a missing srand call isn't a program failure: {stderr}");
    assert!(stderr.contains("never called srand"), "stderr: {stderr}");
}

/// Issue #202: a seeded library call saves the caller's stream with
/// `rrand` and restores it with `srand`. A sweep forces every ordinary
/// `srand`, but the restore must put the saved stream back, or a piece
/// that replays its own random draws (Alpine Lake's reflection) stops
/// matching itself. Here the "replay" after a restore must draw the same
/// colour as the original, for every swept seed.
#[test]
fn sweep_seed_leaves_an_rrand_restore_alone() {
    let out = tmp("sweep-restore.png");
    let source = "7 srand rand pop /saved rrand def \
         rand 255 mod 255 div rand 255 mod 255 div rand 255 mod 255 div setrgbcolor \
         0 0 10 20 rectfill \
         saved srand \
         rand 255 mod 255 div rand 255 mod 255 div rand 255 mod 255 div setrgbcolor \
         10 0 10 20 rectfill";
    let (ok, _out_text, stderr) = run(
        &[
            "--page",
            "20x20",
            "--sweep-seed",
            "1,2",
            "--png",
            out.to_str().unwrap(),
            "-",
        ],
        source,
    );
    assert!(ok, "stderr: {stderr}");
    for frame in ["sweep-restore-001.png", "sweep-restore-002.png"] {
        let pixmap = Pixmap::load_png(out.with_file_name(frame)).expect("decode frame");
        let px = |x: u32| -> [u8; 4] {
            let i = ((10 * pixmap.width() + x) * 4) as usize;
            pixmap.data()[i..i + 4].try_into().unwrap()
        };
        assert_eq!(px(5), px(15), "{frame}: the restored stream must replay");
    }
}

/// The generic named-parameter sweep, and that a contact sheet lays
/// cells out left-to-right in sweep order: three frames each painted
/// a distinct solid color keyed off the swept value, probed at each
/// cell's center after compositing.
#[test]
fn sweep_param_orders_contact_sheet_cells_left_to_right() {
    let sheet = tmp("sweep-hue-sheet.png");
    let source = "/Shade where { pop Shade } { 0 } ifelse dup dup setrgbcolor 0 0 10 10 rectfill";
    let (ok, _out_text, stderr) = run(
        &[
            "--page",
            "10x10",
            "--sweep",
            "Shade=0,0.5,1",
            "--contact-sheet",
            sheet.to_str().unwrap(),
            "--grid",
            "3x1",
            "-",
        ],
        source,
    );
    assert!(ok, "stderr: {stderr}");
    let pixmap = Pixmap::load_png(&sheet).expect("decode contact sheet");
    // 3 cells of 10px + 2 gaps of 4px (contact_sheet::GAP) = 38.
    assert_eq!(pixmap.width(), 38);
    let sample = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * pixmap.width() + x) * 4) as usize;
        pixmap.data()[i..i + 4].try_into().unwrap()
    };
    let cell0 = sample(5, 5);
    let cell1 = sample(19, 5); // 10 + 4(gap) + 5
    let cell2 = sample(33, 5); // 20 + 8(gap) + 5
    assert_eq!(cell0, [0, 0, 0, 255], "Shade=0 -> black");
    assert_eq!(cell2, [255, 255, 255, 255], "Shade=1 -> white");
    assert!(
        cell1[0] == cell1[1] && cell1[1] == cell1[2] && cell1[0] > 0 && cell1[0] < 255,
        "Shade=0.5 -> a mid gray strictly between black and white, got {cell1:?}"
    );
}

/// No `--grid` given: the default layout is `ceil(sqrt(n))` columns.
/// 3 frames -> 2 columns, 2 rows (one cell blank).
#[test]
fn contact_sheet_default_grid_layout_is_square_ish() {
    let sheet = tmp("sweep-default-grid.png");
    let (ok, _out, stderr) = run(
        &[
            "--page",
            "10x10",
            "--sweep-seed",
            "1,2,3",
            "--contact-sheet",
            sheet.to_str().unwrap(),
            "-",
        ],
        "0.4 0.4 0.4 setrgbcolor 0 0 10 10 rectfill",
    );
    assert!(ok, "stderr: {stderr}");
    let pixmap = Pixmap::load_png(&sheet).expect("decode contact sheet");
    // 2 cols x 2 rows of 10px cells + 1 gap (4px, contact_sheet::GAP).
    assert_eq!(pixmap.width(), 24);
    assert_eq!(pixmap.height(), 24);
}

#[test]
fn sweep_requires_an_output_flag() {
    let (ok, _out, stderr) = run(&["--sweep-seed", "1,2", "-"], "");
    assert!(!ok);
    assert!(stderr.contains("--png and/or --contact-sheet"), "{stderr}");
}

#[test]
fn sweep_rejects_incompatible_flags() {
    let (ok, _out, stderr) = run(&["--sweep-seed", "1,2", "--svg", "out.svg", "-"], "");
    assert!(!ok);
    assert!(stderr.contains("--svg"), "{stderr}");
}

#[test]
fn sweep_rejects_both_axes_at_once() {
    let (ok, _out, stderr) = run(
        &[
            "--sweep-seed",
            "1,2",
            "--sweep",
            "Foo=1,2",
            "--png",
            "out.png",
            "-",
        ],
        "",
    );
    assert!(!ok);
    assert!(stderr.contains("mutually exclusive"), "{stderr}");
}

#[test]
fn contact_sheet_grid_too_small_errors() {
    let sheet = tmp("sweep-grid-too-small.png");
    let (ok, _out, stderr) = run(
        &[
            "--sweep-seed",
            "1,2,3",
            "--contact-sheet",
            sheet.to_str().unwrap(),
            "--grid",
            "1x1",
            "-",
        ],
        "0 0 10 10 rectfill",
    );
    assert!(!ok);
    assert!(stderr.contains("--grid 1x1"), "{stderr}");
}

/// A fat-fingered `--grid` must fail cleanly at parse time, not
/// overflow the `cols * rows` multiply downstream (regression: an
/// advisor pass caught this could panic in a debug build before the
/// per-axis cap was added).
#[test]
fn contact_sheet_grid_over_the_cap_errors_at_parse_time() {
    let (ok, _out, stderr) = run(
        &[
            "--sweep-seed",
            "1,2",
            "--png",
            "out.png",
            "--grid",
            "70000x70000",
            "-",
        ],
        "",
    );
    assert!(!ok);
    assert!(stderr.contains("--grid dimensions must be"), "{stderr}");
}

/// Regression (cross-model review round 2, PR #66): `--grid` with a
/// sweep active but no `--contact-sheet` used to validate cleanly and
/// then silently do nothing -- nothing ever reads `--grid` without a
/// sheet to lay out.
#[test]
fn grid_without_contact_sheet_errors() {
    let (ok, _out, stderr) = run(
        &[
            "--sweep-seed",
            "1,2",
            "--png",
            "out.png",
            "--grid",
            "2x1",
            "-",
        ],
        "",
    );
    assert!(!ok);
    assert!(stderr.contains("--grid needs --contact-sheet"), "{stderr}");
}

/// A frame that errors doesn't abort the sweep: later frames still
/// render, and the partial canvas from the failed frame is still
/// written (this CLI's existing partial-render-on-error philosophy),
/// but the process exits nonzero so a caller knows something failed.
#[test]
fn sweep_frame_error_continues_the_sweep_and_reports_nonzero() {
    let out = tmp("sweep-error.png");
    let source = "/Div where { pop Div } { 1 } ifelse 10 exch div pop 0 0 10 10 rectfill";
    let (ok, _out_text, stderr) = run(
        &[
            "--page",
            "10x10",
            "--sweep",
            "Div=0,1",
            "--png",
            out.to_str().unwrap(),
            "-",
        ],
        source,
    );
    assert!(!ok, "a failed frame must exit nonzero");
    assert!(
        stderr.contains("frame 1/2") && stderr.contains("undefinedresult"),
        "stderr: {stderr}"
    );
    assert!(
        out.with_file_name("sweep-error-001.png").exists(),
        "the failed frame's partial canvas is still written"
    );
    assert!(out.with_file_name("sweep-error-002.png").exists());
}

/// Regression (cross-model review, PR #66): `--pstack-on-error` was
/// silently ignored for a failed sweep frame, even though the flag is
/// accepted and works for a normal (non-sweep) headless run.
#[test]
fn sweep_frame_error_honors_pstack_on_error() {
    let out = tmp("sweep-pstack.png");
    let source = "/Div where { pop Div } { 1 } ifelse 10 exch div pop 0 0 10 10 rectfill";
    let (ok, _out_text, stderr) = run(
        &[
            "--page",
            "10x10",
            "--pstack-on-error",
            "--sweep",
            "Div=0",
            "--png",
            out.to_str().unwrap(),
            "-",
        ],
        source,
    );
    assert!(!ok);
    assert!(
        stderr.contains("Operand stack"),
        "pstack printed for the failed frame: {stderr}"
    );
}

/// The error dump for an ordinary stack is unchanged by the #703 bounds.
#[test]
fn pstack_dump_small_output_is_pinned() {
    let (ok, _out, stderr) = run(
        &["--headless", "--pstack-on-error", "-"],
        "1 (a\n) /n [1 2 {3}] null mark << >> /add load 1 0 div",
    );
    assert!(!ok);
    assert_eq!(
        stderr,
        "%%[ Error: undefinedresult; OffendingCommand: div; Line: 2 ]%%\n\
         Operand stack (8): 1 (a\\n) /n [1 2 {3}] null -mark- -dict- --add--\n",
    );
}

/// #703: a huge or deeply nested operand must not flood the error dump.
/// Each case prints a few KB, keeps the error line first and marks the cut.
#[test]
fn pstack_dump_bounds_huge_operands() {
    let cases: [(&str, String); 5] = [
        ("1M-element array", "1000000 array foo".to_string()),
        ("100k-byte string", format!("({}) foo", "a".repeat(100_000))),
        (
            "10k-deep nesting",
            format!("{}{} foo", "[".repeat(10_000), "]".repeat(10_000)),
        ),
        (
            "shared references (2^40 leaves)",
            "/a [0] def 40 { [a a] /a exch def } repeat a foo".to_string(),
        ),
        ("1M-item stack", "1000000 { 1 } repeat foo".to_string()),
    ];
    for (label, source) in &cases {
        let (ok, _out, stderr) = run(&["--headless", "--pstack-on-error", "-"], source);
        assert!(!ok, "{label} fails as written");
        assert!(
            stderr.starts_with("%%[ Error: undefined"),
            "{label}: the error line comes first"
        );
        assert!(
            stderr.len() < 16 * 1024,
            "{label}: dump is {} bytes",
            stderr.len()
        );
        assert!(
            stderr.contains("... truncated"),
            "{label}: marker: {stderr}"
        );
    }
}

/// Regression (cross-model review, PR #66): `--contact-sheet`/`--grid`
/// with no sweep axis used to validate cleanly and then silently do
/// nothing (no file written, since no code path ever consumed them).
#[test]
fn contact_sheet_without_a_sweep_axis_errors() {
    let sheet = tmp("orphan-sheet.png");
    let (ok, _out, stderr) = run(
        &[
            "--headless",
            "--contact-sheet",
            sheet.to_str().unwrap(),
            "-",
        ],
        "0 0 10 10 rectfill",
    );
    assert!(!ok);
    assert!(stderr.contains("--sweep-seed or --sweep"), "{stderr}");
    assert!(!sheet.exists());
}

/// Regression for #630: pscat rejects bad arguments and exits without reading
/// stdin. With more input than a pipe buffer holds the write cannot finish
/// before the child exits, so `run()` always meets BrokenPipe here (it used to
/// panic on it); the exit status and stderr must still be reported.
#[test]
fn run_tolerates_a_child_that_exits_before_reading_stdin() {
    let sheet = tmp("orphan-sheet-big.png");
    let big = "0 0 10 10 rectfill\n".repeat(64 * 1024);
    let (ok, _out, stderr) = run(
        &[
            "--headless",
            "--contact-sheet",
            sheet.to_str().unwrap(),
            "-",
        ],
        &big,
    );
    assert!(!ok);
    assert!(stderr.contains("--sweep-seed or --sweep"), "{stderr}");
}

/// Regression (cross-model review, PR #66): an oversized contact
/// sheet used to be rejected only *after* every frame had already
/// rendered (and stayed resident in memory). It must now fail before
/// the first frame renders -- no `pscat: sweep:` progress line at all.
#[test]
fn oversized_contact_sheet_fails_before_any_frame_renders() {
    let sheet = tmp("oversized-sheet.png");
    let (ok, stdout, stderr) = run(
        &[
            "--page",
            "8000x8000",
            "--sweep-seed",
            "1:8",
            "--contact-sheet",
            sheet.to_str().unwrap(),
            "-",
        ],
        "0 0 10 10 rectfill",
    );
    assert!(!ok);
    assert!(stderr.contains("over the"), "stderr: {stderr}");
    assert!(
        !stdout.contains("pscat: sweep:"),
        "no frame should have started rendering: {stdout}"
    );
}

// ---- --transparent (issue #146) ------------------------------------------

/// Artwork for garments: with --transparent, untouched pixels have alpha 0,
/// painted ones are opaque, and the SVG has no white backdrop rect. The
/// default run stays opaque white.
#[test]
fn transparent_flag_leaves_the_background_unpainted() {
    let art = "1 0 0 setrgbcolor 20 20 60 60 rectfill";
    let png = tmp("transparent.png");
    let svg = tmp("transparent.svg");
    let (ok, _, stderr) = run(
        &[
            "--page",
            "100x100",
            "--transparent",
            "--png",
            png.to_str().unwrap(),
            "--svg",
            svg.to_str().unwrap(),
            "-",
        ],
        art,
    );
    assert!(ok, "stderr: {stderr}");
    let pm = Pixmap::load_png(&png).expect("png");
    assert_eq!(
        pm.pixel(2, 2).unwrap().alpha(),
        0,
        "corner must be transparent"
    );
    let ink = pm.pixel(50, 50).unwrap();
    assert_eq!((ink.alpha(), ink.red()), (255, 255));
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(!text.contains("#ffffff"), "no white backdrop rect: {text}");

    let opaque = tmp("opaque.png");
    let (ok, _, stderr) = run(
        &["--page", "100x100", "--png", opaque.to_str().unwrap(), "-"],
        art,
    );
    assert!(ok, "stderr: {stderr}");
    let pm = Pixmap::load_png(&opaque).expect("png");
    let c = pm.pixel(2, 2).unwrap();
    assert_eq!((c.alpha(), c.red(), c.green()), (255, 255, 255));
}

#[test]
fn transparent_is_rejected_where_it_cannot_apply() {
    let (ok, _, stderr) = run(&["--transparent", "-e", "1 pop"], "");
    assert!(!ok && stderr.contains("--transparent"), "{stderr}");
    let png = tmp("t-halftone.png");
    let (ok, _, stderr) = run(
        &[
            "--transparent",
            "--halftone",
            "--png",
            png.to_str().unwrap(),
            "-",
        ],
        "",
    );
    assert!(!ok && stderr.contains("--halftone"), "{stderr}");
}

/// Codex review of PR #152: a contact-sheet-only sweep may use
/// --transparent; frames are blitted raw, so untouched cells keep alpha 0.
#[test]
fn transparent_works_with_a_contact_sheet_only_sweep() {
    let sheet = tmp("transparent-sheet.png");
    let (ok, _, stderr) = run(
        &[
            "--page",
            "20x20",
            "--transparent",
            "--sweep-seed",
            "1,2",
            "--contact-sheet",
            sheet.to_str().unwrap(),
            "-",
        ],
        "1 0 0 setrgbcolor 5 5 5 5 rectfill",
    );
    assert!(ok, "stderr: {stderr}");
    let pm = Pixmap::load_png(&sheet).expect("sheet");
    let alphas: Vec<u8> = (0..pm.height())
        .flat_map(|y| (0..pm.width()).map(move |x| (x, y)))
        .map(|(x, y)| pm.pixel(x, y).unwrap().alpha())
        .collect();
    assert!(
        alphas.contains(&0),
        "untouched cell pixels stay transparent"
    );
    assert!(alphas.contains(&255), "ink is opaque");
    // the 4px gutter between the two 20px cells stays clear, not white
    assert_eq!(pm.pixel(21, 2).unwrap().alpha(), 0, "gutter is transparent");
}

// --- issue #95: --selftest and --lint-strict --------------------------

#[test]
fn selftest_runs_alone() {
    // Same contract as --spool: it builds its own interpreter per block
    // and produces no page, so pairing it with a render request would
    // silently ignore one of the two.
    for extra in [
        vec!["--selftest", "lib/paintkit.ps", "--png", "out.png"],
        vec!["--selftest", "lib/paintkit.ps", "-e", "showpage"],
        vec!["--selftest", "lib/paintkit.ps", "--lint"],
        vec!["--selftest", "lib/paintkit.ps", "examples/postcard.ps"],
        // Refused rather than honored: each block renders on a fixed
        // canvas it never reads back, so accepting these would mean
        // quietly ignoring them.
        vec!["--selftest", "lib/paintkit.ps", "--page", "200x200"],
        vec!["--selftest", "lib/paintkit.ps", "--dpi", "144"],
        vec!["--selftest", "lib/paintkit.ps", "--halftone"],
        // Accepted-and-ignored is the same silent-no-op class this
        // whole feature exists to close (Codex review, PR #136).
        vec!["--selftest", "lib/paintkit.ps", "--pstack-on-error"],
        vec!["--selftest", "lib/paintkit.ps", "--speed", "10"],
        // Explicitly passed *defaults* count too: comparing each
        // option against its default value accepted these silently
        // (Codex review, PR #136).
        vec!["--selftest", "lib/paintkit.ps", "--page", "612x792"],
        vec!["--selftest", "lib/paintkit.ps", "--dpi", "72"],
        vec!["--selftest", "lib/paintkit.ps", "--speed", "100"],
    ] {
        let (ok, _, err) = run(&extra, "");
        assert!(!ok, "expected a refusal for {extra:?}");
        assert!(err.contains("--selftest runs alone"), "{err}");
    }
}

#[test]
fn selftest_list_runs_alone_too() {
    // It returned before any mode-exclusion check, so
    // `--selftest-list a.ps --selftest b.ps --png out.png` listed a.ps
    // and silently ignored both the test and the output.
    let (ok, _, err) = run(
        &["--selftest-list", "lib/artkit.ps", "--png", "out.png"],
        "",
    );
    assert!(!ok);
    assert!(err.contains("--selftest-list runs alone"), "{err}");
}

#[test]
fn selftest_list_prints_block_names() {
    let (ok, out, err) = run(&["--selftest-list", "lib/artkit.ps"], "");
    assert!(ok, "{err}");
    assert!(out.contains("scatter-validates-its-option-dict"), "{out}");
}

#[test]
fn selftest_reports_a_missing_file_without_panicking() {
    let (ok, _, err) = run(&["--selftest", "no/such/file.ps"], "");
    assert!(!ok);
    assert!(err.contains("cannot read"), "{err}");
}

#[test]
fn selftest_names_each_block_it_ran() {
    let (ok, _, err) = run(&["--selftest", "lib/paintkit.ps"], "");
    assert!(ok, "{err}");
    assert!(
        err.contains("pkribbon-rejects-a-non-callable-pressure"),
        "{err}"
    );
    assert!(err.contains("block(s) passed"), "{err}");
}

#[test]
fn lint_strict_implies_lint_and_its_mode_exclusions() {
    // --lint-strict sets `lint` too, so every place that refuses
    // --lint refuses this without naming it separately.
    let (ok, _, err) = run(&["--lint-strict", "--spool", "jobs"], "");
    assert!(!ok);
    assert!(err.contains("--spool runs alone"), "{err}");
}

#[test]
fn usage_lists_the_new_flags() {
    let (_, out, _) = run(&["--help"], "");
    assert!(out.contains("--lint-strict"), "{out}");
    assert!(out.contains("--selftest"), "{out}");
    assert!(out.contains("--selftest-list"), "{out}");
}

#[test]
fn version_flag_prints_the_crate_version_and_exits_clean() {
    let want = format!("pscat {}\n", env!("CARGO_PKG_VERSION"));
    for flag in ["--version", "-V"] {
        let (ok, out, err) = run(&[flag], "");
        assert!(ok, "{flag} must exit 0; stderr: {err}");
        assert_eq!(out, want, "{flag}");
        assert!(err.is_empty(), "{flag} stderr: {err}");
    }
    let (_, usage, _) = run(&["--help"], "");
    assert!(
        usage.contains("--version"),
        "usage lists --version: {usage}"
    );
}

// --- execution limits (issue #604) ---------------------------------------

#[test]
fn max_steps_stops_a_runaway_loop_with_a_structured_error() {
    let (ok, _out, stderr) = run(
        &["--headless", "--max-steps", "100000", "-e", "{ } loop"],
        "",
    );
    assert!(!ok, "a stopped run exits nonzero");
    assert!(
        stderr.contains("%%[ Error: limitexceeded (steps);"),
        "{stderr}"
    );
}

#[test]
fn max_time_and_max_memory_stop_a_runaway() {
    let (ok, _out, stderr) = run(&["--headless", "--max-time", "0.3", "-e", "{ } loop"], "");
    assert!(!ok);
    assert!(stderr.contains("limitexceeded (time)"), "{stderr}");
    let (ok, _out, stderr) = run(
        &["--headless", "--max-memory", "50", "-e", "{ 1 } loop"],
        "",
    );
    assert!(!ok);
    assert!(stderr.contains("limitexceeded (memory)"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn limits_stop_a_whole_file_read_of_dev_zero() {
    // Issue #702: the read happens inside one operator, so the limits are
    // looked at as the bytes arrive.
    let src = "(/dev/zero) (r) file pop";
    for (flag, value, limit) in [
        ("--max-time", "0.3", "time"),
        ("--max-memory", "50", "memory"),
        ("--max-steps", "5000", "steps"),
    ] {
        let (ok, _out, stderr) = run(&["--headless", flag, value, "-e", src], "");
        assert!(!ok, "{flag}");
        assert!(
            stderr.contains(&format!("limitexceeded ({limit})")),
            "{flag}: {stderr}"
        );
    }
}

#[test]
fn stopped_cannot_catch_a_cli_limit() {
    let (ok, out, stderr) = run(
        &[
            "--headless",
            "--max-steps",
            "50000",
            "-e",
            "{ { { } loop } stopped pop } loop (survived) print",
        ],
        "",
    );
    assert!(!ok && !out.contains("survived"), "{out} {stderr}");
    assert!(stderr.contains("limitexceeded (steps)"), "{stderr}");
}

#[test]
fn a_limited_run_still_writes_the_partial_canvas() {
    let png = tmp("limit-partial.png");
    let (ok, _out, stderr) = run(
        &[
            "--page",
            "20x20",
            "--max-steps",
            "50000",
            "--png",
            png.to_str().unwrap(),
            "-",
        ],
        "0 0 20 20 rectfill { } loop",
    );
    assert!(!ok, "{stderr}");
    let img =
        Pixmap::load_png(&png).expect("the canvas is written after a limit, as after any error");
    assert_eq!(img.pixel(5, 5).unwrap().alpha(), 255);
    assert_eq!(img.pixel(5, 5).unwrap().red(), 0, "the fill is there");
}

#[test]
fn limits_are_off_by_default_and_zero_means_off() {
    let (ok, out, stderr) = run(
        &[
            "--headless",
            "--max-steps",
            "0",
            "--max-time",
            "0",
            "--max-memory",
            "0",
            "-e",
            "1 1 10000 { pop } for 3 4 add =",
        ],
        "",
    );
    assert!(ok, "{stderr}");
    assert_eq!(out.trim(), "7");
}

#[test]
fn a_program_inside_its_limits_prints_what_it_did_without_them() {
    let prog = "0 1 1 50000 { add } for =";
    let (_, free, _) = run(&["--headless", "-e", prog], "");
    let (ok, limited, stderr) = run(
        &[
            "--headless",
            "--max-steps",
            "5000000",
            "--max-time",
            "60",
            "--max-memory",
            "500",
            "-e",
            prog,
        ],
        "",
    );
    assert!(ok, "{stderr}");
    assert_eq!(free, limited);
}

#[test]
fn bad_limit_values_are_refused() {
    for args in [
        &["--max-steps", "many"][..],
        &["--max-steps", "-5"],
        &["--max-time", "soon"],
        &["--max-time", "-1"],
        &["--max-time", "inf"],
        &["--max-memory", "lots"],
        &["--max-steps"],
    ] {
        let mut full = vec!["--headless"];
        full.extend_from_slice(args);
        full.extend(["-e", "1 pop"]);
        let (ok, _out, stderr) = run(&full, "");
        assert!(!ok, "{args:?} should be refused");
        assert!(stderr.contains("--max-"), "{args:?}: {stderr}");
    }
}

#[test]
fn limits_do_not_combine_with_modes_that_cannot_honour_them() {
    // The spool builds its own interpreters; refusing beats ignoring.
    let dir = tmp("limit-spool-dir");
    std::fs::create_dir_all(&dir).unwrap();
    let (ok, _out, stderr) = run(&["--spool", dir.to_str().unwrap(), "--max-steps", "9"], "");
    assert!(!ok);
    assert!(stderr.contains("--spool"), "{stderr}");
    // The self-test modes run alone.
    let (ok, _out, stderr) = run(&["--selftest", "lib/artkit.ps", "--max-steps", "9"], "");
    assert!(!ok);
    assert!(stderr.contains("runs alone"), "{stderr}");
}

#[test]
fn a_limit_passed_in_the_final_slice_still_fails_the_run() {
    // A program that is over its memory cap when it ends, having never reached
    // a second slice, is not a clean run (Codex review of #604, round 1). The
    // procedure is kept (`def`): memory freed again within the slice is not
    // seen, which SECURITY.md says.
    let big = format!("/p {{ {} }} def", "0 ".repeat(100_000));
    let (ok, _out, stderr) = run(&["--headless", "--max-memory", "1", "-"], &big);
    assert!(!ok, "{stderr}");
    assert!(stderr.contains("limitexceeded (memory)"), "{stderr}");
    // The same program with room to spare is fine.
    let (ok, _out, stderr) = run(&["--headless", "--max-memory", "200", "-"], &big);
    assert!(ok, "{stderr}");
}

#[test]
fn the_largest_step_limit_does_not_overflow() {
    // u64::MAX once panicked (overflow checks) or wrapped the budget to zero.
    let (ok, out, stderr) = run(
        &[
            "--headless",
            "--max-steps",
            "18446744073709551615",
            "-e",
            "1 pop 7 =",
        ],
        "",
    );
    assert!(ok, "{stderr}");
    assert_eq!(out.trim(), "7");
}
