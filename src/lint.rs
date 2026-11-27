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

use std::rc::Rc;

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
/// `shadowed-library-name` (#1456) is made only for a run that ended in an
/// error: it names a name the program defined that a library also defines and
/// that a library procedure read by name where the error happened.
///
/// `pscat-mcp`'s `split_lint_findings` (this crate's `mcp_server`) uses the
/// same list, so the two cannot drift.
pub const ERROR_EXPLAINING_CHECKS: &[&str] = &["shadowed-operator", "shadowed-library-name"];

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
    check_shadowed_library_names(interp, &mut findings);
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
    check_shadowed_library_names(interp, &mut findings);
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
/// comment line (issue #1052). Only the heuristics that can be wrong about a
/// deliberate choice can be silenced: `shadowed-operator`,
/// `shadowed-library-name` (#1456) and `utf8-in-byte-font` (#1412). The other
/// checks are what `--lint-strict` drivers depend on, and a program has no
/// business turning them off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LintAllows {
    shadowed_operator: bool,
    shadowed_library_name: bool,
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
                "shadowed-library-name" => allows.shadowed_library_name = true,
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
        if self.shadowed_library_name {
            findings.retain(|f| f.check != "shadowed-library-name");
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

/// Where the operand stack grew, as the interpreter saw it (issues #1081,
/// #1366), so `[stack-leak]` can say where the oldest leftover item came from.
///
/// The stack is sampled at each source token's boundary, where it holds the
/// result of every earlier token (a loop or a call has finished). What a
/// token left is credited to that token's line, and, when the item that
/// lasted from the token's run is the work of one procedure element, to that
/// element too (#1366): between two tokens the interpreter notes after every
/// step how low the stack went and which element pushed the item just above
/// that low point, which is the oldest item the token left.
///
/// A credit is kept only while it can be shown to still hold. The element's
/// site is turned into a line at the boundary, if no body has been added to or
/// dropped from the table of lines since the site was taken (an address freed
/// and given to another body is not the same body) and the item at that slot
/// is still what the element pushed ([`Witness`]); otherwise the token's line
/// is credited, as before #1366. The bottom item's witness is checked at every
/// boundary, so an item replaced in place (`]` for its mark, `exch`, `restore`)
/// takes the token's line too (#1081).
///
/// Kept as runs, not one entry per item: `start` is the depth at which a run
/// of items begins, and a run is dropped when the depth falls back to it, so
/// `clear`, `aload` of 10 000 items or a loop that pushes a million values
/// cost a handful of entries. At most [`MAX_RUNS`] of them, in a vector
/// allocated when tracing starts (so the memory cap sees it then, and a run
/// never grows it): a program that leaks from thousands of lines would
/// otherwise keep one each, so past the cap the newest items share the last
/// run's origin.
#[derive(Clone)]
pub(crate) struct StackTrace {
    runs: Vec<StackRun>,
    /// The lowest depth since the last boundary: the items below it are the
    /// same ones, the items from it up are new.
    low: usize,
    /// What the bottom item was when the first run began, to see it replaced.
    bottom: Option<Witness>,
    /// The element that pushed the item at depth `low`, valid when `item_set`.
    item: Credit,
    item_set: bool,
    /// The procedure element the step now running executes. A `tentative`
    /// one is the first element of the body of the loop frame on top, guessed
    /// before the loop has said whether it iterates; the next sample checks it
    /// ([`StackTrace::ran`]).
    pending: Site,
    tentative: bool,
    /// The depth of the execution stack when a tentative site was set.
    pending_frames: usize,
}

/// A procedure element, as a
/// [`storage_site`](crate::object::PsArray::storage_site), and the generation
/// of the table of lines when it was taken. Key `0` is no element: a scanned
/// token, or a loop's own push.
#[derive(Clone, Copy, Default)]
pub(crate) struct Site {
    pub(crate) key: usize,
    pub(crate) slot: usize,
    pub(crate) generation: u32,
}

/// An element's claim to the item it pushed: the site, and what the item was.
#[derive(Clone, Copy)]
struct Credit {
    site: Site,
    wit: Witness,
}

impl Credit {
    const NONE: Credit = Credit {
        site: Site {
            key: 0,
            slot: 0,
            generation: 0,
        },
        wit: Witness::NONE,
    };
}

/// Enough of an object to notice it was replaced: its kind and, for a scalar,
/// its value; a composite by its storage and extent, so another array or
/// string is another object. Three words, compared as numbers: it is taken at
/// every token boundary.
#[derive(Clone, Copy, PartialEq)]
struct Witness {
    kind: u8,
    a: u64,
    b: u64,
}

impl Witness {
    const NONE: Witness = Witness {
        kind: 0,
        a: 0,
        b: 0,
    };

    #[inline]
    fn of(o: &Object) -> Witness {
        let (kind, a, b) = match &o.value {
            Value::Integer(i) => (1, *i as u64, 0),
            Value::Real(r) => (2, r.to_bits(), 0),
            Value::Boolean(v) => (3, u64::from(*v), 0),
            Value::Name(n) => (4, u64::from(n.id()), 0),
            Value::Array(arr) => {
                let (k, off, len) = arr.identity();
                (5, k as u64, ((off as u64) << 32) ^ len as u64)
            }
            Value::String(st) => {
                let (k, off, len) = st.identity();
                (6, k as u64, ((off as u64) << 32) ^ len as u64)
            }
            Value::Dict(d) => (7, Rc::as_ptr(d) as *const () as u64, 0),
            // The other kinds by their type name (a string literal's address).
            _ => (8, o.type_name().as_ptr() as u64, 0),
        };
        Witness { kind, a, b }
    }
}

/// Most runs a [`StackTrace`] keeps. Each is a few dozen bytes, so the trace
/// is bounded at tens of KiB whatever the program leaks.
const MAX_RUNS: usize = 1024;

#[derive(Clone)]
struct StackRun {
    start: usize,
    at: Origin,
}

/// What the scanner was on when the stack grew, and the line of the procedure
/// element that left the oldest of the new items, when one did (#1366). Plain
/// numbers, no address: nothing here can name a body that is gone.
#[derive(Clone, PartialEq)]
struct Origin {
    /// The element's own line, `0` for none.
    own: u32,
    /// The line of the scanner's last token: the call (or the loop's end)
    /// that ran the element, and what is named when the element's own line is
    /// not known or is not in this text. `0` when unknown, which a run left
    /// by an earlier program is.
    line: u32,
    /// The text the scanner was reading: a `run` file's name, or the main
    /// program's (the host's name for it, `None` if it has none).
    source: Option<Rc<str>>,
    /// Whether `source` is a `run` file (named in the finding) and not the
    /// main program.
    is_run: bool,
}

/// The frame on top of the execution stack after a step, as
/// [`StackTrace::ran`] needs it.
#[derive(Clone, Copy)]
pub(crate) enum TopFrame {
    /// A procedure: its body's key, its `pc`, and the site of the element
    /// before the `pc` (key `0` if it is the first).
    Proc {
        key: usize,
        pc: usize,
        before: Site,
    },
    /// A loop: its body's key.
    Loop {
        key: usize,
    },
    None,
}

/// What the interpreter knows of the scanner when it samples the stack.
#[derive(Clone, Copy)]
pub(crate) struct ScanAt<'a> {
    /// The line of the last token scanned.
    pub(crate) line: Option<usize>,
    /// The `run` file being read, `None` for the main program.
    pub(crate) run: Option<&'a Rc<str>>,
    /// The name the host gave the main program.
    pub(crate) main: Option<&'a Rc<str>>,
}

/// How the trace stands at a boundary, before this boundary's own run.
struct Settled {
    /// The lowest depth that was kept: runs from it up are dropped.
    low: usize,
    /// The bottom item was replaced in place (or the runs are lost): drop them
    /// all, and credit the token.
    replaced: bool,
}

impl StackTrace {
    pub(crate) fn new() -> Self {
        StackTrace {
            runs: Vec::with_capacity(MAX_RUNS),
            low: 0,
            bottom: None,
            item: Credit::NONE,
            item_set: false,
            pending: Site::default(),
            tentative: false,
            pending_frames: 0,
        }
    }

    /// After a step: the stack is `ostack`. Notes how low it has been since the
    /// last boundary and, when an item has just appeared at that low point,
    /// which procedure element pushed it and what it was. Constant work,
    /// inlined into the traced step loop. `ran` is asked for only then (the
    /// element that has just run, with the table generation to match it).
    #[inline(always)]
    pub(crate) fn step(&mut self, ostack: &[Object], ran: impl FnOnce(&Self) -> Site) {
        let d = ostack.len();
        if d <= self.low {
            self.low = d;
            self.item_set = false;
        } else if !self.item_set {
            // A token or a loop's own push has no element to name: nothing
            // to look at, nothing to check later.
            self.item = if self.pending.key == 0 && !self.tentative {
                Credit::NONE
            } else {
                Credit {
                    site: ran(self),
                    wit: Witness::of(&ostack[self.low]),
                }
            };
            self.item_set = true;
        }
    }

    /// Whether the next sample will read the element the step about to run
    /// executes: only when no item has been found at the low point yet.
    #[inline(always)]
    pub(crate) fn wants_pending(&self) -> bool {
        !self.item_set
    }

    /// The procedure element the step about to run executes (key `0` for
    /// none), and whether that is only the guess for a loop's first element.
    #[inline(always)]
    pub(crate) fn set_pending(&mut self, site: Site, tentative: bool, frames: usize) {
        self.pending = site;
        self.tentative = tentative;
        self.pending_frames = frames;
    }

    /// The element the step that has just run executed. A tentative one (the
    /// first element of the body of a loop frame that was on top) is the one
    /// that ran if that body is now on top with one element done, or the loop
    /// is back on top with as many frames as before (a body of one element is
    /// popped as its element starts). Otherwise the loop was over and its
    /// parent's next element ran: the element before the `pc` of the
    /// procedure now on top, which is right unless that element was the
    /// parent's last (its frame is gone, and the call into it is named, as
    /// without #1366).
    pub(crate) fn ran(&self, top: TopFrame, frames: usize) -> Site {
        if !self.tentative {
            return self.pending;
        }
        match top {
            TopFrame::Proc { key, pc, .. } if key == self.pending.key && pc == 1 => self.pending,
            TopFrame::Loop { key } if key == self.pending.key && frames == self.pending_frames => {
                self.pending
            }
            TopFrame::Proc { before, .. } => before,
            _ => Site::default(),
        }
    }

    /// A program begins: what is on the stack was left by an earlier one,
    /// whose lines are not this program's. Its runs name no line.
    pub(crate) fn begin_program(&mut self, depth: usize) {
        for r in &mut self.runs {
            r.at.own = 0;
            r.at.line = 0;
        }
        self.low = depth;
        self.item_set = false;
        self.pending = Site::default();
        self.tentative = false;
    }

    fn settle(&self, ostack: &[Object]) -> Settled {
        let low = self.low.min(ostack.len());
        // Below the low point nothing was touched, unless the bottom item was
        // replaced inside an operator (`]` for its mark, `exch`, `restore`),
        // which the depth does not show.
        let replaced = low > 0
            && (self.runs.is_empty() || self.bottom.is_some_and(|w| Witness::of(&ostack[0]) != w));
        Settled {
            low: if replaced { 0 } else { low },
            replaced,
        }
    }

    /// The origin of what lies above `low`, which is new: the token, and the
    /// element's own line when it can be shown to be the right one.
    fn origin(
        &self,
        ostack: &[Object],
        low: usize,
        trusted: bool,
        at: ScanAt<'_>,
        lines: &crate::srcpos::BodyLines,
    ) -> Origin {
        let source = at.run.or(at.main).cloned();
        let c = &self.item;
        let own = if trusted && self.item_set && c.site.key != 0 {
            (c.site.generation == lines.generation() && Witness::of(&ostack[low]) == c.wit)
                .then(|| lines.position_at((c.site.key, c.site.slot)))
                .flatten()
                .filter(|(_, name)| name.map(|n| &**n) == source.as_deref())
                .map_or(0, |(line, _)| line)
        } else {
            0
        };
        Origin {
            own,
            line: at.line.and_then(|l| u32::try_from(l).ok()).unwrap_or(0),
            source,
            is_run: at.run.is_some(),
        }
    }

    /// A token boundary: the stack is now `ostack` and the scanner `at`. What
    /// the last token (and everything it ran) left above the lowest the stack
    /// went is a new run credited to it, and to the element that pushed its
    /// oldest item when there was one.
    pub(crate) fn note(
        &mut self,
        ostack: &[Object],
        at: ScanAt<'_>,
        lines: &crate::srcpos::BodyLines,
    ) {
        let depth = ostack.len();
        let Settled { low, replaced } = self.settle(ostack);
        if replaced {
            self.runs.clear();
        }
        // Whatever was above the low point is gone.
        while self.runs.last().is_some_and(|r| r.start >= low) {
            self.runs.pop();
        }
        if depth > low {
            let origin = self.origin(ostack, low, !replaced, at, lines);
            // A run that continues from the same place stays one entry; past
            // the cap the newest items join the last run.
            if self.runs.len() < MAX_RUNS && self.runs.last().is_none_or(|r| r.at != origin) {
                if self.runs.is_empty() {
                    self.bottom = Some(Witness::of(&ostack[0]));
                }
                self.runs.push(StackRun {
                    start: low,
                    at: origin,
                });
            }
        }
        self.low = depth;
        self.item_set = false;
        // A boundary step runs a scanned token, no procedure element.
        self.pending = Site::default();
        self.tentative = false;
    }

    #[cfg(test)]
    fn run_count(&self) -> usize {
        self.runs.len()
    }

    /// Where the oldest item on the finished `ostack` came from, as
    /// `line N` (` (file)` for a `run` file). The last token ran after the
    /// last sample, so it is settled first, as [`note`](Self::note) would,
    /// without changing the trace. `None` when no line is known.
    ///
    /// The line is the element's own (#1366) when the program wrote it in the
    /// text the scanner was reading then: the main program, or the `run` file
    /// by that name. Otherwise it is the line of the scanner's token that ran
    /// it: a library procedure that leaves a value has no better name than
    /// the call into it, and a prelude the host named and ran earlier is not
    /// this program.
    pub(crate) fn oldest(
        &self,
        ostack: &[Object],
        last: ScanAt<'_>,
        lines: &crate::srcpos::BodyLines,
    ) -> Option<String> {
        if ostack.is_empty() {
            return None;
        }
        let Settled { low, replaced } = self.settle(ostack);
        let kept = self.runs.first().filter(|r| !replaced && r.start < low);
        let made;
        let at = match kept {
            Some(r) => &r.at,
            None => {
                made = self.origin(ostack, low, !replaced, last, lines);
                &made
            }
        };
        let line = if at.own > 0 { at.own } else { at.line };
        if line == 0 {
            return None;
        }
        let file = at
            .source
            .as_deref()
            .filter(|_| at.is_run)
            .map(|f| format!(" ({})", crate::interp::report_name(f)))
            .unwrap_or_default();
        Some(format!("line {line}{file}"))
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
    /// The names libraries define and the program also does (#1456).
    pub(crate) names: NameClashes,
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

/// Most names the table of library definitions holds. The libraries in `lib/`
/// define about 1300 distinct names among them; the table is allocated whole
/// when tracing starts (3584 is the most a table of 4096 buckets keeps without
/// growing), so a program that loads a file defining a million names costs
/// the memory cap nothing more and the rest are not followed.
pub(crate) const MAX_LIBRARY_NAMES: usize = 3584;
/// Most distinct `run` files whose names are followed.
const MAX_LIBRARY_FILES: usize = 64;
/// Most `shadowed-library-name` findings for one run.
const CLASHES_SHOWN: usize = 5;

/// Where the program defined a name that a library defines too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramDef {
    /// Not (yet) in this program.
    No,
    /// In `userdict` before the library was loaded; the line is unknown.
    Before,
    /// By a `def` or `store` on this line of the program.
    At(u32),
}

#[derive(Clone, Copy)]
struct LibraryName {
    /// An index into [`NameClashes::files`]: the file that defined it first.
    file: u16,
    program: ProgramDef,
    /// The last place a `def` of this name was found not to be the program's
    /// (a library procedure keeping its own state while the program runs): the
    /// storage of the body and the slot that ran it, and the table-of-lines
    /// generation. A `def` made from the same place is the same library
    /// procedure, so it is not looked into again.
    not_program: (usize, usize, u32),
}

/// The names a library defines and the program also does, noted as the
/// program ran when [`Interp::trace_operator_shadows`] turned tracing on
/// (issue #1456), so `[shadowed-library-name]` can say, after an error, that a
/// library procedure read a name the program had also defined.
///
/// A library's name is one a `run` file defines in `userdict` while it is
/// the innermost source being read: the file's own top level. The program's
/// definition of it, in any dictionary (`/shade [..] def` inside
/// `2 dict begin` is the case of #1225), is noted once per program, with its
/// line, the first time it is made (a library procedure `def`ing its own state
/// while the program runs is a library's, and not noted).
///
/// Bounded: [`MAX_LIBRARY_NAMES`] names and [`MAX_LIBRARY_FILES`] files, in
/// tables allocated when tracing starts. A `def` costs a probe of one table in
/// the traced run, and a plain run is not traced at all.
pub(crate) struct NameClashes {
    names: std::collections::HashMap<u32, LibraryName, crate::name::IdHashBuilder>,
    files: Vec<Rc<str>>,
    /// The names that library procedures read where the last run failed, by
    /// bare name: the executable names near the running element of each
    /// procedure frame that was written in a `run` file.
    read: Vec<u32>,
}

impl Default for NameClashes {
    fn default() -> Self {
        NameClashes {
            names: std::collections::HashMap::with_capacity_and_hasher(
                MAX_LIBRARY_NAMES,
                crate::name::IdHashBuilder,
            ),
            files: Vec::with_capacity(MAX_LIBRARY_FILES),
            read: Vec::new(),
        }
    }
}

impl NameClashes {
    /// The program that `file` is loaded by `run` defined `name`. The first
    /// definition counts. `before` is whether `userdict` already held the
    /// name, which only the program can have put there.
    pub(crate) fn library_def(&mut self, name: u32, file: &Rc<str>, before: bool) {
        if self.names.len() >= MAX_LIBRARY_NAMES || self.names.contains_key(&name) {
            return;
        }
        let index = match self.files.iter().position(|f| **f == **file) {
            Some(i) => i,
            None if self.files.len() < MAX_LIBRARY_FILES => {
                self.files.push(file.clone());
                self.files.len() - 1
            }
            None => return,
        };
        self.names.insert(
            name,
            LibraryName {
                file: index as u16,
                program: if before {
                    ProgramDef::Before
                } else {
                    ProgramDef::No
                },
                not_program: (0, 0, 0),
            },
        );
    }

    /// What the program has defined of `name`, when a library defines it.
    pub(crate) fn program_def(&self, name: u32) -> Option<ProgramDef> {
        self.names.get(&name).map(|n| n.program)
    }

    /// Whether a `def` of `name` from `site` was already found not to be the
    /// program's.
    pub(crate) fn is_known_library_site(&self, name: u32, site: (usize, usize, u32)) -> bool {
        self.names.get(&name).is_some_and(|n| n.not_program == site)
    }

    /// A `def` of `name` from `site` was found not to be the program's.
    pub(crate) fn set_library_site(&mut self, name: u32, site: (usize, usize, u32)) {
        if let Some(n) = self.names.get_mut(&name) {
            n.not_program = site;
        }
    }

    /// The program defined `name` on `line`.
    pub(crate) fn set_program_line(&mut self, name: u32, line: u32) {
        if let Some(n) = self.names.get_mut(&name) {
            n.program = ProgramDef::At(line);
        }
    }

    /// A new program is starting: what the last one defined is not this one's
    /// (the libraries it loaded are still loaded).
    pub(crate) fn begin_program(&mut self) {
        for n in self.names.values_mut() {
            n.program = ProgramDef::No;
            n.not_program = (0, 0, 0);
        }
        self.read.clear();
    }

    /// The names library procedures read where the run failed.
    pub(crate) fn set_read(&mut self, names: Vec<u32>) {
        self.read = names;
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
    /// Distinct fonts past [`UTF8_FONTS_SHOWN`] that took UTF-8 text, up to
    /// [`UTF8_MORE_FONTS_COUNTED`]: a set, so a font shown again is not
    /// counted again (#1446).
    more_fonts: std::collections::HashSet<Box<str>>,
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
/// Fonts past [`UTF8_FONTS_SHOWN`] that are counted (by name, so each costs at
/// most a name's bytes): a program that makes more fonts than this reads
/// "at least".
pub(crate) const UTF8_MORE_FONTS_COUNTED: usize = 1024;

impl Utf8TextTrace {
    pub(crate) fn clear(&mut self) {
        self.fonts.clear();
        self.more_fonts.clear();
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
            more_fonts: self.more_fonts.len(),
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
                if self.more_fonts.len() < UTF8_MORE_FONTS_COUNTED
                    && !self.more_fonts.contains(font)
                {
                    self.more_fonts.insert(font.into());
                }
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
    if !trace.more_fonts.is_empty() {
        findings.push(LintFinding {
            check: "utf8-in-byte-font",
            message: format!(
                "UTF-8 text was also shown in {} more byte-mode font(s) ({UTF8_FONTS_SHOWN} shown)",
                trace.more_fonts.len()
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

/// A name the program defined that a library defines too, when a library
/// procedure read it by name where the run failed (issue #1456, after #1225):
/// a program that keeps a colour in a local `/shade` while a library procedure
/// calls `shade` by name gets an error from deep inside the library that never
/// mentions the program's `/shade`.
///
/// Only for a run that ended in an error: the interpreter notes, when an
/// uncaught error unwinds, the names in the elements shown around each library
/// procedure's running element (the ones the error report's context lines
/// show), and this reports those the program also defined. A program that
/// shadows a library name and works is not reported. `pscat lint` without an
/// error has nothing to say here: the sweep over `lib/`, `gallery/` and
/// `examples/` found 7% of the programs that load a library define a name the
/// library defines (`shade`, `tx`, `w`, `pal`, `showctr`), every one on
/// purpose and working, so a static note would be noise
/// (notes/1456-lint-library-name-clash.md).
fn check_shadowed_library_names(interp: &Interp, findings: &mut Vec<LintFinding>) {
    let Some(trace) = interp.shadow_trace() else {
        return;
    };
    let names = &trace.names;
    let mut hits: Vec<(String, ProgramDef, &str)> = Vec::new();
    for &id in &names.read {
        let Some(lib) = names.names.get(&id) else {
            continue;
        };
        if lib.program == ProgramDef::No {
            continue;
        }
        let file = names
            .files
            .get(usize::from(lib.file))
            .map_or("a library", |f| &**f);
        hits.push((crate::name::resolve(id).to_string(), lib.program, file));
    }
    hits.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, program, file) in hits.iter().take(CLASHES_SHOWN) {
        let at = match program {
            ProgramDef::At(line) => format!("at line {line} of your program"),
            _ => "earlier in your program, before the library was loaded".to_string(),
        };
        let file = crate::interp::report_name(file);
        findings.push(LintFinding {
            check: "shadowed-library-name",
            message: format!(
                "`{name}` is defined {at} and also by {file}, and a library procedure reads `{name}` by name where the error happened, so it may have found yours (or yours has replaced the library's): rename yours to a name the library does not define, or, where your own code means the library's procedure, write `//{name} exec`, which is bound when the program is scanned so a dictionary pushed later cannot intercept it"
            ),
        });
    }
    if hits.len() > CLASHES_SHOWN {
        findings.push(LintFinding {
            check: "shadowed-library-name",
            message: format!(
                "{} more name(s) the program defined that libraries define too were read by name where the error happened ({CLASHES_SHOWN} shown, in name order)",
                hits.len() - CLASHES_SHOWN
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
                .and_then(|t| t.oldest(ostack, interp.last_scan_at(), &interp.body_lines()));
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
        it.run_str(src).expect("the program runs");
        check(&it, true)
            .into_iter()
            .find(|f| f.check == "stack-leak")
            .map(|f| f.message)
            .unwrap_or_default()
    }

    /// The line `[stack-leak]` names for the oldest item.
    fn leak_line(src: &str) -> usize {
        let m = leak_message(src);
        let at = m
            .split("was left when the program reached line ")
            .nth(1)
            .unwrap_or_else(|| panic!("no line in: {m}"));
        at.split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("no line number in: {m}"))
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
    fn a_leak_in_a_loop_names_the_element_in_its_body() {
        // #1366: the line of the leaking element, not the closing `} for`.
        // The first item here is the loop index, swapped away by `exch` and
        // `pop`: the tracker cannot show which element's value it is, so the
        // loop's closing line is named (as before #1366).
        let src = "0 1 9 {\n  /gq exch\n  pop\n  /gq 1\n} for\nshowpage";
        let m = leak_message(src);
        assert!(m.contains("30 item(s)"), "{m}");
        assert!(
            m.contains("was left when the program reached line 5"),
            "{m}"
        );
        let src = "0 1 9 {\n  pop\n  /gq 1\n} for\nshowpage";
        let m = leak_message(src);
        assert!(
            m.contains("the oldest, /gq, was left when the program reached line 3"),
            "{m}"
        );
    }

    #[test]
    fn a_leaky_call_names_the_element_that_left_the_item() {
        // #1366: the line inside the procedure, not the call site (#1081).
        let src = "/leak {\n  1 2\n  3\n} def\n1 2 add pop\nleak\nshowpage";
        let m = leak_message(src);
        assert!(m.contains("3 item(s)"), "{m}");
        assert!(
            m.contains("the oldest, 1, was left when the program reached line 2"),
            "{m}"
        );
    }

    #[test]
    fn a_leak_two_calls_deep_names_the_innermost_element() {
        // `mul` replaced the 7 with 14: its producer's value is gone, so the
        // call that ran it is named.
        let src = "/f {\n  1 pop\n  7\n} def\n/g {\n  f\n  2 mul\n} def\ng\n";
        let m = leak_message(src);
        assert!(
            m.contains("the oldest, 14, was left when the program reached line 9"),
            "{m}"
        );
        // The leak is in `f`, which `g` calls and nothing pops.
        let src = "/f {\n  1 pop\n  7\n} def\n/g {\n  f\n} def\ng\n";
        let m = leak_message(src);
        assert!(
            m.contains("the oldest, 7, was left when the program reached line 3"),
            "{m}"
        );
    }

    #[test]
    fn a_leak_in_a_conditional_branch_names_its_line() {
        let src = "/f {\n  true {\n    5\n  } if\n} def\nf\n";
        let m = leak_message(src);
        assert!(
            m.contains("the oldest, 5, was left when the program reached line 3"),
            "{m}"
        );
    }

    #[test]
    fn a_leak_from_a_run_file_stays_on_the_call_into_it_when_the_caller_is_the_program() {
        // A library procedure that returns a value the program does not pop:
        // the lib's own line is no use to its caller, the call is (#1366).
        let mut it = Interp::with_page(50, 50).expect("page");
        it.trace_stack_leaks();
        it.add_virtual_file("lib.ps", b"% lib\n/leak {\n  42\n} def\n".as_slice())
            .expect("virtual file");
        it.run_str("(lib.ps) run\n\nleak\n").expect("runs");
        let m = check(&it, true)
            .into_iter()
            .find(|f| f.check == "stack-leak")
            .expect("leak")
            .message;
        assert!(
            m.contains("the oldest, 42, was left when the program reached line 3"),
            "{m}"
        );
        assert!(!m.contains("lib.ps"), "{m}");
        // Through a procedure of the program's own: the token that ran it,
        // the same as without #1366 (the frames between are not walked, to
        // keep a sample constant work).
        let mut it = Interp::with_page(50, 50).expect("page");
        it.trace_stack_leaks();
        it.add_virtual_file("lib.ps", b"% lib\n/leak {\n  42\n} def\n".as_slice())
            .expect("virtual file");
        it.run_str("(lib.ps) run\n/user {\n  1 pop\n  leak\n  2\n} def\n\nuser\n")
            .expect("runs");
        let m = check(&it, true)
            .into_iter()
            .find(|f| f.check == "stack-leak")
            .expect("leak")
            .message;
        assert!(
            m.contains("the oldest, 42, was left when the program reached line 8"),
            "{m}"
        );
    }

    #[test]
    fn a_leak_in_a_run_file_names_the_line_in_that_file() {
        // The scanner is in the file when its own procedure leaks.
        let mut it = Interp::with_page(50, 50).expect("page");
        it.trace_stack_leaks();
        it.add_virtual_file(
            "lib.ps",
            b"% lib\n/leak {\n  42\n} def\n\nleak\n".as_slice(),
        )
        .expect("virtual file");
        it.run_str("(lib.ps) run\n").expect("runs");
        let m = check(&it, true)
            .into_iter()
            .find(|f| f.check == "stack-leak")
            .expect("leak")
            .message;
        assert!(
            m.contains("the oldest, 42, was left when the program reached line 3 (lib.ps)"),
            "{m}"
        );
    }

    #[test]
    fn a_prelude_the_host_named_is_not_mistaken_for_the_program() {
        // psanim's pattern (#1405): a prelude read in one `run_str` under a
        // name, a scene in a later one. A body of the prelude has a line, but
        // not of this text.
        let mut it = Interp::with_page(50, 50).expect("page");
        it.trace_stack_leaks();
        it.set_main_file_name(Some("prelude.ps"));
        it.run_str("% prelude\n\n/leak {\n  42\n} def\n")
            .expect("runs");
        it.set_main_file_name(None);
        it.run_str("leak\n").expect("runs");
        let m = check(&it, true)
            .into_iter()
            .find(|f| f.check == "stack-leak")
            .expect("leak")
            .message;
        assert!(
            m.contains("the oldest, 42, was left when the program reached line 1"),
            "{m}"
        );
    }

    #[test]
    fn a_program_that_leaks_from_every_line_keeps_the_trace_bounded() {
        let mut it = Interp::with_page(50, 50).expect("page");
        it.trace_stack_leaks();
        it.run_str(&"1\n".repeat(5000)).expect("runs");
        let t = it.stack_trace().expect("trace");
        // One run per line, up to a cap (the items past it share the last
        // run), not one per leaked item.
        assert_eq!(t.run_count(), 1024);
        let m = check(&it, true)
            .into_iter()
            .find(|f| f.check == "stack-leak")
            .expect("leak")
            .message;
        assert!(
            m.contains("the oldest, 1, was left when the program reached line 1"),
            "{m}"
        );
    }

    #[test]
    fn a_loop_that_pushes_from_two_lines_keeps_the_trace_small() {
        let mut it = Interp::with_page(50, 50).expect("page");
        it.trace_stack_leaks();
        it.run_str("100000 {\n  1\n  2\n} repeat").expect("runs");
        assert!(it.stack_trace().expect("trace").run_count() < 4);
        let m = check(&it, true)
            .into_iter()
            .find(|f| f.check == "stack-leak")
            .expect("leak")
            .message;
        assert!(
            m.contains("the oldest, 1, was left when the program reached line 2"),
            "{m}"
        );
    }

    #[test]
    fn a_loop_that_is_over_does_not_lend_its_first_line_to_the_next_element() {
        // The step after a finished loop runs its parent's next element: the
        // guess that it is the loop's first element (line 3) must not stick.
        // The element is the last of `f`, whose frame is gone, so the call
        // is named.
        let src = "/f {\n  2 {\n    1 pop\n  } repeat\n  9\n} def\nf\n";
        let m = leak_message(src);
        assert!(
            m.contains("the oldest, 9, was left when the program reached line 7"),
            "{m}"
        );
        // With an element after it the frame is there, and the line is its own.
        let src = "/f {\n  2 {\n    1 pop\n  } repeat\n  9\n  1 pop\n  8\n} def\nf\n";
        let m = leak_message(src);
        assert!(
            m.contains("the oldest, 9, was left when the program reached line 5"),
            "{m}"
        );
    }

    #[test]
    fn an_empty_loop_body_names_the_loop_not_line_0() {
        // Nothing in the body to name: the loop's own push is the loop's.
        assert_eq!(leak_line("0 1 9 { } for\n"), 1);
        assert_eq!(leak_line("0 1 100000 {\n} for\n"), 2);
        assert_eq!(leak_line("[1 2 3] {\n} forall\n"), 2);
        assert_eq!(leak_line("0 1 9 {\n} for\n"), 2);
    }

    #[test]
    fn a_loop_index_the_body_does_not_pop_is_credited_to_the_first_element() {
        // The step that starts an iteration pushes the index and runs the first
        // element, so the index is the first element's.
        let m = leak_message("0 1 9 {\n  1 pop\n} for\n");
        assert!(
            m.contains("the oldest, 0, was left when the program reached line 2"),
            "{m}"
        );
    }

    #[test]
    fn a_body_of_one_element_is_named_too() {
        let src = "3 {\n  1\n} repeat\n";
        let m = leak_message(src);
        assert!(
            m.contains("the oldest, 1, was left when the program reached line 2"),
            "{m}"
        );
    }

    #[test]
    fn tracing_the_stack_changes_no_step_count_or_result() {
        // The traced run steps through the instrumented loop, which does not
        // fuse `{ } if`: the count and the stacks must be the same (#1213).
        let src =
            "0 1 200 {\n  dup 2 mod 0 eq { 1 add } { 2 sub } ifelse\n  3 gt { 5 } if\n} for\n";
        let run = |on: bool| {
            let mut it = Interp::with_page(50, 50).expect("page");
            it.set_limits(crate::Limits::none().with_max_steps(10_000_000))
                .expect("limits");
            if on {
                it.trace_stack_leaks();
            }
            it.run_str(src).expect("runs");
            let stack: Vec<String> = it.operand_stack().iter().map(preview).collect();
            (it.steps_used(), stack)
        };
        let (off, on) = (run(false), run(true));
        assert_eq!(off, on);
        assert!(off.0 > 0);
        assert!(!off.1.is_empty());
    }

    #[test]
    fn an_item_that_replaced_its_producers_value_falls_back_to_the_token_line() {
        // Codex round 1 on #1474: the tracker must not name an own line of a
        // producer whose value was consumed or replaced. Each program's line
        // is what the base (#1081) named: the token that ran last.
        let mut wrong = Vec::new();
        for (src, line) in [
            // `]` and `>>` consume the mark and push the array or dictionary.
            ("[\n77\n]\n", 3),
            ("<<\n/a 77\n>>\n", 3),
            // `exch pop` leaves the other operand.
            ("77\n88\nexch pop\n", 3),
            ("{\n77\n88\n} exec\nexch pop\n", 5),
            // `restore` takes the save object the program put first.
            ("save\n/f {\n77\n} def\nf\nexch restore\n", 6),
            ("77\n88\n2 copy\n4 2 roll pop pop exch pop\n", 4),
        ] {
            let got = leak_line(src);
            if got != line {
                wrong.push(format!("{src:?}: line {got}, not {line}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[test]
    fn a_swept_body_is_never_named_by_a_later_one() {
        // A swept entry's address is reused by a later body (#1405's class):
        // the leak is the first `{ 77 } exec` (line 2) or, if its body is
        // gone, the call (line 3), never a line of one of the 32766 that follow.
        let mut src = String::from("{\n77\n} exec\n");
        src.push_str(&"{\n1\npop\n} exec\n".repeat(32766));
        for _ in 0..8 {
            let line = leak_line(&src);
            assert!((2..=3).contains(&line), "line {line}");
        }
    }

    #[test]
    fn an_operand_left_by_an_earlier_program_has_no_origin_in_the_next() {
        // psanim's host sequence: run, look, run again. The 77 was left by the
        // first program; the second one's lines are not its lines.
        let mut it = Interp::with_page(50, 50).expect("page");
        it.trace_stack_leaks();
        it.run_str("{\n77\n} exec\n1 pop\n").expect("first");
        let first = check(&it, true)
            .into_iter()
            .find(|f| f.check == "stack-leak")
            .expect("leak")
            .message;
        assert!(
            first.contains("the oldest, 77, was left when the program reached line 2"),
            "{first}"
        );
        it.run_str("{1 pop} exec\n").expect("second");
        let second = check(&it, true)
            .into_iter()
            .find(|f| f.check == "stack-leak")
            .expect("leak")
            .message;
        assert!(!second.contains("was left when"), "{second}");
    }

    #[test]
    fn a_site_taken_before_the_table_changed_is_not_resolved() {
        // Address reuse, controlled (no allocator involved): the site of a
        // body the table knows, but taken at an older generation of the
        // table, as one is when the body it was taken from is gone and its
        // address now belongs to this one. It must not name this one's line.
        let mut it = Interp::with_page(50, 50).expect("page");
        it.run_str("/a {\n  1\n  2\n} def\n").expect("runs");
        let Some(Object {
            value: Value::Array(body),
            ..
        }) = it.load("a")
        else {
            panic!("a is a procedure");
        };
        let (key, slot) = body.storage_site(0);
        let lines = it.body_lines();
        let stack = [Object::int(1)];
        let at = ScanAt {
            line: Some(9),
            run: None,
            main: None,
        };
        let say = |generation: u32| {
            let mut t = StackTrace::new();
            t.set_pending(
                Site {
                    key,
                    slot,
                    generation,
                },
                false,
                0,
            );
            t.step(&stack, |t| t.ran(TopFrame::None, 0));
            t.oldest(&stack, at, &lines)
        };
        // The same site at the table's current generation is the body's line.
        assert_eq!(say(lines.generation()).as_deref(), Some("line 2"));
        // At any other, the token's.
        assert_eq!(
            say(lines.generation().wrapping_sub(1)).as_deref(),
            Some("line 9")
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
    fn a_font_past_the_limit_is_counted_once_however_often_it_shows_text() {
        let fonts = [
            "Helvetica",
            "Times-Roman",
            "Courier",
            "Palatino-Roman",
            "Bookman-Light",
            "AvantGarde-Book",
            "NewCenturySchlbk-Roman",
            "AvantGarde-Book",
            "AvantGarde-Book",
        ];
        let src: String = fonts
            .iter()
            .map(|f| format!("/{f} findfont 12 scalefont setfont 5 40 moveto (\u{3c0}) show "))
            .collect();
        let m = utf8_messages(&src);
        assert_eq!(m.len(), UTF8_FONTS_SHOWN + 1, "{m:?}");
        assert!(
            m.last().unwrap().contains("2 more byte-mode font(s)"),
            "{m:?}"
        );
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
