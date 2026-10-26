//! The PostScript object model.
//!
//! Every PostScript value is an [`Object`]: a [`Value`] plus the
//! literal/executable attribute. The attribute lives as a flag on the
//! object rather than as doubled-up enum variants because *any* type can
//! carry it (`cvx` can mark an integer executable), and because it keeps
//! the "same value, different behavior" pairs — name vs. executable name,
//! array vs. procedure — as one representation each.
//!
//! Composite objects (strings, arrays, dictionaries) use `Rc<RefCell<..>>`
//! because PostScript composites have *reference* semantics: `dup` on an
//! array yields a second handle to the same storage, and mutations through
//! one handle are visible through the other. The interpreter core is
//! single-threaded (live rendering steps the machine rather than sharing
//! it across threads), so `Rc` rather than `Arc`.
//!
//! Arrays and strings are **views** ([`PsArray`], [`PsString`]): a shared
//! backing store plus offset/length. That's what makes `getinterval`
//! return a window onto the same storage — mutate the parent and the
//! interval sees it — which real PostScript code relies on (`cvs`
//! returning a prefix of its buffer, prologs slicing tables).

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::error::PsError;
use crate::file::FileHandle;
use crate::interp::Interp;
use crate::name::{self, IdHashBuilder, PsName};

pub type OpFn = fn(&mut Interp) -> Result<(), PsError>;

/// A built-in operator: its systemdict name (kept for error reporting and
/// `==` output) and the function that implements it.
#[derive(Clone, Copy)]
pub struct Operator {
    pub name: &'static str,
    pub func: OpFn,
}

/// A view into shared array storage.
#[derive(Clone)]
pub struct PsArray {
    data: Rc<RefCell<Vec<Object>>>,
    off: usize,
    len: usize,
}

impl PsArray {
    pub fn from_vec(items: Vec<Object>) -> Self {
        let len = items.len();
        PsArray {
            data: Rc::new(RefCell::new(items)),
            off: 0,
            len,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn get(&self, i: usize) -> Option<Object> {
        if i < self.len {
            Some(self.data.borrow()[self.off + i].clone())
        } else {
            None
        }
    }

    /// Apply `f` to element `i` in place, under one borrow that ends
    /// before this returns (so `f` must not try to execute anything).
    /// The interpreter's procedure fetch uses it to avoid cloning
    /// elements it only needs to look at (#323).
    #[inline(always)]
    pub(crate) fn peek<R>(&self, i: usize, f: impl FnOnce(&Object) -> R) -> Option<R> {
        if i < self.len {
            Some(f(&self.data.borrow()[self.off + i]))
        } else {
            None
        }
    }

    pub fn put(&self, i: usize, obj: Object) -> Result<(), PsError> {
        if i >= self.len {
            return Err(PsError::Rangecheck);
        }
        self.data.borrow_mut()[self.off + i] = obj;
        Ok(())
    }

    /// A sub-view sharing this view's storage.
    pub fn interval(&self, off: usize, len: usize) -> Result<PsArray, PsError> {
        if off > self.len || len > self.len - off {
            return Err(PsError::Rangecheck);
        }
        Ok(PsArray {
            data: self.data.clone(),
            off: self.off + off,
            len,
        })
    }

    /// PostScript `eq`: composites are equal only as identical objects —
    /// same storage *and* same window onto it.
    pub fn same_object(&self, other: &PsArray) -> bool {
        Rc::ptr_eq(&self.data, &other.data) && self.off == other.off && self.len == other.len
    }

    pub fn snapshot(&self) -> Vec<Object> {
        let b = self.data.borrow();
        b[self.off..self.off + self.len].to_vec()
    }

    /// Run `f` over each element in place; returns false (doing nothing)
    /// if the storage is already borrowed — the self-referential case.
    pub fn for_each_mut(&self, mut f: impl FnMut(&mut Object)) -> bool {
        let Ok(mut data) = self.data.try_borrow_mut() else {
            return false;
        };
        for el in &mut data[self.off..self.off + self.len] {
            f(el);
        }
        true
    }

    fn identity(&self) -> (usize, usize, usize) {
        (Rc::as_ptr(&self.data) as usize, self.off, self.len)
    }

    pub(crate) fn data_rc(&self) -> &Rc<RefCell<Vec<Object>>> {
        &self.data
    }
}

/// A view into shared string (byte) storage.
#[derive(Clone)]
pub struct PsString {
    data: Rc<RefCell<Vec<u8>>>,
    off: usize,
    len: usize,
}

impl PsString {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let len = bytes.len();
        PsString {
            data: Rc::new(RefCell::new(bytes)),
            off: 0,
            len,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn borrow_bytes(&self) -> Ref<'_, [u8]> {
        Ref::map(self.data.borrow(), |v| &v[self.off..self.off + self.len])
    }

    pub fn borrow_bytes_mut(&self) -> RefMut<'_, [u8]> {
        RefMut::map(self.data.borrow_mut(), |v| {
            &mut v[self.off..self.off + self.len]
        })
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.borrow_bytes().to_vec()
    }

    pub fn byte(&self, i: usize) -> Option<u8> {
        if i < self.len {
            Some(self.data.borrow()[self.off + i])
        } else {
            None
        }
    }

    pub fn put_byte(&self, i: usize, b: u8) -> Result<(), PsError> {
        if i >= self.len {
            return Err(PsError::Rangecheck);
        }
        self.data.borrow_mut()[self.off + i] = b;
        Ok(())
    }

    pub fn interval(&self, off: usize, len: usize) -> Result<PsString, PsError> {
        if off > self.len || len > self.len - off {
            return Err(PsError::Rangecheck);
        }
        Ok(PsString {
            data: self.data.clone(),
            off: self.off + off,
            len,
        })
    }

    /// Copy `bytes` into this view starting at `at`; rangecheck if they
    /// don't fit.
    pub fn write_at(&self, at: usize, bytes: &[u8]) -> Result<(), PsError> {
        if at > self.len || bytes.len() > self.len - at {
            return Err(PsError::Rangecheck);
        }
        self.borrow_bytes_mut()[at..at + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    pub fn same_object(&self, other: &PsString) -> bool {
        Rc::ptr_eq(&self.data, &other.data) && self.off == other.off && self.len == other.len
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.borrow_bytes()).into_owned()
    }
}

/// What a `save` object points at. The record of what to roll back
/// lives in the interpreter's save stack; the handle only carries
/// identity (restore finds its record by `Rc::ptr_eq`) and validity
/// (restoring an outer save invalidates the inner handles — a later
/// restore on one is `invalidrestore`). Design writeup: `docs/dev/VM.md`.
pub struct SaveHandle {
    pub(crate) valid: Cell<bool>,
}

#[derive(Clone)]
pub enum Value {
    /// PLRM specifies 32-bit integers; we use 64-bit as the implementation
    /// limit (documented deviation — more headroom, no downside for
    /// hand-written programs). Arithmetic that overflows still promotes to
    /// a real, matching the spec's *behavior* if not its exact threshold.
    Integer(i64),
    Real(f64),
    Boolean(bool),
    Mark,
    Null,
    Name(PsName),
    String(PsString),
    Array(PsArray),
    Dict(Rc<RefCell<Dict>>),
    Operator(Operator),
    File(FileHandle),
    Save(Rc<SaveHandle>),
    /// A Level 2 `gstate` object (#394): a whole graphics state held by
    /// reference, so `eq` is identity. The clip inside it is the live
    /// chain's `Rc` (mask, paths and rules), so `setgstate` brings a
    /// clip back exactly, nested and `eoclip` links included.
    GState(Rc<RefCell<crate::gfx::GraphicsState>>),
}

#[derive(Clone)]
pub struct Object {
    pub value: Value,
    pub executable: bool,
}

impl Object {
    pub fn lit(value: Value) -> Self {
        Object {
            value,
            executable: false,
        }
    }

    pub fn exec(value: Value) -> Self {
        Object {
            value,
            executable: true,
        }
    }

    pub fn int(i: i64) -> Self {
        Self::lit(Value::Integer(i))
    }

    pub fn real(r: f64) -> Self {
        Self::lit(Value::Real(r))
    }

    pub fn bool(b: bool) -> Self {
        Self::lit(Value::Boolean(b))
    }

    pub fn name(n: &str) -> Self {
        Self::lit(Value::Name(name::intern(n)))
    }

    pub fn array(items: Vec<Object>) -> Self {
        Self::lit(Value::Array(PsArray::from_vec(items)))
    }

    pub fn procedure(items: Vec<Object>) -> Self {
        Self::exec(Value::Array(PsArray::from_vec(items)))
    }

    pub fn string(bytes: Vec<u8>) -> Self {
        Self::lit(Value::String(PsString::from_bytes(bytes)))
    }

    /// `clone`, with the numbers copied inline. The derived `clone` is an
    /// out-of-line call that switches on the variant; the step loop and the
    /// stack operators copy numbers far more often than anything else, so
    /// they skip that call (#863). Same result as `clone` for every value.
    #[inline(always)]
    pub(crate) fn clone_inline(&self) -> Object {
        let value = match self.value {
            Value::Integer(i) => Value::Integer(i),
            Value::Real(r) => Value::Real(r),
            _ => return self.clone(),
        };
        Object {
            value,
            executable: self.executable,
        }
    }

    /// `drop`, with the plain-data case decided inline. The compiler's
    /// drop glue for `Value` is an out-of-line call that switches on the
    /// variant; nearly every operand an operator pops is a number, which
    /// owns nothing, so the stack and arithmetic operators skip that call
    /// for them (#863). Every other value is dropped as usual.
    #[inline(always)]
    pub(crate) fn drop_inline(self) {
        match self.value {
            Value::Integer(_)
            | Value::Real(_)
            | Value::Boolean(_)
            | Value::Mark
            | Value::Null
            | Value::Operator(_) => std::mem::forget(self),
            _ => drop(self),
        }
    }

    /// The number this object holds, if it is one.
    #[inline(always)]
    pub(crate) fn num(&self) -> Option<Num> {
        match self.value {
            Value::Integer(i) => Some(Num::Int(i)),
            Value::Real(r) => Some(Num::Real(r)),
            _ => None,
        }
    }

    /// The `type` operator's name for this object.
    pub fn type_name(&self) -> &'static str {
        match &self.value {
            Value::Integer(_) => "integertype",
            Value::Real(_) => "realtype",
            Value::Boolean(_) => "booleantype",
            Value::Mark => "marktype",
            Value::Null => "nulltype",
            Value::Name(_) => "nametype",
            Value::String(_) => "stringtype",
            Value::Array(_) => "arraytype",
            Value::Dict(_) => "dicttype",
            Value::Operator(_) => "operatortype",
            Value::File(_) => "filetype",
            Value::Save(_) => "savetype",
            Value::GState(_) => "gstatetype",
        }
    }

    /// Text form, as the `=` operator prints it: the "natural" rendering
    /// for scalars and string contents, `--nostringval--` for everything
    /// that has no string value.
    pub fn text(&self) -> String {
        match &self.value {
            Value::Integer(i) => i.to_string(),
            Value::Real(r) => format_real(*r),
            Value::Boolean(b) => b.to_string(),
            Value::Name(n) => n.to_string(),
            Value::String(s) => s.text(),
            Value::Mark
            | Value::Null
            | Value::Array(_)
            | Value::Dict(_)
            | Value::Operator(_)
            | Value::File(_)
            | Value::Save(_)
            | Value::GState(_) => "--nostringval--".to_string(),
        }
    }

    /// `text()` with strings and names cut to `max` bytes (then
    /// ` ... truncated`), for `stack`. Everything else is already short.
    pub fn text_bounded(&self, max: usize) -> String {
        match &self.value {
            Value::String(s) => {
                let bytes = s.borrow_bytes();
                let shown = &bytes[..bytes.len().min(max)];
                let mut text = String::from_utf8_lossy(shown).into_owned();
                if shown.len() < bytes.len() {
                    text.push(' ');
                    text.push_str(TRUNCATED);
                }
                text
            }
            Value::Name(n) => cap_text(n, max),
            _ => self.text(),
        }
    }

    /// Syntactic form, as `==` prints it before its output bound: valid
    /// PostScript source where possible (`/name`, `(string)`, `[array]`,
    /// `{proc}`). Unbounded except for nesting depth (32), so a
    /// pathological `[[[[...` prints `... truncated` instead of
    /// overflowing the Rust stack. The operators print `repr_bounded`.
    pub fn repr(&self) -> String {
        self.repr_bounded(&UNBOUNDED_LIMITS)
    }

    /// `repr()` under `limits`. Output and work are both bounded.
    pub fn repr_bounded(&self, limits: &DumpLimits) -> String {
        let mut out = String::new();
        self.write_repr(0, limits, limits.max_bytes, &mut out);
        out
    }
}

impl Object {
    /// The one place that spells an object in `==` syntax, appended to
    /// `out`; `repr()`, `==`, `pstack` and the error dump all come here, so
    /// they cannot drift. Once `out` holds `cap` bytes no further element
    /// is started, so the output and the work are bounded whatever the
    /// object's size (shared arrays can hold exponentially many leaves).
    /// True if anything was elided.
    fn write_repr(&self, depth: usize, lim: &DumpLimits, cap: usize, out: &mut String) -> bool {
        match &self.value {
            Value::Integer(i) => {
                out.push_str(&i.to_string());
                false
            }
            Value::Real(r) => {
                out.push_str(&format_real(*r));
                false
            }
            Value::Boolean(b) => {
                out.push_str(if *b { "true" } else { "false" });
                false
            }
            Value::Mark => {
                out.push_str("-mark-");
                false
            }
            Value::Null => {
                out.push_str("null");
                false
            }
            Value::Dict(_) => {
                out.push_str("-dict-");
                false
            }
            Value::Operator(op) => {
                out.push_str("--");
                out.push_str(op.name);
                out.push_str("--");
                false
            }
            Value::File(_) => {
                out.push_str("-file-");
                false
            }
            Value::Save(_) => {
                out.push_str("-save-");
                false
            }
            Value::GState(_) => {
                out.push_str("-gstate-");
                false
            }
            Value::Name(n) => {
                if self.executable {
                    push_cut(out, n, lim.max_string)
                } else {
                    push_cut(out, &format!("/{n}"), lim.max_string)
                }
            }
            Value::String(s) => {
                let bytes = s.borrow_bytes();
                let shown = &bytes[..bytes.len().min(lim.max_string)];
                out.push('(');
                out.push_str(&escape_string(shown));
                let cut = bytes.len() > shown.len();
                if cut {
                    out.push(' ');
                    out.push_str(TRUNCATED);
                }
                out.push(')');
                cut
            }
            Value::Array(a) => {
                if depth >= lim.max_depth {
                    out.push_str(TRUNCATED);
                    return true;
                }
                let (open, close) = if self.executable {
                    ("{", "}")
                } else {
                    ("[", "]")
                };
                out.push_str(open);
                let mut cut = false;
                for i in 0..a.len() {
                    if i > 0 {
                        out.push(' ');
                    }
                    if i >= lim.max_elements || out.len() >= cap {
                        out.push_str(TRUNCATED);
                        cut = true;
                        break;
                    }
                    if let Some(item) = a.get(i)
                        && item.write_repr(depth + 1, lim, cap, out)
                    {
                        cut = true;
                        // Budget spent inside the child: stop here, and
                        // mark this level's own elision if siblings remain.
                        if out.len() >= cap {
                            if i + 1 < a.len() {
                                out.push(' ');
                                out.push_str(TRUNCATED);
                            }
                            break;
                        }
                    }
                }
                out.push_str(close);
                cut
            }
        }
    }
}

impl fmt::Debug for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.repr())
    }
}

thread_local! {
    /// Shared placeholder storage for the teardown swap below. Its
    /// refcount is always ≥ 1 (this handle), so it can never be
    /// try_unwrapped and never recurses.
    static EMPTY_STORE: Rc<RefCell<Vec<Object>>> = Rc::new(RefCell::new(Vec::new()));
}

/// Deeply nested arrays (10k levels of `[`) would drop recursively — one
/// Rust stack frame per level — and abort the process. When this view is
/// the last handle to an array's backing store, tear it down iteratively
/// instead, reusing the store's own element vector as the worklist.
///
/// The impl lives on `PsArray`, not `Object`, so every non-array value
/// (the overwhelming majority of drops — every popped operand) keeps
/// the compiler's plain drop glue; profiling fib showed a custom
/// `Drop for Object` taxing ~18% of the whole run (docs/archive/NOTES.md, Stage 11).
impl Drop for PsArray {
    #[inline]
    fn drop(&mut self) {
        if Rc::strong_count(&self.data) != 1 {
            return;
        }
        let data = std::mem::replace(&mut self.data, EMPTY_STORE.with(Rc::clone));
        let Ok(cell) = Rc::try_unwrap(data) else {
            return;
        };
        let mut pending = cell.into_inner();
        while let Some(obj) = pending.pop() {
            // `Object` has no Drop of its own, so the value can be
            // moved out and destructured freely.
            if let Value::Array(mut inner) = obj.value
                && Rc::strong_count(&inner.data) == 1
            {
                let d = std::mem::replace(&mut inner.data, EMPTY_STORE.with(Rc::clone));
                if let Ok(cell) = Rc::try_unwrap(d) {
                    pending.extend(cell.into_inner());
                }
            }
        }
    }
}

/// A deep chain of dictionaries (10000 levels of `<< /n << ... >> >>`) would
/// drop recursively, one Rust stack frame per level, and abort the process,
/// exactly as `PsArray` guards against for arrays. Dict is the container
/// that gets this guard rather than `Object` (HANDOFF: no `Drop for
/// Object`), and it also covers a dict that holds arrays that hold dicts:
/// the worklist below unwraps uniquely held dicts and arrays alike, so the
/// stack depth stays constant however the chain alternates. It runs
/// wherever a dictionary is freed, so `Drop for Interp` needs no list of
/// the places a program could have hung a chain.
impl Drop for Dict {
    fn drop(&mut self) {
        if self.names.is_empty() && self.exotic.is_empty() {
            return;
        }
        let mut pending: Vec<Object> = Vec::new();
        self.drain_nested_into(&mut pending);
        while let Some(obj) = pending.pop() {
            match obj.value {
                Value::Dict(rc) => {
                    if let Ok(cell) = Rc::try_unwrap(rc) {
                        // Emptied here, so its own `drop` returns at once.
                        cell.into_inner().drain_nested_into(&mut pending);
                    }
                }
                Value::Array(mut inner) if Rc::strong_count(&inner.data) == 1 => {
                    let d = std::mem::replace(&mut inner.data, EMPTY_STORE.with(Rc::clone));
                    if let Ok(cell) = Rc::try_unwrap(d) {
                        pending.extend(cell.into_inner().into_iter().filter(is_container));
                    }
                }
                Value::GState(rc) => {
                    // A gstate keeps the current font's dictionary and the
                    // colour space's operand objects alive, so a chain can
                    // run dict -> gstate -> font dict (or colour space
                    // operand) -> gstate.
                    if let Ok(cell) = Rc::try_unwrap(rc) {
                        cell.into_inner().drain_objects_into(&mut pending);
                    }
                }
                _ => {}
            }
        }
    }
}

fn is_container(o: &Object) -> bool {
    matches!(o.value, Value::Dict(_) | Value::Array(_) | Value::GState(_))
}

/// `{:?}` rather than `{}` so whole reals keep their decimal point
/// (`2.0`, not `2`) — the type distinction is meaningful in PostScript.
pub(crate) fn format_real(r: f64) -> String {
    format!("{r:?}")
}

pub(crate) fn escape_string(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        match b {
            b'(' => out.push_str("\\("),
            b')' => out.push_str("\\)"),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x08 => out.push_str("\\b"),
            0x0c => out.push_str("\\f"),
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out
}

/// Bounds for printing an operand or the operand stack (issues #703,
/// #745): the `--pstack-on-error` dump uses `STACK_DUMP_LIMITS`, the
/// `==`, `pstack` and `stack` operators use `OPERATOR_LIMITS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DumpLimits {
    /// Container nesting shown; a deeper container prints `... truncated`.
    pub max_depth: usize,
    /// Elements shown per array; the rest are elided.
    pub max_elements: usize,
    /// Bytes of a string or name shown, cut before escaping.
    pub max_string: usize,
    /// Approximate bytes for the whole value (or stack dump).
    pub max_bytes: usize,
}

/// The limits `--pstack-on-error` uses. Ordinary stacks print unchanged.
pub const STACK_DUMP_LIMITS: DumpLimits = DumpLimits {
    max_depth: 32,
    max_elements: 256,
    max_string: 512,
    max_bytes: 8192,
};

/// What `repr()` uses: no size cap, but the depth cap that keeps a
/// pathologically nested array from overflowing the Rust stack.
pub(crate) const UNBOUNDED_LIMITS: DumpLimits = DumpLimits {
    max_depth: 32,
    max_elements: usize::MAX,
    max_string: usize::MAX,
    max_bytes: usize::MAX,
};

/// What `==`, `pstack` and `stack` print per operand: generous (a
/// hundreds-of-elements debugging array or a page of text prints in
/// full), but a million-element array or a megabyte string prints a few
/// KB ending in `... truncated`, not 4 MiB of output.
pub const OPERATOR_LIMITS: DumpLimits = DumpLimits {
    max_depth: 32,
    max_elements: 1024,
    max_string: 4096,
    max_bytes: 16 * 1024,
};

/// Total bytes `pstack` and `stack` print across the whole stack (top
/// first); items below the budget are elided with one `... truncated` line.
/// Half of `STDOUT_CAPTURE_LIMIT`, so a captured run normally keeps the whole dump.
pub(crate) const OPERATOR_STACK_BYTES: usize = 32 * 1024;

/// Bytes of an offending name an error line prints.
pub(crate) const ERROR_NAME_LIMIT: usize = 512;

/// The elision marker every bounded print uses.
pub(crate) const TRUNCATED: &str = "... truncated";

/// The operand stack in `==` syntax, bottom to top, under `limits`.
/// Elements are taken from the top down, so the newest operands survive a
/// cut; when bottom elements are elided the text starts with `... truncated`.
/// Empty for an empty stack. Under the limits this equals `repr()` per element.
pub fn stack_repr_bounded(stack: &[Object], limits: &DumpLimits) -> String {
    let mut kept: Vec<String> = Vec::new();
    let mut used = 0;
    for obj in stack.iter().rev() {
        if used >= limits.max_bytes {
            break;
        }
        let mut text = String::new();
        obj.write_repr(0, limits, limits.max_bytes - used, &mut text);
        used += text.len() + 1;
        kept.push(text);
    }
    let mut parts: Vec<String> = Vec::new();
    if kept.len() < stack.len() {
        parts.push(TRUNCATED.to_string());
    }
    parts.extend(kept.into_iter().rev());
    parts.join(" ")
}

/// `text` cut at a char boundary to `max` bytes, then ` ... truncated`.
/// The marker has no `; ` or ` ]%%`, so an error line stays parseable.
pub(crate) fn cap_text(text: &str, max: usize) -> String {
    let mut out = String::new();
    push_cut(&mut out, text, max);
    out
}

/// Push `text`, cut at a char boundary to `max` bytes; true if cut.
fn push_cut(out: &mut String, text: &str, max: usize) -> bool {
    if text.len() <= max {
        out.push_str(text);
        return false;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    out.push_str(&text[..end]);
    out.push(' ');
    out.push_str(TRUNCATED);
    true
}

/// A numeric operand: most arithmetic operators accept either integer or
/// real and promote as needed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Num {
    Int(i64),
    Real(f64),
}

impl Num {
    pub fn to_f64(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Real(r) => r,
        }
    }
}

/// Non-name dictionary keys. Names (and strings, which convert to names
/// per the PLRM) stay in a dedicated name-keyed map — that's the hot path
/// for every operator lookup — while numbers, booleans, marks, and
/// composites-by-identity land here.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ExoticKey {
    Int(i64),
    /// Reals with no exact integer value, keyed by bit pattern. Reals
    /// that *are* whole numbers normalize to `Int` so `3` and `3.0` are
    /// the same key, as the PLRM requires.
    RealBits(u64),
    Bool(bool),
    Mark,
    /// Arrays/strings-as-array-likes by identity: (storage ptr, off, len).
    ArrayId(usize, usize, usize),
    DictId(usize),
    OperatorId(usize),
    FileId(usize),
    SaveId(usize),
    GStateId(usize),
}

enum KeyClass {
    Name(PsName),
    Exotic(ExoticKey),
}

fn classify_key(key: &Object) -> Result<KeyClass, PsError> {
    Ok(match &key.value {
        Value::Name(n) => KeyClass::Name(n.clone()),
        // Strings convert to names when used as keys, per the PLRM.
        Value::String(s) => KeyClass::Name(name::intern(&s.text())),
        Value::Integer(i) => KeyClass::Exotic(ExoticKey::Int(*i)),
        Value::Real(r) => {
            if r.fract() == 0.0 && (i64::MIN as f64..=i64::MAX as f64).contains(r) {
                KeyClass::Exotic(ExoticKey::Int(*r as i64))
            } else {
                KeyClass::Exotic(ExoticKey::RealBits(r.to_bits()))
            }
        }
        Value::Boolean(b) => KeyClass::Exotic(ExoticKey::Bool(*b)),
        Value::Mark => KeyClass::Exotic(ExoticKey::Mark),
        Value::Array(a) => {
            let (p, o, l) = a.identity();
            KeyClass::Exotic(ExoticKey::ArrayId(p, o, l))
        }
        Value::Dict(d) => KeyClass::Exotic(ExoticKey::DictId(Rc::as_ptr(d) as usize)),
        Value::Operator(op) => KeyClass::Exotic(ExoticKey::OperatorId(op.func as usize)),
        Value::File(f) => KeyClass::Exotic(ExoticKey::FileId(Rc::as_ptr(f) as usize)),
        Value::Save(h) => KeyClass::Exotic(ExoticKey::SaveId(Rc::as_ptr(h) as usize)),
        Value::GState(g) => KeyClass::Exotic(ExoticKey::GStateId(Rc::as_ptr(g) as usize)),
        // The one type the PLRM forbids as a key.
        Value::Null => return Err(PsError::Typecheck),
    })
}

/// A PostScript dictionary. `Clone` is shallow (values are `Rc`
/// handles) — it's what the save/restore journal snapshots.
#[derive(Default, Clone)]
pub struct Dict {
    /// Name-keyed entries, keyed by interned id — the hot path. One
    /// u32 multiply per probe instead of hashing the name's text.
    names: HashMap<u32, Object, IdHashBuilder>,
    /// Exotic entries keep the original key object so `forall` and
    /// `copy` can hand it back, plus the sequence number of the key's
    /// first insertion. The map itself has a randomly seeded hasher (and
    /// pointer-keyed entries move with ASLR), so `pairs` re-sorts by
    /// that sequence number to give `forall` a run-to-run stable order.
    exotic: HashMap<ExoticKey, ExoticEntry>,
    /// Next insertion sequence number for `exotic`.
    next_seq: u64,
}

/// One non-name dictionary entry: insertion sequence, key object, value.
#[derive(Clone)]
struct ExoticEntry {
    seq: u64,
    key: Object,
    value: Object,
}

impl Dict {
    pub fn new() -> Self {
        Self::default()
    }

    /// String-keyed convenience lookup. Uses the interner's read-only
    /// probe so misses don't grow the name table.
    #[inline]
    pub fn get(&self, key: &str) -> Option<Object> {
        self.names.get(&name::lookup(key)?).cloned()
    }

    /// Interned fast path (name execution, `load`).
    pub fn get_name(&self, key: &PsName) -> Option<Object> {
        self.names.get(&key.id()).cloned()
    }

    /// [`get_id`](Self::get_id) by reference, for a caller that only
    /// needs to look (name execution, #863).
    #[inline(always)]
    pub(crate) fn get_id_ref(&self, id: u32) -> Option<&Object> {
        self.names.get(&id)
    }

    #[inline]
    pub(crate) fn get_id(&self, id: u32) -> Option<Object> {
        self.names.get(&id).cloned()
    }

    pub fn put(&mut self, key: Rc<str>, value: Object) {
        self.names.insert(name::intern_rc(key).id(), value);
    }

    pub fn put_name(&mut self, key: &PsName, value: Object) {
        self.names.insert(key.id(), value);
    }

    /// General lookup with any PostScript key object.
    pub fn get_obj(&self, key: &Object) -> Result<Option<Object>, PsError> {
        Ok(match classify_key(key)? {
            KeyClass::Name(n) => self.names.get(&n.id()).cloned(),
            KeyClass::Exotic(k) => self.exotic.get(&k).map(|e| e.value.clone()),
        })
    }

    pub fn put_obj(&mut self, key: &Object, value: Object) -> Result<(), PsError> {
        if let Value::Name(n) = &key.value {
            self.names.insert(n.id(), value);
            return Ok(());
        }
        self.put_other(key, value)
    }

    /// The non-name half of `put_obj`, kept out of line so the name-key
    /// `def` hot path stays small.
    #[inline(never)]
    fn put_other(&mut self, key: &Object, value: Object) -> Result<(), PsError> {
        match classify_key(key)? {
            KeyClass::Name(n) => {
                self.names.insert(n.id(), value);
            }
            KeyClass::Exotic(k) => {
                // Redefining a key keeps its original slot in `forall`
                // order; `undef` then `put` moves it to the end.
                let seq = self.next_seq;
                match self.exotic.entry(k) {
                    std::collections::hash_map::Entry::Occupied(mut o) => {
                        let e = o.get_mut();
                        e.key = key.clone();
                        e.value = value;
                    }
                    std::collections::hash_map::Entry::Vacant(v) => {
                        v.insert(ExoticEntry {
                            seq,
                            key: key.clone(),
                            value,
                        });
                        self.next_seq += 1;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn known(&self, key: &Object) -> Result<bool, PsError> {
        Ok(match classify_key(key)? {
            KeyClass::Name(n) => self.names.contains_key(&n.id()),
            KeyClass::Exotic(k) => self.exotic.contains_key(&k),
        })
    }

    /// Empty the dictionary, moving every key and value that can hold more
    /// objects (a dict or an array) into `out`; the rest drop here.
    fn drain_nested_into(&mut self, out: &mut Vec<Object>) {
        out.extend(self.names.drain().map(|(_, v)| v).filter(is_container));
        for (_, e) in self.exotic.drain() {
            if is_container(&e.key) {
                out.push(e.key);
            }
            if is_container(&e.value) {
                out.push(e.value);
            }
        }
    }

    pub fn undef(&mut self, key: &Object) -> Result<(), PsError> {
        match classify_key(key)? {
            KeyClass::Name(n) => {
                self.names.remove(&n.id());
            }
            KeyClass::Exotic(k) => {
                self.exotic.remove(&k);
            }
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.names.len() + self.exotic.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.exotic.is_empty()
    }

    /// Snapshot of (key, value) pairs for `forall`/`copy`. Name keys come
    /// back as literal name objects. The PLRM leaves the order
    /// unspecified, but it is stable from run to run: name keys first, in
    /// the fixed-hasher order of their interned ids (deterministic for a
    /// given program, not insertion order -- name lookups are the hot
    /// path and carry no sequence number), then every other key in
    /// insertion order.
    pub fn pairs(&self) -> Vec<(Object, Object)> {
        let mut out: Vec<(Object, Object)> = self
            .names
            .iter()
            .map(|(k, v)| (Object::lit(Value::Name(name::resolve(*k))), v.clone()))
            .collect();
        if self.exotic.len() < 2 {
            out.extend(
                self.exotic
                    .values()
                    .map(|e| (e.key.clone(), e.value.clone())),
            );
        } else {
            let mut ex: Vec<&ExoticEntry> = self.exotic.values().collect();
            ex.sort_unstable_by_key(|e| e.seq);
            out.extend(ex.into_iter().map(|e| (e.key.clone(), e.value.clone())));
        }
        out
    }
}
