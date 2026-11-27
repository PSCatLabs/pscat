//! Self-check / lint mode (issue #17): heuristic checks an agent driving
//! pscat can run against a finished program to catch whole classes of
//! silent failure — a blank page, a forgotten `grestore`, stuff left on
//! the stack — without eyeballing the rendered PNG.
//!
//! These are heuristics, not correctness rules: a program can
//! legitimately leave data on the operand stack, or paint nothing on
//! purpose (a library file that only defines procedures). Findings are
//! advisory — surfaced to a human/agent to interpret, never fatal on
//! their own.

use tiny_skia::Pixmap;

use crate::object::PsString;
use crate::{Interp, Object, Value};

pub struct LintFinding {
    /// A short, stable slug identifying the check — for filtering/
    /// grepping, not for display on its own.
    pub check: &'static str,
    pub message: String,
}

/// The checks whose findings can *explain* a run that ended in an error
/// (issue #1110), so `--lint` still reports them after one. The rest read
/// where the program ended (a page with no ink, an open `gsave` or `begin`,
/// values left on the stack), and an aborted run ends wherever the error hit,
/// so those describe the abort and are dropped (#942, #947).
///
/// `shadowed-operator` reads `userdict`, which still holds every `def` that
/// ran before the error: `/sub (hello) def` and a later `5 3 sub` is a
/// `typecheck` that never mentions `sub`, and this finding names the cause.
///
/// `pscat-mcp`'s `split_lint_findings` (this crate's `mcp_server`) uses the
/// same list, so the two cannot drift.
pub const ERROR_EXPLAINING_CHECKS: &[&str] = &["shadowed-operator"];

/// Whether `check` (a [`LintFinding::check`] slug) is in
/// [`ERROR_EXPLAINING_CHECKS`].
pub fn explains_an_error(check: &str) -> bool {
    ERROR_EXPLAINING_CHECKS.contains(&check)
}

/// Run every lint check against a finished (or crashed — the checks are
/// still meaningful on a partial canvas) interpreter. `render_checks`
/// gates the checks that only make sense when a page was actually meant
/// to be produced — pass `false` for a pure computation (e.g. an
/// `eval`-style snippet with no `showpage`), where an empty canvas and a
/// result sitting on the operand stack are both the normal, intended
/// outcome rather than mistakes.
pub fn check(interp: &Interp, render_checks: bool) -> Vec<LintFinding> {
    check_with_pages(interp, render_checks, &DeclaredPages::None)
}

/// [`check`] plus the program's own `%%Pages:` declaration (issue #95),
/// which [`scan_declared_pages`] reads from the source.
///
/// A separate entry point rather than an extra argument on `check`:
/// both are public API, and a signature change would break any caller
/// outside this crate for a parameter almost all of them would pass
/// `None` to (Codex review, round 1).
pub fn check_with_pages(
    interp: &Interp,
    render_checks: bool,
    declared_pages: &DeclaredPages,
) -> Vec<LintFinding> {
    let mut findings = Vec::new();
    if render_checks {
        check_blank_pages(&pages_with_ink_flags(interp), &mut findings);
        check_declared_pages(interp, declared_pages, &mut findings);
    }
    check_gsave_balance(interp, &mut findings);
    check_stack_leaks(interp, render_checks, &mut findings);
    check_shadowed_operators(interp, &mut findings);
    check_utf8_in_byte_font(interp, &mut findings);
    findings
}

/// What `--lint` finds in one `--bands` band's interpreter (issue #847):
/// [`check_with_pages`] with `render_checks` on and without the blank-page
/// check. A band-aware program skips the work outside its band, so one
/// band's canvas can be empty when the page isn't; `bands.rs` runs
/// [`blank_page_findings`] on the stitched pages instead. What is left
/// (the declared page count, `gsave` depth, the operand and dictionary
/// stacks) is program behaviour, the same in every band unless the program
/// branches on `/BandIndex`.
pub fn check_band(interp: &Interp, declared_pages: &DeclaredPages) -> Vec<LintFinding> {
    let mut findings = Vec::new();
    check_declared_pages(interp, declared_pages, &mut findings);
    check_gsave_balance(interp, &mut findings);
    check_stack_leaks(interp, true, &mut findings);
    check_shadowed_operators(interp, &mut findings);
    check_utf8_in_byte_font(interp, &mut findings);
    findings
}

/// The blank-page check over finished pages, each with whether anything was
/// painted on it since the previous page (the ink flag).
pub fn blank_page_findings(pages: &[(&Pixmap, bool)]) -> Vec<LintFinding> {
    let mut findings = Vec::new();
    check_blank_pages(pages, &mut findings);
    findings
}

/// The pages `--png` would write: every emitted page, plus the live
/// canvas whenever nothing has emitted it yet (`has_trailing_art`) or
/// nothing was ever emitted at all — the same rule `finish_headless`
/// uses to decide what a `--png` without a `showpage` should write.
/// (Public so `bands.rs`, in the binary crate, can read each band's ink flags.)
pub fn pages_with_ink_flags(interp: &Interp) -> Vec<(&Pixmap, bool)> {
    let gfx = interp.gfx();
    let mut pages: Vec<(&Pixmap, bool)> = gfx
        .pages()
        .iter()
        .zip(gfx.pages_had_ink().iter().copied())
        .collect();
    if pages.is_empty() || gfx.has_trailing_art() {
        pages.push((&gfx.pixmap, gfx.has_trailing_art()));
    }
    pages
}

/// What a program's DSC header says about its own page count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredPages {
    /// No `%%Pages:` header, or the explicit `(atend)` form — DSC's
    /// legitimate "the count comes later", which declares nothing.
    None,
    Count(usize),
    /// A `%%Pages:` whose value is neither a number nor `(atend)`.
    /// Kept as a distinct case rather than folded into `None`: a
    /// typo'd count that reads as "no declaration" would silently
    /// disable the very check it was written to enable (Codex review,
    /// round 1) — precisely the failure mode issue #95 exists to close.
    Malformed(String),
}

/// The page count a program declares about itself.
///
/// DSC's own header comment, not a new convention — which is the
/// point: a program that says how many pages it produces can be
/// checked against what it actually produced.
///
/// This exists for issue #95's rendering drivers, where the rule is
/// one `showpage` per independently-checked scenario: two scenarios
/// that accidentally share a page let the second one's ink hide the
/// first one's blank-page regression, and that mistake is one deleted
/// `showpage` away in ordinary editing. A declared count turns it from
/// a convention into something checkable.
///
/// Scanning stops at `%%EndComments` or the first non-comment line,
/// mirroring `pdf::scan_document_info` — column zero alone doesn't make
/// a line a header, and a document that embeds another one further
/// down would otherwise adopt the *embedded* count and compare it
/// against the outer document's output (Codex review, round 1).
pub fn scan_declared_pages(source: &[u8]) -> DeclaredPages {
    for raw_line in source.split(|&b| b == b'\n') {
        let line = String::from_utf8_lossy(raw_line);
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("%%Pages:") {
            let value = value.trim();
            if value.is_empty() {
                return DeclaredPages::Malformed(String::new());
            }
            // Only the first token is the count. Older DSC allows a
            // page-order operand after it (`%%Pages: 3 1`), and a lint
            // heuristic must not hard-fail on a form real files use —
            // under `--lint-strict` that would abort a run over a
            // header the check was never meant to judge, and under
            // plain `--lint` it would put a spurious finding in front
            // of every `pscat-mcp` caller (blank-context review, PR
            // #136).
            let first = value.split_whitespace().next().unwrap_or(value);
            if first == "(atend)" {
                return DeclaredPages::None;
            }
            return match first.parse() {
                Ok(n) => DeclaredPages::Count(n),
                Err(_) => DeclaredPages::Malformed(value.to_string()),
            };
        }
        if line.starts_with("%%EndComments") || !line.starts_with('%') {
            break;
        }
    }
    DeclaredPages::None
}

fn check_declared_pages(
    interp: &Interp,
    declared: &DeclaredPages,
    findings: &mut Vec<LintFinding>,
) {
    let declared = match declared {
        DeclaredPages::None => return,
        DeclaredPages::Malformed(value) => {
            findings.push(LintFinding {
                check: "page-count",
                message: format!(
                    "`%%Pages: {value}` is neither a count nor `(atend)`, so the \
                     declared-page check can't run"
                ),
            });
            return;
        }
        DeclaredPages::Count(n) => *n,
    };
    // Emitted pages only — deliberately *not* `pages_with_ink_flags`,
    // which appends the live trailing canvas. Counting that made a
    // driver whose *final* `showpage` was deleted still match its
    // declaration, so the one mistake this check exists to catch was
    // the one it missed (Codex review, round 8). The trailing canvas
    // remains the right rule for blank-page checking, which is about
    // what `--png` would write rather than about page boundaries.
    let actual = interp.gfx().pages().len();
    if actual != declared {
        findings.push(LintFinding {
            check: "page-count",
            message: format!(
                "the program declares `%%Pages: {declared}` but emitted {actual} \
                 showpage(s) — a missing or extra one"
            ),
        });
    }
}

fn check_blank_pages(pages: &[(&Pixmap, bool)], findings: &mut Vec<LintFinding>) {
    let total = pages.len();
    for (i, &(page, had_ink)) in pages.iter().enumerate() {
        // `had_ink` catches a page that's a lazy-erase repeat of the
        // previous one (two showpages with nothing painted between
        // them snapshot the identical, non-blank pixmap twice) even
        // when its pixels aren't literally blank; `is_blank` still
        // catches a first/only page that's untouched white.
        if !had_ink || is_blank(page) {
            let why = if had_ink {
                "every pixel is the untouched white background".to_string()
            } else {
                "nothing was painted since the previous page (repeats it, or the program never drew before this showpage)".to_string()
            };
            findings.push(LintFinding {
                check: "blank-page",
                message: format!("page {} of {total} has no ink — {why}", i + 1),
            });
        }
    }
}

/// Untouched device pixels are opaque white — the fill `Gfx::with_scale`
/// starts every page with (or fully transparent under `--transparent`). A deliberate solid fill (even solid white
/// paper drawn on purpose) isn't distinguishable from "nothing drawn"
/// this way, but that's a rare and harmless false negative; the
/// common case this catches is "the program never painted at all".
fn is_blank(pixmap: &Pixmap) -> bool {
    pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| *p == [255, 255, 255, 255] || p[3] == 0)
}

fn check_gsave_balance(interp: &Interp, findings: &mut Vec<LintFinding>) {
    let depth = interp.gfx().gsave_depth();
    if depth > 0 {
        findings.push(LintFinding {
            check: "gsave-imbalance",
            message: format!(
                "{depth} unmatched gsave(s) left open at the end of the program (missing grestore)"
            ),
        });
    }
}

/// A leaked multi-megabyte string is exactly the kind of thing worth a
/// leak warning about, so the preview can't just skip large operands —
/// but it must stay cheap regardless of how large they are.
/// `pscat-mcp`'s `render_postscript` runs this on every render, so an
/// unbounded preview becomes unbounded MCP latency/memory, not just an
/// unbounded message (Codex review, PR #59, round 2: truncating
/// `repr()`'s *output* still built the whole thing first — a 10M-byte
/// string produced a ~40MB intermediate before being cut to 80 chars).
/// Strings are previewed by slicing the bytes *before* escaping, never
/// escaping more than the limit; arrays/procedures report only their
/// length, never recursing into elements, since a short array of huge
/// strings would defeat a length-only bound on the array itself; names
/// are previewed by taking a bounded prefix of chars (round 3: a name
/// has no length limit here — `read_regular_run` just keeps consuming
/// non-delimiter bytes — and `n.to_string()`/`format!("/{n}")` would
/// otherwise copy the whole thing). Everything else (numbers, dicts,
/// ...) already has an inherently bounded `repr()`.
const OPERAND_PREVIEW_LIMIT: usize = 80;

fn preview(obj: &Object) -> String {
    match &obj.value {
        Value::String(s) => string_preview(s),
        Value::Array(a) => {
            let kind = if obj.executable { "procedure" } else { "array" };
            format!("<{kind} of {} element(s)>", a.len())
        }
        Value::Name(n) => {
            let total = n.len();
            let prefix: String = n.chars().take(OPERAND_PREVIEW_LIMIT).collect();
            if prefix.len() == total {
                obj.repr()
            } else {
                let sigil = if obj.executable { "" } else { "/" };
                format!("{sigil}{prefix}...<{total} bytes total>")
            }
        }
        _ => obj.repr(),
    }
}

fn string_preview(s: &PsString) -> String {
    let len = s.len();
    let take = OPERAND_PREVIEW_LIMIT.min(len);
    let escaped = crate::object::escape_string(&s.borrow_bytes()[..take]);
    if take < len {
        format!("({escaped}...<{len} bytes total>)")
    } else {
        format!("({escaped})")
    }
}

/// The checks a program silences for itself with a `%lint: allow NAME`
/// comment line (issue #1052). Only the two heuristics that can be wrong
/// about a deliberate choice can be silenced: `shadowed-operator` and
/// `utf8-in-byte-font` (#1412). The other checks are what `--lint-strict`
/// drivers depend on, and a program has no business turning them off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LintAllows {
    shadowed_operator: bool,
    utf8_in_byte_font: bool,
}

/// Read the `%lint: allow ...` comment lines of a program's source. A line
/// is a comment that starts (after any indentation) with `%lint: allow`,
/// followed by check names separated by spaces or commas; it may sit next
/// to the deliberate `def` it excuses, or in the header. Names this does
/// not know are ignored, so a typo does not silence anything else.
pub fn scan_lint_allows(source: &[u8]) -> LintAllows {
    let mut allows = LintAllows::default();
    for raw_line in source.split(|&b| b == b'\n') {
        let line = String::from_utf8_lossy(raw_line);
        let Some(rest) = line.trim_start().strip_prefix("%lint:") else {
            continue;
        };
        let Some(names) = rest.trim_start().strip_prefix("allow") else {
            continue;
        };
        if !names.starts_with(|c: char| c.is_whitespace() || c == ',') {
            continue;
        }
        for name in names.split(|c: char| c.is_whitespace() || c == ',') {
            match name {
                "shadowed-operator" => allows.shadowed_operator = true,
                "utf8-in-byte-font" => allows.utf8_in_byte_font = true,
                _ => {}
            }
        }
    }
    allows
}

impl LintAllows {
    /// `findings` without the checks the program allowed.
    pub fn filter(&self, mut findings: Vec<LintFinding>) -> Vec<LintFinding> {
        if self.shadowed_operator {
            findings.retain(|f| f.check != "shadowed-operator");
        }
        if self.utf8_in_byte_font {
            findings.retain(|f| f.check != "utf8-in-byte-font");
        }
        findings
    }
}

/// Most `shadowed-operator` findings reported for one program; the rest are
/// counted in a closing finding, so a prologue that redefines fifty names
/// stays readable.
const SHADOWED_SHOWN: usize = 10;

/// Where the operand stack grew, as the interpreter saw it token by token
/// (issue #1081), so `[stack-leak]` can say where the oldest leftover item
/// came from. The stack is sampled at each source token's boundary, where it
/// holds the result of every earlier token (a loop or a call has finished),
/// and growth is credited to the previous token's line.
///
/// Kept as runs, not one entry per item: `start` is the depth at which a run
/// of items begins, and a run is dropped when the depth falls back to it, so
/// `clear`, `aload` of 10 000 items or a loop that pushes a million values
/// cost a handful of entries.
#[derive(Default, Clone)]
pub(crate) struct StackTrace {
    /// The depth at the last sample.
    depth: usize,
    runs: Vec<StackRun>,
    /// What the bottom item was when the first run began. A token that
    /// consumes the whole stack and leaves another (`0 1 9 { ... } for`
    /// takes its three operands and may leave thirty values) is seen only
    /// as growth, since the depth is sampled between tokens; a changed
    /// bottom item says the old runs are gone. A guess: a bottom item that
    /// is replaced by an equal one keeps the old line, which is still the
    /// statement that started it.
    bottom: Option<Witness>,
}

/// Enough of an object to notice it was replaced: its kind and, for a
/// scalar, its value. Composites compare by kind alone.
#[derive(Clone, Copy, PartialEq)]
enum Witness {
    Int(i64),
    Real(u64),
    Bool(bool),
    Name(u32),
    Kind(&'static str),
}

impl Witness {
    fn of(o: &Object) -> Witness {
        match &o.value {
            Value::Integer(i) => Witness::Int(*i),
            Value::Real(r) => Witness::Real(r.to_bits()),
            Value::Boolean(b) => Witness::Bool(*b),
            Value::Name(n) => Witness::Name(n.id()),
            _ => Witness::Kind(o.type_name()),
        }
    }
}

#[derive(Clone)]
struct StackRun {
    start: usize,
    at: Origin,
}

/// A source line and, for a `run` file, the file.
#[derive(Clone, PartialEq)]
struct Origin {
    line: Option<usize>,
    file: Option<std::rc::Rc<str>>,
}

impl StackTrace {
    /// The stack is now `ostack`; the token that ran since the last sample
    /// was on `line` of `file`.
    pub(crate) fn note(
        &mut self,
        ostack: &[Object],
        line: Option<usize>,
        file: Option<&std::rc::Rc<str>>,
    ) {
        let depth = ostack.len();
        if self.depth > 0 && depth > 0 && self.bottom.is_some_and(|w| w != Witness::of(&ostack[0]))
        {
            // The old bottom item was consumed along the way.
            self.runs.clear();
            self.depth = 0;
        }
        if depth > self.depth {
            let at = Origin {
                line,
                file: file.cloned(),
            };
            // A run that continues on the same line stays one entry.
            if self.runs.last().is_none_or(|r| r.at != at) {
                if self.depth == 0 {
                    self.bottom = Some(Witness::of(&ostack[0]));
                }
                self.runs.push(StackRun {
                    start: self.depth,
                    at,
                });
            }
        } else {
            while self.runs.last().is_some_and(|r| r.start >= depth) {
                self.runs.pop();
            }
        }
        self.depth = depth;
    }

    #[cfg(test)]
    fn run_count(&self) -> usize {
        self.runs.len()
    }

    /// Where the oldest item on the finished `ostack` came from, as
    /// `line N` (` (file)` for a `run` file). The last token ran after the
    /// last sample, so it is noted first. `None` when no line is known.
    fn oldest(
        &self,
        ostack: &[Object],
        last: (Option<usize>, Option<&std::rc::Rc<str>>),
    ) -> Option<String> {
        let mut done = self.clone();
        done.note(ostack, last.0, last.1);
        let at = &done.runs.first()?.at;
        let file = at
            .file
            .as_deref()
            .map(|f| format!(" ({})", crate::interp::report_name(f)))
            .unwrap_or_default();
        Some(format!("line {}{file}", at.line?))
    }
}

/// Where a shadowing binding was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ShadowScope {
    /// `userdict`: the program's own global names (the usual `def`).
    User,
    /// `systemdict` itself, where `store` finds an operator's name.
    System,
}

/// The bindings of an operator's name to a plain value that the interpreter
/// saw as they happened (issue #1109), when tracing is on. Keyed by name and
/// scope, so a loop that redefines the name a million times holds one entry.
/// It supplies what the finished run cannot: the line the binding was made on.
///
/// Only `userdict` and `systemdict` are traced. A name bound in a local
/// dictionary (`begin ... end`) is the deliberate way to keep a variable named
/// like an operator out of the program's own names, and the issue's corpus
/// check showed it is used that way (`gallery/ring_of_type.ps` keeps a loop
/// bound as `/stop`), so it is not reported (notes/1109).
#[derive(Default)]
pub(crate) struct ShadowTrace {
    /// Per name and scope: where the program had reached when the binding
    /// was made, as `when the program reached line N` (` (file)` for a
    /// `run` file), `None` when the scanner's line is unknown.
    sites: std::collections::HashMap<(u32, ShadowScope), Option<String>>,
}

impl ShadowTrace {
    /// Record `name` bound to `value` in `scope` while `builtin` is the
    /// operator it would replace, or forget it when the new binding is no
    /// longer a shadow (a procedure wrapping the operator). A repeat keeps
    /// the line the shadow began on.
    pub(crate) fn note(
        &mut self,
        name: &crate::name::PsName,
        builtin: &Object,
        value: &Object,
        scope: ShadowScope,
        location: impl FnOnce() -> Option<String>,
    ) {
        let key = (name.id(), scope);
        match shadow_kind(builtin, value) {
            Some(_) => {
                self.sites.entry(key).or_insert_with(location);
            }
            // A later `/sub { ... } def` ends the shadow, except in
            // `systemdict`, where the operator it replaced is already gone.
            None if scope != ShadowScope::System => {
                self.sites.remove(&key);
            }
            None => {}
        }
    }

    fn location(&self, name: &str, scope: ShadowScope) -> Option<String> {
        let id = crate::name::intern(name).id();
        self.sites.get(&(id, scope)).cloned().flatten()
    }

    /// Forget the `systemdict` sites: a new program starts with none of
    /// the last one's refused stores (the `userdict` ones stay, as the
    /// bindings themselves do).
    pub(crate) fn forget_system(&mut self) {
        self.sites
            .retain(|(_, scope), _| *scope != ShadowScope::System);
    }
}

/// The word for the value's type when binding it over `builtin` is a
/// shadow, `None` when it is not: a procedure (wrapping an operator is the
/// deliberate way to redefine one) or the operator bound to its own name.
pub(crate) fn shadow_kind(builtin: &Object, value: &Object) -> Option<&'static str> {
    let Value::Operator(op) = &builtin.value else {
        return None;
    };
    match &value.value {
        Value::Array(_) if value.executable => return None,
        Value::Operator(bound) if bound.name == op.name => return None,
        _ => {}
    }
    let kind = value.type_name();
    Some(kind.strip_suffix("type").unwrap_or(kind))
}

/// Whether `o` is the `def` or `store` operator, or an executable name that
/// is spelt that way (#1338).
pub(crate) fn is_binding_op(o: &Object) -> bool {
    if !o.executable {
        return false;
    }
    match &o.value {
        Value::Operator(op) => matches!(op.name, "def" | "store"),
        Value::Name(n) => matches!(&**n, "def" | "store"),
        _ => false,
    }
}

/// Whether `body` spells the literal name `/name_id` before element `end`.
pub(crate) fn writes_name(body: &crate::object::PsArray, end: usize, name_id: u32) -> bool {
    body.with_slice(|els| spells_name(&els[..end.min(els.len())], name_id))
}

/// Whether the literal name `/name_id` is among `els`.
pub(crate) fn spells_name(els: &[Object], name_id: u32) -> bool {
    els.iter()
        .any(|o| !o.executable && matches!(&o.value, Value::Name(n) if n.id() == name_id))
}

/// UTF-8 text a show gave a font that reads bytes (issue #1412), noted as
/// the program ran when [`Interp::trace_utf8_text`] turned tracing on, so
/// `[utf8-in-byte-font]` can name the font, the characters and the line.
/// Bounded: [`UTF8_FONTS_SHOWN`] fonts, [`UTF8_CHARS_SHOWN`] distinct
/// characters each, so a program that shows a long UTF-8 text in a loop
/// costs a set lookup per show and no growth.
#[derive(Default)]
pub(crate) struct Utf8TextTrace {
    fonts: Vec<Utf8Font>,
    /// Fonts past [`UTF8_FONTS_SHOWN`] that took UTF-8 text.
    more_fonts: usize,
}

struct Utf8Font {
    name: String,
    chars: Vec<char>,
    /// More distinct characters than are kept.
    more_chars: bool,
    /// Where the program was at the first show in this font.
    site: Utf8Site,
}

/// Where the scanner was when the program showed UTF-8 text in a byte-mode
/// font: the line of the last source token scanned, which is the line that
/// ran the show when a procedure did (the position `ErrorDetails::line`
/// reports, not where the procedure was written).
#[derive(Clone, Default)]
pub(crate) struct Utf8Site {
    pub(crate) line: Option<usize>,
    /// The `run` file `line` is in, as the lint names it; `None` for the
    /// main program.
    pub(crate) run_file: Option<String>,
    /// The name the host gave the main program, when `line` is one of its
    /// lines. Not part of the lint message, which names run files only.
    pub(crate) main_file: Option<String>,
}

/// Fonts named by `[utf8-in-byte-font]`; the rest are counted in a closing
/// finding.
pub(crate) const UTF8_FONTS_SHOWN: usize = 5;
/// Distinct characters named per font.
pub(crate) const UTF8_CHARS_SHOWN: usize = 6;

impl Utf8TextTrace {
    pub(crate) fn clear(&mut self) {
        self.fonts.clear();
        self.more_fonts = 0;
    }

    /// What the program showed, for a host that reads it as data
    /// ([`Interp::utf8_text_findings`]).
    pub(crate) fn report(&self) -> crate::host::Utf8TextReport {
        crate::host::Utf8TextReport {
            fonts: self
                .fonts
                .iter()
                .map(|f| crate::host::Utf8TextFinding {
                    font: f.name.clone(),
                    characters: f.chars.clone(),
                    more_characters: f.more_chars,
                    line: f.site.line,
                    file: f
                        .site
                        .line
                        .and(f.site.run_file.clone().or_else(|| f.site.main_file.clone())),
                })
                .collect(),
            more_fonts: self.more_fonts,
        }
    }

    /// Record `text` (well-formed UTF-8 with a non-ASCII character) shown in
    /// the byte-mode font `font`. `site` runs only for a font not seen yet,
    /// so a loop of shows costs no allocation for where it was.
    pub(crate) fn note(&mut self, font: &str, text: &str, site: impl FnOnce() -> Utf8Site) {
        let idx = match self.fonts.iter().position(|f| f.name == font) {
            Some(i) => i,
            None if self.fonts.len() < UTF8_FONTS_SHOWN => {
                self.fonts.push(Utf8Font {
                    name: font.to_string(),
                    chars: Vec::new(),
                    more_chars: false,
                    site: site(),
                });
                self.fonts.len() - 1
            }
            None => {
                self.more_fonts += 1;
                return;
            }
        };
        let f = &mut self.fonts[idx];
        for c in text.chars().filter(|c| !c.is_ascii()) {
            if f.chars.contains(&c) {
                continue;
            }
            if f.chars.len() >= UTF8_CHARS_SHOWN {
                f.more_chars = true;
                break;
            }
            f.chars.push(c);
        }
    }
}

/// A string of UTF-8 text shown in a font that reads one character per byte
/// (issue #1412): `(aπb) show` in Helvetica draws `aˇ b`, which is correct
/// PostScript and so says nothing at run time. The fix is a font that opts
/// into UTF-8 with `/PSCatUnicode true` (see [`crate::font::utf8_in_byte_font`]
/// for what counts, and what is deliberately left alone: the false positive
/// is the worse mistake). Silenced by `%lint: allow utf8-in-byte-font`.
fn check_utf8_in_byte_font(interp: &Interp, findings: &mut Vec<LintFinding>) {
    let Some(trace) = interp.utf8_text_trace() else {
        return;
    };
    for f in &trace.fonts {
        let chars = f
            .chars
            .iter()
            .map(|c| {
                if c.is_control() {
                    format!("U+{:04X}", u32::from(*c))
                } else {
                    format!("{c} U+{:04X}", u32::from(*c))
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let more = if f.more_chars { ", ..." } else { "" };
        let at = match (f.site.line, &f.site.run_file) {
            (Some(line), Some(file)) => format!(" when the program reached line {line} ({file})"),
            (Some(line), None) => format!(" when the program reached line {line}"),
            (None, _) => String::new(),
        };
        findings.push(LintFinding {
            check: "utf8-in-byte-font",
            message: format!(
                "a string holding UTF-8 text ({chars}{more}) was shown in the font /{}{at}; that font reads each byte as one character, so the text draws as stray accents or blanks; put /PSCatUnicode true in a copy of the font to read UTF-8 (docs/guides/fonts.md, \"Unicode text and emoji\"), or silence this with a `%lint: allow utf8-in-byte-font` comment line",
                f.name
            ),
        });
    }
    if trace.more_fonts > 0 {
        findings.push(LintFinding {
            check: "utf8-in-byte-font",
            message: format!(
                "UTF-8 text was also shown in {} more byte-mode font(s) ({UTF8_FONTS_SHOWN} shown)",
                trace.more_fonts
            ),
        });
    }
}

/// One shadow to report.
struct Shadow {
    name: String,
    scope: ShadowScope,
    kind: &'static str,
    location: Option<String>,
}

/// A name bound to a plain value while a built-in operator of that name
/// exists (issue #1052): `/sub (subtitle) def` makes every later `sub` push
/// a string, and the failure surfaces far from the cause, often as an error
/// that never mentions the name.
///
/// A heuristic over the finished run, so it is deliberately narrow, to keep
/// false positives low:
/// - Only a non-procedure value counts. Wrapping an operator in a procedure
///   (`/showpage { ... showpage } def`, a prologue override) is the normal,
///   deliberate way to redefine one, and is left alone; so is an operator
///   bound to its own name.
/// - `userdict` is read at the end of the run. A name defined inside a
///   `begin ... end` that has already closed is gone from it, which is one
///   way to keep a local like `/sub` out of the report (see [`ShadowTrace`]
///   for why that stays the rule).
/// - `systemdict` is read-only (issue #1337), so a `store` of an operator's
///   name (that is where `store` finds it), `systemdict /sub 5 put` or a
///   `def` after `systemdict begin` raises `invalidaccess` and leaves the
///   dictionary alone; the interpreter keeps what was refused and this
///   reports it, for a program that caught the error with `stopped`.
/// - A deliberate redefinition is silenced by a `%lint: allow
///   shadowed-operator` comment line anywhere in the program
///   ([`scan_lint_allows`]); the callers apply it, since this function
///   never sees the source.
/// - When the interpreter traced the run ([`Interp::trace_operator_shadows`],
///   which `--lint` turns on), each finding says what line the binding was
///   made on (the line the scanner had reached, which for a `def` run from
///   inside a procedure is where that procedure was called from).
///
/// Not gated by `render_checks`: shadowing an operator is a mistake whether
/// or not a page was meant to be drawn.
fn check_shadowed_operators(interp: &Interp, findings: &mut Vec<LintFinding>) {
    let (Some(system), Some(user)) = (
        dict_named(interp, "systemdict"),
        dict_named(interp, "userdict"),
    ) else {
        return;
    };
    let trace = interp.shadow_trace();
    let mut shadowed: Vec<Shadow> = Vec::new();
    let mut push = |name: &str, scope, kind, location: Option<String>| {
        shadowed.push(Shadow {
            name: name.to_string(),
            scope,
            kind,
            location,
        });
    };
    for (key, value) in user.borrow().pairs() {
        let Value::Name(name) = &key.value else {
            continue;
        };
        let Some(builtin) = system.borrow().get(name) else {
            continue;
        };
        let Some(kind) = shadow_kind(&builtin, &value) else {
            continue;
        };
        push(
            name,
            ShadowScope::User,
            kind,
            trace.and_then(|t| t.location(name, ShadowScope::User)),
        );
    }
    // Operators a `store` or `put` tried to replace in `systemdict`: it is
    // read-only (issue #1337), so the program got `invalidaccess` and the
    // dictionary itself shows nothing; the interpreter kept the attempt.
    for (name, kind) in interp.refused_system_stores() {
        let location = trace.and_then(|t| t.location(&name, ShadowScope::System));
        push(&name, ShadowScope::System, kind, location);
    }
    // `pairs` order depends on the interner, so sort for stable output.
    shadowed.sort_by(|a, b| (&a.name, a.scope).cmp(&(&b.name, b.scope)));
    for s in shadowed.iter().take(SHADOWED_SHOWN) {
        findings.push(LintFinding {
            check: "shadowed-operator",
            message: shadow_message(s),
        });
    }
    if shadowed.len() > SHADOWED_SHOWN {
        findings.push(LintFinding {
            check: "shadowed-operator",
            message: format!(
                "{} more operator name(s) redefined as plain values ({SHADOWED_SHOWN} shown, in name order)",
                shadowed.len() - SHADOWED_SHOWN
            ),
        });
    }
}

fn shadow_message(s: &Shadow) -> String {
    let Shadow {
        name,
        scope,
        kind,
        location,
    } = s;
    let article = article(kind);
    let at = location
        .as_deref()
        .map(|l| format!(" {l}"))
        .unwrap_or_default();
    match scope {
        ShadowScope::User => format!(
            "/{name} was redefined as {article} {kind}{at}, replacing the operator {name}: every later `{name}` pushes that value instead of running the operator (rename it, or keep it in a local dictionary with begin ... end)"
        ),
        ShadowScope::System => format!(
            "/{name} was stored as {article} {kind} into systemdict{at}, which would replace the operator {name} for the whole program; systemdict is read-only (Ghostscript raises invalidaccess, and so does pscat) -- rename it, or use def, which defines in userdict"
        ),
    }
}

fn article(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

/// The dictionary a permanent name (`systemdict`, `userdict`) is bound to.
fn dict_named(
    interp: &Interp,
    name: &str,
) -> Option<std::rc::Rc<std::cell::RefCell<crate::object::Dict>>> {
    match interp.load(name)?.value {
        Value::Dict(d) => Some(d),
        _ => None,
    }
}

fn check_stack_leaks(interp: &Interp, render_checks: bool, findings: &mut Vec<LintFinding>) {
    // A snippet with no output format requested (eval-style usage) is
    // expected to leave its result on the stack — that's the entire
    // point of `pscat -e`/`eval_postscript` — so this check only
    // applies when a render was actually asked for.
    if render_checks {
        let ostack = interp.operand_stack();
        if !ostack.is_empty() {
            const SHOWN: usize = 5;
            let top: Vec<String> = ostack.iter().rev().take(SHOWN).map(preview).collect();
            let more = if ostack.len() > SHOWN {
                format!(" (top {SHOWN} of {} shown, topmost first)", ostack.len())
            } else {
                " (topmost first)".to_string()
            };
            // The oldest item is the first one leaked: everything above it
            // may be a consequence (a missing `def` shifts every later
            // operand). The line is known only when the run traced it.
            let from = interp
                .stack_trace()
                .and_then(|t| t.oldest(ostack, interp.last_scan_location()));
            let oldest = match (&from, ostack.len() > SHOWN) {
                (Some(at), _) => format!(
                    "; the oldest, {}, was left when the program reached {at}",
                    preview(&ostack[0])
                ),
                (None, true) => format!("; the oldest is {}", preview(&ostack[0])),
                (None, false) => String::new(),
            };
            findings.push(LintFinding {
                check: "stack-leak",
                message: format!(
                    "{} item(s) left on the operand stack at the end of the program: {}{more}{oldest}",
                    ostack.len(),
                    top.join(" ")
                ),
            });
        }
    }
    // systemdict, globaldict and userdict are the permanent baseline (see
    // `Interp::pop_dict`); anything deeper is an unmatched `begin` —
    // true regardless of whether a render was requested, unlike the
    // operand stack, so this isn't gated by `render_checks`.
    let dict_depth = interp.dict_stack_len();
    if dict_depth > crate::interp::PERMANENT_DICTS {
        findings.push(LintFinding {
            check: "dict-leak",
            message: format!(
                "{} dictionary(ies) still open at the end of the program (missing `end`)",
                dict_depth - crate::interp::PERMANENT_DICTS
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Interp {
        let mut it = Interp::with_page(50, 50).expect("page");
        let _ = it.run_str(src);
        it
    }

    fn checks(it: &Interp, render_checks: bool) -> Vec<&'static str> {
        check(it, render_checks).iter().map(|f| f.check).collect()
    }

    fn checks_with_pages(it: &Interp, declared: &DeclaredPages) -> Vec<&'static str> {
        check_with_pages(it, true, declared)
            .iter()
            .map(|f| f.check)
            .collect()
    }

    #[test]
    fn blank_canvas_is_flagged() {
        let it = run("showpage");
        assert!(checks(&it, true).contains(&"blank-page"));
    }

    #[test]
    fn a_matching_declared_page_count_is_not_flagged() {
        let it = run("0 0 40 40 rectfill showpage 5 5 30 30 rectfill showpage");
        assert!(!checks_with_pages(&it, &DeclaredPages::Count(2)).contains(&"page-count"));
    }

    #[test]
    fn a_missing_showpage_merges_two_scenarios_and_is_flagged() {
        // The failure mode this check exists for (issue #95): a
        // rendering driver declares one page per checked scenario, and
        // a deleted `showpage` silently merges two of them — after
        // which the second scenario's ink hides whether the first drew
        // anything at all.
        let it = run("0 0 40 40 rectfill 5 5 30 30 rectfill showpage");
        let found = check_with_pages(&it, true, &DeclaredPages::Count(2));
        let msg = found
            .iter()
            .find(|f| f.check == "page-count")
            .expect("page-count finding");
        assert!(msg.message.contains("emitted 1"), "{}", msg.message);
    }

    #[test]
    fn a_deleted_final_showpage_is_flagged() {
        // Regression test (Codex review, PR #136): counting the live
        // trailing canvas as a page made the *last* scenario's missing
        // `showpage` invisible — the check matched its declaration
        // anyway.
        let it = run("0 0 40 40 rectfill showpage 5 5 30 30 rectfill");
        let found = check_with_pages(&it, true, &DeclaredPages::Count(2));
        let msg = found
            .iter()
            .find(|f| f.check == "page-count")
            .expect("page-count finding");
        assert!(msg.message.contains("emitted 1"), "{}", msg.message);
    }

    #[test]
    fn an_undeclared_page_count_is_never_flagged() {
        // Most programs carry no `%%Pages:` at all; they must not
        // acquire a finding for it.
        let it = run("0 0 40 40 rectfill showpage");
        assert!(!checks_with_pages(&it, &DeclaredPages::None).contains(&"page-count"));
    }

    #[test]
    fn declared_pages_reads_a_dsc_header() {
        assert_eq!(
            scan_declared_pages(b"%!PS\n%%Pages: 7\n%%EndComments\n"),
            DeclaredPages::Count(7)
        );
    }

    #[test]
    fn declared_pages_tolerates_a_trailing_page_order_operand() {
        // Regression test (blank-context review, PR #136): `%%Pages: 3 1`
        // is a real DSC form, and reporting it as malformed would fail
        // a strict-lint run over a header the check never judges.
        assert_eq!(
            scan_declared_pages(b"%%Pages: 3 1\n"),
            DeclaredPages::Count(3)
        );
        assert_eq!(
            scan_declared_pages(b"%%Pages: 3\n"),
            DeclaredPages::Count(3)
        );
    }

    #[test]
    fn declared_pages_treats_atend_as_no_declaration() {
        // DSC's legitimate "the count comes later" form.
        assert_eq!(
            scan_declared_pages(b"%%Pages: (atend)\n"),
            DeclaredPages::None
        );
        assert_eq!(scan_declared_pages(b"%!PS\n"), DeclaredPages::None);
    }

    #[test]
    fn a_malformed_page_count_is_reported_not_ignored() {
        // Regression test (Codex review, PR #136): treating a typo'd
        // count as "no declaration" would silently disable the very
        // check it was written to enable.
        assert_eq!(
            scan_declared_pages(b"%%Pages: nine\n"),
            DeclaredPages::Malformed("nine".to_string())
        );
        let it = run("0 0 40 40 rectfill showpage");
        let found = check_with_pages(&it, true, &scan_declared_pages(b"%%Pages: nine\n"));
        let msg = found
            .iter()
            .find(|f| f.check == "page-count")
            .expect("page-count finding");
        assert!(msg.message.contains("neither a count"), "{}", msg.message);
    }

    #[test]
    fn declared_pages_stops_at_the_end_of_the_dsc_header() {
        // Regression test (Codex review, PR #136): column zero alone
        // doesn't make a line a header. A document that embeds another
        // one further down would otherwise adopt the *embedded* count
        // and compare it against the outer document's output.
        assert_eq!(
            scan_declared_pages(b"%!PS\n%%EndComments\n%%Pages: 12\n"),
            DeclaredPages::None
        );
        assert_eq!(
            scan_declared_pages(b"%!PS\n0 0 moveto\n%%Pages: 12\n"),
            DeclaredPages::None
        );
        // ...and an indented one isn't a header comment either.
        assert_eq!(scan_declared_pages(b"  %%Pages: 3\n"), DeclaredPages::None);
    }

    #[test]
    fn painted_canvas_is_not_flagged() {
        let it = run("0 0 40 40 rectfill showpage");
        assert!(!checks(&it, true).contains(&"blank-page"));
    }

    #[test]
    fn blank_page_is_skipped_when_render_checks_are_off() {
        let it = run("3 4 add pop");
        assert!(!checks(&it, false).contains(&"blank-page"));
    }

    #[test]
    fn nothing_painted_but_no_showpage_still_flags_the_trailing_canvas() {
        let it = run("");
        assert!(checks(&it, true).contains(&"blank-page"));
    }

    #[test]
    fn unbalanced_gsave_is_flagged() {
        let it = run("gsave");
        assert!(checks(&it, true).contains(&"gsave-imbalance"));
    }

    #[test]
    fn balanced_gsave_is_not_flagged() {
        let it = run("gsave grestore");
        assert!(!checks(&it, true).contains(&"gsave-imbalance"));
    }

    #[test]
    fn leftover_operands_are_flagged() {
        let it = run("1 2 3");
        let found = check(&it, true);
        let leak = found
            .iter()
            .find(|f| f.check == "stack-leak")
            .expect("stack-leak finding");
        assert!(
            leak.message.contains('3'),
            "count in message: {}",
            leak.message
        );
    }

    #[test]
    fn clean_stack_is_not_flagged() {
        let it = run("1 1 add pop");
        assert!(!checks(&it, true).contains(&"stack-leak"));
    }

    #[test]
    fn unmatched_begin_is_flagged() {
        let it = run("userdict begin");
        assert!(checks(&it, true).contains(&"dict-leak"));
    }

    #[test]
    fn matched_begin_end_is_not_flagged() {
        let it = run("userdict begin end");
        assert!(!checks(&it, true).contains(&"dict-leak"));
    }

    #[test]
    fn stack_leak_is_skipped_when_render_checks_are_off() {
        // Regression test (Codex review, PR #59): eval-style usage
        // (`pscat -e`, `eval_postscript`) leaves its result on the
        // stack on purpose — that used to still trip stack-leak because
        // only check_blank_pages was gated on render_checks.
        let it = run("3 4 add");
        assert!(!checks(&it, false).contains(&"stack-leak"));
    }

    #[test]
    fn dict_leak_still_fires_when_render_checks_are_off() {
        // Unlike stack-leak, an unmatched `begin` is a mistake in eval
        // usage too — not gated by render_checks.
        let it = run("userdict begin");
        assert!(checks(&it, false).contains(&"dict-leak"));
    }

    #[test]
    fn a_second_showpage_with_no_new_ink_is_a_logical_blank_page() {
        // Regression test (Codex review, PR #59): showpage's lazy erase
        // means two showpages with nothing painted between them
        // snapshot the identical, non-blank pixmap twice — pixel
        // content alone can't see that the second one has no ink of
        // its own.
        let it = run("0 0 40 40 rectfill showpage showpage");
        let findings = check(&it, true);
        let blanks: Vec<&str> = findings
            .iter()
            .filter(|f| f.check == "blank-page")
            .map(|f| f.message.as_str())
            .collect();
        assert_eq!(blanks.len(), 1, "exactly the second page: {blanks:?}");
        assert!(blanks[0].contains("page 2 of 2"), "{blanks:?}");
    }

    #[test]
    fn a_long_leaked_operand_is_truncated() {
        let it = run("100000 string");
        let findings = check(&it, true);
        let leak = findings
            .iter()
            .find(|f| f.check == "stack-leak")
            .expect("stack-leak finding");
        assert!(
            leak.message.len() < 1000,
            "message should be bounded, got {} bytes",
            leak.message.len()
        );
        assert!(
            leak.message.contains("bytes total"),
            "should note it was truncated: {}",
            leak.message
        );
    }

    #[test]
    fn a_huge_leaked_string_is_bounded_without_building_the_whole_repr() {
        // Regression test (Codex review round 2, PR #59): the original
        // fix truncated repr()'s *output*, which still built the whole
        // escaped string first -- a 10M-byte string produced a ~40MB
        // intermediate. This should stay fast and the message small
        // regardless of the leaked string's size.
        let it = run("10000000 string");
        let findings = check(&it, true);
        let leak = findings
            .iter()
            .find(|f| f.check == "stack-leak")
            .expect("stack-leak finding");
        assert!(
            leak.message.len() < 1000,
            "message should be bounded, got {} bytes",
            leak.message.len()
        );
        assert!(
            leak.message.contains("10000000 bytes total") || leak.message.contains("10000002"),
            "should report the real size: {}",
            leak.message
        );
    }

    #[test]
    fn a_leaked_array_reports_length_without_recursing_into_elements() {
        // A short array of one huge string must not defeat the bound
        // by recursing into repr() for each element.
        let it = run("[10000000 string]");
        let findings = check(&it, true);
        let leak = findings
            .iter()
            .find(|f| f.check == "stack-leak")
            .expect("stack-leak finding");
        assert!(
            leak.message.len() < 1000,
            "message should be bounded, got {} bytes",
            leak.message.len()
        );
        assert!(
            leak.message.contains("array of 1 element"),
            "should describe the array by length, not its contents: {}",
            leak.message
        );
    }

    #[test]
    fn a_huge_leaked_name_is_bounded() {
        // Regression test (Codex review round 3, PR #59): a literal
        // name has no length limit here (`read_regular_run` just keeps
        // consuming non-delimiter bytes), so the fallback-to-repr()
        // path missed the same unbounded-preview problem as strings.
        let src = format!("/{}", "a".repeat(1_000_000));
        let it = run(&src);
        let findings = check(&it, true);
        let leak = findings
            .iter()
            .find(|f| f.check == "stack-leak")
            .expect("stack-leak finding");
        assert!(
            leak.message.len() < 1000,
            "message should be bounded, got {} bytes",
            leak.message.len()
        );
        assert!(
            leak.message.contains("bytes total"),
            "should note it was truncated: {}",
            leak.message
        );
    }

    fn shadow_messages(it: &Interp) -> Vec<String> {
        check(it, true)
            .into_iter()
            .filter(|f| f.check == "shadowed-operator")
            .map(|f| f.message)
            .collect()
    }

    #[test]
    fn a_string_defined_over_an_operator_is_flagged_by_name() {
        // The issue's repro: `/sub` meant as "subtitle".
        let it = run("/sub (hello) def /x 5 3 sub def showpage");
        let found = shadow_messages(&it);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("/sub was redefined as a string"),
            "{}",
            found[0]
        );
        assert!(
            found[0].contains("replacing the operator sub"),
            "{}",
            found[0]
        );
    }

    #[test]
    fn numbers_and_arrays_are_flagged_and_name_the_type() {
        let it = run("/div 2 def /dict [1] def /rand 0.5 def");
        let found = shadow_messages(&it);
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(
            found[0].starts_with("/dict was redefined as an array"),
            "{found:?}"
        );
        assert!(
            found[1].starts_with("/div was redefined as an integer"),
            "{found:?}"
        );
        assert!(
            found[2].starts_with("/rand was redefined as a real"),
            "{found:?}"
        );
    }

    #[test]
    fn a_procedure_wrapping_an_operator_is_a_deliberate_redefinition() {
        let it = run("/showpage { showpage } bind def /sub { neg add } def");
        assert!(shadow_messages(&it).is_empty());
    }

    #[test]
    fn an_ordinary_name_and_an_operator_bound_to_itself_are_not_flagged() {
        let it = run("/subtitle (hello) def /n 3 def /sub /sub load def /add2 /add load def");
        assert!(shadow_messages(&it).is_empty());
    }

    #[test]
    fn an_operator_bound_to_another_operators_name_is_flagged() {
        let it = run("/sub /add load def");
        let found = shadow_messages(&it);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("/sub was redefined as an operator"),
            "{found:?}"
        );
    }

    #[test]
    fn a_local_dictionary_that_has_ended_is_the_opt_out() {
        let src = "4 dict begin /sub (hello) def /dict 3 def end /x 5 3 sub def showpage";
        assert!(shadow_messages(&run(src)).is_empty());
        // Tracing sees the `def`s, and still leaves a local variable alone.
        assert!(shadow_messages(&traced(src)).is_empty());
        let open = "4 dict begin /stop 3 def";
        assert!(shadow_messages(&traced(open)).is_empty());
    }

    fn traced(src: &str) -> Interp {
        let mut it = Interp::with_page(50, 50).expect("page");
        it.trace_operator_shadows();
        let _ = it.run_str(src);
        it
    }

    fn leak_message(src: &str) -> String {
        let mut it = Interp::with_page(50, 50).expect("page");
        it.trace_stack_leaks();
        let _ = it.run_str(src);
        check(&it, true)
            .into_iter()
            .find(|f| f.check == "stack-leak")
            .map(|f| f.message)
            .unwrap_or_default()
    }

    #[test]
    fn a_leak_names_the_line_that_left_the_oldest_item() {
        let m = leak_message("1 2 add pop\n7 8\n3 4 add pop");
        assert!(
            m.contains("2 item(s) left on the operand stack at the end of the program: 8 7 (topmost first); the oldest, 7, was left when the program reached line 2"),
            "{m}"
        );
        // The message before the line was known is a prefix of the new one.
        let untraced = check(&run("1 2 add pop\n7 8"), true)
            .into_iter()
            .find(|f| f.check == "stack-leak")
            .expect("leak")
            .message;
        assert_eq!(
            untraced,
            "2 item(s) left on the operand stack at the end of the program: 8 7 (topmost first)"
        );
    }

    #[test]
    fn a_leak_on_the_last_line_is_credited_to_the_last_token() {
        // No token follows it, so no later sample sees the growth.
        let m = leak_message("1 2 add pop\n\n9");
        assert!(
            m.contains("the oldest, 9, was left when the program reached line 3"),
            "{m}"
        );
    }

    #[test]
    fn the_first_leak_wins_over_later_ones_and_balanced_work_between_is_ignored() {
        let m = leak_message("5\n1 2 add pop\n6\n[ 1 2 ] pop\n7");
        assert!(m.contains("3 item(s)"), "{m}");
        assert!(
            m.contains("the oldest, 5, was left when the program reached line 1"),
            "{m}"
        );
        // A cleared stack forgets the old origin; the next leak is a new one.
        let m = leak_message("5\nclear\n\n6");
        assert!(
            m.contains("the oldest, 6, was left when the program reached line 4"),
            "{m}"
        );
    }

    #[test]
    fn a_leak_in_a_loop_points_at_the_loop_not_its_body() {
        let src = "0 1 9 {\n  /gq exch\n  pop\n  /gq 1\n} for\nshowpage";
        let m = leak_message(src);
        assert!(m.contains("30 item(s)"), "{m}");
        assert!(
            m.contains("was left when the program reached line 5"),
            "{m}"
        );
    }

    #[test]
    fn a_leaky_call_is_credited_to_the_call_not_the_definition() {
        let src = "/leak {\n  1 2\n  3\n} def\n1 2 add pop\nleak\nshowpage";
        let m = leak_message(src);
        assert!(m.contains("3 item(s)"), "{m}");
        assert!(
            m.contains("was left when the program reached line 6"),
            "{m}"
        );
    }

    #[test]
    fn a_missing_curveto_is_found() {
        // The issue's shape: five numbers where a `curveto` belonged.
        let src = "newpath -20 -10 moveto -20 118 lineto\n  20 124 60 118 110 100 curveto\n  250 24 270 10 300 -10 closepath\n0 setgray";
        let m = leak_message(src);
        assert!(
            m.contains("the oldest, 250, was left when the program reached line 3"),
            "{m}"
        );
    }

    #[test]
    fn a_long_stack_shows_its_bottom_item_even_without_the_trace() {
        let it = run("(first) 1 2 3 4 5 6");
        let m = check(&it, true)
            .into_iter()
            .find(|f| f.check == "stack-leak")
            .expect("leak")
            .message;
        assert!(
            m.contains("(top 5 of 7 shown, topmost first); the oldest is (first)"),
            "{m}"
        );
    }

    #[test]
    fn a_big_aload_keeps_the_trace_small() {
        let mut it = Interp::with_page(50, 50).expect("page");
        it.trace_stack_leaks();
        let _ = it.run_str("10000 array aload pop\n0 1 5000 { pop 1 2 3 } for\nclear 1");
        let t = it.stack_trace().expect("trace");
        assert!(t.run_count() < 10, "{}", t.run_count());
    }

    #[test]
    fn a_fresh_interpreter_has_no_shadows() {
        assert!(shadow_messages(&run("")).is_empty());
        assert!(shadow_messages(&traced("")).is_empty());
    }

    #[test]
    fn a_traced_run_names_the_line_and_an_untraced_one_does_not() {
        let src = "% header\n\n/sub (hello) def\nshowpage";
        let found = shadow_messages(&traced(src));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with(
                "/sub was redefined as a string when the program reached line 3, replacing the operator sub: "
            ),
            "{found:?}"
        );
        // The same finding without a trace is the message from before the line was known.
        let found = shadow_messages(&run(src));
        assert!(
            found[0].starts_with("/sub was redefined as a string, replacing the operator sub: "),
            "{found:?}"
        );
    }

    #[test]
    fn a_def_inside_a_procedure_names_the_line_that_ran_it() {
        let found = shadow_messages(&traced("/f {\n /sub 1 def\n} def\n\nf"));
        assert!(
            found[0].contains("as an integer when the program reached line 2, replacing"),
            "{found:?}"
        );
    }

    /// The line the one `shadowed-operator` finding of `src` names.
    fn shadow_line(src: &str) -> String {
        let found = shadow_messages(&traced(src));
        assert_eq!(found.len(), 1, "{found:?}");
        let rest = found[0]
            .split("when the program reached line ")
            .nth(1)
            .unwrap_or_else(|| panic!("no line in {found:?}"));
        rest.split([',', ' ']).next().unwrap_or("").to_string()
    }

    #[test]
    fn a_def_in_a_loop_body_names_its_own_line() {
        // The `for` loop ends on line 4; the `def` is on line 2 (#1338).
        assert_eq!(shadow_line("0 1 3 {\n  /ln exch def\n  1 pop\n} for"), "2");
        assert_eq!(shadow_line("3 {\n  1 pop\n  /ln 2 def\n} repeat"), "3");
        assert_eq!(shadow_line("[1 2] {\n  /ln exch def\n} forall"), "2");
        assert_eq!(shadow_line("{\n  /ln 1 def\n  exit\n} loop"), "2");
    }

    #[test]
    fn a_def_that_is_not_the_last_element_of_its_body_names_its_own_line() {
        assert_eq!(
            shadow_line("/f {\n  /ln 1 def\n  1 pop\n  2 pop\n} def\n\n\nf"),
            "2"
        );
        // Two shadows in one body: each has its own line, the first is kept per name.
        let found = shadow_messages(&traced("/f {\n /sub 1 def\n\n /add 2 def\n} def\nf"));
        assert!(found[0].contains("line 4"), "{found:?}");
        assert!(found[1].contains("line 2"), "{found:?}");
    }

    #[test]
    fn a_def_reached_through_a_chain_of_calls_names_its_own_line() {
        assert_eq!(
            shadow_line("/g {\n /div 3 def\n} def\n/h { g } def\n/i { h } def\n\n\ni"),
            "2"
        );
        // From inside another procedure's body, as one of its elements.
        assert_eq!(
            shadow_line("/g {\n /div 3 def\n} def\n/h { g 1 pop } def\n\nh"),
            "2"
        );
    }

    #[test]
    fn a_def_in_a_conditional_branch_names_its_own_line() {
        assert_eq!(shadow_line("true {\n  /abs 1 def\n} if"), "2");
        assert_eq!(
            shadow_line("/f { true {\n  /abs 1 def\n} {\n  /neg 2 pop\n} ifelse } def\nf"),
            "2"
        );
        assert_eq!(shadow_line("{\n /abs 1 def\n} stopped pop"), "2");
        assert_eq!(shadow_line("/c true def\n\nc {\n  /abs 1 def\n} if\n"), "4");
    }

    #[test]
    fn a_bound_procedure_still_names_the_line_of_its_def() {
        assert_eq!(shadow_line("/f {\n /ln 1 def\n} bind def\n\nf"), "2");
    }

    #[test]
    fn a_nested_procedure_names_the_line_of_its_def() {
        assert_eq!(
            shadow_line("/f {\n  1 {\n    /ln 1 def\n  } repeat\n} def\n\n\nf"),
            "3"
        );
    }

    #[test]
    fn a_def_that_cannot_be_placed_names_the_line_that_ran_it() {
        // A body taken off the stack, a computed name, and a name defined by a
        // helper: no body spells `/name` next to a `def`, so the line the scanner
        // had reached is kept, as before (#1109).
        assert_eq!(shadow_line("/ln 5\n\n{ def }\n\nexec"), "5");
        assert_eq!(shadow_line("/set {\n exch def\n} def\n\n3 /ln set"), "5");
        // Two branches that both spell the name: not guessed.
        assert_eq!(
            shadow_line("/f { true {\n /ln 1 def\n} {\n /ln 2 def\n} ifelse } def\n\n\nf"),
            "8"
        );
        // Called in order, the first one to run is the one reported.
        assert_eq!(
            shadow_line("/a { /ln 1 def } def\n/b {\n /ln 2 def } def\n/c { a b } def\n\nc"),
            "1"
        );
    }

    #[test]
    fn a_guess_between_two_bodies_is_not_made() {
        // The `def` on line 1 runs through `f`; the scanner is at `ifelse`, and the
        // branch on line 3 never ran. Two bodies could be it: the line that ran it.
        assert_eq!(
            shadow_line("/f { /ln 1 def } def\nfalse {\n  /ln 2 def\n} {\n  f\n} ifelse"),
            "6"
        );
    }

    #[test]
    fn a_body_that_did_not_run_is_not_picked_for_an_indirect_def() {
        // `unused` never ran: the def is `setter`'s, run by an `exec` of a
        // procedure off the stack, so the line is the caller's (#1338).
        assert_eq!(
            shadow_line(
                "/unused { /ln 1 def } def\n/setter { exch def } def\n3 /ln /setter load exec"
            ),
            "3"
        );
        // The same inside a procedure and through `store`, which finds `ln` in systemdict.
        assert_eq!(
            shadow_line(
                "/unused { /ln 1 store } def\n/setter { exch store } def\n3 /ln /setter load exec"
            ),
            "3"
        );
        assert_eq!(
            shadow_line(
                "/unused { /ln 1 store } def\n/w {\n 3 /ln /setter load exec } def\n/setter { exch store } def\n\nw"
            ),
            "6"
        );
        // A body written elsewhere, run by `exec` of something else.
        assert_eq!(
            shadow_line(
                "/ln { 1 } def\n/g { /ln 1 def } def\n\n/setter { exch def } def\n\n3 /ln /setter load exec"
            ),
            "6"
        );
        // A literal body written right before the operator still names its own line.
        assert_eq!(shadow_line("/x 1 def\n{\n /ln 1 def\n} exec"), "3");
        assert_eq!(
            shadow_line("/unused { /ln 1 def } def\ntrue {\n /ln 2 def\n} { 1 pop } ifelse"),
            "3"
        );
    }

    #[test]
    fn a_def_that_cannot_be_placed_in_a_procedure_names_the_line_that_ran_it() {
        // An `exec` or `ifelse` in a body whose operand is not a literal body.
        assert_eq!(
            shadow_line(
                "/unused { /ln 1 def } def\n/w {\n 3 /ln /setter load exec } def\n/setter { exch def } def\nw"
            ),
            "5"
        );
        // A procedure literal that is merely pushed runs nothing.
        assert_eq!(
            shadow_line(
                "/w {\n 3 /ln /setter load { /ln 1 def } pop exec } def\n/setter { exch def } def\nw"
            ),
            "4"
        );
    }

    #[test]
    fn a_top_level_def_after_a_call_still_names_its_own_line() {
        // An earlier body that spells the same name must not be picked.
        assert_eq!(shadow_line("/f { /ln { 1 } def } def\nf\n\n/ln 5 def"), "4");
    }

    #[test]
    fn a_repeated_def_keeps_the_first_line_and_a_loop_keeps_one_entry() {
        let found = shadow_messages(&traced("/ln 1 def\n/ln 2 def\n"));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains(" when the program reached line 1,"),
            "{found:?}"
        );
        let it = traced("0 1 5000 { /ln exch def } for");
        assert_eq!(shadow_messages(&it).len(), 1);
        assert_eq!(it.shadow_trace().expect("tracing").sites.len(), 1);
    }

    #[test]
    fn a_def_into_userdict_or_systemdict_made_current_still_names_its_line() {
        let found = shadow_messages(&traced("userdict begin\n/sub 1 def\nend"));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("when the program reached line 2, replacing"),
            "{found:?}"
        );
        // systemdict is read-only (#1337): the def is refused, and noted.
        let found = shadow_messages(&traced(
            "{ systemdict begin\n/sub 1 def } stopped\ncleardictstack",
        ));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with(
                "/sub was stored as an integer into systemdict when the program reached line 2,"
            ),
            "{found:?}"
        );
    }

    #[test]
    fn a_refused_store_is_forgotten_when_the_next_program_begins() {
        for traced_run in [false, true] {
            let mut it = Interp::with_page(50, 50).expect("page");
            if traced_run {
                it.trace_operator_shadows();
            }
            it.run_str("{ /add 1 store } stopped pop clear").unwrap();
            assert_eq!(shadow_messages(&it).len(), 1, "traced: {traced_run}");
            it.run_str("1 2 add pop").unwrap();
            assert!(shadow_messages(&it).is_empty(), "traced: {traced_run}");
            assert!(it.refused_system_stores().is_empty());
            if traced_run {
                assert!(it.shadow_trace().expect("tracing").sites.is_empty());
            }
        }
    }

    #[test]
    fn a_procedure_defined_later_ends_the_shadow() {
        let it = traced("/sub 1 def\n/sub { neg add } def");
        assert!(shadow_messages(&it).is_empty());
    }

    #[test]
    fn store_and_put_into_systemdict_are_refused_and_reported() {
        // `store` finds the name in systemdict when no other dictionary defines it.
        let found = shadow_messages(&traced("\n{ /sub 5 store } stopped pop"));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("/sub was stored as an integer into systemdict when the program reached line 2, which would replace the operator sub for the whole program; systemdict is read-only"),
            "{found:?}"
        );
        // Without a trace the refusal still shows (no line), `put` included.
        let found = shadow_messages(&run(
            "{ /sub 5 store } stopped pop { systemdict /dict (x) put } stopped pop",
        ));
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found[0].starts_with("/dict was stored as a string into systemdict"),
            "{found:?}"
        );
        assert!(
            found[1].starts_with("/sub was stored as an integer into systemdict"),
            "{found:?}"
        );
        // A store into a name userdict already defines is a userdef shadow.
        let found = shadow_messages(&traced("/sub 1 def /sub 2 store"));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("when the program reached line 1, replacing"),
            "{found:?}"
        );
    }

    #[test]
    fn another_operator_stored_into_systemdict_is_flagged() {
        // Codex round 1 on #1323: `sub` now adds; an operator stored over
        // itself is still no shadow.
        let found = shadow_messages(&run("{ /sub /add load store } stopped pop"));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("/sub was stored as an operator into systemdict"),
            "{found:?}"
        );
        assert!(shadow_messages(&run("{ /sub /sub load store } stopped pop")).is_empty());
    }

    #[test]
    fn shadowing_is_flagged_without_render_checks_too() {
        let it = run("/sub 1 def");
        let found: Vec<_> = check(&it, false).into_iter().map(|f| f.check).collect();
        assert_eq!(found, ["shadowed-operator"]);
    }

    #[test]
    fn a_long_list_is_capped_and_counted() {
        let names = [
            "add", "sub", "mul", "div", "mod", "neg", "abs", "exp", "dict", "string", "rand",
            "count", "copy",
        ];
        let src: String = names.iter().map(|n| format!("/{n} 1 def ")).collect();
        let found = shadow_messages(&run(&src));
        assert_eq!(found.len(), SHADOWED_SHOWN + 1, "{found:?}");
        assert!(
            found[SHADOWED_SHOWN].starts_with("3 more operator name(s)"),
            "{found:?}"
        );
    }

    #[test]
    fn a_band_lint_reports_it_too() {
        let it = run("/sub (hello) def");
        let found = check_band(&it, &DeclaredPages::None);
        assert!(found.iter().any(|f| f.check == "shadowed-operator"));
    }

    #[test]
    fn the_allow_comment_names_what_it_silences() {
        let findings = || {
            vec![
                LintFinding {
                    check: "shadowed-operator",
                    message: String::new(),
                },
                LintFinding {
                    check: "blank-page",
                    message: String::new(),
                },
            ]
        };
        let names = |src: &[u8]| -> Vec<&'static str> {
            scan_lint_allows(src)
                .filter(findings())
                .iter()
                .map(|f| f.check)
                .collect()
        };
        assert_eq!(names(b"showpage\n"), ["shadowed-operator", "blank-page"]);
        // The directive silences that check only, whatever else it names.
        for src in [
            &b"%lint: allow shadowed-operator\n"[..],
            b"  %lint: allow shadowed-operator\r\n",
            b"/sub 1 def %lint: allow shadowed-operator\n%lint: allow shadowed-operator, blank-page\n",
            b"%lint: allow blank-page shadowed-operator\n",
        ] {
            assert_eq!(names(src), ["blank-page"], "{src:?}");
        }
        // A typo, another check, or a longer word silences nothing.
        for src in [
            &b"%lint: allow shadowed-operators\n"[..],
            b"%lint: allow blank-page\n",
            b"%lint: allowshadowed-operator\n",
            b"% lint: allow shadowed-operator\n",
            b"%lint: deny shadowed-operator\n",
        ] {
            assert_eq!(names(src), ["shadowed-operator", "blank-page"], "{src:?}");
        }
    }

    // --- utf8-in-byte-font (#1412) ---------------------------------------

    fn utf8_messages(src: &str) -> Vec<String> {
        let mut it = Interp::with_page(80, 80).expect("page");
        it.trace_utf8_text();
        let _ = it.run_str(src);
        check(&it, false)
            .into_iter()
            .filter(|f| f.check == "utf8-in-byte-font")
            .map(|f| f.message)
            .collect()
    }

    const BYTE_FONT: &str = "/Helvetica findfont 12 scalefont setfont 5 40 moveto ";
    const UNICODE_COPY: &str = "/Helvetica findfont dup length dict copy dup /PSCatUnicode true put \
         /Helvetica-U exch definefont 12 scalefont setfont 5 40 moveto ";

    #[test]
    fn utf8_text_in_a_byte_font_is_named_with_the_fix() {
        let m = utf8_messages(&format!("{BYTE_FONT}\n(a\u{3c0}b) show"));
        assert_eq!(m.len(), 1, "{m:?}");
        for want in [
            "\u{3c0} U+03C0",
            "the font /Helvetica",
            "when the program reached line 2",
            "/PSCatUnicode true",
        ] {
            assert!(m[0].contains(want), "{want}: {}", m[0]);
        }
    }

    #[test]
    fn every_show_operator_is_checked() {
        for show in [
            "(\u{2260}) show",
            "(\u{2260}) stringwidth pop pop",
            "(\u{2260}) false charpath",
            "1 1 (\u{2260}) ashow",
            "1 0 32 (\u{2260}) widthshow",
            "1 1 32 1 1 (\u{2260}) awidthshow",
            "(\u{2260}) [1 1 1] xshow",
            "{ pop pop pop } (\u{2260}) cshow",
        ] {
            let m = utf8_messages(&format!("{BYTE_FONT}{show}"));
            assert_eq!(m.len(), 1, "{show}: {m:?}");
        }
    }

    #[test]
    fn a_custom_encoding_is_not_reported() {
        // Codes 195 and 169 deliberately assigned to glyphs: the text draws
        // "AB" through the Encoding, and /PSCatUnicode true would change it.
        let custom = "/Helvetica findfont dup length dict copy dup /Encoding StandardEncoding \
            256 array copy put dup /Encoding get dup 195 /A put 169 /B put \
            /Custom exch definefont 12 scalefont setfont 5 40 moveto ";
        assert!(utf8_messages(&format!("{custom}(\u{e9}) show")).is_empty());
        // An unmodified copy of StandardEncoding is still the standard one.
        let std = "/Helvetica findfont dup length dict copy dup /Encoding StandardEncoding \
            256 array copy put /Std exch definefont 12 scalefont setfont 5 40 moveto ";
        assert_eq!(utf8_messages(&format!("{std}(\u{e9}) show")).len(), 1);
        // A non-array Encoding is not the standard one either.
        let odd = "/Helvetica findfont dup length dict copy dup /Encoding 1 put \
            /Odd exch definefont 12 scalefont setfont 5 40 moveto ";
        assert!(utf8_messages(&format!("{odd}(\u{e9}) show")).is_empty());
    }

    #[test]
    fn a_kshow_is_not_checked_because_its_proc_may_change_the_font() {
        let src = format!(
            "{UNICODE_COPY} /U currentfont def {BYTE_FONT} \
             {{ pop pop U setfont }} (a\u{3c0}) kshow"
        );
        assert!(utf8_messages(&src).is_empty());
        // A kshow with a plain proc is a deliberate miss too.
        assert!(utf8_messages(&format!("{BYTE_FONT}{{ pop pop }} (\u{3c0}) kshow")).is_empty());
    }

    #[test]
    fn a_long_string_is_not_examined() {
        // The scan per show is bounded (the rest of the string is NULs,
        // which are ASCII): 4096 bytes are examined, 4097 are not.
        let at = |n: usize| {
            utf8_messages(&format!(
                "{BYTE_FONT}{n} string dup 0 195 put dup 1 169 put show"
            ))
            .len()
        };
        assert_eq!(at(4096), 1);
        assert_eq!(at(4097), 0);
    }

    #[test]
    fn a_unicode_mode_font_is_not_reported() {
        let text = "(a\u{3c0} \u{1f44d}) show";
        // The key in a definefont copy, on a scalefont result, through
        // selectfont-free scale, and inside a procedure.
        for setup in [
            UNICODE_COPY,
            "/Helvetica findfont 12 scalefont dup /PSCatUnicode true put setfont 5 40 moveto ",
        ] {
            assert!(
                utf8_messages(&format!("{setup}{text}")).is_empty(),
                "{setup}"
            );
        }
        let in_proc = format!("/say {{ {text} }} def {UNICODE_COPY} say");
        assert!(utf8_messages(&in_proc).is_empty());
        // An explicit false is bytes on purpose.
        let off = "/Helvetica findfont dup length dict copy dup /PSCatUnicode false put \
                   /H-B exch definefont 12 scalefont setfont 5 40 moveto ";
        assert!(utf8_messages(&format!("{off}{text}")).is_empty());
        // A composite (Type 0) dict reads codes of its own.
        let t0 = "/Helvetica findfont dup length dict copy dup /FontType 0 put \
                  /H-0 exch definefont 12 scalefont setfont 5 40 moveto ";
        assert!(utf8_messages(&format!("{t0}{text}")).is_empty());
        // The catalog's Unicode-mode faces, when this build has one.
        // (A face that is not found is replaced by Helvetica, so the guard
        // reads the name the dict ended up with.)
        let mut it = run("/NanumBrushScript-Regular findfont /FontName get");
        let loaded = it.pop().is_ok_and(
            |n| matches!(&n.value, Value::Name(n) if &**n == "NanumBrushScript-Regular"),
        );
        if loaded {
            let src = format!(
                "/NanumBrushScript-Regular findfont 12 scalefont setfont 5 40 moveto {text}"
            );
            assert!(utf8_messages(&src).is_empty());
        }
    }

    #[test]
    fn a_copy_without_the_key_is_still_a_byte_font() {
        let copy = "/Helvetica findfont dup length dict copy /Helvetica-C exch definefont \
                    12 scalefont setfont 5 40 moveto ";
        let m = utf8_messages(&format!("{copy}(\u{3c0}) show"));
        assert_eq!(m.len(), 1, "{m:?}");
        // The same copy re-encoded, which is the idiom for Latin-1 text.
        let latin1 = "/Helvetica findfont dup length dict copy dup /Encoding ISOLatin1Encoding put \
                      /Helvetica-L exch definefont 12 scalefont setfont 5 40 moveto ";
        assert_eq!(utf8_messages(&format!("{latin1}(\u{3c0}) show")).len(), 1);
    }

    #[test]
    fn a_string_in_procedures_and_loops_is_reported_once_per_font() {
        let src = format!(
            "{BYTE_FONT} /say {{ (\u{e9}\u{3c0}) show }} def 200 {{ say }} repeat \
             /Times-Roman findfont 12 scalefont setfont (\u{e9}) show"
        );
        let m = utf8_messages(&src);
        assert_eq!(m.len(), 2, "{m:?}");
        assert!(m[0].contains("\u{e9} U+00E9, \u{3c0} U+03C0"), "{}", m[0]);
        assert!(m[1].contains("/Times-Roman"), "{}", m[1]);
    }

    #[test]
    fn latin1_binary_and_ascii_strings_are_not_reported() {
        for text in [
            r"(caf\351) show",          // the Latin-1 idiom the guides teach
            r"(Fr\350re Jacques) show", // an accent between ASCII
            r"(\377\376\200\201) show", // binary
            r"(a\317) show",            // a cut-off sequence
            r"(\317\200\351) show",     // UTF-8 then a Latin-1 byte: a deliberate miss
            "(plain ASCII) show",
            "() show",
            // Symbol's high bytes are symbol codes that can look like UTF-8.
            "/Symbol findfont 12 scalefont setfont 5 40 moveto (\u{3c0}) show",
        ] {
            let src = if text.contains("findfont") {
                text.to_string()
            } else {
                format!("{BYTE_FONT}{text}")
            };
            assert!(utf8_messages(&src).is_empty(), "{text}");
        }
        // ZapfDingbats too, and a Type 3 font (a user dict, not a face).
        assert!(
            utf8_messages(
                "/ZapfDingbats findfont 12 scalefont setfont 5 40 moveto (\u{2713}) show"
            )
            .is_empty()
        );
        let t3 = "/T3 8 dict def T3 begin /FontType 3 def /FontMatrix [0.001 0 0 0.001 0 0] def \
                  /FontBBox [0 0 1000 1000] def /Encoding 256 array def \
                  /BuildChar { pop pop 1000 0 setcharwidth } def end \
                  /T3 T3 definefont 12 scalefont setfont 5 40 moveto (\u{3c0}) show";
        assert!(utf8_messages(t3).is_empty());
    }

    #[test]
    fn the_report_is_bounded_and_a_new_program_starts_clean() {
        let many: String = (0x3b1u32..0x3b1 + 30).filter_map(char::from_u32).collect();
        let m = utf8_messages(&format!("{BYTE_FONT}({many}) show"));
        assert_eq!(m.len(), 1);
        assert!(m[0].contains(", ...)"), "{}", m[0]);
        assert_eq!(m[0].matches("U+").count(), UTF8_CHARS_SHOWN, "{}", m[0]);
        // More fonts than are named are counted.
        let fonts = [
            "Helvetica",
            "Times-Roman",
            "Courier",
            "Palatino-Roman",
            "Bookman-Light",
            "AvantGarde-Book",
            "NewCenturySchlbk-Roman",
        ];
        let src: String = fonts
            .iter()
            .map(|f| format!("/{f} findfont 12 scalefont setfont 5 40 moveto (\u{3c0}) show "))
            .collect();
        let m = utf8_messages(&src);
        assert_eq!(m.len(), UTF8_FONTS_SHOWN + 1, "{m:?}");
        assert!(m.last().unwrap().contains("2 more byte-mode font(s)"));
        // The next program on the same interpreter is not blamed for it.
        let mut it = Interp::with_page(80, 80).expect("page");
        it.trace_utf8_text();
        let _ = it.run_str(&format!("{BYTE_FONT}(\u{3c0}) show"));
        assert_eq!(
            check(&it, false)
                .iter()
                .filter(|f| f.check == "utf8-in-byte-font")
                .count(),
            1
        );
        let _ = it.run_str(&format!("{BYTE_FONT}(ascii) show"));
        assert!(!checks(&it, false).contains(&"utf8-in-byte-font"));
    }

    #[test]
    fn an_untraced_run_reports_nothing() {
        let it = run(&format!("{BYTE_FONT}(\u{3c0}) show"));
        assert!(!checks(&it, false).contains(&"utf8-in-byte-font"));
    }

    #[test]
    fn the_allow_comment_silences_utf8_in_a_byte_font_only() {
        let findings = || {
            ["utf8-in-byte-font", "shadowed-operator"]
                .into_iter()
                .map(|check| LintFinding {
                    check,
                    message: String::new(),
                })
                .collect::<Vec<_>>()
        };
        let names = |src: &[u8]| -> Vec<&'static str> {
            scan_lint_allows(src)
                .filter(findings())
                .iter()
                .map(|f| f.check)
                .collect()
        };
        assert_eq!(
            names(b"showpage\n"),
            ["utf8-in-byte-font", "shadowed-operator"]
        );
        assert_eq!(
            names(b"%lint: allow utf8-in-byte-font\n"),
            ["shadowed-operator"]
        );
        assert_eq!(
            names(b"%lint: allow shadowed-operator utf8-in-byte-font\n"),
            Vec::<&str>::new()
        );
    }
}
