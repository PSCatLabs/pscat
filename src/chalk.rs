//! LaTeX to a chalkboard (issue #555): the driver generator behind
//! `pscat chalk`, shared with anything else that wants a board (the MCP
//! `chalkboard_math` tool, a browser demo).
//!
//! It takes raw LaTeX, as a user types it (single backslashes, `\\` row
//! breaks, an `align` wrapper or `$...$` around it), and a [`Config`], and
//! writes one self-contained PostScript program ([`driver`]) that:
//!
//! 1. loads `lib/mathkit.ps` and lays the formula out with `mkalign` in a
//!    measuring pass (its ink thrown away), so a formula outside mathkit's
//!    subset fails here, before the board costs anything;
//! 2. shrinks the writing size until the block fits the board inside the
//!    margins and above the ledge, and centres it (or sets it at the left);
//! 3. dresses the board with drychalk's `dcboard` (or, with no board, sets
//!    a bare surface over a transparent page);
//! 4. writes the lines with `mkalign`, each inked with `dcchalk`.
//!
//! Same LaTeX, same config (the seed is part of it): byte-identical output.
//! Nothing the caller passes reaches the program unchecked: the TeX is a
//! PostScript string literal with `\`, `(` and `)` escaped, every number is
//! validated and printed by Rust, every name comes from a fixed list.
//!
//! [`render`] runs the driver and returns PNG, SVG or PDF bytes; a TeX
//! error comes back as a [`ChalkError::Latex`] naming the command or
//! character, never a PostScript error report. Pure: no files, no
//! environment, so it builds for wasm too.

use std::fmt;

use serde_json::{Map, Value};

use crate::{Interp, Limits, PsError};

/// The chalk colours drychalk's `dcchalkcolor` knows.
pub const CHALK_COLORS: [&str; 6] = ["white", "yellow", "pink", "blue", "green", "orange"];

/// What [`render`] produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// A PNG at the config's dpi.
    Png,
    /// An SVG document.
    Svg,
    /// A one-page PDF.
    Pdf,
}

/// Where the block of lines sits across the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// Centred between the margins.
    Center,
    /// Against the left margin.
    Left,
}

/// The board's tone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tone {
    /// Near-black slate.
    Slate,
    /// Dark green board.
    Greenboard,
    /// A board of this colour (three numbers 0..1).
    Rgb([f64; 3]),
}

/// The board dressing (`dcboard`'s keys). `None` in [`Config::board`] is
/// chalk alone on a transparent page.
#[derive(Clone, Debug, PartialEq)]
pub struct Board {
    /// The board's tone.
    pub tone: Tone,
    /// The dust film over the board, 0 (freshly washed) to 1.
    pub haze: f64,
    /// How much of an earlier lecture's erased writing still shows, 0 (none) to 1.
    pub ghosting: f64,
    /// Eraser swipe arcs, 0..40.
    pub swipes: u32,
    /// Finger or cloth wipes, 0..20.
    pub wipes: u32,
    /// Lighter smears of dust, 0..20.
    pub smudges: u32,
    /// Height in points of the dusty band along the bottom edge.
    pub ledge: f64,
}

impl Default for Board {
    fn default() -> Self {
        Board {
            tone: Tone::Slate,
            haze: 0.4,
            ghosting: 0.4,
            swipes: 6,
            wipes: 2,
            smudges: 3,
            ledge: 18.0,
        }
    }
}

/// Everything about a board but the LaTeX. [`Config::default`] is the
/// documented defaults; [`Config::from_json`] reads the JSON schema (every
/// key optional, unknown keys refused).
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    // layout
    /// The em size of the writing in points, before any shrink to fit.
    pub size: f64,
    /// How far the layout departs from clean TeX (mathkit's `/Irregular`): 0 clean.
    pub irregular: f64,
    /// The baselines' tilt in degrees; `None` lets mathkit pick a small one.
    pub slope: Option<f64>,
    /// Each glyph's wobble as a fraction of the size; `None` is mathkit's 0.016.
    pub jitter: Option<f64>,
    /// Where the block sits across the board.
    pub align: Align,
    /// Clear space in points between the writing and the board's edges.
    pub margin: f64,
    /// Page width in points.
    pub width: u32,
    /// Page height in points.
    pub height: u32,
    /// Resolution of a PNG.
    pub dpi: f32,
    // chalk
    /// One of [`CHALK_COLORS`].
    pub color: &'static str,
    /// How far the brightest deposit reads from the board toward the chalk, 0..1.
    pub strength: f64,
    /// How hard the stick presses (drychalk's `/Touch`), 0..1; `None` is 0.68
    /// on a board and 0.34 without one, where the marks have no grey levels
    /// and only a lighter touch shows the grain.
    pub pressure: Option<f64>,
    /// The stick's tilt in degrees: 0 writes with the tip, toward 90 on its side.
    pub tilt: f64,
    /// Loose dust beside each stroke, 0..1.
    pub dust: f64,
    /// Stroke width in points; `None` is 0.048 of the fitted size.
    pub stroke_width: Option<f64>,
    // board
    /// The board, or `None` for chalk on a transparent page.
    pub board: Option<Board>,
    /// The seed for the layout, the chalk and the board.
    pub seed: i64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            size: 64.0,
            irregular: 1.0,
            slope: None,
            jitter: None,
            align: Align::Center,
            margin: 40.0,
            width: 800,
            height: 400,
            dpi: 144.0,
            color: "white",
            strength: 0.55,
            pressure: None,
            tilt: 0.0,
            dust: 0.5,
            stroke_width: None,
            board: Some(Board::default()),
            seed: 1,
        }
    }
}

/// The largest seed magnitude: mathkit adds 101 per line and the chalk
/// 1000 per line to it, and both must stay inside a 32-bit integer.
pub const MAX_SEED: i64 = 1_000_000_000;

/// Why a board could not be made.
#[derive(Clone, Debug, PartialEq)]
pub enum ChalkError {
    /// A config key or value is wrong (the message names it).
    Config(String),
    /// There is no formula: the input is empty once comments, wrappers
    /// and blank rows are gone.
    Empty,
    /// The TeX is outside mathkit's subset or malformed. `kind` is
    /// mathkit's error without its `mk-` prefix (`unsupported-command`,
    /// `unsupported-character`, `missing-argument`, ...), `token` what it
    /// named (`\oplus`, `@`; empty if nothing).
    Latex {
        /// mathkit's error name, without `mk-`.
        kind: String,
        /// The command, character or environment it named.
        token: String,
    },
    /// The PostScript run failed for another reason: the interpreter's
    /// error report (a bug, not bad input).
    Render(String),
}

impl ChalkError {
    /// A stable code for tools: `config`, `empty`, `render`, or the
    /// LaTeX error's kind with `-` as `_` (`unsupported_command`, ...).
    pub fn code(&self) -> String {
        match self {
            ChalkError::Config(_) => "config".into(),
            ChalkError::Empty => "empty".into(),
            ChalkError::Latex { kind, .. } => kind.replace('-', "_"),
            ChalkError::Render(_) => "render".into(),
        }
    }
}

impl fmt::Display for ChalkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChalkError::Config(m) => write!(f, "config: {m}"),
            ChalkError::Empty => write!(f, "no formula given"),
            ChalkError::Latex { kind, token } => match kind.as_str() {
                "unsupported-command" => write!(
                    f,
                    "unsupported LaTeX command {token} (the supported subset is listed at the top of lib/mathkit.ps)"
                ),
                "unsupported-character" => {
                    write!(f, "unsupported character: {token}")?;
                    // name an invisible or non-ASCII one by its code point
                    match token.chars().next() {
                        Some(c) if !c.is_ascii_graphic() => write!(f, " (U+{:04X})", c as u32),
                        _ => Ok(()),
                    }
                }
                "unsupported-environment" => {
                    write!(f, "unsupported environment: \\begin{{{token}}}")
                }
                "unsupported-delimiter" => write!(f, "unsupported delimiter: {token}"),
                "missing-argument" => write!(f, "{token} is missing an argument"),
                "unclosed-math-delimiter" => write!(
                    f,
                    "math delimiter {token} is never closed: end it with {}",
                    closer(token)
                ),
                "unmatched-math-delimiter" => write!(
                    f,
                    "math delimiter {token} closes nothing: each \\( needs a \\) and each \\[ a \\], in order"
                ),
                "text-outside-math" => write!(
                    f,
                    "text outside the math delimiters ({token}...): a board sets only the formula, so write one formula, or move the words out"
                ),
                _ => {
                    let what = kind.replace('-', " ");
                    if token.is_empty() {
                        write!(f, "LaTeX error: {what}")
                    } else {
                        write!(f, "LaTeX error: {what}: {token}")
                    }
                }
            },
            ChalkError::Render(r) => write!(f, "render failed: {r}"),
        }
    }
}

impl std::error::Error for ChalkError {}

/// The closing delimiter for an opening one.
fn closer(open: &str) -> &'static str {
    MATH_DELIMS
        .iter()
        .find(|d| d.0 == open)
        .map_or("its closer", |d| d.1)
}

/// A mathkit error (`undefined` naming `mk-<kind>:<token>`) as a
/// [`ChalkError::Latex`]; `None` for any other error.
pub fn explain(err: &PsError) -> Option<ChalkError> {
    let PsError::Undefined(name) = err else {
        return None;
    };
    let rest = name.strip_prefix("mk-")?;
    let (kind, token) = rest.split_once(':').unwrap_or((rest, ""));
    Some(ChalkError::Latex {
        kind: kind.to_string(),
        token: token.to_string(),
    })
}

// ------------------------------------------------------------------ config

fn bad(msg: impl Into<String>) -> ChalkError {
    ChalkError::Config(msg.into())
}

fn num(key: &str, v: &Value, lo: f64, hi: f64) -> Result<f64, ChalkError> {
    match v.as_f64() {
        Some(x) if x.is_finite() && x >= lo && x <= hi => Ok(x),
        _ => Err(bad(format!(
            "{key} must be a number from {lo} to {hi}, got {v}"
        ))),
    }
}

/// A number strictly above `lo`.
fn positive(key: &str, v: &Value, hi: f64) -> Result<f64, ChalkError> {
    match v.as_f64() {
        Some(x) if x.is_finite() && x > 0.0 && x <= hi => Ok(x),
        _ => Err(bad(format!(
            "{key} must be a number above 0, at most {hi}, got {v}"
        ))),
    }
}

fn int(key: &str, v: &Value, lo: i64, hi: i64) -> Result<i64, ChalkError> {
    match v.as_i64() {
        Some(x) if (lo..=hi).contains(&x) => Ok(x),
        _ => Err(bad(format!(
            "{key} must be a whole number from {lo} to {hi}, got {v}"
        ))),
    }
}

fn group<'a>(
    key: &str,
    v: &'a Value,
    known: &[&str],
) -> Result<&'a Map<String, Value>, ChalkError> {
    let map = v
        .as_object()
        .ok_or_else(|| bad(format!("{key} must be an object, got {v}")))?;
    for k in map.keys() {
        if !known.contains(&k.as_str()) {
            return Err(bad(format!(
                "unknown key {key}.{k} (known: {})",
                known.join(", ")
            )));
        }
    }
    Ok(map)
}

/// The chalk colour named `s`, as the `'static` name.
pub fn chalk_color(s: &str) -> Option<&'static str> {
    CHALK_COLORS.iter().copied().find(|c| *c == s)
}

impl Config {
    /// Read a config from JSON text (see [`Config::from_value`]).
    pub fn from_json(text: &str) -> Result<Config, ChalkError> {
        let v: Value =
            serde_json::from_str(text).map_err(|e| bad(format!("not valid JSON: {e}")))?;
        Config::from_value(&v)
    }

    /// Read a config from a JSON value: an object with the optional groups
    /// `layout`, `chalk` and `board` (or `"board": false`) and the optional
    /// keys `seed` and `hand`. Every key is optional; a missing one keeps
    /// [`Config::default`]'s value; an unknown one is an error.
    pub fn from_value(v: &Value) -> Result<Config, ChalkError> {
        let mut c = Config::default();
        c.apply(v)?;
        Ok(c)
    }

    /// Lay the keys in `v` over this config (the same schema as
    /// [`Config::from_value`]).
    pub fn apply(&mut self, v: &Value) -> Result<(), ChalkError> {
        let top = group("config", v, &["layout", "chalk", "board", "seed", "hand"]).map_err(
            |e| match e {
                ChalkError::Config(m) => bad(m.replace("unknown key config.", "unknown key ")),
                e => e,
            },
        )?;
        if let Some(l) = top.get("layout") {
            let l = group(
                "layout",
                l,
                &[
                    "size",
                    "irregular",
                    "slope",
                    "jitter",
                    "align",
                    "margin",
                    "width",
                    "height",
                    "dpi",
                ],
            )?;
            for (k, v) in l {
                let key = format!("layout.{k}");
                match k.as_str() {
                    "size" => self.size = positive(&key, v, 1000.0)?,
                    "irregular" => self.irregular = num(&key, v, 0.0, 5.0)?,
                    "slope" => self.slope = Some(num(&key, v, -45.0, 45.0)?),
                    "jitter" => self.jitter = Some(num(&key, v, 0.0, 0.5)?),
                    "align" => {
                        self.align = match v.as_str() {
                            Some("center" | "centre") => Align::Center,
                            Some("left") => Align::Left,
                            _ => {
                                return Err(bad(format!(
                                    "{key} must be \"center\" or \"left\", got {v}"
                                )));
                            }
                        }
                    }
                    "margin" => self.margin = num(&key, v, 0.0, 4000.0)?,
                    "width" => self.width = int(&key, v, 1, 8000)? as u32,
                    "height" => self.height = int(&key, v, 1, 8000)? as u32,
                    "dpi" => self.dpi = num(&key, v, 9.0, 1200.0)? as f32,
                    _ => unreachable!(),
                }
            }
        }
        if let Some(ch) = top.get("chalk") {
            let ch = group(
                "chalk",
                ch,
                &["color", "strength", "pressure", "tilt", "dust", "width"],
            )?;
            for (k, v) in ch {
                let key = format!("chalk.{k}");
                match k.as_str() {
                    "color" => {
                        self.color = v.as_str().and_then(chalk_color).ok_or_else(|| {
                            bad(format!(
                                "{key} must be one of {}, got {v}",
                                CHALK_COLORS.join(", ")
                            ))
                        })?
                    }
                    "strength" => self.strength = num(&key, v, 0.0, 1.0)?,
                    "pressure" => self.pressure = Some(num(&key, v, 0.0, 1.0)?),
                    "tilt" => self.tilt = num(&key, v, 0.0, 90.0)?,
                    "dust" => self.dust = num(&key, v, 0.0, 1.0)?,
                    "width" => self.stroke_width = Some(positive(&key, v, 100.0)?),
                    _ => unreachable!(),
                }
            }
        }
        if let Some(b) = top.get("board") {
            match b {
                Value::Bool(false) => self.board = None,
                Value::Bool(true) => {
                    if self.board.is_none() {
                        self.board = Some(Board::default());
                    }
                }
                _ => {
                    let b = group(
                        "board",
                        b,
                        &[
                            "tone", "haze", "ghosting", "swipes", "wipes", "smudges", "ledge",
                        ],
                    )?;
                    let mut board = self.board.clone().unwrap_or_default();
                    for (k, v) in b {
                        let key = format!("board.{k}");
                        match k.as_str() {
                            "tone" => board.tone = tone(&key, v)?,
                            "haze" => board.haze = num(&key, v, 0.0, 1.0)?,
                            "ghosting" => {
                                board.ghosting = match v {
                                    Value::Bool(true) => Board::default().ghosting,
                                    Value::Bool(false) => 0.0,
                                    _ => num(&key, v, 0.0, 1.0)?,
                                }
                            }
                            "swipes" => board.swipes = int(&key, v, 0, 40)? as u32,
                            "wipes" => board.wipes = int(&key, v, 0, 20)? as u32,
                            "smudges" => board.smudges = int(&key, v, 0, 20)? as u32,
                            "ledge" => board.ledge = num(&key, v, 0.0, 4000.0)?,
                            _ => unreachable!(),
                        }
                    }
                    self.board = Some(board);
                }
            }
        }
        if let Some(s) = top.get("seed") {
            self.seed = int("seed", s, -MAX_SEED, MAX_SEED)?;
        }
        if let Some(h) = top.get("hand") {
            // Reserved until configurable hands land: only the one there is.
            if h.as_str() != Some("strokeglyphs") {
                return Err(bad(format!(
                    "hand: only \"strokeglyphs\" is available, got {h}"
                )));
            }
        }
        Ok(())
    }

    /// The page and margins leave room to write in.
    /// Every field is in its documented range (a `Config` built or edited
    /// in Rust skips `apply`'s checks), and the page and margins leave
    /// room to write in. The same validators as the JSON reader, so a
    /// non-finite number is refused too.
    pub fn check(&self) -> Result<(), ChalkError> {
        positive("layout.size", &Value::from(self.size), 1000.0)?;
        num("layout.irregular", &Value::from(self.irregular), 0.0, 5.0)?;
        if let Some(v) = self.slope {
            num("layout.slope", &Value::from(v), -45.0, 45.0)?;
        }
        if let Some(v) = self.jitter {
            num("layout.jitter", &Value::from(v), 0.0, 0.5)?;
        }
        num("layout.margin", &Value::from(self.margin), 0.0, 4000.0)?;
        int("layout.width", &Value::from(self.width), 1, 8000)?;
        int("layout.height", &Value::from(self.height), 1, 8000)?;
        num("layout.dpi", &Value::from(self.dpi), 9.0, 1200.0)?;
        if chalk_color(self.color).is_none() {
            return Err(bad(format!(
                "chalk.color must be one of {}, got {:?}",
                CHALK_COLORS.join(", "),
                self.color
            )));
        }
        num("chalk.strength", &Value::from(self.strength), 0.0, 1.0)?;
        if let Some(v) = self.pressure {
            num("chalk.pressure", &Value::from(v), 0.0, 1.0)?;
        }
        num("chalk.tilt", &Value::from(self.tilt), 0.0, 90.0)?;
        num("chalk.dust", &Value::from(self.dust), 0.0, 1.0)?;
        if let Some(v) = self.stroke_width {
            positive("chalk.width", &Value::from(v), 100.0)?;
        }
        if let Some(b) = &self.board {
            if let Tone::Rgb(rgb) = b.tone {
                for c in rgb {
                    num("board.tone", &Value::from(c), 0.0, 1.0)?;
                }
            }
            num("board.haze", &Value::from(b.haze), 0.0, 1.0)?;
            num("board.ghosting", &Value::from(b.ghosting), 0.0, 1.0)?;
            int("board.swipes", &Value::from(b.swipes), 0, 40)?;
            int("board.wipes", &Value::from(b.wipes), 0, 20)?;
            int("board.smudges", &Value::from(b.smudges), 0, 20)?;
            num("board.ledge", &Value::from(b.ledge), 0.0, 4000.0)?;
        }
        int("seed", &Value::from(self.seed), -MAX_SEED, MAX_SEED)?;
        let (aw, ah) = self.writing_box_size();
        if aw <= 0.0 || ah <= 0.0 {
            return Err(bad(format!(
                "a {}x{} page with margin {} and ledge {} leaves no room to write",
                self.width,
                self.height,
                self.margin,
                self.ledge()
            )));
        }
        Ok(())
    }

    fn ledge(&self) -> f64 {
        self.board.as_ref().map_or(0.0, |b| b.ledge)
    }

    fn writing_box_size(&self) -> (f64, f64) {
        (
            f64::from(self.width) - 2.0 * self.margin,
            f64::from(self.height) - 2.0 * self.margin - self.ledge(),
        )
    }
}

fn tone(key: &str, v: &Value) -> Result<Tone, ChalkError> {
    match v {
        Value::String(s) if s == "slate" => Ok(Tone::Slate),
        Value::String(s) if s == "greenboard" => Ok(Tone::Greenboard),
        Value::Array(a) if a.len() == 3 => {
            let mut rgb = [0.0; 3];
            for (i, x) in a.iter().enumerate() {
                rgb[i] = num(key, x, 0.0, 1.0)?;
            }
            Ok(Tone::Rgb(rgb))
        }
        _ => Err(bad(format!(
            "{key} must be \"slate\", \"greenboard\" or [r, g, b] (0..1), got {v}"
        ))),
    }
}

// -------------------------------------------------------------------- TeX

/// Environments that only wrap a display (and may hold `\\` rows and `&`
/// columns): stripped when they enclose the whole input.
const WRAPPERS: [&str; 16] = [
    "equation",
    "equation*",
    "align",
    "align*",
    "aligned",
    "gather",
    "gather*",
    "gathered",
    "split",
    "multline",
    "multline*",
    "flalign",
    "flalign*",
    "displaymath",
    "math",
    "eqnarray*",
];

/// Labels and numbering marks, meaningless on a board: dropped.
const DROPPED: [&str; 3] = ["\\nonumber", "\\notag", "\\label"];

/// The LaTeX as the lines of a display: comments, newlines, display
/// wrappers (`$...$`, `\[...\]`, `\(...\)`, one pair or several; an
/// `align`/`gather`/... environment around the whole) and labels removed, split at top-level `\\` row breaks (not
/// those inside braces or inside an environment such as `pmatrix` or
/// `cases`). Blank rows are dropped; none left is [`ChalkError::Empty`].
pub fn lines(latex: &str) -> Result<Vec<String>, ChalkError> {
    layout(latex).map(|(rows, _)| rows)
}

/// How the rows of a display sit against each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowAlign {
    /// On the first `&` or relation of each row: `align`, and whatever has
    /// no wrapper of its own.
    Relation,
    /// Each row centred on its own width: `gather`, as LaTeX sets it.
    Center,
}

/// [`lines`], and how its rows align. The innermost stripped environment
/// that sets rows decides (`equation` only holds a display): `gather`,
/// `gather*` and `gathered` centre each row, as LaTeX does; every other
/// wrapper, or none, aligns the rows on their relations.
fn layout(latex: &str) -> Result<(Vec<String>, RowAlign), ChalkError> {
    let mut row_align = RowAlign::Relation;
    let mut s = strip_comments(latex)
        .chars()
        // pasted LaTeX: a BOM and zero-width characters go, and every kind
        // of space (newlines, tabs, NBSP, thin spaces) is a plain space,
        // which mathkit ignores as TeX does
        .filter(|c| !matches!(c, '\u{feff}' | '\u{200b}'..='\u{200d}' | '\u{2060}'))
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect::<String>();
    s = drop_labels(&s);
    loop {
        let t = s.trim();
        let inner = match unwrap_math(t)? {
            Some(i) => Some(i),
            None => strip_env(t).map(|(name, body)| {
                if !matches!(name, "equation" | "equation*" | "displaymath" | "math") {
                    row_align = if matches!(name, "gather" | "gather*" | "gathered") {
                        RowAlign::Center
                    } else {
                        RowAlign::Relation
                    };
                }
                body.to_string()
            }),
        };
        match inner {
            Some(i) => s = i,
            None => {
                s = t.to_string();
                break;
            }
        }
    }
    let rows: Vec<String> = split_rows(&s)
        .into_iter()
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty())
        .collect();
    if rows.is_empty() {
        return Err(ChalkError::Empty);
    }
    Ok((rows, row_align))
}

/// `%` to the end of the line goes; `\%` (and the `%` of `\\%`'s row
/// break followed by a comment) is judged by pairing each backslash with
/// the character after it.
fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => {
                out.push(c);
                if let Some(n) = it.next() {
                    out.push(n);
                }
            }
            '%' => {
                for n in it.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// `\label{...}`, `\nonumber` and `\notag` removed (only as whole control
/// words: `\notagain` stays).
fn drop_labels(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    'scan: while i < b.len() {
        if b[i] == b'\\' {
            for word in DROPPED {
                let w = word.as_bytes();
                let end = i + w.len();
                if b[i..].starts_with(w) && !b.get(end).is_some_and(u8::is_ascii_alphabetic) {
                    let mut j = end;
                    if word == "\\label" {
                        while b.get(j) == Some(&b' ') {
                            j += 1;
                        }
                        if b.get(j) == Some(&b'{') {
                            let mut depth = 0;
                            while j < b.len() {
                                match b[j] {
                                    b'{' => depth += 1,
                                    b'}' => {
                                        depth -= 1;
                                        if depth == 0 {
                                            j += 1;
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                                j += 1;
                            }
                        }
                    }
                    out.push(' ');
                    i = j;
                    continue 'scan;
                }
            }
            // keep the backslash and the character after it together, so
            // `\\label` (a row break, then the letters) is not a label
            let ch_len = s[i + 1..].chars().next().map_or(0, char::len_utf8);
            out.push_str(&s[i..i + 1 + ch_len]);
            i += 1 + ch_len;
            continue;
        }
        let ch = s[i..].chars().next().expect("in bounds");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// The opening and closing math delimiters, longest opener first.
const MATH_DELIMS: [(&str, &str); 4] = [("$$", "$$"), ("\\[", "\\]"), ("\\(", "\\)"), ("$", "$")];

/// A `ChalkError::Latex` for a delimiter problem (`kind` without `mk-`).
fn delim_error(kind: &str, token: &str) -> ChalkError {
    ChalkError::Latex {
        kind: kind.into(),
        token: token.into(),
    }
}

/// `\left`, `\right` and the `\big` family name a fence, so a `\(` or `\]`
/// right after one is a (mathkit-unsupported) delimiter, not a math pair.
fn is_fence(word: &str) -> bool {
    matches!(word, "left" | "right")
}

/// The length of the control word (`\alpha`) or backslash-and-one-character
/// (`\\`, `\$`, `\(`) at the start of `rest`, which begins with a backslash.
fn escape_len(rest: &str) -> usize {
    let word = rest[1..]
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .count();
    if word > 0 {
        1 + word
    } else {
        1 + rest[1..].chars().next().map_or(0, char::len_utf8)
    }
}

/// Whether `t` has an unescaped `\(`, `\)`, `\[` or `\]` that is not
/// right after `\left` or `\right`: never valid mathkit input, so always a
/// delimiter. (`\\(` is a row break and `(`.)
fn has_backslash_delim(t: &str) -> bool {
    let mut rest = t;
    let mut after_fence = false;
    while let Some(c) = rest.chars().next() {
        if c != '\\' {
            after_fence &= c == ' ';
            rest = &rest[c.len_utf8()..];
            continue;
        }
        if !after_fence
            && ["\\(", "\\)", "\\[", "\\]"]
                .iter()
                .any(|d| rest.starts_with(d))
        {
            return true;
        }
        let len = escape_len(rest);
        after_fence = is_fence(&rest[1..len]);
        rest = &rest[len..];
    }
    false
}

/// The input with its math delimiters (`$$..$$`, `$..$`, `\[..\]`,
/// `\(..\)`, one or several pairs, nested or side by side) replaced by
/// spaces; `None` when it has none to remove. Each backslash is paired with
/// the character after it, so `\\(`, `\$` and `\right)` are not
/// delimiters, nor is a `\(` right after `\left` (mathkit names that one).
/// A `$` in the middle of plain input is left to mathkit, which
/// ignores it: the scan starts only for input that begins with `$` or holds a
/// backslash delimiter. Pieces between pairs are joined (`\(a\) + \(b\)` is
/// `a + b`), but a letter outside any pair and any `{...}` group is an
/// error: TeX sets it as text, which a board does not (`\text{and}` is fine).
fn unwrap_math(t: &str) -> Result<Option<String>, ChalkError> {
    if !t.starts_with('$') && !has_backslash_delim(t) {
        return Ok(None);
    }
    let mut out = String::with_capacity(t.len());
    // the open pairs, innermost last, as `MATH_DELIMS` indexes
    let mut open: Vec<usize> = Vec::new();
    // the depth of `{` groups outside every pair
    let mut braces = 0usize;
    let mut after_fence = false;
    let mut rest = t;
    while let Some(c) = rest.chars().next() {
        let fenced = after_fence;
        after_fence &= c == ' ';
        if c == '\\' {
            // a control word or a backslash and one character; a delimiter
            // unless right after `\left`/`\right`
            let len = escape_len(rest);
            let word = &rest[..len];
            if !fenced
                && matches!(word, "\\)" | "\\]")
                && open.last().is_none_or(|&i| MATH_DELIMS[i].1 != word)
            {
                return Err(delim_error("unmatched-math-delimiter", word));
            }
            if fenced || !matches!(word, "\\(" | "\\[" | "\\)" | "\\]") {
                after_fence = is_fence(&word[1..]);
                out.push_str(word);
                rest = &rest[len..];
                continue;
            }
        }
        let closes = open
            .last()
            .map(|&i| MATH_DELIMS[i].1)
            .filter(|cl| rest.starts_with(cl));
        if let Some(cl) = closes {
            open.pop();
            out.push(' ');
            rest = &rest[cl.len()..];
            continue;
        }
        if let Some(i) = MATH_DELIMS.iter().position(|d| rest.starts_with(d.0)) {
            open.push(i);
            out.push(' ');
            rest = &rest[MATH_DELIMS[i].0.len()..];
            continue;
        }
        if open.is_empty() {
            match c {
                '{' => braces += 1,
                '}' => braces = braces.saturating_sub(1),
                _ if braces == 0 && c.is_ascii_alphabetic() => {
                    return Err(delim_error("text-outside-math", &rest[..1]));
                }
                _ => {}
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    if let Some(&i) = open.last() {
        return Err(delim_error("unclosed-math-delimiter", MATH_DELIMS[i].0));
    }
    Ok(Some(out))
}

/// The control word starting at byte `i` (just after a backslash), if any.
fn word_at(b: &[u8], i: usize) -> &[u8] {
    let mut j = i;
    while j < b.len() && b[j].is_ascii_alphabetic() {
        j += 1;
    }
    &b[i..j]
}

/// The `{name}` after `\begin` or `\end` whose word ends at byte `i`:
/// the name and the byte after the closing brace.
fn env_name(s: &str, i: usize) -> Option<(&str, usize)> {
    let rest = &s[i..];
    let trimmed = rest.trim_start_matches(' ');
    let start = i + (rest.len() - trimmed.len());
    let inner = trimmed.strip_prefix('{')?;
    let close = inner.find('}')?;
    Some((&inner[..close], start + 1 + close + 1))
}

/// The name and the inside of a wrapper environment, when its `\begin`
/// opens the input and its matching `\end` closes it.
fn strip_env(t: &str) -> Option<(&str, &str)> {
    let b = t.as_bytes();
    if !t.starts_with("\\begin") || word_at(b, 1) != b"begin" {
        return None;
    }
    let (name, body_start) = env_name(t, 6)?;
    if !WRAPPERS.contains(&name) {
        return None;
    }
    // find the \end that closes this \begin
    let mut depth = 0usize;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            let w = word_at(b, i + 1);
            if w == b"begin" {
                depth += 1;
            } else if w == b"end" {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let (end_name, after) = env_name(t, i + 4)?;
                    if end_name == name && t[after..].trim().is_empty() {
                        return Some((name, &t[body_start..i]));
                    }
                    return None;
                }
            }
            i += 1 + w.len().max(1);
            continue;
        }
        i += 1;
    }
    None
}

/// Split at `\\` (and `\\*`, `\\[2pt]`) outside braces and environments.
fn split_rows(s: &str) -> Vec<String> {
    let b = s.as_bytes();
    let mut rows = Vec::new();
    let mut start = 0;
    let mut braces = 0i32;
    let mut envs = 0i32;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' => braces += 1,
            b'}' => braces -= 1,
            b'\\' => {
                if b.get(i + 1) == Some(&b'\\') {
                    if braces == 0 && envs == 0 {
                        rows.push(s[start..i].to_string());
                        let mut j = i + 2;
                        if b.get(j) == Some(&b'*') {
                            j += 1;
                        }
                        let k = j + s[j..].len() - s[j..].trim_start_matches(' ').len();
                        if b.get(k) == Some(&b'[')
                            && let Some(close) = s[k..].find(']')
                        {
                            j = k + close + 1;
                        }
                        start = j;
                        i = j;
                        continue;
                    }
                    i += 2;
                    continue;
                }
                let w = word_at(b, i + 1);
                if w == b"begin" {
                    envs += 1;
                } else if w == b"end" {
                    envs -= 1;
                }
                // a control symbol (`\{`, `\}`, `\,`) is two bytes and never
                // a brace; a multi-byte character after it is skipped whole
                let step = if w.is_empty() {
                    s[i + 1..].chars().next().map_or(0, char::len_utf8)
                } else {
                    w.len()
                };
                i += 1 + step;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    rows.push(s[start..].to_string());
    rows
}

/// A PostScript string literal holding `bytes`: `\`, `(` and `)` escaped,
/// everything else as it is.
fn ps_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    out.push('(');
    for c in text.chars() {
        if matches!(c, '\\' | '(' | ')') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push(')');
    out
}

/// A number as PostScript reads it: plain decimal, never an exponent.
fn n(x: f64) -> String {
    let x = if x == 0.0 { 0.0 } else { x }; // no "-0"
    let s = format!("{x}");
    debug_assert!(!s.contains(['e', 'E', 'N', 'i']), "{s}");
    s
}

// ----------------------------------------------------------------- driver

/// A generated program and the page it is meant for.
#[derive(Clone, Debug, PartialEq)]
pub struct Driver {
    /// The PostScript program.
    pub source: String,
    /// Page width in points.
    pub width: u32,
    /// Page height in points.
    pub height: u32,
    /// Resolution of a PNG.
    pub dpi: f32,
    /// The page background is transparent (no board).
    pub transparent: bool,
}

/// Erased writing for the board's ghosting: a few formulas from an
/// earlier lecture, picked and placed from the seed.
const GHOSTS: [&str; 8] = [
    r"\int_0^1 f(x)\,dx",
    r"a^2 + b^2 = c^2",
    r"\sum_{k=1}^{n} k = \frac{n(n+1)}{2}",
    r"\nabla \cdot \vec{E} = \frac{\rho}{\varepsilon_0}",
    r"F = ma",
    r"e^{i\pi} + 1 = 0",
    r"\lim_{x \to 0} \frac{\sin x}{x} = 1",
    r"\det(A - \lambda I) = 0",
];

/// A small deterministic generator for placing the ghosts (never the
/// PostScript `rand`): 0.0..1.0.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

const COLORS_RGB: [(&str, [f64; 3]); 6] = [
    ("white", [0.95, 0.95, 0.92]),
    ("yellow", [0.99, 0.91, 0.45]),
    ("pink", [0.98, 0.62, 0.74]),
    ("blue", [0.55, 0.78, 0.99]),
    ("green", [0.66, 0.92, 0.58]),
    ("orange", [1.00, 0.70, 0.40]),
];

/// The PostScript program that writes `latex` on a board as `cfg` says.
pub fn driver(latex: &str, cfg: &Config) -> Result<Driver, ChalkError> {
    cfg.check()?;
    let (rows, row_align) = layout(latex)?;
    let (w, h) = (f64::from(cfg.width), f64::from(cfg.height));
    let (aw, ah) = cfg.writing_box_size();
    let bx = cfg.margin;
    let by = cfg.margin + cfg.ledge();
    let seed = cfg.seed;

    let mut p = String::new();
    p.push_str("%!PS-Adobe-3.0\n%%Title: Chalkboard\n%%Creator: pscat chalk\n");
    p.push_str(&format!(
        "%%BoundingBox: 0 0 {} {}\n",
        cfg.width, cfg.height
    ));
    p.push_str("%%EndComments\n\n");
    p.push_str("% Generated by `pscat chalk` (src/chalk.rs). The TeX subset is mathkit's.\n");
    p.push_str("(lib/handscript.ps) run\n(lib/strokeglyphs.ps) run\n(lib/mathkit.ps) run\n\n");
    p.push_str("/ckdict 80 dict def\nckdict begin\n");
    p.push_str("/cklines [\n");
    for r in &rows {
        p.push_str("  ");
        p.push_str(&ps_string(r));
        p.push('\n');
    }
    p.push_str("] def\n");
    p.push_str(&format!("/cksize {} def\n", n(cfg.size)));
    let mut opts = format!("/Size cksize /Seed {seed} /Irregular {}", n(cfg.irregular));
    // a single row is centred by the block already: leave its layout alone
    if row_align == RowAlign::Center && rows.len() > 1 {
        opts.push_str(" /LineAlign /center");
    }
    if let Some(s) = cfg.slope {
        opts.push_str(&format!(" /Slope {}", n(s)));
    }
    if let Some(j) = cfg.jitter {
        opts.push_str(&format!(" /Jitter {}", n(j)));
    }
    p.push_str(&format!("/ckopts {{ << {opts} >> }} bind def\n"));
    p.push_str(&format!(
        "/ckaw {} def /ckah {} def /ckbx {} def /ckby {} def\n",
        n(aw),
        n(ah),
        n(bx),
        n(by)
    ));
    // The measuring pass: the lines laid out at 0 0, their ink thrown
    // away, and the union of the records' boxes kept. A TeX error stops
    // the program here, before the board.
    p.push_str(
        "\
% - -> -   ckx0 cky0 ckx1 cky1: the block's box with its first baseline at 0 0
/ckmeasure {
  gsave newpath
  cklines 0 0 ckopts dup /Ink { pop newpath } put mkalign
  grestore
  /ckx0 1e9 def /cky0 1e9 def /ckx1 -1e9 def /cky1 -1e9 def
  {
    /BBox get aload pop
    dup cky1 gt { /cky1 exch def } { pop } ifelse
    dup ckx1 gt { /ckx1 exch def } { pop } ifelse
    dup cky0 lt { /cky0 exch def } { pop } ifelse
    dup ckx0 lt { /ckx0 exch def } { pop } ifelse
  } forall
  /ckbw ckx1 ckx0 sub 1 max def /ckbh cky1 cky0 sub 1 max def
} bind def
/ckscale { ckaw ckbw div ckah ckbh div 2 copy gt { exch } if pop } bind def
ckmeasure
% shrink until it fits (the layout is close to linear in the size)
4 { ckscale 1 ge { exit } if /cksize cksize ckscale 0.99 mul mul def ckmeasure } repeat
",
    );
    match cfg.align {
        Align::Center => p.push_str("/ckox ckbx ckaw ckbw sub 2 div add ckx0 sub def\n"),
        Align::Left => p.push_str("/ckox ckbx ckx0 sub def\n"),
    }
    p.push_str("/ckoy ckby ckah ckbh sub 2 div add cky0 sub def\n");
    match cfg.stroke_width {
        Some(sw) => p.push_str(&format!("/ckw {} def\n", n(sw))),
        None => p.push_str("/ckw cksize 0.048 mul def\n"),
    }
    p.push_str("end\n\n");
    p.push_str(
        "(lib/artkit.ps) run\n(lib/paintkit.ps) run\n(lib/drykit.ps) run\n(lib/drychalk.ps) run\n\n",
    );
    p.push_str("ckdict begin\n");

    let rgb = COLORS_RGB
        .iter()
        .find(|(c, _)| *c == cfg.color)
        .map(|(_, v)| *v)
        .expect("validated colour");
    let mut under = String::new();
    match &cfg.board {
        Some(b) => {
            let mut bo = String::new();
            match b.tone {
                Tone::Slate => bo.push_str("/Surface /slate"),
                Tone::Greenboard => bo.push_str("/Surface /greenboard"),
                Tone::Rgb([r, g, bl]) => bo.push_str(&format!(
                    "/Surface /greenboard /Ground [{} {} {}]",
                    n(r),
                    n(g),
                    n(bl)
                )),
            }
            bo.push_str(&format!(
                " /Seed {seed} /Haze {} /Swipes {} /Wipes {} /Smudges {} /Ledge {}",
                n(b.haze),
                b.swipes,
                b.wipes,
                b.smudges,
                n(b.ledge)
            ));
            if b.ghosting > 0.0 {
                // placed now, clear of the measured writing, as plain
                // numbers: /Ghost only draws them
                p.push_str(&ghost_placement(cfg, w, h));
                bo.push_str(&format!(
                    "\n   /Erased {} /Ghost {{ ckghostdraw }}",
                    n(1.0 - b.ghosting)
                ));
            }
            p.push_str(&format!("0 0 {} {} << {bo} >> dcboard\n", n(w), n(h)));
        }
        None => {
            // No board: a bare surface for the chalk to catch on, and the
            // chalk blended against its own colour, so every mark is the
            // stick's colour and the gaps between are transparent.
            p.push_str(&format!("<< /Surface /slate /Seed {seed} >> dksurface\n"));
            under = format!(" /Under [{} {} {}]", n(rgb[0]), n(rgb[1]), n(rgb[2]));
        }
    }
    p.push_str(&format!("/{} dcchalkcolor\n", cfg.color));
    p.push_str(&format!(
        "/ckink {{ 1000 mul {seed} add << /Seed 3 -1 roll /Width ckw /Touch {} /Strength {} /Tilt {} /Dust {}{under} >> dcchalk }} bind def\n",
        n(cfg
            .pressure
            .unwrap_or(if cfg.board.is_some() { 0.68 } else { 0.34 })),
        n(cfg.strength),
        n(cfg.tilt),
        n(cfg.dust)
    ));
    p.push_str("newpath cklines ckox ckoy ckopts dup /Ink /ckink load put mkalign pop\n");
    p.push_str("end\nshowpage\n");

    Ok(Driver {
        source: p,
        width: cfg.width,
        height: cfg.height,
        dpi: cfg.dpi,
        transparent: cfg.board.is_none(),
    })
}

/// The PostScript that places the board's ghost writing (issue #698): two
/// or three formulas from [`GHOSTS`], at sizes and places picked from the
/// seed, each kept clear of the writing's measured box.
///
/// Rust only draws the dice; the writing's box exists only once the driver's
/// measuring pass has run (`ckx0`..`cky1`, `ckox`, `ckoy`), so the placing is
/// PostScript. The free board around that box (inflated by a margin) is four
/// regions, top, bottom, left and right; each ghost tries its regions in its
/// own order, at its size and then at smaller ones, takes the first it
/// fits in (and that no earlier ghost occupies), and is dropped when none
/// does: a crowded board has fewer ghosts, never one against the maths.
/// The ghost's box is its real path's (`pathbbox`, tilt and all), padded for
/// the chalk's width and dust. Leaves `ckghostdraw` for the `/Ghost`.
fn ghost_placement(cfg: &Config, w: f64, h: f64) -> String {
    let mut rng = Lcg(cfg.seed as u64 ^ 0x9e37_79b9_7f4a_7c15);
    rng.next();
    let count = 2 + (rng.next() * 2.0) as usize;
    let first = (rng.next() * GHOSTS.len() as f64) as usize;
    let mut out = String::new();
    out.push_str(&format!(
        "/ckgw {} def /ckgh {} def /ckgl {} def\n",
        n(w),
        n(h),
        n(cfg.ledge())
    ));
    out.push_str("/ckgspecs [\n");
    for k in 0..count {
        let tex = GHOSTS[(first + k * 3) % GHOSTS.len()];
        let size = (cfg.size * (0.6 + 0.3 * rng.next())).max(8.0);
        // the top and the bottom of the board first, in turns, where the
        // centred formula seldom is; the sides only when those are full
        let (u, v, flip) = (rng.next(), rng.next(), rng.next() < 0.5);
        let order = match (k % 2 == 0, flip) {
            (true, false) => "[0 1 2 3]",
            (true, true) => "[0 1 3 2]",
            (false, false) => "[1 0 2 3]",
            (false, true) => "[1 0 3 2]",
        };
        let gseed = cfg.seed.rem_euclid(MAX_SEED) + 7 * k as i64 + 9001;
        out.push_str(&format!(
            "  [{} {} {gseed} {} {} {order}]\n",
            ps_string(tex),
            n(size),
            n(u),
            n(v)
        ));
    }
    out.push_str("] def\n");
    out.push_str(GHOST_PLACER);
    out
}

/// The placing half of [`ghost_placement`], in `ckdict` after the measuring
/// pass (and ahead of the writing, which it measures along its path). Each spec is `[tex size seed u v [regions]]`; `ckgplaced` ends as
/// `[tex x y size seed]` for the ghosts that found room.
const GHOST_PLACER: &str = "\
% the writing's box, inflated: ghosts keep outside it, by 0.6 of the
% writing size above and below and 0.9 beside it (a ghost just past the end
% of a row reads as the row going on). The box is that of
% the strokes actually drawn (tilt included): the path the writing is inked
% along. Writing with no ink (a bare space) falls back to the line boxes.
/ckgm cksize 0.6 mul def /ckgmx cksize 0.9 mul def
gsave newpath cklines ckox ckoy ckopts mkalign pop
{ pathbbox } stopped {
  ckox ckx0 add ckoy cky0 add ckox ckx1 add ckoy cky1 add
} if
grestore
/ckgfy1 exch ckgm add def /ckgfx1 exch ckgmx add def
/ckgfy0 exch ckgm sub def /ckgfx0 exch ckgmx sub def
/ckgov ckgw 0.06 mul def /ckglo ckgl cksize 0.1 mul add def
% free regions [x0 y0 x1 y1]: top, bottom, left, right (the top and the
% bottom may hang a little off the sides, as erased writing does)
/ckgreg [
  [ ckgov neg ckgfy1 ckgw ckgov add ckgh ]
  [ ckgov neg ckglo ckgw ckgov add ckgfy0 ]
  [ ckgov neg ckglo ckgfx0 ckgh ]
  [ ckgfx1 ckglo ckgw ckgov add ckgh ]
] def
/ckgbox [ ] def
/ckgplaced [
  ckgspecs {
    /ckgs exch def
    /ckgdone false def
    [ 1 0.82 0.66 0.52 ] {
      ckgdone { pop exit } if
      ckgs 1 get mul round 6 max /ckgsz exch def
      gsave newpath
      ckgs 0 get 0 0 << /Size ckgsz /Seed ckgs 2 get /Irregular 1.3 >> mkpath
      pathbbox grestore
      /ckgby1 exch def /ckgbx1 exch def /ckgby0 exch def /ckgbx0 exch def
      % the box, padded for the chalk's width and dust
      /ckgp ckgsz 0.08 mul 3 add def
      /ckgbw ckgbx1 ckgbx0 sub ckgp 2 mul add def
      /ckgbh ckgby1 ckgby0 sub ckgp 2 mul add def
      ckgs 5 get {
        ckgdone { pop exit } if
        ckgreg exch get aload pop
        /ckgry1 exch def /ckgrx1 exch def /ckgry0 exch def /ckgrx0 exch def
        ckgrx1 ckgrx0 sub ckgbw ge ckgry1 ckgry0 sub ckgbh ge and {
          /ckgx ckgrx0 ckgrx1 ckgrx0 sub ckgbw sub ckgs 3 get mul add def
          /ckgy ckgry0 ckgry1 ckgry0 sub ckgbh sub ckgs 4 get mul add def
          true ckgbox {
            aload pop /ckgoy1 exch def /ckgox1 exch def /ckgoy0 exch def /ckgox0 exch def
            ckgox1 ckgx le ckgox0 ckgx ckgbw add ge or
            ckgoy1 ckgy le or ckgoy0 ckgy ckgbh add ge or
            not { pop false } if
          } forall
          {
            [ ckgs 0 get ckgx ckgbx0 sub ckgp add ckgy ckgby0 sub ckgp add ckgsz ckgs 2 get ]
            /ckgbox [ ckgbox aload pop [ ckgx ckgy ckgx ckgbw add ckgy ckgbh add ] ] def
            /ckgdone true def
          } if
        } if
      } forall
    } forall
  } forall
] def
/ckghostdraw {
  ckgplaced {
    aload pop /ckgsd exch def /ckgsz exch def
    << /Size ckgsz /Seed ckgsd /Irregular 1.3 >> mkpath
  } forall
} bind def
";

/// Render `latex` on a board as `cfg` says, as PNG, SVG or PDF bytes.
pub fn render(latex: &str, cfg: &Config, format: Format) -> Result<Vec<u8>, ChalkError> {
    let d = driver(latex, cfg)?;
    run(&d, format)
}

/// [`render`] under `limits` (issue #701): the whole board is one program
/// run, so the limits cover it as they cover one `pscat` render. A program
/// that passes one comes back as [`ChalkError::Render`] carrying the
/// interpreter's `limitexceeded (steps|time|memory)` report.
pub fn render_with_limits(
    latex: &str,
    cfg: &Config,
    format: Format,
    limits: Limits,
) -> Result<Vec<u8>, ChalkError> {
    let d = driver(latex, cfg)?;
    run_with_limits(&d, format, limits)
}

/// Run a generated program and return the page in `format`.
pub fn run(d: &Driver, format: Format) -> Result<Vec<u8>, ChalkError> {
    run_with_limits(d, format, Limits::none())
}

/// [`run`] under `limits` (issue #701).
pub fn run_with_limits(d: &Driver, format: Format, limits: Limits) -> Result<Vec<u8>, ChalkError> {
    let mut interp =
        Interp::with_page_scaled(d.width, d.height, d.dpi / 72.0).ok_or_else(|| {
            ChalkError::Config(format!("unusable page size {}x{}", d.width, d.height))
        })?;
    match format {
        Format::Png => {}
        Format::Svg => interp.enable_svg(),
        Format::Pdf => {
            interp.enable_pdf();
            let (title, author) = crate::pdf::scan_document_info(d.source.as_bytes());
            interp.gfx_mut().set_pdf_info(title, author);
        }
    }
    if d.transparent {
        interp.gfx_mut().set_transparent_background();
    }
    interp
        .set_limits(limits)
        .map_err(|e| ChalkError::Config(e.to_string()))?;
    // Nothing the program prints may reach the caller's stdout: an MCP
    // server speaks JSON-RPC on it.
    interp.capture_stdout();
    if let Err(e) = interp.run_source(d.source.as_bytes()) {
        return Err(explain(&e).unwrap_or_else(|| ChalkError::Render(interp.error_report(&e))));
    }
    let missing = || ChalkError::Render("the program produced no page".into());
    match format {
        Format::Png => interp
            .pages()
            .first()
            .ok_or_else(missing)?
            .to_png()
            .map_err(|e| ChalkError::Render(format!("PNG encoding: {e}"))),
        Format::Svg => interp
            .svg_pages()
            .and_then(|p| p.into_iter().next())
            .map(String::into_bytes)
            .ok_or_else(missing),
        Format::Pdf => interp.pdf_document().ok_or_else(missing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(s: &str) -> Vec<String> {
        lines(s).unwrap()
    }

    #[test]
    fn rows_split_only_at_top_level_breaks() {
        assert_eq!(l(r"a &= b \\ c &= d"), ["a &= b", "c &= d"]);
        assert_eq!(l(r"a \\* b \\[2pt] c\\"), ["a", "b", "c"]);
        assert_eq!(
            l(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix} = M"),
            [r"\begin{pmatrix} a & b \\ c & d \end{pmatrix} = M"]
        );
        assert_eq!(
            l(r"f = \begin{cases} 1 \\ 0 \end{cases} \\ g = 2"),
            [r"f = \begin{cases} 1 \\ 0 \end{cases}", "g = 2"]
        );
        assert_eq!(
            l(r"\sum_{\substack{i<m \\ j<n}} a"),
            [r"\sum_{\substack{i<m \\ j<n}} a"]
        );
        // escaped braces are not grouping
        assert_eq!(l(r"\{ a \\ b \}"), [r"\{ a", r"b \}"]);
    }

    #[test]
    fn wrappers_comments_and_labels_go() {
        assert_eq!(l("$x^2$"), ["x^2"]);
        assert_eq!(l("$$x^2$$"), ["x^2"]);
        assert_eq!(l(r"\[ x^2 \]"), ["x^2"]);
        assert_eq!(
            l("\\begin{align*}\n a &= b \\label{eq:1} \\\\\n c &= d \\notag\n\\end{align*}"),
            ["a &= b", "c &= d"]
        );
        assert_eq!(l("x % a comment\n+ y"), ["x  + y"]);
        assert_eq!(l(r"50\% = \frac12"), [r"50\% = \frac12"]);
        // a nested environment is not a wrapper, nor one that doesn't close the input
        assert_eq!(
            l(r"\left\{ \begin{aligned} a \\ b \end{aligned} \right."),
            [r"\left\{ \begin{aligned} a \\ b \end{aligned} \right."]
        );
        assert_eq!(
            l(r"\begin{aligned} a \end{aligned} + \begin{aligned} b \end{aligned}"),
            [r"\begin{aligned} a \end{aligned} + \begin{aligned} b \end{aligned}"]
        );
        assert_eq!(lines("  % only a comment\n \\\\ "), Err(ChalkError::Empty));
        // pasted text: a BOM, zero-width and non-breaking spaces
        assert_eq!(l("\u{feff}x\u{a0}=\u{2009}1\u{200b}"), ["x = 1"]);
        assert_eq!(lines(""), Err(ChalkError::Empty));
    }

    #[test]
    fn gather_rows_centre_and_everything_else_aligns() {
        let a = |s: &str| layout(s).unwrap().1;
        let gather = "a = b \\\\ ccc + d = e";
        for env in ["gather", "gather*", "gathered"] {
            assert_eq!(
                a(&format!("\\begin{{{env}}} {gather} \\end{{{env}}}")),
                RowAlign::Center
            );
        }
        // equation only holds a display; math delimiters go first
        assert_eq!(
            a(&format!(
                "\\begin{{equation}}\\begin{{gathered}} {gather} \\end{{gathered}}\\end{{equation}}"
            )),
            RowAlign::Center
        );
        assert_eq!(
            a(&format!(
                "$$\\begin{{gathered}} {gather} \\end{{gathered}}$$"
            )),
            RowAlign::Center
        );
        // the innermost environment that sets rows decides
        assert_eq!(
            a(&format!(
                "\\begin{{gather}}\\begin{{aligned}} {gather} \\end{{aligned}}\\end{{gather}}"
            )),
            RowAlign::Relation
        );
        assert_eq!(
            a(&format!(
                "\\begin{{aligned}}\\begin{{gathered}} {gather} \\end{{gathered}}\\end{{aligned}}"
            )),
            RowAlign::Center
        );
        // align, aligned, split, none, and a gather that does not wrap the input
        for tex in [
            format!("\\begin{{align*}} {gather} \\end{{align*}}"),
            format!("\\begin{{aligned}} {gather} \\end{{aligned}}"),
            format!("\\begin{{split}} {gather} \\end{{split}}"),
            format!("\\begin{{equation}} {gather} \\end{{equation}}"),
            gather.to_string(),
            r"\begin{gather} a \end{gather} + b \\ c".to_string(),
        ] {
            assert_eq!(a(&tex), RowAlign::Relation, "{tex}");
        }
    }

    #[test]
    fn several_math_pairs_are_joined_and_bad_ones_named() {
        // the issue's input, in each delimiter form
        assert_eq!(l(r"\(a\) + \(b\)"), ["a  +  b"]);
        assert_eq!(l(r"\[a\] \[b\]"), ["a   b"]);
        assert_eq!(l("$a$ + $b$"), ["a  +  b"]);
        assert_eq!(l("$$a$$ + $$b$$"), ["a  +  b"]);
        // adjacent pairs stay apart: `\alpha` + `b`, not `\alphab`
        assert_eq!(l(r"\(\alpha\)\(b\)"), [r"\alpha  b"]);
        // single pairs and nesting come out as before
        assert_eq!(l(r"\(a\)"), ["a"]);
        assert_eq!(l("$$x$$"), ["x"]);
        assert_eq!(l(r"\[ \(x\) \]"), ["x"]);
        assert_eq!(l(r"\[ x \\ y \]"), ["x", "y"]);
        // not delimiters: a row break then a bracket, `\right)`, an escaped dollar
        assert_eq!(l(r"\( a \\ (b) \)"), ["a", "(b)"]);
        assert_eq!(l(r"\[ \left( a \right) \]"), [r"\left( a \right)"]);
        assert_eq!(l(r"\( a \$ \)"), [r"a \$"]);
        // plain input with no delimiter at its start is left alone
        assert_eq!(l("a $ b"), ["a $ b"]);
        assert_eq!(l(r"a \\(b)"), ["a", "(b)"]);
        let e = |s: &str| lines(s).unwrap_err();
        let msg = |s: &str| e(s).to_string();
        assert_eq!(e(r"\(a\) + \(b").code(), "unclosed_math_delimiter");
        assert_eq!(
            msg(r"\(a\) + \(b"),
            r"math delimiter \( is never closed: end it with \)"
        );
        assert_eq!(e("$a$ + $b").code(), "unclosed_math_delimiter");
        assert_eq!(e(r"1 + 2\)").code(), "unmatched_math_delimiter");
        assert_eq!(e(r"\(a\]").code(), "unmatched_math_delimiter");
        assert_eq!(e(r"\(x\) where \(y\)").code(), "text_outside_math");
        assert!(msg(r"\(x\) where \(y\)").starts_with("text outside the math delimiters (w"));
        // `\left\(` is mathkit's to name, not a pair; `\text{..}` outside a pair is fine
        assert_eq!(l(r"\left\( b \right\)"), [r"\left\( b \right\)"]);
        assert_eq!(l(r"a \left\( b"), [r"a \left\( b"]);
        assert_eq!(l(r"\(a\) \text{and} \(b\)"), [r"a  \text{and}  b"]);
        assert_eq!(l(r"\(x\) = α"), ["x  = α"]);
        // control words outside a pair are math, not text
        assert_eq!(l(r"\(a\) \quad \(b\)"), [r"a  \quad  b"]);
        assert_eq!(msg("$$a$$ $b").lines().count(), 1);
    }

    #[test]
    fn the_string_literal_escapes_the_three_metacharacters() {
        assert_eq!(ps_string(r"\frac{(a)}{b}"), r"(\\frac{\(a\)}{b})");
        assert_eq!(ps_string("α"), "(α)");
    }

    #[test]
    fn numbers_print_plain() {
        assert_eq!(n(56.0), "56");
        assert_eq!(n(0.048), "0.048");
        assert_eq!(n(-0.0), "0");
        assert_eq!(n(1e-7), "0.0000001");
    }

    #[test]
    fn config_reads_every_group_and_refuses_unknown_keys() {
        let c = Config::from_json(
            r#"{"layout": {"size": 40, "irregular": 0, "align": "left", "width": 600},
                "chalk": {"color": "yellow", "tilt": 30, "width": 2},
                "board": {"tone": [0.1, 0.2, 0.15], "ghosting": false, "swipes": 0},
                "seed": 7, "hand": "strokeglyphs"}"#,
        )
        .unwrap();
        assert_eq!(c.size, 40.0);
        assert_eq!(c.align, Align::Left);
        assert_eq!(c.color, "yellow");
        assert_eq!(c.stroke_width, Some(2.0));
        let b = c.board.unwrap();
        assert_eq!(b.tone, Tone::Rgb([0.1, 0.2, 0.15]));
        assert_eq!((b.ghosting, b.swipes, b.haze), (0.0, 0, 0.4));
        assert_eq!(c.seed, 7);
        assert!(
            Config::from_json(r#"{"board": false}"#)
                .unwrap()
                .board
                .is_none()
        );
        for (bad, says) in [
            (r#"{"colour": "red"}"#, "unknown key colour"),
            (r#"{"layout": {"sise": 3}}"#, "unknown key layout.sise"),
            (
                r#"{"chalk": {"color": "mauve"}}"#,
                "chalk.color must be one of",
            ),
            (
                r#"{"layout": {"size": 0}}"#,
                "layout.size must be a number above 0",
            ),
            (r#"{"layout": {"size": "big"}}"#, "layout.size"),
            (
                r#"{"board": {"swipes": 2.5}}"#,
                "board.swipes must be a whole number",
            ),
            (r#"{"board": {"tone": "chalk"}}"#, "board.tone must be"),
            (r#"{"seed": 1e12}"#, "seed must be a whole number"),
            (r#"{"hand": "cursive"}"#, "only \"strokeglyphs\""),
            (r#"{"layout": {"align": "right"}}"#, "layout.align"),
            ("[1]", "config must be an object"),
            ("{", "not valid JSON"),
        ] {
            let e = Config::from_json(bad).unwrap_err().to_string();
            assert!(e.contains(says), "{bad}: {e}");
        }
    }

    #[test]
    fn a_config_built_in_rust_is_checked_too() {
        for (cfg, says) in [
            (
                Config {
                    color: "red",
                    ..Config::default()
                },
                "chalk.color",
            ),
            (
                Config {
                    size: f64::NAN,
                    ..Config::default()
                },
                "layout.size",
            ),
            (
                Config {
                    tilt: f64::INFINITY,
                    ..Config::default()
                },
                "chalk.tilt",
            ),
            (
                Config {
                    seed: i64::MAX,
                    ..Config::default()
                },
                "seed",
            ),
            (
                Config {
                    board: Some(Board {
                        tone: Tone::Rgb([0.0, 2.0, 0.0]),
                        ..Board::default()
                    }),
                    ..Config::default()
                },
                "board.tone",
            ),
        ] {
            let e = driver("x", &cfg).unwrap_err().to_string();
            assert!(e.contains(says), "{says}: {e}");
        }
        assert!(Config::default().check().is_ok());
    }

    #[test]
    fn a_page_with_no_room_is_refused() {
        let c = Config::from_json(r#"{"layout": {"width": 60, "margin": 40}}"#).unwrap();
        let e = driver("x", &c).unwrap_err().to_string();
        assert!(e.contains("leaves no room"), "{e}");
    }

    #[test]
    fn mathkit_errors_become_named_latex_errors() {
        let e = explain(&PsError::Undefined(r"mk-unsupported-command:\oplus".into())).unwrap();
        assert_eq!(e.code(), "unsupported_command");
        assert!(
            e.to_string()
                .starts_with(r"unsupported LaTeX command \oplus")
        );
        let e = explain(&PsError::Undefined("mk-unsupported-character:@".into())).unwrap();
        assert_eq!(e.to_string(), "unsupported character: @");
        let e = explain(&PsError::Undefined("mk-missing-closing-brace".into())).unwrap();
        assert_eq!(e.to_string(), "LaTeX error: missing closing brace");
        let e = explain(&PsError::Undefined(
            "mk-unsupported-character:\u{2603}".into(),
        ))
        .unwrap();
        assert_eq!(e.to_string(), "unsupported character: \u{2603} (U+2603)");
        assert!(explain(&PsError::Undefined("foo".into())).is_none());
        assert!(explain(&PsError::Typecheck).is_none());
    }

    #[test]
    fn the_driver_is_deterministic_and_carries_no_raw_input() {
        let c = Config::default();
        let tex = r"x) showpage (\frac{1}{2}";
        let a = driver(tex, &c).unwrap();
        assert_eq!(a, driver(tex, &c).unwrap());
        assert!(
            a.source.contains(r"(x\) showpage \(\\frac{1}{2})"),
            "{}",
            a.source
        );
    }
}
