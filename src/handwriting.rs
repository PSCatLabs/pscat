//! Handwriting-style notes: the generator behind `pscat handwrite` and the
//! plugin's `handwrite` tool.
//!
//! [`HandOptions`] holds the text and the look of the note. The functions in
//! this module turn it into PostScript **source** that a caller runs where it
//! runs untrusted PostScript (a sandboxed worker, under [`Limits`](crate::Limits)).
//! Nothing here runs a program, so the caller can check the page against its
//! own pixel cap between the two passes, and run each pass under its own
//! limits:
//!
//! 1. [`measure_source`] is a program that prints how many lines the text
//!    wraps to at the options' width ([`parse_line_count`] reads the number).
//! 2. [`HandOptions::height_pt`] (or [`page_height_pt`], for the default
//!    margin and leading) turns the line count into the page height that
//!    holds the text.
//! 3. [`render_source`] is the program that writes the note on a page of
//!    that height.
//!
//! `pscat handwrite` calls exactly these functions, so a note rendered
//! through the library is the same PNG as the subcommand's for the same
//! options and page size.
//!
//! The text is never code in the program. It is lowercased (the font has no
//! capitals; ASCII only), written as a string literal with `\`, `(` and `)`
//! escaped, and stripped of trailing newlines, and every number and name is
//! written by this module from a typed value. A character the font does not
//! have advances as a blank rather than erroring, so it still takes up room
//! in the wrap.
//!
//! ```
//! use pscat::handwriting::{HandOptions, Paper, MAX_PAGE_POINTS, measure_source,
//!     parse_line_count, render_source};
//! use pscat::Interp;
//!
//! let mut opts = HandOptions::new("meet me at nine\nbring the (blue) folder");
//! opts.paper = Paper::Ruled;
//! opts.seed = 7;
//! opts.validate()?;
//!
//! // Pass 1: how many lines does the text wrap to?
//! let mut interp = Interp::new();
//! interp.capture_stdout();
//! interp.run_str(&measure_source(&opts)).expect("the measuring program runs");
//! let stdout = String::from_utf8_lossy(interp.captured_stdout()).into_owned();
//! let lines = parse_line_count(&stdout).expect("it printed a count");
//! assert_eq!(lines, 2);
//!
//! // The page that holds them. Check it against your own limits here.
//! let height = opts.height_pt(lines);
//! assert!(height <= f64::from(MAX_PAGE_POINTS));
//! let height = height as u32;
//!
//! // Pass 2: write the note.
//! let mut interp = Interp::with_page_scaled(opts.width, height, 1.0).expect("valid page");
//! interp.run_str(&render_source(&opts, height)).expect("the note renders");
//! assert_eq!(interp.pages().len(), 1);
//! # Ok::<(), pscat::handwriting::HandError>(())
//! ```

use std::fmt;

/// The largest page dimension, in points, `pscat` draws: a note's width and
/// height are each at most this (the `--page` limit).
pub const MAX_PAGE_POINTS: u32 = 8000;

/// The smallest page dimension, in points.
pub const MIN_PAGE_POINTS: u32 = 1;

/// Default font size in points.
pub const DEFAULT_SIZE: f64 = 36.0;
/// Smallest font size [`HandOptions::validate`] accepts.
pub const MIN_SIZE: f64 = 0.0;
/// Largest font size [`HandOptions::validate`] accepts.
pub const MAX_SIZE: f64 = 2000.0;

/// Default page width in points.
pub const DEFAULT_WIDTH: u32 = 612;
/// Smallest page width.
pub const MIN_WIDTH: u32 = MIN_PAGE_POINTS;
/// Largest page width.
pub const MAX_WIDTH: u32 = MAX_PAGE_POINTS;

/// Default page margin in points.
pub const DEFAULT_MARGIN: f64 = 54.0;
/// Smallest margin.
pub const MIN_MARGIN: f64 = 0.0;
/// Largest margin.
pub const MAX_MARGIN: f64 = MAX_PAGE_POINTS as f64;

/// Default baseline spacing, as a multiple of the font size.
pub const DEFAULT_LEADING: f64 = 1.5;
/// Smallest leading.
pub const MIN_LEADING: f64 = 0.0;
/// Largest leading.
pub const MAX_LEADING: f64 = 100.0;

/// Default ink wobble in glyph units (0 is calm).
pub const DEFAULT_JITTER: u32 = 16;
/// Smallest jitter.
pub const MIN_JITTER: u32 = 0;
/// Largest jitter.
pub const MAX_JITTER: u32 = 10_000;

/// Default stroke width in glyph units.
pub const DEFAULT_PEN: f64 = 22.0;
/// Smallest pen width.
pub const MIN_PEN: f64 = 0.0;
/// Largest pen width.
pub const MAX_PEN: f64 = 1000.0;

/// Default ink colour, each component in `0..=1`.
pub const DEFAULT_INK: [f64; 3] = [0.13, 0.14, 0.34];
/// Smallest ink colour component.
pub const MIN_INK: f64 = 0.0;
/// Largest ink colour component.
pub const MAX_INK: f64 = 1.0;

/// Default random seed: the same seed and options give the same page.
pub const DEFAULT_SEED: i32 = 1985;

/// The paper a note is written on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Paper {
    /// Blank paper.
    #[default]
    Plain,
    /// Ruled lines and a margin line.
    Ruled,
}

impl Paper {
    /// The name `--paper` and the `paper` tool argument use: `plain` or `ruled`.
    pub fn as_str(self) -> &'static str {
        match self {
            Paper::Plain => "plain",
            Paper::Ruled => "ruled",
        }
    }
}

impl std::str::FromStr for Paper {
    type Err = HandError;

    fn from_str(s: &str) -> Result<Paper, HandError> {
        match s {
            "plain" => Ok(Paper::Plain),
            "ruled" => Ok(Paper::Ruled),
            _ => Err(HandError::UnknownPaper(s.to_string())),
        }
    }
}

/// The text of a note and how it looks. [`Default`] is `pscat handwrite`'s
/// defaults (36 pt on a 612 pt page, plain paper, dark blue ink, seed 1985)
/// with no text: set [`text`](Self::text), or start from [`new`](Self::new).
///
/// Check the options with [`validate`](Self::validate) before building a
/// program. `#[non_exhaustive]`: new options may arrive in a minor release, so
/// start from `HandOptions::default()` or [`new`](Self::new) and set fields.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct HandOptions {
    /// What to write. Newlines force line breaks and blank lines are kept.
    /// Must not be empty.
    pub text: String,
    /// Font size in points ([`MIN_SIZE`]..=[`MAX_SIZE`]).
    pub size: f64,
    /// Page width in points ([`MIN_WIDTH`]..=[`MAX_WIDTH`]).
    pub width: u32,
    /// Page margin in points ([`MIN_MARGIN`]..=[`MAX_MARGIN`]).
    pub margin: f64,
    /// Baseline spacing as a multiple of the size
    /// ([`MIN_LEADING`]..=[`MAX_LEADING`]).
    pub leading: f64,
    /// Ink wobble in glyph units, 0 for calm ([`MIN_JITTER`]..=[`MAX_JITTER`]).
    /// A whole number: the font takes it modulo something.
    pub jitter: u32,
    /// Stroke width in glyph units ([`MIN_PEN`]..=[`MAX_PEN`]).
    pub pen: f64,
    /// Ink colour as red, green and blue, each in [`MIN_INK`]..=[`MAX_INK`].
    pub ink: [f64; 3],
    /// The paper.
    pub paper: Paper,
    /// Random seed. The same seed and options write the same page.
    pub seed: i32,
}

impl Default for HandOptions {
    fn default() -> HandOptions {
        HandOptions {
            text: String::new(),
            size: DEFAULT_SIZE,
            width: DEFAULT_WIDTH,
            margin: DEFAULT_MARGIN,
            leading: DEFAULT_LEADING,
            jitter: DEFAULT_JITTER,
            pen: DEFAULT_PEN,
            ink: DEFAULT_INK,
            paper: Paper::Plain,
            seed: DEFAULT_SEED,
        }
    }
}

impl HandOptions {
    /// The default options writing `text`.
    pub fn new(text: impl Into<String>) -> HandOptions {
        HandOptions {
            text: text.into(),
            ..HandOptions::default()
        }
    }

    /// Check every option against its range (the `MIN_` and `MAX_` consts),
    /// and that there is text. Run it before [`measure_source`] and
    /// [`render_source`]; they do not check, though a value that fails here
    /// can only make the program fail, never carry code into it.
    pub fn validate(&self) -> Result<(), HandError> {
        if self.text.is_empty() {
            return Err(HandError::EmptyText);
        }
        let float = |field: &'static str, got: f64, min: f64, max: f64| {
            if !got.is_finite() {
                Err(HandError::NotFinite { field })
            } else if got < min || got > max {
                Err(HandError::OutOfRange {
                    field,
                    min,
                    max,
                    got,
                })
            } else {
                Ok(())
            }
        };
        let whole = |field: &'static str, got: u32, min: u32, max: u32| {
            float(field, f64::from(got), f64::from(min), f64::from(max))
        };
        float("size", self.size, MIN_SIZE, MAX_SIZE)?;
        whole("width", self.width, MIN_WIDTH, MAX_WIDTH)?;
        float("margin", self.margin, MIN_MARGIN, MAX_MARGIN)?;
        float("leading", self.leading, MIN_LEADING, MAX_LEADING)?;
        whole("jitter", self.jitter, MIN_JITTER, MAX_JITTER)?;
        float("pen", self.pen, MIN_PEN, MAX_PEN)?;
        for c in self.ink {
            float("ink", c, MIN_INK, MAX_INK)?;
        }
        Ok(())
    }

    /// The page height, in whole points, that holds `lines` lines at these
    /// options: two margins, a first line's 1.05 em, and a leading step for
    /// each further line, rounded up. `0.0` for options that give no usable
    /// height (not [`validate`](Self::validate)d). Compare it with
    /// [`MAX_PAGE_POINTS`] and your own pixel cap before rendering.
    pub fn height_pt(&self, lines: u64) -> f64 {
        auto_height(lines, self.margin, self.size, self.leading)
    }

    /// The Text of the options dictionary: lowercased (ASCII: the font has no
    /// capitals), line endings normalised (a bare CR is a newline in a
    /// PostScript literal anyway), escaped for a string literal, and stripped
    /// of trailing newlines.
    fn literal_text(&self) -> String {
        let normal = self
            .text
            .to_ascii_lowercase()
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        let mut out = String::with_capacity(normal.len() + 8);
        for c in normal.chars() {
            if matches!(c, '\\' | '(' | ')') {
                out.push('\\');
            }
            out.push(c);
        }
        while out.ends_with('\n') {
            out.pop();
        }
        out
    }

    /// The `/HandOpts` dictionary definition, with the page `height`.
    fn dictionary(&self, height: u32) -> String {
        format!(
            "/HandOpts <<\n  /Text ({})\n  /Size {}\n  /Width {}\n  /Height {height}\n  \
             /Margin {}\n  /Leading {}\n  /Jitter {}\n  /Pen {}\n  /Ink [ {} {} {} ]\n  \
             /Paper /{}\n  /Seed {}\n>> def\n",
            self.literal_text(),
            self.size,
            self.width,
            self.margin,
            self.leading,
            self.jitter,
            self.pen,
            self.ink[0],
            self.ink[1],
            self.ink[2],
            self.paper.as_str(),
            self.seed
        )
    }
}

/// The handwriting library, `lib/handscript.ps`, as the resolver finds it
/// (`$PSCAT_HOME` and the other roots first, then the copy compiled in), so a
/// program is self-contained and the interpreter that runs it needs no file
/// access. Only a build with no embedded copy at all falls back to a `run`.
fn library() -> String {
    const NAME: &str = "lib/handscript.ps";
    match crate::paths::open(NAME) {
        Some(r) => String::from_utf8_lossy(&r.bytes).into_owned(),
        None => format!("({NAME}) run\n"),
    }
}

/// The program that prints the number of lines `opts` wraps to, then a
/// newline. It contains `lib/handscript.ps` itself, read when you call this
/// from `$PSCAT_HOME`, `$PSCAT_LIB` and the other roots of the calling
/// process, then the copy compiled in, so the interpreter that runs it needs
/// no file access. Run it on a default page with the interpreter's stdout captured
/// and give the output to [`parse_line_count`]. The page height does not
/// affect the wrap, so none is needed.
pub fn measure_source(opts: &HandOptions) -> String {
    let mut src = library();
    src.push_str(&opts.dictionary(792));
    src.push_str("HandOpts hs-linecount =\n");
    src
}

/// The number of lines a [`measure_source`] program printed, or `None` when
/// the output is not a whole number.
pub fn parse_line_count(stdout: &str) -> Option<u64> {
    stdout.trim().parse().ok()
}

/// [`HandOptions::height_pt`] at the default margin ([`DEFAULT_MARGIN`]) and
/// leading ([`DEFAULT_LEADING`]), for a note of `lines` lines at font size
/// `size`.
pub fn page_height_pt(lines: u64, size: f64) -> f64 {
    auto_height(lines, DEFAULT_MARGIN, size, DEFAULT_LEADING)
}

/// The program that writes the note (it carries `lib/handscript.ps`, found as
/// [`measure_source`] describes) on a page `height_pt` points high and
/// `opts.width` wide, then shows it (one page). Make the page with
/// `Interp::with_page_scaled(opts.width, height_pt, dpi / 72.0)`, and keep
/// `height_pt` within [`MIN_PAGE_POINTS`]..=[`MAX_PAGE_POINTS`].
pub fn render_source(opts: &HandOptions, height_pt: u32) -> String {
    let mut src = library();
    src.push_str(&opts.dictionary(height_pt));
    src.push_str("HandOpts hs-write showpage\n");
    src
}

/// The page height that holds `lines` lines: two margins, a first line's
/// 1.05 em, and a leading step for each further line, rounded up. The
/// script's awk did this in doubles, in this order.
fn auto_height(lines: u64, margin: f64, size: f64, leading: f64) -> f64 {
    let n = (lines as f64).max(1.0);
    let h = 2.0 * margin + 1.05 * size + (n - 1.0) * leading * size;
    if h.is_finite() && h > 0.0 {
        h.ceil()
    } else {
        0.0
    }
}

/// Why [`HandOptions::validate`] (or parsing a [`Paper`]) refused a value.
/// Read it with [`code`](Self::code) and [`field`](Self::field), which are
/// stable, or show it (the `Display` text names the field and its range).
///
/// `#[non_exhaustive]`: a new check is a new variant, so keep a `_` arm.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum HandError {
    /// The text is empty.
    EmptyText,
    /// A number option is NaN or infinite.
    NotFinite {
        /// The option, by its [`HandOptions`] field name.
        field: &'static str,
    },
    /// A number option is outside its `MIN_` to `MAX_` range.
    OutOfRange {
        /// The option, by its [`HandOptions`] field name.
        field: &'static str,
        /// The smallest value allowed.
        min: f64,
        /// The largest value allowed.
        max: f64,
        /// The value given.
        got: f64,
    },
    /// A paper name that is not `plain` or `ruled`.
    UnknownPaper(String),
}

impl HandError {
    /// A stable machine-readable code: `empty_text`, `not_finite`,
    /// `out_of_range` or `unknown_paper`.
    pub fn code(&self) -> &'static str {
        match self {
            HandError::EmptyText => "empty_text",
            HandError::NotFinite { .. } => "not_finite",
            HandError::OutOfRange { .. } => "out_of_range",
            HandError::UnknownPaper(_) => "unknown_paper",
        }
    }

    /// The option that was refused, by its [`HandOptions`] field name (also
    /// the `pscat handwrite` flag without its dashes): `text`, `size`,
    /// `width`, `margin`, `leading`, `jitter`, `pen`, `ink` or `paper`.
    pub fn field(&self) -> &'static str {
        match self {
            HandError::EmptyText => "text",
            HandError::NotFinite { field } | HandError::OutOfRange { field, .. } => field,
            HandError::UnknownPaper(_) => "paper",
        }
    }
}

impl fmt::Display for HandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandError::EmptyText => f.write_str("text must not be empty"),
            HandError::NotFinite { field } => write!(f, "{field} must be a finite number"),
            HandError::OutOfRange {
                field,
                min,
                max,
                got,
            } => write!(f, "{field} must be {min} to {max}, got {got}"),
            HandError::UnknownPaper(got) => write!(f, "paper must be plain or ruled, got: {got}"),
        }
    }
}

impl std::error::Error for HandError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_height_matches_the_scripts_arithmetic() {
        // 2*54 + 1.05*36 + 0*1.5*36 = 145.8 -> 146
        assert_eq!(auto_height(1, 54.0, 36.0, 1.5), 146.0);
        assert_eq!(auto_height(0, 54.0, 36.0, 1.5), 146.0);
        // 2*54 + 37.8 + 2*54 = 253.8 -> 254
        assert_eq!(auto_height(3, 54.0, 36.0, 1.5), 254.0);
        // an exact value is not bumped
        assert_eq!(auto_height(1, 10.0, 20.0, 1.0), 41.0);
        // 20 + 10.5 + 10 = 40.5 -> 41
        assert_eq!(auto_height(2, 10.0, 10.0, 1.0), 41.0);
        assert_eq!(auto_height(2, 10.0, 20.0, 1.5), 71.0);
        assert_eq!(page_height_pt(3, 36.0), 254.0);
        assert_eq!(auto_height(1, f64::NAN, 36.0, 1.5), 0.0);
    }

    #[test]
    fn the_dictionary_lowercases_escapes_and_trims() {
        let o = HandOptions::new("Hi (There)\\\r\nOK\n\n");
        let d = o.dictionary(792);
        assert!(d.starts_with("/HandOpts <<\n  /Text (hi \\(there\\)\\\\\nok)\n  /Size 36\n"));
        assert!(d.contains("  /Ink [ 0.13 0.14 0.34 ]\n  /Paper /plain\n  /Seed 1985\n>> def\n"));
        // A lone CR is a newline; non-ASCII bytes pass through untouched.
        assert_eq!(HandOptions::new("A\rB\u{e9}").literal_text(), "a\nb\u{e9}");
    }

    #[test]
    fn numbers_are_written_the_way_the_flags_were() {
        let mut o = HandOptions::new("x");
        o.size = 36.5;
        o.leading = 1.0;
        o.pen = 0.5;
        o.ink = [1.0, 0.0, 0.25];
        let d = o.dictionary(100);
        for want in [
            "/Size 36.5",
            "/Leading 1\n",
            "/Pen 0.5\n",
            "/Ink [ 1 0 0.25 ]",
            "/Height 100",
        ] {
            assert!(d.contains(want), "{want:?} in {d}");
        }
    }

    #[test]
    fn line_counts_parse() {
        assert_eq!(parse_line_count("3\n"), Some(3));
        assert_eq!(parse_line_count(" 12 "), Some(12));
        assert_eq!(parse_line_count(""), None);
        assert_eq!(parse_line_count("2.5"), None);
        assert_eq!(parse_line_count("-1"), None);
    }

    #[test]
    fn validate_reports_a_stable_code_and_field() {
        let ok = HandOptions::new("hi");
        assert_eq!(ok.validate(), Ok(()));
        let err = |f: &dyn Fn(&mut HandOptions)| {
            let mut o = HandOptions::new("hi");
            f(&mut o);
            let e = o.validate().unwrap_err();
            (e.code(), e.field())
        };
        assert_eq!(
            HandOptions::default().validate().unwrap_err().code(),
            "empty_text"
        );
        assert_eq!(err(&|o| o.text.clear()), ("empty_text", "text"));
        assert_eq!(err(&|o| o.size = 2001.0), ("out_of_range", "size"));
        assert_eq!(err(&|o| o.size = -1.0), ("out_of_range", "size"));
        assert_eq!(err(&|o| o.size = f64::NAN), ("not_finite", "size"));
        assert_eq!(err(&|o| o.width = 0), ("out_of_range", "width"));
        assert_eq!(err(&|o| o.width = 8001), ("out_of_range", "width"));
        assert_eq!(err(&|o| o.margin = -0.5), ("out_of_range", "margin"));
        assert_eq!(err(&|o| o.leading = 101.0), ("out_of_range", "leading"));
        assert_eq!(
            err(&|o| o.leading = f64::INFINITY),
            ("not_finite", "leading")
        );
        assert_eq!(err(&|o| o.jitter = 10_001), ("out_of_range", "jitter"));
        assert_eq!(err(&|o| o.pen = 1001.0), ("out_of_range", "pen"));
        assert_eq!(err(&|o| o.ink = [0.0, 1.5, 0.0]), ("out_of_range", "ink"));
        assert_eq!(
            err(&|o| o.ink = [0.0, 0.0, f64::NAN]),
            ("not_finite", "ink")
        );
        // The bounds themselves are fine.
        let mut edge = HandOptions::new("hi");
        edge.size = MAX_SIZE;
        edge.width = MAX_WIDTH;
        edge.jitter = MAX_JITTER;
        edge.ink = [MIN_INK, MAX_INK, MIN_INK];
        assert_eq!(edge.validate(), Ok(()));
        // A Paper name that is not one.
        let bad = "lined".parse::<Paper>().unwrap_err();
        assert_eq!((bad.code(), bad.field()), ("unknown_paper", "paper"));
        assert_eq!("ruled".parse::<Paper>(), Ok(Paper::Ruled));
    }

    #[test]
    fn the_error_text_names_the_field_and_range() {
        let mut o = HandOptions::new("hi");
        o.size = 5000.0;
        assert_eq!(
            o.validate().unwrap_err().to_string(),
            "size must be 0 to 2000, got 5000"
        );
    }
}
