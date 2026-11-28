//! Parameter, cache, job-control and device-parameter operators (#1270):
//! `setuserparams`, `currentuserparams`, `setsystemparams`,
//! `currentsystemparams`, `setdevparams`, `currentdevparams`,
//! `cachestatus`, `setcachelimit`, `setcacheparams`, `currentcacheparams`,
//! `setucacheparams`, `ucachestatus`, `setvmthreshold`, `vmreclaim`,
//! `startjob` and `exitserver` (which lives in `serverdict`).
//!
//! The behaviour is Ghostscript 10.07.1's, measured run for run in
//! `tests/params.rs` (`tests/data/params_cases.txt`). In gs most of these are
//! PostScript procedures over a few C parameters (`gs_lev2.ps`,
//! `gs_dps2.ps`), and the shape is kept here: the font-cache operators are
//! views onto parameters (`setcachelimit` is `MaxFontItem`, the cache size is
//! the system parameter `MaxFontCache`, the unencapsulated path cache is
//! `MaxUPathItem`), `vmreclaim` and `setvmthreshold` set `VMReclaim` and
//! `VMThreshold`.
//!
//! What is stored is only what pscat can honestly say: a table of the PLRM
//! parameters (those gs also has), each with the type and range check gs
//! applies. A parameter that changes nothing in pscat (a cache size) is stored
//! and read back. A parameter that would change a limit or reach outside the
//! sandbox (`MaxOpStack`, `MaxLocalVM`, the file-permission arrays, a resource
//! directory, a password) is refused with `invalidaccess` unless it is set to
//! the value it already has. `notes/1270-parameter-operators.md` lists every
//! difference from gs.
//!
//! User parameters follow `save`/`restore` (as in gs, where the dictionary is
//! local VM); system parameters do not.

use std::cell::RefCell;
use std::rc::Rc;

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "setuserparams", setuserparams);
    op(dict, "currentuserparams", currentuserparams);
    op(dict, "setsystemparams", setsystemparams);
    op(dict, "currentsystemparams", currentsystemparams);
    op(dict, "setdevparams", setdevparams);
    op(dict, "currentdevparams", currentdevparams);
    op(dict, "cachestatus", cachestatus);
    op(dict, "setcachelimit", setcachelimit);
    op(dict, "setcacheparams", setcacheparams);
    op(dict, "currentcacheparams", currentcacheparams);
    op(dict, "ucachestatus", ucachestatus);
    op(dict, "setucacheparams", setucacheparams);
    op(dict, "setvmthreshold", setvmthreshold);
    op(dict, "vmreclaim", vmreclaim);
    op(dict, "startjob", startjob);
}

// --- the parameter tables ----------------------------------------------------

/// A parameter's value. Strings are shared bytes: a program reading one gets a
/// fresh string made from them, so nothing it does to that string can reach
/// the stored value (pscat has no read-only strings).
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum PVal {
    Int(i64),
    Bool(bool),
    Str(Rc<[u8]>),
}

/// How a parameter is checked when set.
#[derive(Clone, Copy)]
enum Kind {
    Bool,
    /// Any integer.
    Int,
    /// 0 up to 2^32 - 1, else `rangecheck`.
    UInt32,
    /// As `UInt32`, but gs reads back 0 (the timeouts it does not implement),
    /// as pscat has no timeouts either.
    ZeroUInt32,
    /// -2^31 up to 2^32 - 1, else `rangecheck`; gs keeps the value as a
    /// signed 32-bit integer clamped at 0, so 2^31 and over read back 0.
    Wrap32,
    /// -1 is the default (8000000), 0 is stored as 1, below -1 `rangecheck`.
    VmThreshold,
    /// -2 to 0, else `rangecheck`.
    VmReclaim,
    /// 0 up to 2^32 - 1, else `rangecheck`; stored clamped to 100000 to
    /// 100000000.
    FontCache,
    /// A string (`typecheck` otherwise, `invalidaccess` if `noaccess`).
    Str,
    /// An integer the sandbox does not let a program change: it is accepted
    /// only equal to what it already is, else `invalidaccess`.
    FixedInt,
    /// The same for a string (a resource directory).
    FixedStr,
    /// gs ignores a write, whatever its type.
    ReadOnly,
}

#[derive(Clone, Copy)]
enum Init {
    Int(i64),
    Bool(bool),
    Str(&'static str),
}

struct Spec {
    name: &'static str,
    kind: Kind,
    init: Init,
}

const fn spec(name: &'static str, kind: Kind, init: Init) -> Spec {
    Spec { name, kind, init }
}

use Init::{Bool as B, Int as I, Str as S};
use Kind as K;

/// The PLRM user parameters gs also has, in name order (Appendix C of the PLRM
/// and gs's `currentuserparams`). The gs-only ones (ICC profile names,
/// `GridFitTT`, `AlignToPixels`, `RenderTTNotdef`, the comment hooks) are not
/// modelled: setting them is ignored, as gs ignores any unknown key.
const USER: &[Spec] = &[
    spec("AccurateScreens", K::Bool, B(false)),
    spec("HalftoneMode", K::Int, I(0)),
    spec("IdiomRecognition", K::Bool, B(true)),
    spec("JobName", K::Str, S("")),
    spec("JobTimeout", K::ZeroUInt32, I(0)),
    // Limits pscat enforces its own way (`--max-memory`, the 10000-frame
    // execution stack); their values read back, and only those values set.
    spec("MaxDictStack", K::FixedInt, I(500)),
    spec(
        "MaxExecStack",
        K::FixedInt,
        I(crate::interp::EXEC_STACK_LIMIT as i64),
    ),
    spec("MaxFontItem", K::Wrap32, I(10000)),
    spec("MaxFormItem", K::Int, I(100000)),
    spec("MaxLocalVM", K::FixedInt, I(i64::MAX)),
    spec("MaxOpStack", K::FixedInt, I(300000)),
    spec("MaxPatternItem", K::Int, I(20000)),
    spec("MaxScreenItem", K::Int, I(48000)),
    spec("MaxSuperScreen", K::Int, I(1016)),
    spec("MaxUPathItem", K::Int, I(0)),
    spec("MinFontCompress", K::Wrap32, I(1000)),
    spec("MinScreenLevels", K::UInt32, I(1)),
    spec("VMReclaim", K::VmReclaim, I(0)),
    spec("VMThreshold", K::VmThreshold, I(8_000_000)),
    spec("WaitTimeout", K::ZeroUInt32, I(0)),
];

/// User parameters whose being set at all would reach outside the sandbox:
/// what files a program may read, write or rename. gs holds them in
/// `currentuserparams` (`[(*)]`, wide open); pscat does not report them.
const USER_REFUSED: &[&str] = &[
    "PermitFileReading",
    "PermitFileWriting",
    "PermitFileControl",
    "LockFilePermissions",
];

/// The PLRM system parameters gs also has. The two resource directories and
/// the path separator are fixed fictions (gs reports host paths; pscat has no
/// resource directory a program may redirect).
const SYSTEM: &[Spec] = &[
    spec("BuildTime", K::ReadOnly, I(0)),
    spec("ByteOrder", K::ReadOnly, B(true)),
    spec("CurDisplayList", K::Int, I(0)),
    spec("CurFontCache", K::ReadOnly, I(0)),
    spec("CurFormCache", K::Int, I(0)),
    spec("CurInputDevice", K::Str, S("")),
    spec("CurOutlineCache", K::Int, I(0)),
    spec("CurOutputDevice", K::Str, S("")),
    spec("CurPatternCache", K::Int, I(0)),
    spec("CurScreenStorage", K::Int, I(0)),
    spec("CurSourceList", K::Int, I(0)),
    spec("CurUPathCache", K::Int, I(0)),
    spec("DoPrintErrors", K::Bool, B(false)),
    spec("FontResourceDir", K::FixedStr, S("/Resource/Font/")),
    spec("GenericResourceDir", K::FixedStr, S("/Resource/")),
    spec("GenericResourcePathSep", K::FixedStr, S("/")),
    spec("JobTimeout", K::Int, I(0)),
    spec("LicenseID", K::Str, S("LN-001")),
    spec("MaxDisplayAndSourceList", K::Int, I(160000)),
    spec("MaxDisplayList", K::Int, I(140000)),
    spec("MaxFontCache", K::FontCache, I(1_000_000)),
    spec("MaxFormCache", K::Int, I(100000)),
    spec("MaxGlobalVM", K::FixedInt, I(i64::MAX)),
    spec("MaxImageBuffer", K::Int, I(524288)),
    spec("MaxOutlineCache", K::Int, I(65000)),
    spec("MaxPatternCache", K::Int, I(100000)),
    spec("MaxScreenStorage", K::Int, I(84000)),
    spec("MaxSourceList", K::Int, I(25000)),
    spec("MaxUPathCache", K::Int, I(300000)),
    spec("PageCount", K::ReadOnly, I(0)),
    spec("PrinterName", K::Str, S("pscat")),
    spec("RamSize", K::Int, I(4_194_304)),
    spec("RealFormat", K::ReadOnly, S("IEEE")),
    spec("Revision", K::ReadOnly, I(super::level2::REVISION)),
    spec("WaitTimeout", K::Int, I(40)),
];

/// System parameters that carry a password: pscat has none, so a program that
/// sends one is refused (gs, started without `-dSAFER`, takes any).
const SYSTEM_REFUSED: &[&str] = &["Password", "StartJobPassword", "SystemParamsPassword"];

const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn index_of(table: &[Spec], name: &str) -> usize {
    let mut i = 0;
    while i < table.len() {
        if same(table[i].name, name) {
            return i;
        }
        i += 1;
    }
    panic!("no such parameter in the table");
}

const U_MAX_FONT_ITEM: usize = index_of(USER, "MaxFontItem");
const U_MIN_FONT_COMPRESS: usize = index_of(USER, "MinFontCompress");
const U_MAX_UPATH_ITEM: usize = index_of(USER, "MaxUPathItem");
const U_VM_RECLAIM: usize = index_of(USER, "VMReclaim");
const U_VM_THRESHOLD: usize = index_of(USER, "VMThreshold");
const S_MAX_FONT_CACHE: usize = index_of(SYSTEM, "MaxFontCache");

fn initial(table: &[Spec]) -> Vec<PVal> {
    table
        .iter()
        .map(|s| match s.init {
            Init::Int(i) => PVal::Int(i),
            Init::Bool(b) => PVal::Bool(b),
            Init::Str(t) => PVal::Str(t.as_bytes().into()),
        })
        .collect()
}

/// The parameters of one interpreter. The user ones are shared with the
/// `save`s that snapshot them and copied on the first write after (`Rc`), so a
/// `save` costs one pointer; `restore` puts the saved set back.
pub(crate) struct ParamState {
    pub(crate) user: Rc<Vec<PVal>>,
    system: Vec<PVal>,
}

impl Default for ParamState {
    fn default() -> Self {
        ParamState {
            user: Rc::new(initial(USER)),
            system: initial(SYSTEM),
        }
    }
}

impl ParamState {
    fn user_int(&self, i: usize) -> i64 {
        match self.user[i] {
            PVal::Int(v) => v,
            _ => unreachable!("an integer parameter"),
        }
    }

    fn set_user(&mut self, i: usize, v: PVal) {
        Rc::make_mut(&mut self.user)[i] = v;
    }
}

// --- checking and applying a parameter dictionary ----------------------------

/// Work charged (steps) for walking a table of this many parameters: one
/// step per eight lookups, rounded up, on top of the operator's own step.
fn table_steps(n: usize) -> u64 {
    (n / 8 + 1) as u64
}

/// A value of `kind` from `obj`: the value to store (`None` when gs ignores
/// it) or the error gs gives. `current` is what is stored now, for the kinds
/// that only accept it unchanged.
fn check_value(kind: Kind, obj: &Object, current: &PVal) -> Result<Option<PVal>, PsError> {
    let int = |o: &Object| match o.value {
        Value::Integer(i) => Ok(i),
        _ => Err(PsError::Typecheck),
    };
    let in_u32 = |v: i64| u32::try_from(v).map_err(|_| PsError::Rangecheck);
    Ok(Some(match kind {
        Kind::ReadOnly => return Ok(None),
        Kind::Bool => match obj.value {
            Value::Boolean(b) => PVal::Bool(b),
            _ => return Err(PsError::Typecheck),
        },
        Kind::Int => PVal::Int(int(obj)?),
        Kind::UInt32 => PVal::Int(i64::from(in_u32(int(obj)?)?)),
        Kind::ZeroUInt32 => {
            in_u32(int(obj)?)?;
            PVal::Int(0)
        }
        Kind::Wrap32 => PVal::Int(wrap32(int(obj)?)?),
        Kind::VmThreshold => PVal::Int(match int(obj)? {
            -1 => 8_000_000,
            0 => 1,
            v if v > 0 => v,
            _ => return Err(PsError::Rangecheck),
        }),
        Kind::VmReclaim => match int(obj)? {
            v @ -2..=0 => PVal::Int(v),
            _ => return Err(PsError::Rangecheck),
        },
        Kind::FontCache => {
            PVal::Int(i64::from(in_u32(int(obj)?)?).clamp(MIN_FONT_CACHE, MAX_FONT_CACHE))
        }
        Kind::Str | Kind::FixedStr => {
            let Value::String(s) = &obj.value else {
                return Err(PsError::Typecheck);
            };
            if obj.noaccess {
                return Err(PsError::InvalidAccess);
            }
            if matches!(kind, Kind::FixedStr) {
                // Compared in place: a refusal makes no copy.
                match current {
                    PVal::Str(c) if **c == *s.borrow_bytes() => return Ok(None),
                    _ => return Err(PsError::InvalidAccess),
                }
            }
            // One copy, of the size `check_dict` has already asked for.
            PVal::Str(Rc::from(&*s.borrow_bytes()))
        }
        Kind::FixedInt => {
            let v = PVal::Int(int(obj)?);
            if &v != current {
                return Err(PsError::InvalidAccess);
            }
            return Ok(None);
        }
    }))
}

/// The cache size (`MaxFontCache`) is kept between these, as gs keeps it.
const MIN_FONT_CACHE: i64 = 100_000;
const MAX_FONT_CACHE: i64 = 100_000_000;

/// gs keeps `MaxFontItem` and `MinFontCompress` as a signed 32-bit integer
/// clamped at 0: -2^31 to -1 read back 0, 2^31 to 2^32 - 1 wrap negative and
/// read back 0, anything outside -2^31 to 2^32 - 1 is `rangecheck`.
fn wrap32(v: i64) -> Result<i64, PsError> {
    if !(i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(&v) {
        return Err(PsError::Rangecheck);
    }
    Ok(i64::from(v as i32).max(0))
}

/// The dictionary operand of `set*params`, popped (and so put back by the
/// operand ledger on any error): a non-dictionary is `typecheck`.
fn pop_dict(it: &mut Interp) -> Result<Rc<RefCell<Dict>>, PsError> {
    match it.pop()?.value {
        Value::Dict(d) => Ok(d),
        _ => Err(PsError::Typecheck),
    }
}

/// Check every parameter of `table` that `d` mentions, all before any is
/// applied (gs applies a parameter dictionary all or nothing), and return
/// what to store, as `(index, value)`. A key in `refused` is `invalidaccess`
/// first, as gs's own SAFER mode checks its refusals first. Reads only the
/// table's own keys from `d`, never walks it, so its cost does not depend on
/// how big the program's dictionary is. Strings that will be stored are
/// charged to the memory limit before they are copied.
fn check_dict(
    it: &mut Interp,
    d: &Rc<RefCell<Dict>>,
    table: &[Spec],
    refused: &[&str],
    current: &[PVal],
) -> Result<Vec<(usize, PVal)>, PsError> {
    it.poll_limits(table_steps(table.len() + refused.len()))?;
    let d = d.borrow();
    if refused.iter().any(|k| d.get(k).is_some()) {
        return Err(PsError::InvalidAccess);
    }
    let mut out = Vec::new();
    let mut bytes = 0usize;
    for (i, s) in table.iter().enumerate() {
        let Some(obj) = d.get(s.name) else { continue };
        // The size of a string is known before it is copied: refuse then.
        if let Value::String(st) = &obj.value
            && matches!(s.kind, Kind::Str | Kind::FixedStr)
        {
            bytes = bytes.saturating_add(st.len());
            it.check_alloc(bytes)?;
        }
        if let Some(v) = check_value(s.kind, &obj, &current[i])? {
            out.push((i, v));
        }
    }
    it.poll_limits((bytes / 1024) as u64)?;
    Ok(out)
}

/// `dict setuserparams -`.
fn setuserparams(it: &mut Interp) -> Result<(), PsError> {
    let d = pop_dict(it)?;
    let current = it.params.user.clone();
    let changes = check_dict(it, &d, USER, USER_REFUSED, &current)?;
    drop(current);
    for (i, v) in changes {
        it.params.set_user(i, v);
    }
    Ok(())
}

/// `dict setsystemparams -`.
fn setsystemparams(it: &mut Interp) -> Result<(), PsError> {
    let d = pop_dict(it)?;
    let current = it.params.system.clone();
    let changes = check_dict(it, &d, SYSTEM, SYSTEM_REFUSED, &current)?;
    for (i, v) in changes {
        it.params.system[i] = v;
    }
    Ok(())
}

/// A new dictionary holding `table`'s parameters with `vals`. Every string is
/// a fresh copy; the dictionary and the strings are charged to the memory and
/// step limits before anything is built.
fn param_dict(it: &mut Interp, table: &[Spec], vals: &[PVal]) -> Result<Object, PsError> {
    let text: usize = vals
        .iter()
        .map(|v| match v {
            PVal::Str(s) => s.len(),
            _ => 0,
        })
        .sum();
    let entry = 2 * (std::mem::size_of::<Object>() + 16);
    it.check_alloc(table.len().saturating_mul(entry).saturating_add(text))?;
    it.poll_limits(table_steps(table.len()) + (text / 1024) as u64)?;
    let mut dict = Dict::new();
    for (s, v) in table.iter().zip(vals) {
        dict.put(s.name.into(), to_object(v));
    }
    Ok(Object::lit(Value::Dict(Rc::new(RefCell::new(dict)))))
}

fn to_object(v: &PVal) -> Object {
    match v {
        PVal::Int(i) => Object::int(*i),
        PVal::Bool(b) => Object::bool(*b),
        PVal::Str(s) => Object::string(s.to_vec()),
    }
}

/// `- currentuserparams dict`.
fn currentuserparams(it: &mut Interp) -> Result<(), PsError> {
    let vals = it.params.user.clone();
    let d = param_dict(it, USER, &vals)?;
    it.push(d);
    Ok(())
}

/// `- currentsystemparams dict`.
fn currentsystemparams(it: &mut Interp) -> Result<(), PsError> {
    let vals = it.params.system.clone();
    let d = param_dict(it, SYSTEM, &vals)?;
    it.push(d);
    Ok(())
}

// --- the font cache ----------------------------------------------------------

/// What `cachestatus` reports of the glyph cache pscat does not have: nothing
/// is cached (`bsize` 0), the other two pairs are gs's fixed limits.
const CACHE_MMAX: i64 = 200;
const CACHE_CMAX: i64 = 5000;

/// Room for `n` more operands, before the first is pushed: what the memory
/// limit would stop a runaway push loop at, asked once for a fixed few.
fn push_room(it: &Interp, n: usize) -> Result<(), PsError> {
    it.check_alloc(n.saturating_mul(std::mem::size_of::<Object>()))
}

/// `- cachestatus bsize bmax msize mmax csize cmax blimit`.
fn cachestatus(it: &mut Interp) -> Result<(), PsError> {
    push_room(it, 7)?;
    let PVal::Int(bmax) = it.params.system[S_MAX_FONT_CACHE] else {
        unreachable!("MaxFontCache is an integer")
    };
    let blimit = it.params.user_int(U_MAX_FONT_ITEM);
    for v in [0, bmax, 0, CACHE_MMAX, 0, CACHE_CMAX, blimit] {
        it.push(Object::int(v));
    }
    Ok(())
}

/// `int setcachelimit -`: the upper limit, which gs keeps as `MaxFontItem`.
fn setcachelimit(it: &mut Interp) -> Result<(), PsError> {
    let Value::Integer(v) = it.pop()?.value else {
        return Err(PsError::Typecheck);
    };
    let v = wrap32(v)?;
    it.params.set_user(U_MAX_FONT_ITEM, PVal::Int(v));
    Ok(())
}

/// `- currentcacheparams mark size lower upper`.
fn currentcacheparams(it: &mut Interp) -> Result<(), PsError> {
    push_room(it, 4)?;
    let PVal::Int(size) = it.params.system[S_MAX_FONT_CACHE] else {
        unreachable!("MaxFontCache is an integer")
    };
    it.push(Object::lit(Value::Mark));
    it.push(Object::int(size));
    it.push(Object::int(it.params.user_int(U_MIN_FONT_COMPRESS)));
    it.push(Object::int(it.params.user_int(U_MAX_FONT_ITEM)));
    Ok(())
}

/// Distance from the top of the operand stack down to the topmost mark, the
/// number of operands above it, charged to the step and time limits for the
/// walk. `unmatchedmark` with no mark; the stack is not touched.
fn operands_to_mark(it: &mut Interp) -> Result<usize, PsError> {
    let d = it
        .ostack
        .iter()
        .rev()
        .position(|o| matches!(o.value, Value::Mark))
        .ok_or(PsError::UnmatchedMark)?;
    it.poll_limits((d / 64 + 1) as u64)?;
    Ok(d)
}

/// Remove the `d` operands above the mark and the mark. Nothing here can fail,
/// and the stack was never popped through the ledger, so a refusal before it
/// leaves every operand where it was.
fn drop_to_mark(it: &mut Interp, d: usize) {
    let keep = it.ostack.len() - d - 1;
    it.ostack.truncate(keep);
}

/// `mark size lower upper setcacheparams -`. Only the last three operands
/// count (fewer fill upper, then lower), the rest are dropped with the mark;
/// they are looked at top first, a `typecheck` or `rangecheck` on the first
/// bad one, with the mark and every operand left. gs: the size is kept
/// between 100000 and 100000000, the other two as `MinFontCompress` and
/// `MaxFontItem`.
fn setcacheparams(it: &mut Interp) -> Result<(), PsError> {
    let d = operands_to_mark(it)?;
    let top = it.ostack.len();
    let mut got = [None; 3]; // upper, lower, size
    for (slot, depth) in got.iter_mut().zip(1..=d.min(3)) {
        let Value::Integer(v) = it.ostack[top - depth].value else {
            return Err(PsError::Typecheck);
        };
        if !(0..=i64::from(u32::MAX)).contains(&v) {
            return Err(PsError::Rangecheck);
        }
        *slot = Some(v);
    }
    let [upper, lower, size] = got;
    // In range, so the 32-bit wrap cannot refuse them; all are worked out
    // before anything is set.
    let lower = lower.map(wrap32).transpose()?;
    let upper = upper.map(wrap32).transpose()?;
    if let Some(v) = size {
        it.params.system[S_MAX_FONT_CACHE] = PVal::Int(v.clamp(MIN_FONT_CACHE, MAX_FONT_CACHE));
    }
    if let Some(v) = lower {
        it.params.set_user(U_MIN_FONT_COMPRESS, PVal::Int(v));
    }
    if let Some(v) = upper {
        it.params.set_user(U_MAX_FONT_ITEM, PVal::Int(v));
    }
    drop_to_mark(it, d);
    Ok(())
}

/// `- ucachestatus mark bsize bmax rsize rmax blimit`: nothing is cached, and
/// the limit is `MaxUPathItem`.
fn ucachestatus(it: &mut Interp) -> Result<(), PsError> {
    push_room(it, 6)?;
    it.push(Object::lit(Value::Mark));
    for _ in 0..4 {
        it.push(Object::int(0));
    }
    it.push(Object::int(it.params.user_int(U_MAX_UPATH_ITEM)));
    Ok(())
}

/// `mark ... blimit setucacheparams -`. gs only ever raises the limit
/// (`max` of the old and the new), and takes the topmost operand as it.
/// `rangecheck` with nothing above the mark, `typecheck` for a non-integer.
fn setucacheparams(it: &mut Interp) -> Result<(), PsError> {
    let d = operands_to_mark(it)?;
    if d == 0 {
        return Err(PsError::Rangecheck);
    }
    let Some(Value::Integer(v)) = it.ostack.last().map(|o| &o.value) else {
        return Err(PsError::Typecheck);
    };
    let raised = (*v).max(it.params.user_int(U_MAX_UPATH_ITEM));
    it.params.set_user(U_MAX_UPATH_ITEM, PVal::Int(raised));
    drop_to_mark(it, d);
    Ok(())
}

// --- VM parameters -----------------------------------------------------------

/// `int setvmthreshold -`.
fn setvmthreshold(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Some(v) = check_value(
        Kind::VmThreshold,
        &obj,
        &PVal::Int(it.params.user_int(U_VM_THRESHOLD)),
    )?
    else {
        return Ok(());
    };
    it.params.set_user(U_VM_THRESHOLD, v);
    Ok(())
}

/// `int vmreclaim -`. 1 and 2 are ignored, as gs ignores them (a collection
/// is not something a program asks pscat for), anything over 2 is
/// `rangecheck`; 0, -1 and -2 set `VMReclaim`, which `restore` rolls back.
fn vmreclaim(it: &mut Interp) -> Result<(), PsError> {
    let Value::Integer(v) = it.pop()?.value else {
        return Err(PsError::Typecheck);
    };
    match v {
        1 | 2 => Ok(()),
        v if v > 2 => Err(PsError::Rangecheck),
        v @ -2..=0 => {
            it.params.set_user(U_VM_RECLAIM, PVal::Int(v));
            Ok(())
        }
        _ => Err(PsError::Rangecheck),
    }
}

// --- job control -------------------------------------------------------------

/// `exit_bool password startjob bool`. pscat runs one program and has no job
/// server and no password that opens one, so it is always `false`, as gs's
/// is for a password that does not match (nothing is reset, the two operands
/// are replaced by the answer). The first operand must be a boolean, else
/// `typecheck`; the password may be anything.
fn startjob(it: &mut Interp) -> Result<(), PsError> {
    // gs looks at the operand below the top first: an empty stack, or one
    // with only the password, is `stackunderflow` with nothing consumed.
    let n = it.ostack.len();
    if n < 2 {
        return Err(PsError::StackUnderflow);
    }
    if !matches!(it.ostack[n - 2].value, Value::Boolean(_)) {
        return Err(PsError::Typecheck);
    }
    it.pop()?;
    it.pop()?;
    it.push(Object::bool(false));
    Ok(())
}

/// `password exitserver -` (in `serverdict`). Always `invalidaccess`: there is
/// no job server to leave. As in gs, where it is a procedure over `startjob`,
/// the operand is consumed by the failure (`7 8 0 exitserver` leaves `7 8`).
/// With nothing on the stack gs leaves a `true` behind `stackunderflow`;
/// pscat leaves nothing.
pub(crate) fn exitserver(it: &mut Interp) -> Result<(), PsError> {
    it.pop()?;
    it.forget_operands();
    Err(PsError::InvalidAccess)
}

// --- device parameters -------------------------------------------------------

/// The IO devices pscat names, and what kind each is.
enum Device {
    /// `%stdin%`, `%stdout%`, ... : `/Type /Special`, nothing to set.
    Special,
    /// `%os%`: a file system, which a program may not reconfigure.
    FileSystem,
}

/// The device a name string names: `%stdout%` or `%stdout`, case sensitive,
/// nothing after the closing `%`. `undefined` for anything else, `typecheck`
/// for a non-string, `invalidaccess` for a `noaccess` one.
fn device_of(name: &Object) -> Result<Device, PsError> {
    let Value::String(s) = &name.value else {
        return Err(PsError::Typecheck);
    };
    if name.noaccess {
        return Err(PsError::InvalidAccess);
    }
    let bytes = s.borrow_bytes();
    let inner = bytes
        .strip_prefix(b"%")
        .map(|b| b.strip_suffix(b"%").unwrap_or(b));
    match inner {
        Some(b"stdin" | b"stdout" | b"stderr" | b"lineedit" | b"statementedit") => {
            Ok(Device::Special)
        }
        Some(b"os") => Ok(Device::FileSystem),
        // The name in the error is cut: a program may name a device with a
        // string of any length.
        _ => Err(PsError::Undefined(name.text_bounded(256))),
    }
}

/// `device currentdevparams dict`: a new dictionary each time. The file
/// system's sizes are fixed (gs reports the host's, which a program has no
/// business reading).
fn currentdevparams(it: &mut Interp) -> Result<(), PsError> {
    let name = it.pop()?;
    let device = device_of(&name)?;
    let entries: Vec<(&str, Object)> = match device {
        Device::Special => vec![("Type", Object::name("Special"))],
        Device::FileSystem => vec![
            ("Type", Object::name("FileSystem")),
            ("SearchOrder", Object::int(2)),
            ("LogicalSize", Object::int(1_953_125)),
            ("Searchable", Object::bool(true)),
            ("BlockSize", Object::int(1024)),
            ("HasNames", Object::bool(true)),
            ("Removable", Object::bool(false)),
            ("Free", Object::int(1_953_125)),
            ("InitializeAction", Object::int(0)),
            // Whether `(path) (w) file` can succeed at all: only inside the
            // directories `--allow-write` named.
            ("Writeable", Object::bool(it.can_write_files())),
            ("Mounted", Object::bool(true)),
        ],
    };
    let entry = 2 * (std::mem::size_of::<Object>() + 16);
    it.check_alloc(entries.len() * entry)?;
    it.poll_limits(table_steps(entries.len()))?;
    let mut dict = Dict::new();
    for (k, v) in entries {
        dict.put(k.into(), v);
    }
    it.push(Object::lit(Value::Dict(Rc::new(RefCell::new(dict)))));
    Ok(())
}

/// `device dict setdevparams -`. The special devices have nothing to set and
/// ignore the dictionary, as gs does. A file system's parameters are the
/// host's, so a dictionary that sets any of them (gs ignores it silently) is
/// `invalidaccess`; an empty one is accepted.
fn setdevparams(it: &mut Interp) -> Result<(), PsError> {
    let dict = it.pop()?;
    // gs checks the top operand first: with one operand that is no
    // dictionary it is `typecheck`, not `stackunderflow`.
    let Value::Dict(d) = &dict.value else {
        return Err(PsError::Typecheck);
    };
    let name = it.pop()?;
    let device = device_of(&name)?;
    if matches!(device, Device::FileSystem) && !d.borrow().is_empty() {
        return Err(PsError::InvalidAccess);
    }
    Ok(())
}
