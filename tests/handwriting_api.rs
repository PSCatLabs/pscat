//! The library handwriting API (issue #1135): `pscat::handwriting` builds the
//! programs `pscat handwrite` runs, so a note made through the library is the
//! subcommand's PNG, byte for byte.
//!
//! The server story (pscat-mcp) is: validate, run `measure_source` under its
//! own limits, size the page, check it against its own pixel cap, then run
//! `render_source` under its own limits. `library_note` does exactly that on an
//! `Interp`, and each test compares it with what the binary writes.

use pscat::Interp;
use pscat::handwriting::{
    HandError, HandOptions, MAX_PAGE_POINTS, Paper, measure_source, parse_line_count, render_source,
};

mod common;
use common::CommandDeadline as _;

/// The note the way a library user makes it: two passes and the page in
/// between. Returns the page size in points and the PNG, encoded as the
/// binary does at `dpi`.
fn library_note(opts: &HandOptions, height: Option<u32>, dpi: f32) -> ((u32, u32), Vec<u8>) {
    opts.validate().expect("valid options");
    let height = height.unwrap_or_else(|| {
        let mut interp = Interp::new();
        interp.capture_stdout();
        interp
            .run_str(&measure_source(opts))
            .expect("the measuring program runs");
        let printed = String::from_utf8_lossy(interp.captured_stdout()).into_owned();
        let lines = parse_line_count(&printed).expect("a line count");
        let h = opts.height_pt(lines);
        assert!(h >= 1.0 && h <= f64::from(MAX_PAGE_POINTS), "height {h}");
        h as u32
    });
    let mut interp =
        Interp::with_page_scaled(opts.width, height, dpi / 72.0).expect("a valid page");
    interp
        .run_str(&render_source(opts, height))
        .expect("the note renders");
    assert_eq!(interp.page_count(), 1);
    let png = interp.page(0).expect("one page").to_png().expect("PNG");
    ((opts.width, height), png)
}

fn cli_note(args: &[&str], text: &str) -> Vec<u8> {
    let dir = common::scratch_dir("handwriting-api");
    let out = dir.join("note.png");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .arg("handwrite")
        .args(args)
        .arg("-o")
        .arg(&out)
        .arg("--")
        .arg(text)
        .env_remove("PSCAT_HOME")
        .env_remove("PSCAT_LIB")
        .output_deadline()
        .expect("run pscat");
    assert!(
        result.status.success(),
        "pscat handwrite {args:?} failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    std::fs::read(&out).expect("pscat wrote the note")
}

#[test]
fn the_subcommands_png_is_the_librarys_for_the_same_options() {
    // (flags, the same options in the library, text, dpi, height flag)
    let mut ruled = HandOptions::new("Meet me (at) nine\r\nbring the \\ folder\n\n");
    ruled.paper = Paper::Ruled;
    ruled.seed = 7;
    ruled.ink = [0.6, 0.1, 0.1];
    ruled.size = 28.0;
    let mut tall = HandOptions::new("a longer note that has to wrap across several lines of page");
    tall.width = 300;
    tall.margin = 20.0;
    tall.leading = 1.25;
    tall.jitter = 3;
    tall.pen = 18.5;
    tall.size = 24.0;
    let mut plain = HandOptions::new("UPPER lower caf\u{e9} 123 !?.,-'");
    plain.seed = -3;

    let cases: Vec<(Vec<&str>, HandOptions, f32, Option<u32>)> = vec![
        (vec![], HandOptions::new("hello world"), 72.0, None),
        (
            vec![
                "--paper",
                "ruled",
                "--seed",
                "7",
                "--ink",
                "0.6,0.1,0.1",
                "--size",
                "28",
            ],
            ruled.clone(),
            72.0,
            None,
        ),
        // A given --height skips the measuring pass.
        (
            vec![
                "--paper",
                "ruled",
                "--seed",
                "7",
                "--ink",
                "0.6,0.1,0.1",
                "--size",
                "28",
                "--height",
                "300",
            ],
            ruled,
            72.0,
            Some(300),
        ),
        (
            vec![
                "--width",
                "300",
                "--margin",
                "20",
                "--leading",
                "1.25",
                "--jitter",
                "3",
                "--pen",
                "18.5",
                "--size",
                "24",
                "--dpi",
                "144",
            ],
            tall,
            144.0,
            None,
        ),
        (vec!["--seed", "-3"], plain, 72.0, None),
    ];
    for (flags, opts, dpi, height) in cases {
        let cli = cli_note(&flags, &opts.text);
        let ((w, h), lib) = library_note(&opts, height, dpi);
        assert!(
            cli == lib,
            "{flags:?}: the library's {w}x{h} note differs from the subcommand's"
        );
    }
}

#[test]
fn the_auto_height_is_the_subcommands() {
    // The height is the number of lines and nothing else: a one-line and a
    // four-line note differ by exactly three leading steps.
    let one = HandOptions::new("hi");
    let many = HandOptions::new("a\nb\nc\nd");
    assert_eq!(one.height_pt(1), 146.0);

    assert_eq!(pscat::handwriting::page_height_pt(1, 36.0), 146.0);
    assert_eq!(many.height_pt(4), 308.0); // 108 + 37.8 + 3 * 54 = 307.8
    assert_eq!(
        pscat::handwriting::page_height_pt(4, 36.0),
        many.height_pt(4)
    );
}

#[test]
fn the_measuring_program_counts_wrapped_and_blank_lines() {
    let count = |opts: &HandOptions| {
        let mut interp = Interp::new();
        interp.capture_stdout();
        interp.run_str(&measure_source(opts)).unwrap();
        parse_line_count(&String::from_utf8_lossy(interp.captured_stdout())).unwrap()
    };
    assert_eq!(count(&HandOptions::new("one line")), 1);
    assert_eq!(count(&HandOptions::new("a\n\nb")), 3);
    let mut narrow = HandOptions::new("many short words that wrap around a narrow page");
    narrow.width = 160;
    assert!(count(&narrow) > 3);
}

#[test]
fn a_hostile_text_stays_text() {
    // Parentheses, backslashes and PostScript in the text never leave the
    // string literal: the program still draws one page.
    let opts = HandOptions::new("x) pop (y\\ ) quit % (\n/Text (z) def stop");
    opts.validate().unwrap();
    let mut interp = Interp::with_page_scaled(612, 400, 1.0).unwrap();
    interp.run_str(&render_source(&opts, 400)).unwrap();
    assert_eq!(interp.page_count(), 1);
}

#[test]
fn validate_codes_and_fields() {
    let bad = |f: fn(&mut HandOptions)| {
        let mut o = HandOptions::new("hi");
        f(&mut o);
        o.validate().expect_err("refused")
    };
    let e = HandOptions::default().validate().unwrap_err();
    assert_eq!((e.code(), e.field()), ("empty_text", "text"));
    for (err, code, field) in [
        (bad(|o| o.size = 3000.0), "out_of_range", "size"),
        (bad(|o| o.size = f64::NAN), "not_finite", "size"),
        (bad(|o| o.width = 0), "out_of_range", "width"),
        (
            bad(|o| o.width = MAX_PAGE_POINTS + 1),
            "out_of_range",
            "width",
        ),
        (bad(|o| o.margin = -1.0), "out_of_range", "margin"),
        (bad(|o| o.leading = f64::INFINITY), "not_finite", "leading"),
        (bad(|o| o.jitter = 20_000), "out_of_range", "jitter"),
        (bad(|o| o.pen = -2.0), "out_of_range", "pen"),
        (bad(|o| o.ink = [0.0, 2.0, 0.0]), "out_of_range", "ink"),
    ] {
        assert_eq!((err.code(), err.field()), (code, field), "{err}");
    }
    assert!(matches!(
        "blue".parse::<Paper>(),
        Err(HandError::UnknownPaper(_))
    ));
}
