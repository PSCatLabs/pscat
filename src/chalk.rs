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

use crate::{Interp, PsError};

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
                "unsupported-character" => write!(f, "unsupported character: {token}"),
                "unsupported-environment" => {
                    write!(f, "unsupported environment: \\begin{{{token}}}")
                }
                "unsupported-delimiter" => write!(f, "unsupported delimiter: {token}"),
                "missing-argument" => write!(f, "{token} is missing an argument"),
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
    fn check(&self) -> Result<(), ChalkError> {
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
/// wrappers (`$...$`, `\[...\]`, an `align`/`gather`/... environment around
/// the whole) and labels removed, split at top-level `\\` row breaks (not
/// those inside braces or inside an environment such as `pmatrix` or
/// `cases`). Blank rows are dropped; none left is [`ChalkError::Empty`].
pub fn lines(latex: &str) -> Result<Vec<String>, ChalkError> {
    let mut s = strip_comments(latex)
        .chars()
        .map(|c| {
            if matches!(c, '\n' | '\r' | '\t') {
                ' '
            } else {
                c
            }
        })
        .collect::<String>();
    s = drop_labels(&s);
    loop {
        let t = s.trim();
        let inner = strip_delims(t).or_else(|| strip_env(t));
        match inner {
            Some(i) => s = i.to_string(),
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
    Ok(rows)
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

/// The inside of `$$...$$`, `$...$`, `\[...\]` or `\(...\)` around the whole.
fn strip_delims(t: &str) -> Option<&str> {
    for (open, close) in [("$$", "$$"), ("\\[", "\\]"), ("\\(", "\\)"), ("$", "$")] {
        if t.len() >= open.len() + close.len()
            && t.starts_with(open)
            && t.ends_with(close)
            && !t.ends_with(&format!("\\{close}"))
        {
            return Some(&t[open.len()..t.len() - close.len()]);
        }
    }
    None
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

/// The inside of a wrapper environment, when its `\begin` opens the input
/// and its matching `\end` closes it.
fn strip_env(t: &str) -> Option<&str> {
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
                        return Some(&t[body_start..i]);
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
    let rows = lines(latex)?;
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
    p.push_str("/ckdict 40 dict def\nckdict begin\n");
    p.push_str("/cklines [\n");
    for r in &rows {
        p.push_str("  ");
        p.push_str(&ps_string(r));
        p.push('\n');
    }
    p.push_str("] def\n");
    p.push_str(&format!("/cksize {} def\n", n(cfg.size)));
    let mut opts = format!("/Size cksize /Seed {seed} /Irregular {}", n(cfg.irregular));
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
                bo.push_str(&format!(
                    "\n   /Erased {} /Ghost {{\n{}   }}",
                    n(1.0 - b.ghosting),
                    ghost_paths(cfg, w, h)
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

/// Two or three ghost formulas for the board's `/Ghost`, at sizes and
/// places picked from the seed.
fn ghost_paths(cfg: &Config, w: f64, h: f64) -> String {
    let mut rng = Lcg(cfg.seed as u64 ^ 0x9e37_79b9_7f4a_7c15);
    rng.next();
    let count = 2 + (rng.next() * 2.0) as usize;
    let first = (rng.next() * GHOSTS.len() as f64) as usize;
    let mut out = String::new();
    for k in 0..count {
        let tex = GHOSTS[(first + k * 3) % GHOSTS.len()];
        let size = (cfg.size * (0.6 + 0.3 * rng.next())).max(8.0);
        // along the top and bottom of the board, where the centred
        // formula seldom is, a random way across (and partly off the
        // edge sometimes, as erased writing is)
        let x = (w * (0.02 + 0.55 * rng.next())).round();
        let y = if k % 2 == 0 {
            h - size * (1.0 + 0.4 * rng.next())
        } else {
            cfg.ledge() + size * (0.4 + 0.4 * rng.next())
        }
        .round();
        let gseed = cfg.seed.rem_euclid(MAX_SEED) + 7 * k as i64 + 9001;
        out.push_str(&format!(
            "     {} {} {} << /Size {} /Seed {gseed} /Irregular 1.3 >> mkpath\n",
            ps_string(tex),
            n(x),
            n(y),
            n(size.round())
        ));
    }
    out
}

/// Render `latex` on a board as `cfg` says, as PNG, SVG or PDF bytes.
pub fn render(latex: &str, cfg: &Config, format: Format) -> Result<Vec<u8>, ChalkError> {
    let d = driver(latex, cfg)?;
    run(&d, format)
}

/// Run a generated program and return the page in `format`.
pub fn run(d: &Driver, format: Format) -> Result<Vec<u8>, ChalkError> {
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
        assert_eq!(lines(""), Err(ChalkError::Empty));
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
