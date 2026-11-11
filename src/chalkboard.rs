//! LaTeX on a chalkboard: the generator behind `pscat chalk` and the
//! plugin's `chalkboard_math` tool.
//!
//! [`BoardConfig::driver`] turns a formula, typed as LaTeX, into one
//! self-contained PostScript program that writes it in chalk on a board.
//! The program is returned, not run, so a caller can run it wherever it runs
//! untrusted PostScript (a sandboxed worker, under [`Limits`]). The LaTeX is
//! never code in that program: it is a string literal with `\`, `(` and `)`
//! escaped, and every number and name in it is checked and written by pscat.
//!
//! The board config is the JSON schema of `pscat chalk --json` (see
//! "From LaTeX to a board" in the dry-media guide): the groups `layout`,
//! `chalk` and `board` (or `"board": false` for chalk on a transparent
//! page), and the keys `seed` and `hand`. Every key is optional and an
//! unknown key is an error. Same LaTeX, same config: the same program, byte
//! for byte.
//!
//! A formula outside the supported LaTeX subset fails when the program
//! runs, as a PostScript `undefined` error that [`explain`] turns back into
//! the TeX error it stands for.
//!
//! ```
//! use pscat::chalkboard::{BoardConfig, explain};
//! use pscat::Interp;
//!
//! let mut config = BoardConfig::default();
//! config.apply(&serde_json::json!({ "layout": { "width": 400, "height": 200, "dpi": 72 } }))?;
//! let driver = config.driver(r"e^{i\pi} + 1 = 0")?;
//! let (w, h) = driver.page_size();
//!
//! let mut interp = Interp::with_page_scaled(w, h, driver.dpi() / 72.0).expect("valid page");
//! driver.prepare(&mut interp);
//! match interp.run_str(driver.source()) {
//!     Ok(()) => assert_eq!(interp.pages().len(), 1),
//!     Err(e) => panic!("{}", explain(&e).map_or(interp.error_report(&e), |c| c.to_string())),
//! }
//! # Ok::<(), pscat::chalkboard::ChalkError>(())
//! ```

use std::fmt;

use serde_json::Value;

#[cfg(doc)]
use crate::Limits;
use crate::chalk;
use crate::{Interp, PsError};

/// The chalk colours a board can be written in (the `chalk.color` key).
pub const CHALK_COLORS: [&str; 6] = chalk::CHALK_COLORS;

/// Everything about a board but the LaTeX: its page, resolution, layout,
/// chalk, board dressing and seed. [`Default`] is `pscat chalk`'s defaults
/// (an 800x400 pt slate board at 144 dpi, white chalk, seed 1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardConfig(chalk::Config);

impl BoardConfig {
    /// Read a config from JSON text in the `pscat chalk --json` schema,
    /// over the defaults.
    pub fn from_json(text: &str) -> Result<BoardConfig, ChalkError> {
        chalk::Config::from_json(text)
            .map(BoardConfig)
            .map_err(ChalkError)
    }

    /// Read a config from a JSON value in the `pscat chalk --json` schema,
    /// over the defaults.
    pub fn from_value(value: &Value) -> Result<BoardConfig, ChalkError> {
        chalk::Config::from_value(value)
            .map(BoardConfig)
            .map_err(ChalkError)
    }

    /// Lay the keys in `value` (the same schema) over this config. A key
    /// `value` leaves out keeps its current setting, so a tool can lay its
    /// own arguments over a base config one at a time:
    /// `{"layout": {"width": 500}}`, then `{"seed": 7}`. Each value is
    /// checked on its own; [`check`](Self::check) checks the whole.
    pub fn apply(&mut self, value: &Value) -> Result<(), ChalkError> {
        self.0.apply(value).map_err(ChalkError)
    }

    /// Check the config as a whole: every value in range, and room to write
    /// once the margins and the board's ledge are taken off the page.
    /// [`driver`](Self::driver) checks this too.
    pub fn check(&self) -> Result<(), ChalkError> {
        self.0.check().map_err(ChalkError)
    }

    /// The page size in points, width first.
    pub fn page_size(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }

    /// The resolution a raster of the board is rendered at, in dpi.
    pub fn dpi(&self) -> f32 {
        self.0.dpi
    }

    /// Whether the chalk is on a board (`false`: on a transparent page).
    pub fn has_board(&self) -> bool {
        self.0.board.is_some()
    }

    /// The PostScript program that writes `latex` on this board. `latex` is
    /// raw LaTeX as a person types it: single backslashes, `\\` row breaks,
    /// optionally inside `$...$`, `\[...\]` or an `align` environment. Its
    /// rows are split here and the TeX itself is laid out when the program
    /// runs.
    pub fn driver(&self, latex: &str) -> Result<Driver, ChalkError> {
        chalk::driver(latex, &self.0)
            .map(Driver)
            .map_err(ChalkError)
    }
}

/// A generated board program and the page it is meant for.
#[derive(Clone, Debug, PartialEq)]
pub struct Driver(chalk::Driver);

impl Driver {
    /// The PostScript program.
    pub fn source(&self) -> &str {
        &self.0.source
    }

    /// The program, taken out of the driver.
    pub fn into_source(self) -> String {
        self.0.source
    }

    /// The page size in points, width first.
    pub fn page_size(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }

    /// The resolution to render a raster at, in dpi:
    /// `Interp::with_page_scaled(w, h, dpi / 72.0)`.
    pub fn dpi(&self) -> f32 {
        self.0.dpi
    }

    /// Whether the page background is transparent (a config with
    /// `"board": false`).
    pub fn transparent(&self) -> bool {
        self.0.transparent
    }

    /// Set `interp` up to run this program as `pscat chalk` does: a
    /// transparent page background when the board has none, and the PDF's
    /// title from the program's `%%Title:` comment.
    ///
    /// Call it after [`Interp::enable_svg`] or [`Interp::enable_pdf`] (the
    /// PDF title is set on the PDF being recorded, so before `enable_pdf`
    /// it would be lost) and before running anything: a transparent
    /// background clears the page.
    pub fn prepare(&self, interp: &mut Interp) {
        let gfx = interp.gfx_mut();
        if self.0.transparent {
            gfx.set_transparent_background();
        }
        let (title, author) = crate::pdf::scan_document_info(self.0.source.as_bytes());
        gfx.set_pdf_info(title, author);
    }
}

/// Why a board could not be made.
#[derive(Clone, Debug, PartialEq)]
pub struct ChalkError(chalk::ChalkError);

impl ChalkError {
    /// A stable code for tools: `config` (a config key or value is wrong),
    /// `empty` (no formula), `render` (the program failed for a reason that
    /// is not the TeX's), or the LaTeX error's kind (`unsupported_command`,
    /// `unsupported_character`, `missing_argument`, ...).
    pub fn code(&self) -> String {
        self.0.code()
    }

    /// Whether the input is wrong in a way no run is needed to see: a config
    /// key or value, or no formula at all. False for a TeX error and for a
    /// render failure.
    pub fn is_input_error(&self) -> bool {
        matches!(
            self.0,
            chalk::ChalkError::Config(_) | chalk::ChalkError::Empty
        )
    }

    /// The command, character or environment a LaTeX error named (`\aleph`,
    /// `@`), if it named one.
    pub fn token(&self) -> Option<&str> {
        match &self.0 {
            chalk::ChalkError::Latex { token, .. } if !token.is_empty() => Some(token),
            _ => None,
        }
    }

    /// The message without its prefix. For a config error, the text names
    /// the key at fault by its path in the schema (`layout.width must be
    /// ...`, `color must be one of ...`), so a tool that takes the same
    /// settings as flat arguments can substitute its own argument names.
    /// [`Display`](fmt::Display) adds `config: ` in front of a config error.
    pub fn message(&self) -> String {
        match &self.0 {
            chalk::ChalkError::Config(m) => m.clone(),
            e => e.to_string(),
        }
    }
}

impl fmt::Display for ChalkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for ChalkError {}

/// A PostScript error raised by a generated board, as the TeX error it
/// stands for: the program reports a formula outside the supported subset
/// as an `undefined` error naming `mk-<kind>:<token>`. `None` for any other
/// error.
///
/// ```
/// let err = pscat::PsError::Undefined("mk-unsupported-command:\\aleph".into());
/// let tex = pscat::chalkboard::explain(&err).expect("a TeX error");
/// assert_eq!(tex.code(), "unsupported_command");
/// assert_eq!(tex.token(), Some("\\aleph"));
/// ```
pub fn explain(err: &PsError) -> Option<ChalkError> {
    chalk::explain(err).map(ChalkError)
}
