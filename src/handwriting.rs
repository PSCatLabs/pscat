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
//! in the wrap. [`has_glyph`] and [`drawable`] say which characters the font
//! does have, so a caller can refuse text that would draw nothing before it
//! spends a worker on it.
//!
//! # Which `lib/handscript.ps`
//!
//! The programs carry the font, `lib/handscript.ps`, as text, so the
//! interpreter that runs them needs no file access. This module reads that
//! file in the **calling** process, outside any interpreter, so an
//! interpreter's read roots (`Interp::set_read_roots`) and its limits do not
//! apply to it. By default ([`Library::Resolved`]) it is found the way
//! `run` finds a library, first match wins:
//!
//! 1. `$PSCAT_LIB`,
//! 2. `$PSCAT_HOME` (or `$PSCAT_ROOT`),
//! 3. the process's **current working directory**,
//! 4. the per-user data directory,
//! 5. the directory of the running executable,
//! 6. the copy compiled into the crate.
//!
//! A server whose working directory, home or install directory it does not
//! control can therefore end up with someone else's font. To use the
//! compiled-in font and nothing else, set [`HandOptions::library`] to
//! [`Library::Embedded`]: [`measure_source`] and [`render_source`] then embed
//! the compiled-in copy, and [`has_glyph_in`] and [`drawable_in`] answer for
//! it. One value covers all four; the environment, the working directory
//! and the disk are not consulted.
//!
//! # Checking characters without running PostScript
//!
//! For the compiled-in font, [`has_glyph_in`] and [`drawable_in`] with
//! [`Library::Embedded`] read a table checked in to this module, so they run no
//! PostScript and build no interpreter (a test recomputes the table from the
//! font and fails if they differ). [`has_glyph`] and [`drawable`], which
//! follow the resolver, give the same answer without PostScript when it lands
//! on the compiled-in copy. For any other text (an override) they run the
//! font's own definitions in a throwaway interpreter, bounded in steps, time
//! and (with [`TrackingAllocator`](crate::TrackingAllocator) installed)
//! memory, and with no file reads, and cache the answer by the library text.
//! They read the library the way [`render_source`] does, so they always agree
//! with it.
//!
//! ```
//! use pscat::handwriting::{HandOptions, Library, drawable_in};
//!
//! // One setting: the compiled-in font for the program and for the check.
//! let mut opts = HandOptions::new("hello");
//! opts.library = Library::Embedded;
//! assert!(drawable_in(opts.library, &opts.text)); // no PostScript ran
//! ```
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
use std::sync::OnceLock;

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

/// Which copy of the handwriting font, `lib/handscript.ps`, the functions in
/// this module use. Set it once, on [`HandOptions::library`], and pass the same
/// value to [`has_glyph_in`] and [`drawable_in`], so the program and the
/// character check can not disagree. See the module documentation for the
/// search order.
///
/// `#[non_exhaustive]`: keep a `_` arm.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Library {
    /// Search for the file the way `run` does: `$PSCAT_LIB`,
    /// `$PSCAT_HOME`, the working directory, the per-user data directory, the
    /// executable's directory, then the compiled-in copy. What the module
    /// always did, and the default.
    #[default]
    Resolved,
    /// The copy compiled into the crate, and nothing from the environment,
    /// the working directory or the disk.
    Embedded,
}

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
    /// Which `lib/handscript.ps` [`measure_source`] and [`render_source`] carry.
    /// [`Library::Resolved`] by default; [`Library::Embedded`] pins the
    /// compiled-in copy. Pass the same value to [`has_glyph_in`] and
    /// [`drawable_in`]. It is not part of the program's options dictionary.
    pub library: Library,
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
            library: Library::Resolved,
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

/// The name of the handwriting library.
const LIBRARY_NAME: &str = "lib/handscript.ps";

/// The handwriting library, `lib/handscript.ps`, as text, so a program is
/// self-contained and the interpreter that runs it needs no file access.
/// `Resolved` is what the resolver finds (`$PSCAT_HOME` and the other roots
/// first, then the copy compiled in); only a build with no embedded copy at
/// all falls back to a `run`. `Embedded` is the compiled-in copy, never a
/// `run` (which would let the interpreter go looking on disk).
///
/// # Panics
///
/// `Embedded` panics if the crate was built without `lib/handscript.ps`,
/// which `build.rs` and the package's file list rule out.
fn library_text(which: Library) -> String {
    match which {
        Library::Embedded => String::from_utf8_lossy(embedded_library()).into_owned(),
        Library::Resolved => match crate::paths::open(LIBRARY_NAME) {
            Some(r) => String::from_utf8_lossy(&r.bytes).into_owned(),
            None => format!("({LIBRARY_NAME}) run\n"),
        },
    }
}

/// The bytes of the compiled-in `lib/handscript.ps`. `build.rs` embeds every
/// `lib/**/*.ps` that is in the source tree, and the package ships `lib/`
/// (`Cargo.toml` `include`), so a build always has it; a test checks. Only
/// [`Library::Embedded`] programs need it (`Resolved` falls back to a `run`).
fn embedded_library() -> &'static [u8] {
    crate::paths::embedded(LIBRARY_NAME).expect("lib/handscript.ps is compiled in")
}

/// The ASCII characters the compiled-in font draws ink for, in code order:
/// what [`discover_glyphs`] finds over `lib/handscript.ps`. Checked in so the
/// compiled-in font needs no PostScript to answer; the test
/// `the_checked_in_table_is_what_discovery_finds` fails, and prints the line to
/// paste here, when the font changes.
const EMBEDDED_GLYPHS: &str = "!',-.0123456789?abcdefghijklmnopqrstuvwxyz";

/// Whether the compiled-in font draws ink for `c`, which is already lowercase.
fn embedded_has(c: char) -> bool {
    EMBEDDED_GLYPHS.contains(c)
}

/// [`EMBEDDED_GLYPHS`] as a table by character code.
fn embedded_table() -> [bool; 128] {
    let mut table = [false; 128];
    for b in EMBEDDED_GLYPHS.bytes() {
        table[usize::from(b)] = true;
    }
    table
}

/// The PostScript that prints the character codes the font draws ink for, one
/// per line. It reads the same tables `BuildChar` and the wrap read: a code's
/// name in `/Encoding`, that name in `/CharDefs` (a name with no entry is
/// `.notdef`), and a glyph counts when it has at least one stroke beyond its
/// advance width. `space` and `.notdef` are only an advance, so they do not.
const GLYPH_TABLE_PROGRAM: &str = "\
HandScriptDict begin\n\
0 1 127 {\n\
  /c exch def\n\
  Encoding c get CharDefs 1 index known not { pop /.notdef } if\n\
  CharDefs exch get length 1 gt { c = } if\n\
} for\n\
end\n";

/// Steps, wall-clock time and (when the process has `TrackingAllocator`
/// installed) heap growth the glyph discovery may use. Loading the library and
/// reading 128 codes takes a few tens of thousands of steps, a few
/// milliseconds and well under a megabyte; the caps only stop a library that
/// never finishes or that allocates without end.
const DISCOVERY_STEPS: u64 = 5_000_000;
const DISCOVERY_TIME: std::time::Duration = std::time::Duration::from_secs(2);
const DISCOVERY_MEMORY: usize = 64 << 20;

// How many interpreters discovery has built on this thread. This module builds
// an interpreter nowhere else, so a test that sees it stand still has shown a
// call ran no PostScript.
#[cfg(test)]
thread_local! {
    static INTERPS_BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// What discovery found, and whether it is safe to remember. A memory-limit
/// stop is not remembered: the count is process-wide, so other threads'
/// allocation could have caused it, and the next call should try again.
struct Discovery {
    table: [bool; 128],
    cacheable: bool,
}

/// Which ASCII codes the font in `library_src` draws ink for, found by running
/// its own tables. The run is bounded (steps, time and memory) and reads no
/// files, so a library that loops, allocates without end or tries to `run`
/// something yields no glyphs rather than hanging the caller.
fn discover_glyphs(library_src: &str) -> Discovery {
    #[cfg(test)]
    INTERPS_BUILT.with(|n| n.set(n.get() + 1));
    let mut found = Discovery {
        table: [false; 128],
        cacheable: true,
    };
    let mut interp = crate::Interp::new();
    let no_extra_roots: [&str; 0] = [];
    let mut limits = crate::Limits::none()
        .with_max_steps(DISCOVERY_STEPS)
        .with_max_time(DISCOVERY_TIME);
    if crate::memtrack::installed() {
        limits = limits.with_max_memory(DISCOVERY_MEMORY);
    }
    if interp.set_limits(limits).is_err() || interp.set_read_roots(&no_extra_roots).is_err() {
        found.cacheable = false;
        return found;
    }
    interp.capture_stdout();
    let mut src = String::with_capacity(library_src.len() + GLYPH_TABLE_PROGRAM.len());
    src.push_str(library_src);
    src.push_str(GLYPH_TABLE_PROGRAM);
    match interp.run_str(&src) {
        Ok(()) => {
            let out = String::from_utf8_lossy(interp.captured_stdout()).into_owned();
            for code in out.lines().filter_map(|l| l.trim().parse::<usize>().ok()) {
                if let Some(slot) = found.table.get_mut(code) {
                    *slot = true;
                }
            }
        }
        Err(e) => found.cacheable = e.limit() != Some(crate::limits::Limit::Memory),
    }
    found
}

/// The glyph table for the library text `render_source` would embed right
/// now. The compiled-in text is answered from [`EMBEDDED_GLYPHS`] without
/// running anything. Tables are cached by the library text itself (hashed, then compared
/// on a hit, so a hash collision cannot return another font's table), so the
/// same bytes are never run twice and a changed library gets a fresh answer.
/// The lock is not held while discovery runs, and a poisoned lock is
/// recovered, so one failed or slow discovery never blocks or breaks later
/// calls.
fn glyph_table_for(library_src: &str) -> [bool; 128] {
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::{Mutex, PoisonError};
    if crate::paths::embedded(LIBRARY_NAME).is_some_and(|b| b == library_src.as_bytes()) {
        return embedded_table();
    }
    /// Distinct library texts kept. A process sees one or two in practice.
    const KEPT: usize = 8;
    type Cache = Mutex<HashMap<u64, (String, [bool; 128])>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    library_src.hash(&mut hasher);
    let key = hasher.finish();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((text, table)) = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
        && text == library_src
    {
        return *table;
    }
    let found = discover_glyphs(library_src);
    if found.cacheable {
        let mut map = cache.lock().unwrap_or_else(PoisonError::into_inner);
        if map.len() >= KEPT {
            map.clear();
        }
        map.insert(key, (library_src.to_string(), found.table));
    }
    found.table
}

/// Whether the handwriting font draws ink for `c`, under the same case rule
/// the note uses: ASCII letters are lowercased first, so `'A'` and `'a'`
/// agree.
///
/// A space is `false`: it advances the line but puts no ink down. So is any
/// character the font lacks, including every non-ASCII one: those advance as a
/// blank and draw nothing.
///
/// The answer comes from the font's own glyph table in `lib/handscript.ps`,
/// the copy [`render_source`] would embed on this very call, so it follows the
/// font when the font gains a glyph and follows an override that changes. When
/// that copy is the compiled-in one the answer is a checked-in table and no
/// PostScript runs. Otherwise the table is found by running the font's
/// definitions in a throwaway interpreter, bounded in steps, time and (where
/// the process installs [`TrackingAllocator`](crate::TrackingAllocator))
/// memory, allowed to read no files, and cached by a hash of the library text;
/// a call reads the library the way [`render_source`] does, so use
/// [`drawable`] for a whole string rather than calling this per character in a
/// hot loop. If the table cannot be found (a broken or runaway
/// `lib/handscript.ps` in `$PSCAT_HOME`), every character is `false`.
///
/// This is [`has_glyph_in`] with [`Library::Resolved`]. To check the
/// compiled-in font whatever the environment holds, use
/// `has_glyph_in(Library::Embedded, c)`.
///
/// ```
/// use pscat::handwriting::has_glyph;
///
/// assert!(has_glyph('a') && has_glyph('A') && has_glyph('7') && has_glyph('?'));
/// assert!(!has_glyph(' ') && !has_glyph('@') && !has_glyph('\u{e9}'));
/// ```
pub fn has_glyph(c: char) -> bool {
    has_glyph_in(Library::Resolved, c)
}

/// [`has_glyph`] for the font `library` names: the one `opts.library` sets for
/// [`measure_source`] and [`render_source`]. With [`Library::Embedded`] it reads
/// a table checked in to this crate: it runs no PostScript, builds no
/// interpreter, and looks at no environment variable, directory or file.
///
/// ```
/// use pscat::handwriting::{Library, has_glyph_in};
///
/// assert!(has_glyph_in(Library::Embedded, 'Q') && !has_glyph_in(Library::Embedded, '@'));
/// ```
pub fn has_glyph_in(library: Library, c: char) -> bool {
    let c = c.to_ascii_lowercase();
    if !c.is_ascii() {
        return false;
    }
    match library {
        Library::Embedded => embedded_has(c),
        Library::Resolved => glyph_table_for(&library_text(Library::Resolved))[c as usize],
    }
}

/// Whether `text` has at least one character the font draws ([`has_glyph`]).
/// Text that is not `drawable` would render as a blank page, so a server can
/// refuse it before measuring. Empty text is not drawable.
///
/// This is [`drawable_in`] with [`Library::Resolved`].
///
/// ```
/// use pscat::handwriting::drawable;
///
/// assert!(drawable("Hi there"));
/// assert!(!drawable("  \n\t "));
/// assert!(!drawable("\u{4f60}\u{597d}"));
/// ```
pub fn drawable(text: &str) -> bool {
    drawable_in(Library::Resolved, text)
}

/// [`drawable`] for the font `library` names, with [`has_glyph_in`]'s
/// guarantee: [`Library::Embedded`] runs no PostScript.
pub fn drawable_in(library: Library, text: &str) -> bool {
    match library {
        Library::Embedded => text.chars().any(|c| embedded_has(c.to_ascii_lowercase())),
        Library::Resolved => {
            let table = glyph_table_for(&library_text(Library::Resolved));
            text.chars().any(|c| {
                let c = c.to_ascii_lowercase();
                c.is_ascii() && table[c as usize]
            })
        }
    }
}

/// The program that prints the number of lines `opts` wraps to, then a
/// newline. It contains `lib/handscript.ps` itself, so the interpreter that
/// runs it needs no file access: the copy [`HandOptions::library`] names,
/// which by default is read when you call this from `$PSCAT_LIB`,
/// `$PSCAT_HOME` and the other roots of the calling process, then the copy
/// compiled in (the module documentation has the order). Run it on a default page with the interpreter's stdout captured
/// and give the output to [`parse_line_count`]. The page height does not
/// affect the wrap, so none is needed.
pub fn measure_source(opts: &HandOptions) -> String {
    let mut src = library_text(opts.library);
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
    let mut src = library_text(opts.library);
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
    fn the_glyph_table_is_the_fonts_own() {
        // Pinned only by shape, not by a list: tests/handwriting_api.rs checks
        // every printable ASCII character against what the renderer draws.
        assert!(has_glyph('a') && has_glyph('A') && has_glyph('0') && has_glyph('!'));
        assert!(!has_glyph(' ') && !has_glyph('@') && !has_glyph('\n'));
        assert!(!has_glyph('\u{e9}') && !has_glyph('\u{0}') && !has_glyph('\u{7f}'));
        assert!(drawable("  x") && !drawable("") && !drawable(" \t\n") && !drawable("\u{e9}"));
    }

    #[test]
    fn discovery_is_bounded_and_follows_the_library_text() {
        let good = library_text(Library::Resolved);
        let table = glyph_table_for(&good);
        assert!(table[usize::from(b'a')] && table[usize::from(b'.')] && !table[32]);

        // A library that never finishes, or reads a file, gives no glyphs and
        // does not hold up (or break) the calls after it.
        let started = std::time::Instant::now();
        for bad in ["{ } loop", "%!\n1 0 div"] {
            assert_eq!(glyph_table_for(bad), [false; 128], "{bad}");
        }
        // A valid font whose only glyph appears if a file read succeeds: the
        // read is refused (no read roots), so it has none; run unconfined, the
        // same text does get its glyph, so the confinement is what stopped it.
        let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        let reads = format!(
            "/HandScriptDict 4 dict def HandScriptDict begin\n\
             /Encoding 256 array def 0 1 255 {{ Encoding exch /.notdef put }} for\n\
             /CharDefs 4 dict def CharDefs begin /.notdef [300] def /a [500 [0 0 1 1]] def end\n\
             ({manifest}) (r) file closefile Encoding 97 /a put end\n"
        );
        let mut open = crate::Interp::new();
        open.capture_stdout();
        open.run_str(&format!("{reads}{GLYPH_TABLE_PROGRAM}"))
            .unwrap();
        assert_eq!(open.captured_stdout(), b"97\n", "the probe font works");
        assert_eq!(
            glyph_table_for(&reads),
            [false; 128],
            "the read was refused"
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
        assert_eq!(glyph_table_for(&good), table);

        // A different font text is a different answer, and the old one still
        // answers for its own text.
        let changed = good.replace(
            "/period     [200 [60 25 75 15 65 5]] def",
            "/period [200] def",
        );
        assert_ne!(
            changed, good,
            "the period glyph def is where the test expects"
        );
        let t2 = glyph_table_for(&changed);
        assert!(!t2[usize::from(b'.')] && t2[usize::from(b'a')]);
        assert_eq!(glyph_table_for(&good), table);
    }

    fn built() -> usize {
        INTERPS_BUILT.with(std::cell::Cell::get)
    }

    #[test]
    fn the_checked_in_table_is_what_discovery_finds() {
        // Discovery is called directly: going through `glyph_table_for` would
        // hit the fast path and compare the table with itself.
        let text = String::from_utf8_lossy(embedded_library()).into_owned();
        let found = discover_glyphs(&text);
        assert!(
            found.cacheable,
            "discovery over the compiled-in font stopped"
        );
        let want: String = (0u8..128)
            .filter(|&b| found.table[usize::from(b)])
            .map(char::from)
            .collect();
        assert_eq!(
            EMBEDDED_GLYPHS, want,
            "lib/handscript.ps draws different characters than the checked-in table; \
             set EMBEDDED_GLYPHS in src/handwriting.rs to {want:?}"
        );
        assert_eq!(embedded_table(), found.table);
        assert!(EMBEDDED_GLYPHS.is_ascii() && !EMBEDDED_GLYPHS.contains(' '));
    }

    #[test]
    fn the_embedded_checks_run_no_postscript() {
        // The control: a library text no other test uses misses the cache, so
        // it must build an interpreter, once. If the counter could not move,
        // the zero below would prove nothing.
        let unique = format!(
            "{}\n% only the no-postscript test uses this line\n",
            String::from_utf8_lossy(embedded_library())
        );
        let before = built();
        assert!(glyph_table_for(&unique)[usize::from(b'a')]);
        assert_eq!(
            built(),
            before + 1,
            "the control did not build an interpreter"
        );

        let before = built();
        for c in ['a', 'Q', '.', ' ', '@', '\u{e9}'] {
            assert_eq!(
                has_glyph_in(Library::Embedded, c),
                c.is_ascii_alphabetic() || c == '.'
            );
        }
        assert!(drawable_in(Library::Embedded, "  hi") && !drawable_in(Library::Embedded, " \n"));
        assert!(!drawable_in(Library::Embedded, ""));
        assert_eq!(built(), before, "an embedded check built an interpreter");

        // The resolver landing on the compiled-in copy is answered the same way.
        if library_text(Library::Resolved).as_bytes() == embedded_library() {
            assert!(has_glyph('a') && drawable("x") && !drawable(" "));
            assert_eq!(
                built(),
                before,
                "a resolved check on the compiled-in font ran PostScript"
            );
        }
    }

    #[test]
    fn the_pinned_library_is_the_compiled_in_text() {
        assert!(crate::paths::embedded(LIBRARY_NAME).is_some());
        assert_eq!(Library::default(), Library::Resolved);
        assert_eq!(HandOptions::default().library, Library::Resolved);
        let mut o = HandOptions::new("hi");
        o.library = Library::Embedded;
        let text = String::from_utf8_lossy(embedded_library()).into_owned();
        assert_eq!(library_text(Library::Embedded), text);
        assert!(measure_source(&o).starts_with(&text));
        assert!(render_source(&o, 100).starts_with(&text));
        assert!(!render_source(&o, 100).contains(") run"), "no run fallback");
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
