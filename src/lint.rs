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
/// comment line (issue #1052). Only `shadowed-operator` can be silenced:
/// the other checks are what `--lint-strict` drivers depend on, and a
/// program has no business turning them off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LintAllows {
    shadowed_operator: bool,
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
            if name == "shadowed-operator" {
                allows.shadowed_operator = true;
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
        findings
    }
}

/// Most `shadowed-operator` findings reported for one program; the rest are
/// counted in a closing finding, so a prologue that redefines fifty names
/// stays readable.
const SHADOWED_SHOWN: usize = 10;

/// Where a shadowing binding was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ShadowScope {
    /// `userdict`: the program's own global names (the usual `def`).
    User,
    /// `systemdict` itself, where `store` finds an operator's name.
    System,
}

/// What [`Interp::trace_operator_shadows`] keeps about one shadowing binding.
pub(crate) struct ShadowSite {
    kind: &'static str,
    /// `when the program reached line N`, with ` (file)` for a `run`
    /// file; `None` when the scanner's line is unknown.
    location: Option<String>,
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
    sites: std::collections::HashMap<(u32, ShadowScope), (String, ShadowSite)>,
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
            Some(kind) => match self.sites.get_mut(&key) {
                Some((_, site)) => site.kind = kind,
                None => {
                    let site = ShadowSite {
                        kind,
                        location: location(),
                    };
                    self.sites.insert(key, (name.to_string(), site));
                }
            },
            // A later `/sub { ... } def` ends the shadow, except in
            // `systemdict`, where the operator it replaced is already gone.
            None if scope != ShadowScope::System => {
                self.sites.remove(&key);
            }
            None => {}
        }
    }

    fn site(&self, name: &str, scope: ShadowScope) -> Option<&ShadowSite> {
        let id = crate::name::intern(name).id();
        self.sites.get(&(id, scope)).map(|(_, site)| site)
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
/// - `systemdict` is compared with the operators a fresh interpreter has: a
///   `store` of an operator's name overwrites the operator there (that is
///   where `store` finds the name), and `systemdict /sub 5 put` does the same.
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
    let mut push = |name: &str, scope, kind, trace_site: Option<&ShadowSite>| {
        shadowed.push(Shadow {
            name: name.to_string(),
            scope,
            kind,
            location: trace_site.and_then(|s| s.location.clone()),
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
            trace.and_then(|t| t.site(name, ShadowScope::User)),
        );
    }
    // Operators that `store` or `put` replaced in `systemdict`.
    let mut pristine = crate::object::Dict::new();
    crate::ops::install_all(&mut pristine);
    for (key, original) in pristine.pairs() {
        let (Value::Name(name), Value::Operator(_)) = (&key.value, &original.value) else {
            continue;
        };
        let Some(now) = system.borrow().get(name) else {
            continue;
        };
        if matches!(now.value, Value::Operator(_)) {
            continue;
        }
        let Some(kind) = shadow_kind(&original, &now) else {
            continue;
        };
        push(
            name,
            ShadowScope::System,
            kind,
            trace.and_then(|t| t.site(name, ShadowScope::System)),
        );
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
            "/{name} was redefined as {article} {kind} in systemdict{at}, replacing the operator {name} for the whole program: every later `{name}` pushes that value instead of running the operator (rename it; `store` and `put` change systemdict itself, `def` defines in userdict)"
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
            findings.push(LintFinding {
                check: "stack-leak",
                message: format!(
                    "{} item(s) left on the operand stack at the end of the program: {}{more}",
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
            found[0].contains("as an integer when the program reached line 5, replacing"),
            "{found:?}"
        );
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
    fn a_procedure_defined_later_ends_the_shadow() {
        let it = traced("/sub 1 def\n/sub { neg add } def");
        assert!(shadow_messages(&it).is_empty());
    }

    #[test]
    fn store_and_put_into_systemdict_replace_the_operator() {
        // `store` finds the name in systemdict when no other dictionary defines it.
        let found = shadow_messages(&traced("\n/sub 5 store"));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("/sub was redefined as an integer in systemdict when the program reached line 2, replacing the operator sub for the whole program"),
            "{found:?}"
        );
        // Without a trace the end state still shows it (no line), `put` included.
        let found = shadow_messages(&run("/sub 5 store systemdict /dict (x) put"));
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found[0].starts_with("/dict was redefined as a string in systemdict"),
            "{found:?}"
        );
        assert!(
            found[1].starts_with("/sub was redefined as an integer in systemdict"),
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
}
