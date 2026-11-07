//! The kernel compiler: an abstract interpreter over a PostScript
//! procedure (design: `docs/dev/design/KERNELS.md`).
//!
//! It runs the procedure once, at compile time, on a *symbolic* operand
//! stack whose entries are constants or IR registers. Stack shuffles,
//! name lookups and procedure calls happen here, so they cost nothing per
//! pixel; what is left is arithmetic and control flow, emitted as IR.
//!
//! **State and joins.** The compile-time state is the symbolic stack plus
//! the compile-time dict stack (the pixel's own dict and any `N dict
//! begin` scopes), whose bindings are symbolic values too. Both are SSA-like:
//! nothing is ever overwritten in place. Where control flow merges (after
//! an `if`, at a loop head, after a loop), every *slot* (a stack position, or
//! a name in a scope) whose value differs between the paths gets a merge
//! register, and each path ends with a move into it. A name that one path
//! defines and the other does not merges with whatever the other path
//! would find for it (an outer scope's value, or the program's own
//! definition): exactly what the per-pixel fresh dict means.
//!
//! **Loops** reach a fixpoint: the body is compiled, the slots that change
//! across the back edge or at an `exit` become loop-carried registers, and
//! the body is compiled again until that set stops growing.
//!
//! Anything outside the subset is a [`NotCompilable`] with a reason; the
//! caller then runs the procedure through the interpreter instead.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::exec::{INT_LIMIT_I, IOut, lane_bin, lane_ibin, lane_iun, lane_un, real_to_int};
use super::ir::{
    BinOp, IBinOp, IUnOp, Ins, Kernel, NoiseKind, Op, Reg, Sub, Table, TableData, TableKind, UnOp,
};
use crate::interp::Interp;
use crate::name::PsName;
use crate::object::{Dict, Object, Operator, PsArray, Value};
use crate::ops::arith::{RAND_INC, RAND_MASK, RAND_MUL, RRAND_TAG};

/// A compiled kernel and what compiling it found.
pub(crate) struct Compiled {
    pub kernel: Kernel,
    /// It samples the noise table.
    pub noise: bool,
    /// Branches that run in the interpreter when taken (why, each).
    pub soft: Vec<String>,
    /// Compile-time work, charged as interpreter steps.
    pub work: u64,
}

/// Why `compile` gave no kernel.
pub(crate) enum Refusal {
    /// Not in the compiled subset: run the interpreter instead. With the
    /// compile-time work spent finding that out (charged as steps).
    Not(String, u64),
    /// A limit (`--max-time`, `--max-steps`) fired while compiling.
    Limit(crate::error::PsError),
}

/// Why a procedure is not in the compiled subset.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NotCompilable(pub String);

type CResult<T> = Result<T, NotCompilable>;

fn nc<T>(msg: impl Into<String>) -> CResult<T> {
    Err(NotCompilable(msg.into()))
}

/// Static number type: integer, real, or either per lane (with a tag
/// register, 1.0 for an integer).
#[derive(Clone, Copy, Debug, PartialEq)]
enum NT {
    Int,
    Real,
    Dyn(Reg),
}

impl NT {
    fn tag(self) -> Op {
        match self {
            NT::Int => Op::Imm(1.0),
            NT::Real => Op::Imm(0.0),
            NT::Dyn(r) => Op::R(r),
        }
    }
}

/// A symbolic value.
#[derive(Clone)]
enum SVal {
    Num(Op, NT),
    Bool(Op),
    Mark,
    Null,
    /// A kernel scope's dictionary, as `where` finds it: only to drop.
    LocalDict,
    Name(PsName),
    Proc(PsArray),
    /// A literal array or string from outside the kernel: read only.
    Cap(Object),
    /// An array built in the kernel with `[ ]`, its elements symbolic; the
    /// id of the block it was built in (it may be changed only there).
    Tuple(Rc<RefCell<Vec<SVal>>>, u32),
    /// A row of a two-level table, chosen at run time.
    Row {
        table: u32,
        row: Op,
        rtag: Op,
        cols: usize,
    },
    /// A per-lane local array (`N array`).
    LArr(u32, usize),
    /// `N dict`, not yet begun (its id).
    NewDict(u32),
    CapDict(Rc<RefCell<Dict>>),
}

impl PartialEq for SVal {
    fn eq(&self, o: &SVal) -> bool {
        use SVal::*;
        match (self, o) {
            (Num(a, at), Num(b, bt)) => a == b && at == bt,
            (Bool(a), Bool(b)) => a == b,
            (Mark, Mark) => true,
            (Null, Null) => true,
            (Name(a), Name(b)) => a == b,
            (Proc(a), Proc(b)) => a.same_object(b),
            (Cap(a), Cap(b)) => match (&a.value, &b.value) {
                (Value::Array(x), Value::Array(y)) => x.identity() == y.identity(),
                (Value::String(x), Value::String(y)) => x.identity() == y.identity(),
                _ => false,
            },
            (Tuple(a, _), Tuple(b, _)) => Rc::ptr_eq(a, b),
            (
                Row {
                    table: t1,
                    row: r1,
                    rtag: g1,
                    ..
                },
                Row {
                    table: t2,
                    row: r2,
                    rtag: g2,
                    ..
                },
            ) => t1 == t2 && r1 == r2 && g1 == g2,
            (LArr(a, _), LArr(b, _)) => a == b,
            (NewDict(a), NewDict(b)) => a == b,
            (CapDict(a), CapDict(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl SVal {
    fn is_scalar(&self) -> bool {
        matches!(self, SVal::Num(..) | SVal::Bool(_))
    }
}

#[derive(Clone)]
enum Binding {
    Val(SVal),
    /// Defined on some paths only, with nothing to merge with.
    Maybe,
    /// A number or boolean defined on some paths only: its value where it
    /// is, and a per-lane flag register (1 where defined). A read asserts
    /// the flag, so a lane that would read it undefined aborts to the
    /// interpreter, which then does whatever the program really does.
    Partial(SVal, Reg),
}

#[derive(Clone)]
enum ScopeKind {
    /// The pixel's own fresh dict, or an `N dict begin` in the kernel.
    Local,
    /// `somedict begin` with a dict from outside: read only.
    Captured(Rc<RefCell<Dict>>),
}

#[derive(Clone)]
struct Scope {
    uid: u32,
    kind: ScopeKind,
    names: Vec<(u32, Binding)>,
}

impl Scope {
    fn get(&self, id: u32) -> Option<&Binding> {
        self.names.iter().find(|(k, _)| *k == id).map(|(_, b)| b)
    }

    fn set(&mut self, id: u32, b: Binding) {
        match self.names.iter_mut().find(|(k, _)| *k == id) {
            Some(slot) => slot.1 = b,
            None => self.names.push((id, b)),
        }
    }
}

#[derive(Clone)]
struct State {
    stack: Vec<SVal>,
    scopes: Vec<Scope>,
    /// No lane reaches here (after `exit` or a certain error).
    dead: bool,
}

/// A place a value lives in the state.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Slot {
    Stack(usize),
    /// A name in the scope with this uid.
    Bind(u32, u32),
}

/// What a lookup of a slot finds.
#[derive(Clone, PartialEq)]
enum Lk {
    Val(SVal),
    Maybe,
    Partial(SVal, Reg),
    Undef,
}

/// The type a merged or carried slot holds.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CT {
    Int,
    Real,
    Dyn,
    Bool,
}

fn ct_of(v: &SVal) -> Option<CT> {
    match v {
        SVal::Num(_, NT::Int) => Some(CT::Int),
        SVal::Num(_, NT::Real) => Some(CT::Real),
        SVal::Num(_, NT::Dyn(_)) => Some(CT::Dyn),
        SVal::Bool(_) => Some(CT::Bool),
        _ => None,
    }
}

fn ct_join(a: CT, b: CT) -> Option<CT> {
    match (a, b) {
        (CT::Bool, CT::Bool) => Some(CT::Bool),
        (CT::Bool, _) | (_, CT::Bool) => None,
        (x, y) if x == y => Some(x),
        _ => Some(CT::Dyn),
    }
}

#[derive(Clone)]
struct Carried {
    slot: Slot,
    reg: Reg,
    tag: Option<Reg>,
    ty: CT,
}

impl Carried {
    fn sval(&self) -> SVal {
        match self.ty {
            CT::Int => SVal::Num(Op::R(self.reg), NT::Int),
            CT::Real => SVal::Num(Op::R(self.reg), NT::Real),
            CT::Dyn => SVal::Num(Op::R(self.reg), NT::Dyn(self.tag.unwrap_or(self.reg))),
            CT::Bool => SVal::Bool(Op::R(self.reg)),
        }
    }
}

/// Moves into merge or loop-carried registers: conversions first, then
/// the moves (`(dst, src, dst is I64)`), done as a parallel assignment.
#[derive(Default)]
struct Moves {
    pre: Vec<Ins>,
    pairs: Vec<(Reg, Op, bool)>,
}

struct LoopCtx {
    carried: Vec<Carried>,
    exits: Vec<State>,
    depth: usize,
    /// An unrolled loop: `exit` cannot be compiled inside it.
    unrolled: bool,
    /// An `exit` found a value that does not fit its carried slot's type
    /// yet: the fixpoint widens the slot and compiles the body again.
    mismatch: bool,
}

/// The `exit`-inside-an-unrolled-loop refusal, retried as a real loop.
const EXIT_UNROLLED: &str = "exit inside an unrolled loop";
/// Constant loops with at most this many trips are unrolled.
const UNROLL_SMALL: usize = 16;
/// A loop whose body changes the stack depth can only be unrolled; up to
/// this many trips.
const UNROLL_MAX: usize = 256;

/// The counter values of a `for` with constant operands, as
/// `ForControl` steps them, or `None` past `cap` trips.
fn for_trips(init: f64, inc: f64, lim: f64, cap: usize) -> Option<Vec<f64>> {
    let mut v = Vec::new();
    let mut c = init;
    loop {
        let fin = if inc >= 0.0 { c > lim } else { c < lim };
        if fin {
            return Some(v);
        }
        if v.len() >= cap {
            return None;
        }
        v.push(c);
        c += inc;
    }
}

/// A point to roll the compiler back to after a failed attempt.
struct Checkpoint {
    st: State,
    block: u32,
    code: usize,
    emitted: usize,
    trail: usize,
    loops: usize,
    /// Exits the innermost loop had recorded.
    exits: usize,
    frames: usize,
}

/// How the compiler shares procedure bodies (#1064): see [`Compiler::call_named`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShareMode {
    /// Inline everything; share only if that makes the kernel too large.
    Auto,
    Never,
    /// Share a procedure of [`SHARE_MIN`] instructions or more from its
    /// second call on.
    Large,
    /// Share every named procedure from its first call (a test mode).
    All,
}

/// A procedure body is worth sharing from this many instructions.
const SHARE_MIN: usize = 48;
/// Subroutines one procedure may have (one per distinct context).
const MAX_DEFS: usize = 16;
/// Failed attempts to share one procedure before it is always inlined.
const MAX_SHARE_FAILS: u32 = 3;
/// The size cap's refusal, which `ShareMode::Auto` retries with sharing.
const TOO_LARGE: &str = "the kernel is too large";

/// A procedure, by identity.
type ProcId = (usize, usize, usize);

/// One element of a sharing key: the shape of the values a body reads,
/// with registers numbered in order of first appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Atom {
    /// The `n`th distinct register, and its kind: bit 0 for I64, bit 1
    /// for a register known to hold a seeded random state (#1063), which
    /// changes what `rand` compiles to.
    Reg(u32, u8),
    Imm(u64),
    T(u8),
    N(u64),
}

/// A key under construction: its atoms and the registers it numbered.
#[derive(Default)]
struct KeyB {
    atoms: Vec<Atom>,
    regs: Vec<Reg>,
    idx: HashMap<Reg, u32>,
}

impl KeyB {
    fn reg(&mut self, r: Reg, kinds: &dyn Fn(Reg) -> u8) {
        let n = self.regs.len() as u32;
        let k = *self.idx.entry(r).or_insert(n);
        if k == n {
            self.regs.push(r);
        }
        self.atoms.push(Atom::Reg(k, kinds(r)));
    }

    fn op(&mut self, o: Op, kinds: &dyn Fn(Reg) -> u8) {
        match o {
            Op::R(r) => self.reg(r, kinds),
            Op::Imm(v) => self.atoms.push(Atom::Imm(v.to_bits())),
        }
    }

    fn n(&mut self, v: usize) {
        self.atoms.push(Atom::N(v as u64));
    }

    /// `None` for a value a shared body may not take (an array built in
    /// the kernel, which a body could change in place; a dict not begun).
    fn sval(&mut self, v: &SVal, kinds: &dyn Fn(Reg) -> u8) -> Option<()> {
        match v {
            SVal::Num(o, t) => {
                self.atoms.push(Atom::T(0));
                self.op(*o, kinds);
                match t {
                    NT::Int => self.atoms.push(Atom::T(10)),
                    NT::Real => self.atoms.push(Atom::T(11)),
                    NT::Dyn(r) => {
                        self.atoms.push(Atom::T(12));
                        self.reg(*r, kinds);
                    }
                }
            }
            SVal::Bool(o) => {
                self.atoms.push(Atom::T(1));
                self.op(*o, kinds);
            }
            SVal::Mark => self.atoms.push(Atom::T(2)),
            SVal::Null => self.atoms.push(Atom::T(3)),
            SVal::LocalDict => self.atoms.push(Atom::T(4)),
            SVal::Name(n) => {
                self.atoms.push(Atom::T(5));
                self.n(n.id() as usize);
            }
            SVal::Proc(p) => {
                self.atoms.push(Atom::T(6));
                let (a, b, c) = p.identity();
                self.n(a);
                self.n(b);
                self.n(c);
            }
            SVal::Cap(o) => {
                let (t, (a, b, c)) = match &o.value {
                    Value::Array(x) => (7, x.identity()),
                    Value::String(x) => (8, x.identity()),
                    _ => return None,
                };
                self.atoms.push(Atom::T(t));
                self.n(a);
                self.n(b);
                self.n(c);
            }
            SVal::Row {
                table,
                row,
                rtag,
                cols,
            } => {
                self.atoms.push(Atom::T(9));
                self.n(*table as usize);
                self.op(*row, kinds);
                self.op(*rtag, kinds);
                self.n(*cols);
            }
            SVal::LArr(id, n) => {
                self.atoms.push(Atom::T(13));
                self.n(*id as usize);
                self.n(*n);
            }
            SVal::CapDict(d) => {
                self.atoms.push(Atom::T(14));
                self.n(Rc::as_ptr(d) as *const () as usize);
            }
            SVal::Tuple(..) | SVal::NewDict(_) => return None,
        }
        Some(())
    }

    fn binding(&mut self, b: Option<&Binding>, kinds: &dyn Fn(Reg) -> u8) -> Option<()> {
        match b {
            None => self.atoms.push(Atom::T(20)),
            Some(Binding::Val(v)) => {
                self.atoms.push(Atom::T(21));
                self.sval(v, kinds)?;
            }
            Some(Binding::Maybe) => self.atoms.push(Atom::T(22)),
            Some(Binding::Partial(v, f)) => {
                self.atoms.push(Atom::T(23));
                self.sval(v, kinds)?;
                self.reg(*f, kinds);
            }
        }
        Some(())
    }
}

/// Every register in a value.
fn sval_regs(v: &SVal, f: &mut impl FnMut(Reg)) {
    let o = |o: &Op, f: &mut dyn FnMut(Reg)| {
        if let Op::R(r) = o {
            f(*r);
        }
    };
    match v {
        SVal::Num(x, t) => {
            o(x, f);
            if let NT::Dyn(r) = t {
                f(*r);
            }
        }
        SVal::Bool(x) => o(x, f),
        SVal::Row { row, rtag, .. } => {
            o(row, f);
            o(rtag, f);
        }
        _ => {}
    }
}

fn binding_regs(b: &Binding, f: &mut impl FnMut(Reg)) {
    match b {
        Binding::Val(v) => sval_regs(v, f),
        Binding::Maybe => {}
        Binding::Partial(v, flag) => {
            sval_regs(v, f);
            f(*flag);
        }
    }
}

/// `v` with each register renamed by `m`.
fn map_sval(v: &SVal, m: &impl Fn(Reg) -> Reg) -> SVal {
    let o = |o: Op| match o {
        Op::R(r) => Op::R(m(r)),
        x => x,
    };
    match v {
        SVal::Num(x, t) => SVal::Num(
            o(*x),
            match t {
                NT::Dyn(r) => NT::Dyn(m(*r)),
                t => *t,
            },
        ),
        SVal::Bool(x) => SVal::Bool(o(*x)),
        SVal::Row {
            table,
            row,
            rtag,
            cols,
        } => SVal::Row {
            table: *table,
            row: o(*row),
            rtag: o(*rtag),
            cols: *cols,
        },
        other => other.clone(),
    }
}

fn map_binding(b: &Binding, m: &impl Fn(Reg) -> Reg) -> Binding {
    match b {
        Binding::Val(v) => Binding::Val(map_sval(v, m)),
        Binding::Maybe => Binding::Maybe,
        Binding::Partial(v, f) => Binding::Partial(map_sval(v, m), m(*f)),
    }
}

fn binding_eq(a: Option<&Binding>, b: Option<&Binding>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Binding::Val(x)), Some(Binding::Val(y))) => x == y,
        (Some(Binding::Maybe), Some(Binding::Maybe)) => true,
        (Some(Binding::Partial(v, f)), Some(Binding::Partial(w, g))) => v == w && f == g,
        _ => false,
    }
}

/// A procedure body being compiled so that it may be shared: what of the
/// state it entered with it has looked at (#1064).
///
/// The compiler is deterministic, and a body's compilation depends on the
/// state only through what this records: the stack entries it reached
/// (`low` up), the names it looked up, defined or merged, and the
/// structure of the dictionary stack. Another call whose state agrees on
/// all of that (constants equal, registers in the same pattern and of the
/// same kinds) would compile to the same code with its registers in place
/// of these, so it may call the code compiled here instead.
struct Frame {
    entry: State,
    /// Lowest stack index the body read or changed.
    low: usize,
    /// Deepest stack the body made (checked against the symbolic cap).
    high: usize,
    ids: HashSet<u32>,
    /// Names it defined without reading them first: not part of the
    /// key (a `def` does not look at what it replaces), but outputs.
    /// (by scope uid and name: a def lands in the scope then on top, and
    /// is an output even when it writes the value already there).
    wids: HashSet<(u32, u32)>,
    /// The names in `wids`.
    wnames: HashSet<u32>,
    /// Registers made before the body.
    reg_base: u32,
    loops: usize,
    larrays: usize,
    code_start: usize,
    /// Why the body cannot be shared, whatever its context.
    unshareable: bool,
}

/// Where a register in a subroutine's outputs comes from at a call.
#[derive(Clone, Copy, Debug)]
enum Src {
    /// The caller's `n`th input register.
    In(u32),
    /// The subroutine's `n`th result, copied out.
    Res(u32),
}

/// A shared subroutine of one procedure, and the context it serves.
struct SubDef {
    proc: PsArray,
    id: u32,
    nstack: usize,
    ids: Vec<u32>,
    /// Names it defines: entry scope index, name.
    wids: Vec<(usize, u32)>,
    key: Vec<Atom>,
    /// Parameter registers, by the input they take.
    params: Vec<(u32, Reg)>,
    results: Vec<Reg>,
    growth: usize,
    dead: bool,
    stack: Vec<SVal>,
    /// Bindings it changes: scope index, name, binding.
    binds: Vec<(usize, u32, Binding)>,
    src: HashMap<Reg, Src>,
}

/// Procedure nesting allowed at compile time (inlining depth).
const MAX_INLINE: usize = 48;
/// Upper bound on emitted instructions, so a pathological program cannot
/// make the compiler itself expensive.
const MAX_INSNS: usize = 200_000;
/// Procedure elements a compile may execute (inlining can multiply a
/// short program's work: `{p p}` nested 30 deep).
const MAX_WORK: u64 = 20_000_000;
/// Deepest symbolic operand stack a compile may build.
const MAX_SYMBOLIC_STACK: usize = 4096;
/// Largest `N array` a kernel may make.
const MAX_LOCAL_ARRAY: usize = 4096;
/// Largest table a kernel may snapshot (elements).
const MAX_TABLE: usize = 16 << 20;
/// Largest string a kernel may snapshot (bytes, kept at one byte each).
const MAX_TABLE_BYTES: usize = 256 << 20;

/// The random state's slot (issue #1063): a binding in the pixel's own
/// scope under an id no name can have, so it merges at joins and is carried
/// round loops like any name. It holds an integer: the LCG state, or
/// [`UNSEEDED`] on the paths that have not called `srand` yet.
const RAND_ID: u32 = u32::MAX;
/// The random state before the pixel's first `srand`. `srand` and `rand`
/// always mask to [0, 2^31), so no seeded state is ever -1.
const UNSEEDED: i64 = -1;

pub(crate) struct Compiler<'a> {
    it: &'a Interp,
    nregs: u32,
    /// Storage kind of each register (see `ir.rs`): true for I64.
    kinds: Vec<bool>,
    st: State,
    code: Vec<Ins>,
    emitted: usize,
    block: u32,
    next_block: u32,
    next_uid: u32,
    loops: Vec<LoopCtx>,
    depth: usize,
    tables: Vec<Table>,
    table_ids: HashMap<(u8, usize, usize, usize), (u32, bool)>,
    larrays: Vec<usize>,
    begun_dicts: Vec<u32>,
    pub(crate) uses_noise: bool,
    /// It calls `srand`: the state each pixel leaves is an output.
    uses_rand: bool,
    /// Registers that hold a seeded random state on every lane that writes
    /// them (`srand` and `rand` results): a `rand` of one needs no check.
    seeded: std::collections::HashSet<Reg>,
    /// The procedures being inlined, by name, for error messages.
    trail: Vec<String>,
    /// `[ ]` arrays merged at a join: no longer to be changed.
    frozen: std::collections::HashSet<usize>,
    /// Weak references to the frozen arrays: a weak reference keeps the
    /// allocation (not its elements), so no array made later can take a
    /// frozen one's address.
    frozen_keep: Vec<std::rc::Weak<RefCell<Vec<SVal>>>>,
    /// Procedure elements executed at compile time: bounded, and charged
    /// as steps, so compiling cannot escape the program's limits.
    pub(crate) work: u64,
    /// The time limit as a `Clock::mono_ns` instant, and the steps left.
    deadline: Option<u64>,
    steps_left: Option<u64>,
    /// A limit fired while compiling: reported as that limit's error.
    pub(crate) limit: Option<crate::error::PsError>,
    /// Branches that did not compile and abort to the interpreter when a
    /// lane takes them (why, for `kernelcompile`); with `strict`, refused.
    pub(crate) soft: Vec<String>,
    strict: bool,
    share: ShareMode,
    /// Shared subroutines made so far (some may end up unused).
    subs: Vec<Sub>,
    /// How deep calls nest from each subroutine, itself included.
    sub_depth: Vec<usize>,
    /// Owner of each register, as `Kernel::owner` (missing: 0).
    owner: Vec<u32>,
    /// Instructions moved into subroutines, counted against the cap.
    sub_insns: usize,
    /// Bodies being compiled for sharing, innermost last.
    frames: Vec<Frame>,
    defs: HashMap<ProcId, Vec<Rc<SubDef>>>,
    /// Size of a procedure's inlined body, the most seen.
    seen: HashMap<ProcId, usize>,
    share_fails: HashMap<ProcId, u32>,
}

/// Compile `proc` as a kernel taking `i j` and leaving `ncomp` numbers.
/// With `strict`, a branch that does not compile fails the whole kernel
/// instead of aborting to the interpreter when taken.
pub(crate) fn compile(
    it: &Interp,
    proc: &PsArray,
    ncomp: usize,
    strict: bool,
    share: ShareMode,
) -> Result<Compiled, Refusal> {
    let first = if share == ShareMode::Auto {
        ShareMode::Never
    } else {
        share
    };
    let r = compile_with(it, proc, ncomp, strict, first, 0);
    if share != ShareMode::Auto {
        return r;
    }
    let (big, spent) = match &r {
        Ok(c) => (c.soft.iter().any(|w| w.starts_with(TOO_LARGE)), c.work),
        Err(Refusal::Not(w, n)) => (w.starts_with(TOO_LARGE), *n),
        Err(Refusal::Limit(_)) => (false, 0),
    };
    if !big {
        return r;
    }
    // Too large inlined: compile again sharing the bodies called often,
    // the first attempt's work charged too.
    match compile_with(it, proc, ncomp, strict, ShareMode::Large, spent) {
        Ok(mut c) => {
            c.work += spent;
            Ok(c)
        }
        Err(Refusal::Not(w, n)) => Err(Refusal::Not(w, n + spent)),
        Err(e) => Err(e),
    }
}

fn compile_with(
    it: &Interp,
    proc: &PsArray,
    ncomp: usize,
    strict: bool,
    share: ShareMode,
    spent: u64,
) -> Result<Compiled, Refusal> {
    let mut c = Compiler {
        it,
        nregs: 0,
        kinds: Vec::new(),
        st: State {
            stack: Vec::new(),
            scopes: vec![Scope {
                uid: 0,
                kind: ScopeKind::Local,
                names: vec![(
                    RAND_ID,
                    Binding::Val(SVal::Num(Op::Imm(UNSEEDED as f64), NT::Int)),
                )],
            }],
            dead: false,
        },
        code: Vec::new(),
        emitted: 0,
        block: 0,
        next_block: 1,
        next_uid: 1,
        loops: Vec::new(),
        depth: 0,
        tables: Vec::new(),
        table_ids: HashMap::new(),
        larrays: Vec::new(),
        begun_dicts: Vec::new(),
        uses_noise: false,
        uses_rand: false,
        seeded: std::collections::HashSet::new(),
        trail: Vec::new(),
        frozen: std::collections::HashSet::new(),
        frozen_keep: Vec::new(),
        soft: Vec::new(),
        strict,
        work: 0,
        deadline: it.remaining_budget().1,
        steps_left: it.remaining_budget().0.map(|s| s.saturating_sub(spent)),
        limit: None,
        share,
        subs: Vec::new(),
        sub_depth: Vec::new(),
        owner: Vec::new(),
        sub_insns: 0,
        frames: Vec::new(),
        defs: HashMap::new(),
        seen: HashMap::new(),
        share_fails: HashMap::new(),
    };
    let r = (|c: &mut Compiler| -> CResult<(Kernel, bool, Vec<String>)> {
        let (ri, rj) = (c.ireg(), c.ireg());
        c.st.stack.push(SVal::Num(Op::R(ri), NT::Int));
        c.st.stack.push(SVal::Num(Op::R(rj), NT::Int));
        if let Err(NotCompilable(why)) = c.exec_proc(proc) {
            // Say where: the innermost named procedures being inlined.
            let t = &c.trail;
            let path = t[t.len().saturating_sub(4)..].join(" > ");
            return Err(NotCompilable(if path.is_empty() {
                why
            } else {
                format!("{why}, in {path}")
            }));
        }
        let mut outputs = Vec::new();
        let rand = if !c.uses_rand {
            None
        } else if c.st.dead {
            Some(Op::Imm(UNSEEDED as f64))
        } else {
            Some(c.rand_state()?)
        };
        if !c.st.dead {
            if c.st.scopes.len() != 1 {
                return nc("the procedure leaves a dictionary begun (begin without end)");
            }
            if c.st.stack.len() != ncomp {
                return nc(format!(
                    "the procedure must consume i and j and leave {ncomp} numbers; it leaves {} values",
                    c.st.stack.len()
                ));
            }
            for v in c.st.stack.clone() {
                match v {
                    SVal::Num(o, t) => {
                        let o = c.as_f(o, t)?;
                        outputs.push(o);
                    }
                    _ => return nc("an output component is not a number"),
                }
            }
        } else {
            outputs = vec![Op::Imm(0.0); ncomp];
        }
        let noise = c.uses_noise;
        let soft = std::mem::take(&mut c.soft);
        let mut owner = std::mem::take(&mut c.owner);
        owner.resize(c.nregs as usize, 0);
        let mut kernel = Kernel {
            nregs: c.nregs as usize,
            body: std::mem::take(&mut c.code),
            subs: std::mem::take(&mut c.subs),
            owner,
            in_i: ri,
            in_j: rj,
            outputs,
            rand,
            tables: std::mem::take(&mut c.tables),
            larrays: std::mem::take(&mut c.larrays),
            perm: it.noise_perm,
        };
        super::opt::prune_subs(&mut kernel);
        super::opt::dce(&mut kernel);
        if cfg!(debug_assertions) {
            super::check::kinds(&kernel, &c.kinds)
                .map_err(|e| NotCompilable(format!("internal: {e}")))?;
        }
        let dump = std::env::var_os("PSCAT_KERNEL_DUMP").is_some();
        if dump {
            eprintln!("--- kernel before allocation ---\n{:#?}", kernel.body);
        }
        if std::env::var_os("PSCAT_KERNEL_NO_REGALLOC").is_none() {
            super::regalloc::allocate(&mut kernel);
        }
        if dump {
            eprintln!(
                "--- after ---\n{:#?}\noutputs {:?}",
                kernel.body, kernel.outputs
            );
        }
        Ok((kernel, noise, soft))
    })(&mut c);
    if let Some(e) = c.limit.take() {
        return Err(Refusal::Limit(e));
    }
    let work = c.work;
    match r {
        Ok((kernel, noise, soft)) => Ok(Compiled {
            kernel,
            noise,
            soft,
            work,
        }),
        Err(NotCompilable(why)) => Err(Refusal::Not(why, work)),
    }
}

impl<'a> Compiler<'a> {
    /// A new F64 register.
    fn reg(&mut self) -> Reg {
        self.kinds.push(false);
        self.nregs += 1;
        self.nregs - 1
    }

    /// A new I64 register.
    fn ireg(&mut self) -> Reg {
        self.kinds.push(true);
        self.nregs += 1;
        self.nregs - 1
    }

    /// A register for a merged or carried value of type `t`.
    fn reg_for(&mut self, t: CT) -> Reg {
        if t == CT::Int {
            self.ireg()
        } else {
            self.reg()
        }
    }

    /// `o` (of type `t`) as an f64 operand, as the interpreter's
    /// `to_f64` makes it: an I64 register is converted.
    fn as_f(&mut self, o: Op, t: NT) -> CResult<Op> {
        match (o, t) {
            (Op::R(_), NT::Int) => {
                let d = self.reg();
                self.emit(Ins::I2F(d, o))?;
                Ok(Op::R(d))
            }
            _ => Ok(o),
        }
    }

    /// Two numbers to compare as f64s. Beside a Dyn value (an integer on
    /// some lanes) an integer must convert exactly, as the interpreter
    /// compares two integers exactly.
    fn cmp_operands(&mut self, a: Op, at: NT, b: Op, bt: NT) -> CResult<(Op, Op)> {
        if matches!(at, NT::Dyn(_)) || matches!(bt, NT::Dyn(_)) {
            Ok((self.as_exact_f(a, at)?, self.as_exact_f(b, bt)?))
        } else {
            Ok((self.as_f(a, at)?, self.as_f(b, bt)?))
        }
    }

    /// `o` as an exact f64 (into storage that holds integers as f64s, a
    /// local array): an I64 register past 2^53 retires its lane.
    fn as_exact_f(&mut self, o: Op, t: NT) -> CResult<Op> {
        match (o, t) {
            (Op::R(_), NT::Int) => {
                let d = self.reg();
                self.emit(Ins::I2FExact(d, o))?;
                Ok(Op::R(d))
            }
            _ => Ok(o),
        }
    }

    /// `o` as an integer operand for an integer-only operator (I64 or an
    /// integer immediate); `None` when the path certainly errors.
    fn as_i(&mut self, o: Op, t: NT) -> CResult<Option<Op>> {
        match t {
            NT::Int => Ok(Some(o)),
            NT::Real => {
                self.abort_path()?;
                Ok(None)
            }
            NT::Dyn(tag) => {
                self.emit(Ins::NeedInt(Op::R(tag)))?;
                match o {
                    Op::Imm(_) => Ok(Some(o)),
                    Op::R(_) => {
                        let d = self.ireg();
                        self.emit(Ins::F2I(d, o))?;
                        Ok(Some(Op::R(d)))
                    }
                }
            }
        }
    }

    /// Push an integer constant: an immediate when exact in an f64.
    fn push_int(&mut self, v: i64) -> CResult<()> {
        if v.unsigned_abs() <= INT_LIMIT_I {
            self.push_num(Op::Imm(v as f64), NT::Int);
        } else {
            let d = self.ireg();
            self.emit(Ins::MovI(d, v))?;
            self.push_num(Op::R(d), NT::Int);
        }
        Ok(())
    }

    /// An integer op on two integer operands, folded when both are
    /// constants (an overflow folds to the interpreter's real result).
    fn ibin(&mut self, op: IBinOp, a: Op, b: Op) -> CResult<()> {
        if let (Op::Imm(x), Op::Imm(y)) = (a, b) {
            let (x, y) = (x as i64, y as i64);
            return match lane_ibin(op, x, y) {
                Some(IOut::I(v)) => self.push_int(v),
                Some(IOut::B(v)) => {
                    self.push(SVal::Bool(Op::Imm(f64::from(u8::from(v)))));
                    Ok(())
                }
                None => {
                    let (fx, fy) = (x as f64, y as f64);
                    let r = match op {
                        IBinOp::Add => fx + fy,
                        IBinOp::Sub => fx - fy,
                        IBinOp::Mul => fx * fy,
                        _ => return self.abort_path(),
                    };
                    self.push_num(Op::Imm(r), NT::Real);
                    Ok(())
                }
            };
        }
        if op.is_compare() {
            let d = self.reg();
            self.emit(Ins::IBin(op, d, a, b))?;
            self.push(SVal::Bool(Op::R(d)));
        } else {
            let d = self.ireg();
            self.emit(Ins::IBin(op, d, a, b))?;
            self.push_num(Op::R(d), NT::Int);
        }
        Ok(())
    }

    fn iun(&mut self, op: IUnOp, a: Op) -> CResult<()> {
        if let Op::Imm(x) = a {
            let x = x as i64;
            return match lane_iun(op, x) {
                Some(v) => self.push_int(v),
                None => {
                    // neg/abs of i64::MIN: the interpreter's real.
                    self.push_num(Op::Imm(-(x as f64)), NT::Real);
                    Ok(())
                }
            };
        }
        let d = self.ireg();
        self.emit(Ins::IUn(op, d, a))?;
        self.push_num(Op::R(d), NT::Int);
        Ok(())
    }

    fn emit(&mut self, i: Ins) -> CResult<()> {
        self.emitted += 1;
        if self.emitted + self.sub_insns > MAX_INSNS {
            return nc(TOO_LARGE);
        }
        self.code.push(i);
        Ok(())
    }

    fn push(&mut self, v: SVal) {
        self.st.stack.push(v);
    }

    /// The symbolic stack may not outgrow what a kernel plausibly needs
    /// (`n copy` doubles it); past that the interpreter runs the procedure
    /// and meets the program's real limits.
    fn check_depth(&mut self) -> CResult<()> {
        if let Some(f) = self.frames.last_mut() {
            f.high = f.high.max(self.st.stack.len());
        }
        if self.st.stack.len() > MAX_SYMBOLIC_STACK {
            return nc("the stack grows too deep");
        }
        Ok(())
    }

    fn pop(&mut self) -> CResult<SVal> {
        match self.st.stack.pop() {
            Some(v) => {
                self.touch(self.st.stack.len());
                Ok(v)
            }
            None => nc("stack underflow at compile time"),
        }
    }

    /// A shared body read or changed stack entry `i` (see [`Frame`]).
    #[inline]
    fn touch(&mut self, i: usize) {
        if let Some(f) = self.frames.last_mut() {
            f.low = f.low.min(i);
        }
    }

    /// A shared body looked up or merged name `id`.
    #[inline]
    fn touch_name(&mut self, id: u32) {
        if let Some(f) = self.frames.last_mut() {
            f.ids.insert(id);
        }
    }

    /// A shared body looks name `id` up. What it finds depends on the
    /// state it entered with unless it finds a binding the body itself
    /// made on this path (one that differs from what that scope held on
    /// entry: bindings are never removed, so no scope above it held the
    /// name either).
    fn touch_read(&mut self, id: u32) {
        let Some(f) = self.frames.last() else {
            return;
        };
        for s in self.st.scopes.iter().rev() {
            match &s.kind {
                ScopeKind::Local => {
                    if let Some(b) = s.get(id) {
                        // (A kernel dict compares unequal even to itself.)
                        if let Binding::Val(SVal::LocalDict) = b {
                            break;
                        }
                        let at_entry = f.entry.scopes.iter().find(|e| e.uid == s.uid);
                        let made = match at_entry {
                            Some(e) => !binding_eq(e.get(id), Some(b)),
                            // A scope begun in the body.
                            None => true,
                        };
                        if made {
                            return;
                        }
                        break;
                    }
                }
                ScopeKind::Captured(d) => {
                    if d.borrow().get_id(id).is_some() {
                        break;
                    }
                }
            }
        }
        self.touch_name(id);
    }

    /// A shared body defined name `id`.
    #[inline]
    fn touch_def(&mut self, id: u32) {
        let uid = self.st.scopes.last().map_or(0, |s| s.uid);
        if let Some(f) = self.frames.last_mut() {
            f.wids.insert((uid, id));
            f.wnames.insert(id);
        }
    }

    /// A join found name slot `sl` holding `v` on every path. If a shared
    /// body defined that name somewhere and `v` is what the body's entry
    /// state shows there, a path may hold it only because it skipped the
    /// def, and another context (where the def changes the value) would
    /// need a merge: the entry value is then part of the key.
    fn touch_equal_join(&mut self, sl: Slot, v: &Lk) -> CResult<()> {
        let Slot::Bind(uid, id) = sl else {
            return Ok(());
        };
        if !self.frames.iter().any(|f| f.wnames.contains(&id)) {
            return Ok(());
        }
        let mut frames = std::mem::take(&mut self.frames);
        for f in &mut frames {
            if !f.wnames.contains(&id) {
                continue;
            }
            // From that scope down, as at entry (a scope the body began
            // sits above all of the entry's).
            let euid = if f.entry.scopes.iter().any(|s| s.uid == uid) {
                uid
            } else {
                f.entry.scopes.last().map_or(0, |s| s.uid)
            };
            let same = self
                .slot_val(&f.entry, Slot::Bind(euid, id))
                .is_ok_and(|ev| ev == *v);
            let unknown = self.slot_val(&f.entry, Slot::Bind(euid, id)).is_err();
            if same || unknown {
                f.ids.insert(id);
            }
        }
        self.frames = frames;
        Ok(())
    }

    fn touch_slot(&mut self, sl: Slot) {
        match sl {
            Slot::Stack(i) => self.touch(i),
            Slot::Bind(_, id) => self.touch_name(id),
        }
    }

    fn pop_num(&mut self) -> CResult<(Op, NT)> {
        match self.pop()? {
            SVal::Num(o, t) => Ok((o, t)),
            other => nc(format!("expected a number, found {}", describe(&other))),
        }
    }

    fn pop_bool(&mut self) -> CResult<Op> {
        match self.pop()? {
            SVal::Bool(o) => Ok(o),
            other => nc(format!("expected a boolean, found {}", describe(&other))),
        }
    }

    fn pop_const_int(&mut self, what: &str) -> CResult<i64> {
        match self.pop()? {
            SVal::Num(Op::Imm(v), NT::Int) => Ok(v as i64),
            _ => nc(format!("{what} must be a constant integer")),
        }
    }

    fn pop_proc(&mut self) -> CResult<PsArray> {
        match self.pop()? {
            SVal::Proc(p) => Ok(p),
            other => nc(format!("expected a procedure, found {}", describe(&other))),
        }
    }

    /// Mark the current path dead because every lane on it errors.
    fn abort_path(&mut self) -> CResult<()> {
        self.emit(Ins::Abort)?;
        self.st.dead = true;
        Ok(())
    }

    // --- execution --------------------------------------------------------

    fn exec_proc(&mut self, p: &PsArray) -> CResult<()> {
        if self.depth >= MAX_INLINE {
            return nc("procedures nest too deeply (recursion is not compiled)");
        }
        self.depth += 1;
        let items = p.snapshot();
        let mut r = Ok(());
        for obj in &items {
            if self.st.dead {
                break;
            }
            r = self.exec_item(obj);
            if r.is_err() {
                break;
            }
        }
        self.depth -= 1;
        r
    }

    /// Count one element of compile-time work; look at the limits now and
    /// then.
    fn charge(&mut self) -> CResult<()> {
        self.work += 1;
        if self.work > MAX_WORK {
            return nc("the kernel takes too long to compile");
        }
        if self.work.is_multiple_of(4096) {
            if let Some(s) = self.steps_left
                && self.work > s
            {
                self.limit = Some(crate::error::PsError::StepsExceeded);
                return nc("limit");
            }
            if let Some(d) = self.deadline
                && crate::clock::Clock::mono_ns() >= d
            {
                self.limit = Some(crate::error::PsError::TimeExceeded);
                return nc("limit");
            }
        }
        Ok(())
    }

    /// One element of a procedure body.
    fn exec_item(&mut self, obj: &Object) -> CResult<()> {
        self.charge()?;
        self.check_depth()?;
        match &obj.value {
            Value::Name(n) if obj.executable => self.exec_name(n),
            Value::Operator(op) => self.apply(op),
            Value::Array(a) if obj.executable => {
                self.push(SVal::Proc(a.clone()));
                Ok(())
            }
            _ => {
                let v = self.cap_to_sval(obj)?;
                self.push(v);
                Ok(())
            }
        }
    }

    /// A value from outside the kernel (or a literal in its body).
    fn cap_to_sval(&self, obj: &Object) -> CResult<SVal> {
        Ok(match &obj.value {
            Value::Integer(i) => {
                if i.unsigned_abs() > INT_LIMIT_I {
                    return nc("an integer past 2^53");
                }
                SVal::Num(Op::Imm(*i as f64), NT::Int)
            }
            Value::Real(r) => SVal::Num(Op::Imm(*r), NT::Real),
            Value::Boolean(b) => SVal::Bool(Op::Imm(f64::from(u8::from(*b)))),
            Value::Mark => SVal::Mark,
            Value::Null => SVal::Null,
            Value::Name(n) if !obj.executable => SVal::Name(n.clone()),
            Value::Array(a) if obj.executable => SVal::Proc(a.clone()),
            Value::Array(_) | Value::String(_) => SVal::Cap(obj.clone()),
            Value::Dict(d) => SVal::CapDict(d.clone()),
            _ => return nc(format!("unsupported value {}", type_name(obj))),
        })
    }

    fn exec_name(&mut self, n: &PsName) -> CResult<()> {
        self.touch_read(n.id());
        match self.lookup(&self.st, n.id(), self.st.scopes.len() - 1)? {
            Found::Partial(v, f, uid) => {
                let v = self.read_partial(n, v, f, uid)?;
                self.push(v);
                Ok(())
            }
            Found::Val(SVal::Proc(p)) => self.exec_named(n, &p),
            Found::Val(v) => {
                self.push(v);
                Ok(())
            }
            Found::Obj(obj) => match &obj.value {
                Value::Operator(op) => self.apply(op),
                Value::Array(a) if obj.executable => self.exec_named(n, a),
                Value::Name(n2) if obj.executable => {
                    // An alias (`/a /b cvx def`): counts as a level of
                    // nesting, so a cycle (`/a /a cvx def`) is refused.
                    if self.depth >= MAX_INLINE {
                        return nc("names refer to each other in a cycle");
                    }
                    let n2 = n2.clone();
                    self.depth += 1;
                    let r = self.exec_name(&n2);
                    self.depth -= 1;
                    r
                }
                _ => {
                    let v = self.cap_to_sval(&obj)?;
                    self.push(v);
                    Ok(())
                }
            },
            Found::None => nc(format!("/{n} is not defined")),
        }
    }

    /// Read a partly defined name: abort the lanes where it is undefined;
    /// past here it is defined on this path.
    fn read_partial(&mut self, n: &PsName, v: SVal, flag: Reg, uid: u32) -> CResult<SVal> {
        self.emit(Ins::Assert(Op::R(flag)))?;
        if let Some(s) = self.st.scopes.iter_mut().find(|s| s.uid == uid) {
            s.set(n.id(), Binding::Val(v.clone()));
        }
        Ok(v)
    }

    /// Inline a procedure reached by name, keeping the name for messages.
    fn exec_named(&mut self, n: &PsName, p: &PsArray) -> CResult<()> {
        self.trail.push(n.to_string());
        let r = self.call_named(p);
        if r.is_ok() {
            self.trail.pop();
        }
        r
    }

    // --- shared subroutines (#1064) ---------------------------------------
    //
    // A named procedure is inlined, or (by `share`) compiled once as a
    // subroutine and called from each place it is used with the same
    // context: see [`Frame`] for what "the same" is. The first call that
    // makes a subroutine compiles the body inline as usual, recording what
    // it reads; the code is then moved out, the registers it read from
    // outside renamed to parameters, and a call put in its place. A body
    // that writes a register from outside, exits a loop around it, or
    // leaves an array it built (or anything not a plain value) where the
    // caller can see it, stays inline.

    fn call_named(&mut self, p: &PsArray) -> CResult<()> {
        if self.share == ShareMode::Never || self.st.dead {
            return self.exec_proc(p);
        }
        let pid = p.identity();
        if self.reuse(pid, p)? {
            return Ok(());
        }
        let candidate = (match self.share {
            ShareMode::All => true,
            _ => self.seen.get(&pid).is_some_and(|&n| n >= SHARE_MIN),
        }) && self.defs.get(&pid).map_or(0, Vec::len) < MAX_DEFS
            && self.share_fails.get(&pid).copied().unwrap_or(0) < MAX_SHARE_FAILS;
        if candidate {
            return self.compile_shared(pid, p);
        }
        let e0 = self.emitted + self.sub_insns;
        let r = self.exec_proc(p);
        let size = (self.emitted + self.sub_insns).saturating_sub(e0);
        let e = self.seen.entry(pid).or_insert(0);
        *e = (*e).max(size);
        r
    }

    /// Call a subroutine already made for `p`, if one serves this context.
    fn reuse(&mut self, pid: ProcId, p: &PsArray) -> CResult<bool> {
        let Some(defs) = self.defs.get(&pid) else {
            return Ok(false);
        };
        let len = self.st.stack.len();
        let mut hit = None;
        for d in defs {
            // A call nests as deep as inlining would: within MAX_INLINE
            // (the executor and the GPU lowering recurse per call).
            if !d.proc.same_object(p)
                || d.nstack > len
                || len + d.growth > MAX_SYMBOLIC_STACK
                || self.depth + self.sub_depth[d.id as usize] >= MAX_INLINE
            {
                continue;
            }
            let Some(kb) = self.key_of(&self.st, d.nstack, &d.ids) else {
                continue;
            };
            if kb.atoms == d.key {
                hit = Some((d.clone(), kb.regs));
                break;
            }
        }
        let Some((d, regs)) = hit else {
            return Ok(false);
        };
        // What the body reads, an enclosing shared body reads too.
        self.touch(len - d.nstack);
        for &id in &d.ids {
            self.touch_name(id);
        }
        for &(si, id) in &d.wids {
            let uid = self.st.scopes[si].uid;
            if let Some(f) = self.frames.last_mut() {
                f.wids.insert((uid, id));
                f.wnames.insert(id);
            }
        }
        if let Some(f) = self.frames.last_mut() {
            f.high = f.high.max(len + d.growth);
        }
        self.call_sub(&d, &regs)?;
        Ok(true)
    }

    /// The sharing key of `st` for a body that reads the top `nstack`
    /// entries and the names `ids` (sorted); `None` if it holds a value no
    /// subroutine may take.
    fn key_of(&self, st: &State, nstack: usize, ids: &[u32]) -> Option<KeyB> {
        let kinds = &|r: Reg| {
            u8::from(self.kinds.get(r as usize).copied().unwrap_or(false))
                | u8::from(self.seeded.contains(&r)) << 1
        };
        let mut kb = KeyB::default();
        for s in &st.scopes {
            match &s.kind {
                ScopeKind::Local => kb.atoms.push(Atom::T(30)),
                ScopeKind::Captured(d) => {
                    kb.atoms.push(Atom::T(31));
                    kb.n(Rc::as_ptr(d) as *const () as usize);
                }
            }
        }
        kb.n(nstack);
        for v in &st.stack[st.stack.len() - nstack..] {
            kb.sval(v, kinds)?;
        }
        for &id in ids {
            kb.n(id as usize);
            for s in &st.scopes {
                if let ScopeKind::Local = s.kind {
                    kb.binding(s.get(id), kinds)?;
                }
            }
        }
        Some(kb)
    }

    /// Compile `p` inline, recording what it reads; then make it a
    /// subroutine if it can be one.
    fn compile_shared(&mut self, pid: ProcId, p: &PsArray) -> CResult<()> {
        let len = self.st.stack.len();
        self.frames.push(Frame {
            entry: self.st.clone(),
            low: len,
            high: len,
            ids: HashSet::new(),
            wids: HashSet::new(),
            wnames: HashSet::new(),
            reg_base: self.nregs,
            loops: self.loops.len(),
            larrays: self.larrays.len(),
            code_start: self.code.len(),
            unshareable: false,
        });
        let r = self.exec_proc(p);
        let Some(f) = self.frames.pop() else {
            return nc("internal: sharing frame lost");
        };
        if let Some(parent) = self.frames.last_mut() {
            parent.low = parent.low.min(f.low);
            parent.high = parent.high.max(f.high);
            parent.ids.extend(f.ids.iter().copied());
            parent.wids.extend(f.wids.iter().copied());
            parent.wnames.extend(f.wnames.iter().copied());
        }
        r?;
        if !self.extract(p, pid, f)? {
            *self.share_fails.entry(pid).or_insert(0) += 1;
        }
        Ok(())
    }

    /// Is `r` a register from before the body of `f` (and not a
    /// subroutine's)?
    fn outer(&self, r: Reg, base: u32) -> bool {
        r < base && self.owner.get(r as usize).is_none_or(|&o| o == 0)
    }

    /// Move the code compiled for frame `f` into a subroutine and call it;
    /// false (the code left inline) if it cannot be shared.
    fn extract(&mut self, p: &PsArray, pid: ProcId, f: Frame) -> CResult<bool> {
        if f.unshareable {
            return Ok(false);
        }
        let x = &self.st;
        let e = &f.entry;
        let dead = x.dead;
        if !x.dead
            && (x.scopes.len() != e.scopes.len()
                || x.scopes.iter().zip(&e.scopes).any(|(a, b)| a.uid != b.uid)
                || x.stack.len() < f.low)
        {
            return Ok(false);
        }
        let mut ids: Vec<u32> = f.ids.iter().copied().collect();
        ids.sort_unstable();
        let mut extra: Vec<u32> = Vec::new();
        // What it leaves: the stack from `low` up, and the bindings it
        // changed (only names it touched).
        let mut out_stack = Vec::new();
        let mut binds = Vec::new();
        if !x.dead {
            out_stack = x.stack[f.low..].to_vec();
            for (si, (xs, es)) in x.scopes.iter().zip(&e.scopes).enumerate() {
                if xs.names.len() < es.names.len() {
                    return Ok(false);
                }
                for (nid, b) in &xs.names {
                    let defined = f.wids.contains(&(xs.uid, *nid));
                    if !defined && binding_eq(es.get(*nid), Some(b)) {
                        continue;
                    }
                    if !defined && ids.binary_search(nid).is_err() {
                        return Ok(false);
                    }
                    // A defined name left as the entry had it may hold that
                    // value only because the paths that defined it died (a
                    // join keeps the live side as it is): the value is then
                    // the caller's, so it goes in the key.
                    if defined && binding_eq(es.get(*nid), Some(b)) {
                        extra.push(*nid);
                    }
                    binds.push((si, *nid, b.clone()));
                }
            }
        }
        ids.extend(extra);
        ids.sort_unstable();
        ids.dedup();
        let nstack = e.stack.len() - f.low;
        let Some(kb) = self.key_of(e, nstack, &ids) else {
            return Ok(false);
        };
        let base = f.reg_base;
        // The body may read registers from outside only through its key,
        // and write none.
        let mut used = vec![false; kb.regs.len()];
        let mut ok = true;
        super::ir::walk(&self.code[f.code_start..], &mut |ins| {
            ins.regs(&mut |r, w| {
                if self.outer(r, base) {
                    match kb.idx.get(&r) {
                        Some(&k) if !w => used[k as usize] = true,
                        _ => ok = false,
                    }
                }
            });
        });
        if !ok {
            return Ok(false);
        }
        // Each register it leaves is an input or a result.
        let mut src: HashMap<Reg, Src> = HashMap::new();
        let mut results: Vec<Reg> = Vec::new();
        let mut ok = true;
        let mut plain_ok = true;
        {
            let mut see = |r: Reg| {
                if src.contains_key(&r) {
                    return;
                }
                if self.outer(r, base) {
                    match kb.idx.get(&r) {
                        Some(&k) => {
                            src.insert(r, Src::In(k));
                        }
                        None => ok = false,
                    }
                } else if self.owner.get(r as usize).is_some_and(|&o| o != 0) || r < base {
                    ok = false;
                } else {
                    src.insert(r, Src::Res(results.len() as u32));
                    results.push(r);
                }
            };
            let plain = |v: &SVal| match v {
                SVal::Tuple(..) | SVal::NewDict(_) => false,
                SVal::LArr(id, _) => (*id as usize) < f.larrays,
                _ => true,
            };
            for v in &out_stack {
                plain_ok &= plain(v);
                sval_regs(v, &mut see);
            }
            for (_, _, b) in &binds {
                if let Binding::Val(v) | Binding::Partial(v, _) = b {
                    plain_ok &= plain(v);
                }
                binding_regs(b, &mut see);
            }
        }
        if !ok || !plain_ok {
            return Ok(false);
        }
        // Move the code out, the inputs it reads renamed to parameters.
        let mut body = self.code.split_off(f.code_start);
        let mut pmap: HashMap<Reg, Reg> = HashMap::new();
        let mut params = Vec::new();
        for (k, &r) in kb.regs.iter().enumerate() {
            if used[k] {
                let pr = if self.kinds[r as usize] {
                    self.ireg()
                } else {
                    self.reg()
                };
                pmap.insert(r, pr);
                params.push((k as u32, pr));
            }
        }
        super::ir::walk_regs_mut(&mut body, &mut |r, _| {
            if let Some(p) = pmap.get(r) {
                *r = *p;
            }
        });
        let sid = self.subs.len() as u32;
        self.owner.resize(self.nregs as usize, 0);
        {
            let owner = &mut self.owner;
            super::ir::walk(&body, &mut |ins| {
                ins.regs(&mut |r, _| {
                    if owner[r as usize] == 0 {
                        owner[r as usize] = sid + 1;
                    }
                });
            });
            for &(_, r) in &params {
                owner[r as usize] = sid + 1;
            }
            for &r in &results {
                owner[r as usize] = sid + 1;
            }
        }
        let size = super::ir::count(&body);
        self.emitted = self.emitted.saturating_sub(size);
        self.sub_insns += size;
        let mut depth = 0;
        super::ir::walk(&body, &mut |i| {
            if let Ins::Call(c) = i {
                depth = depth.max(self.sub_depth[*c as usize]);
            }
        });
        self.sub_depth.push(depth + 1);
        self.subs.push(Sub {
            body,
            params: params.iter().map(|p| p.1).collect(),
            results: results.clone(),
        });
        let def = Rc::new(SubDef {
            proc: p.clone(),
            id: sid,
            nstack,
            ids,
            wids: f
                .wids
                .iter()
                .filter_map(|&(uid, id)| {
                    let si = e.scopes.iter().position(|s| s.uid == uid)?;
                    Some((si, id))
                })
                .collect(),
            key: kb.atoms,
            params,
            results,
            growth: f.high - e.stack.len(),
            dead,
            stack: out_stack,
            binds,
            src,
        });
        // Back to the state the body entered with, then the call.
        self.st = f.entry;
        self.call_sub(&def, &kb.regs)?;
        self.defs.entry(pid).or_default().push(def);
        Ok(true)
    }

    /// Call subroutine `d` with `site` as its inputs, and leave the state
    /// its body would have.
    fn call_sub(&mut self, d: &SubDef, site: &[Reg]) -> CResult<()> {
        for &(k, p) in &d.params {
            self.emit(Ins::Mov(p, Op::R(site[k as usize])))?;
        }
        self.emit(Ins::Call(d.id))?;
        let mut outs = Vec::with_capacity(d.results.len());
        for &r in &d.results {
            let t = if self.kinds[r as usize] {
                self.ireg()
            } else {
                self.reg()
            };
            self.emit(Ins::Mov(t, Op::R(r)))?;
            if self.seeded.contains(&r) {
                self.seeded.insert(t);
            }
            outs.push(t);
        }
        if d.dead {
            self.st.dead = true;
            return Ok(());
        }
        let m = |r: Reg| match d.src.get(&r) {
            Some(Src::In(k)) => site[*k as usize],
            Some(Src::Res(j)) => outs[*j as usize],
            None => r,
        };
        let len = self.st.stack.len();
        self.st.stack.truncate(len - d.nstack);
        for v in &d.stack {
            self.st.stack.push(map_sval(v, &m));
        }
        for (si, id, b) in &d.binds {
            let b = map_binding(b, &m);
            self.st.scopes[*si].set(*id, b);
        }
        Ok(())
    }

    /// `where`, decided at compile time: the dictionary defining `n` (a
    /// kernel scope is a `LocalDict`, usable only to `pop`).
    fn where_(&self, n: &PsName) -> CResult<Option<SVal>> {
        for s in self.st.scopes.iter().rev() {
            match &s.kind {
                ScopeKind::Local => match s.get(n.id()) {
                    Some(Binding::Val(_)) => return Ok(Some(SVal::LocalDict)),
                    Some(Binding::Maybe | Binding::Partial(..)) => {
                        return nc(format!("where on /{n}, defined on some paths only"));
                    }
                    None => {}
                },
                ScopeKind::Captured(d) => {
                    if d.borrow().get_id(n.id()).is_some() {
                        return Ok(Some(SVal::CapDict(d.clone())));
                    }
                }
            }
        }
        let key = Object::lit(Value::Name(n.clone()));
        Ok(self
            .it
            .find_defining_dict(&key)
            .map_err(|_| NotCompilable("where".into()))?
            .map(SVal::CapDict))
    }

    /// Look name `id` up from scope `top` down, then in the program's
    /// dictionary stack.
    fn lookup(&self, st: &State, id: u32, top: usize) -> CResult<Found> {
        for s in st.scopes[..=top].iter().rev() {
            match &s.kind {
                ScopeKind::Local => match s.get(id) {
                    Some(Binding::Val(v)) => return Ok(Found::Val(v.clone())),
                    Some(Binding::Partial(v, f)) => {
                        return Ok(Found::Partial(v.clone(), *f, s.uid));
                    }
                    Some(Binding::Maybe) => {
                        return nc(format!(
                            "/{} is defined on some paths only",
                            crate::name::resolve(id)
                        ));
                    }
                    None => {}
                },
                ScopeKind::Captured(d) => {
                    if let Some(o) = d.borrow().get_id(id) {
                        return Ok(Found::Obj(o));
                    }
                }
            }
        }
        Ok(match self.it.load_id(id) {
            Some(o) => Found::Obj(o),
            None => Found::None,
        })
    }

    /// The elements of a short array of numbers or booleans, if `v` is one.
    fn elems(&self, v: &SVal) -> Option<Vec<SVal>> {
        let items = match v {
            SVal::Tuple(t, _) => t.borrow().clone(),
            SVal::Cap(o) => match &o.value {
                Value::Array(a) if a.len() <= UNROLL_MAX => a
                    .snapshot()
                    .iter()
                    .map(|e| self.cap_to_sval(e).ok())
                    .collect::<Option<Vec<_>>>()?,
                _ => return None,
            },
            _ => return None,
        };
        items.iter().all(SVal::is_scalar).then_some(items)
    }

    /// What a slot holds in `st`.
    fn slot_val(&mut self, st: &State, slot: Slot) -> CResult<Lk> {
        match slot {
            Slot::Stack(i) => Ok(match st.stack.get(i) {
                Some(v) => Lk::Val(v.clone()),
                None => Lk::Undef,
            }),
            Slot::Bind(uid, id) => {
                let Some(top) = st.scopes.iter().position(|s| s.uid == uid) else {
                    return nc("a dictionary scope does not match between paths");
                };
                for s in st.scopes[..=top].iter().rev() {
                    match &s.kind {
                        ScopeKind::Local => match s.get(id) {
                            Some(Binding::Val(v)) => return Ok(Lk::Val(v.clone())),
                            Some(Binding::Maybe) => return Ok(Lk::Maybe),
                            Some(Binding::Partial(v, f)) => return Ok(Lk::Partial(v.clone(), *f)),
                            None => {}
                        },
                        ScopeKind::Captured(d) => {
                            if let Some(o) = d.borrow().get_id(id) {
                                return Ok(match self.cap_to_sval(&o) {
                                    Ok(v) => Lk::Val(v),
                                    Err(_) => Lk::Maybe,
                                });
                            }
                        }
                    }
                }
                Ok(match self.it.load_id(id) {
                    Some(o) => match self.cap_to_sval(&o) {
                        Ok(v) => Lk::Val(v),
                        Err(_) => Lk::Maybe,
                    },
                    None => Lk::Undef,
                })
            }
        }
    }

    // --- joins ------------------------------------------------------------

    /// The slots to compare across `states` (same scope structure).
    fn slots_of(states: &[&State]) -> CResult<Vec<Slot>> {
        let first = states[0];
        let mut out: Vec<Slot> = (0..first.stack.len()).map(Slot::Stack).collect();
        for st in states {
            if st.stack.len() != first.stack.len() {
                return nc("paths leave different stack depths");
            }
            if st.scopes.len() != first.scopes.len()
                || st
                    .scopes
                    .iter()
                    .zip(&first.scopes)
                    .any(|(a, b)| a.uid != b.uid)
            {
                return nc("paths leave different dictionaries begun");
            }
            for s in &st.scopes {
                for (id, _) in &s.names {
                    let sl = Slot::Bind(s.uid, *id);
                    if !out.contains(&sl) {
                        out.push(sl);
                    }
                }
            }
        }
        Ok(out)
    }

    fn set_slot(st: &mut State, slot: Slot, v: Binding) {
        match slot {
            Slot::Stack(i) => {
                if let Binding::Val(v) = v {
                    st.stack[i] = v;
                }
            }
            Slot::Bind(uid, id) => {
                if let Some(s) = st.scopes.iter_mut().find(|s| s.uid == uid) {
                    s.set(id, v);
                }
            }
        }
    }

    /// Moves putting `v` into a merge/carried slot of type `ty`.
    fn moves_into(
        &mut self,
        reg: Reg,
        tag: Option<Reg>,
        ty: CT,
        v: &SVal,
        out: &mut Moves,
    ) -> CResult<()> {
        match (v, ty) {
            (SVal::Num(o, nt), CT::Int) if *nt == NT::Int => out.pairs.push((reg, *o, true)),
            (SVal::Num(o, NT::Int), CT::Dyn) => {
                // An integer joining values that are real on other paths:
                // held exactly as an f64, so it must be within 2^53.
                let o = match o {
                    Op::R(_) => {
                        let t = self.reg();
                        out.pre.push(Ins::I2FExact(t, *o));
                        Op::R(t)
                    }
                    Op::Imm(_) => *o,
                };
                out.pairs.push((reg, o, false));
                out.pairs.push((tag.unwrap_or(reg), Op::Imm(1.0), false));
            }
            (SVal::Num(o, nt), CT::Real | CT::Dyn) if *nt != NT::Int => {
                out.pairs.push((reg, *o, false));
                if ty == CT::Dyn {
                    out.pairs.push((tag.unwrap_or(reg), nt.tag(), false));
                }
            }
            (SVal::Bool(o), CT::Bool) => out.pairs.push((reg, *o, false)),
            (v, ty) => {
                return nc(format!(
                    "a value changes type between paths ({} into {ty:?})",
                    describe(v)
                ));
            }
        }
        Ok(())
    }

    /// Emit `moves` as a parallel assignment (after its conversions).
    fn par_moves(&mut self, moves: Moves, code: &mut Vec<Ins>) -> CResult<()> {
        let before = code.len();
        code.extend(moves.pre);
        let mv = |d: Reg, s: Op, int: bool| match (s, int) {
            (Op::Imm(v), true) => Ins::MovI(d, v as i64),
            _ => Ins::Mov(d, s),
        };
        let pairs: Vec<_> = moves
            .pairs
            .into_iter()
            .filter(|(d, s, _)| *s != Op::R(*d))
            .collect();
        let hazard = pairs
            .iter()
            .any(|(_, s, _)| pairs.iter().any(|(d, _, _)| s.uses(*d)));
        if hazard {
            let mut tmps = Vec::new();
            for (_, s, int) in &pairs {
                let t = if *int { self.ireg() } else { self.reg() };
                code.push(mv(t, *s, *int));
                tmps.push(t);
            }
            for ((d, _, _), t) in pairs.iter().zip(tmps) {
                code.push(Ins::Mov(*d, Op::R(t)));
            }
        } else {
            for (d, s, int) in pairs {
                code.push(mv(d, s, int));
            }
        }
        self.emitted += code.len() - before;
        if self.emitted + self.sub_insns > MAX_INSNS {
            return nc(TOO_LARGE);
        }
        Ok(())
    }

    /// Join the states at the end of an `if`'s two branches.
    fn join2(
        &mut self,
        a: State,
        mut ca: Vec<Ins>,
        b: State,
        mut cb: Vec<Ins>,
    ) -> CResult<(State, Vec<Ins>, Vec<Ins>)> {
        if a.dead {
            return Ok((b, ca, cb));
        }
        if b.dead {
            return Ok((a, ca, cb));
        }
        let slots = Self::slots_of(&[&a, &b])?;
        let mut out = a.clone();
        let (mut ma, mut mb) = (Moves::default(), Moves::default());
        for sl in slots {
            let (va, vb) = (self.slot_val(&a, sl)?, self.slot_val(&b, sl)?);
            if va == vb {
                self.touch_equal_join(sl, &va)?;
                continue;
            }
            self.touch_slot(sl);
            let merged = match (&va, &vb) {
                (Lk::Val(x), Lk::Val(y)) if x.is_scalar() && y.is_scalar() => {
                    let (Some(tx), Some(ty)) = (ct_of(x), ct_of(y)) else {
                        unreachable!("scalars have a type")
                    };
                    match ct_join(tx, ty) {
                        Some(t) => {
                            let reg = self.reg_for(t);
                            let tag = (t == CT::Dyn).then(|| self.reg());
                            self.moves_into(reg, tag, t, x, &mut ma)?;
                            self.moves_into(reg, tag, t, y, &mut mb)?;
                            Binding::Val(
                                Carried {
                                    slot: sl,
                                    reg,
                                    tag,
                                    ty: t,
                                }
                                .sval(),
                            )
                        }
                        None => Binding::Maybe,
                    }
                }
                (Lk::Val(x), Lk::Val(y)) => match (self.elems(x), self.elems(y)) {
                    // Two arrays of numbers of one length (a colour
                    // chosen by a branch): merged element by element.
                    (Some(ex), Some(ey)) if ex.len() == ey.len() => {
                        let mut out = Vec::with_capacity(ex.len());
                        let mut ok = true;
                        for (a, b) in ex.iter().zip(&ey) {
                            if a == b {
                                out.push(a.clone());
                                continue;
                            }
                            let t = match (ct_of(a), ct_of(b)) {
                                (Some(ta), Some(tb)) => ct_join(ta, tb),
                                _ => None,
                            };
                            let Some(t) = t else {
                                ok = false;
                                break;
                            };
                            let reg = self.reg_for(t);
                            let tag = (t == CT::Dyn).then(|| self.reg());
                            self.moves_into(reg, tag, t, a, &mut ma)?;
                            self.moves_into(reg, tag, t, b, &mut mb)?;
                            out.push(
                                Carried {
                                    slot: sl,
                                    reg,
                                    tag,
                                    ty: t,
                                }
                                .sval(),
                            );
                        }
                        if ok {
                            // The merge is a new array: neither it nor the
                            // arrays merged may be changed later, as an
                            // alias of either would not see the change.
                            let merged = Rc::new(RefCell::new(out));
                            for v in [x, y] {
                                if let SVal::Tuple(t, _) = v
                                    && self.frozen.insert(Rc::as_ptr(t) as usize)
                                {
                                    self.frozen_keep.push(Rc::downgrade(t));
                                }
                            }
                            self.frozen.insert(Rc::as_ptr(&merged) as usize);
                            self.frozen_keep.push(Rc::downgrade(&merged));
                            Binding::Val(SVal::Tuple(merged, self.block))
                        } else {
                            Binding::Maybe
                        }
                    }
                    _ => Binding::Maybe,
                },
                _ if matches!(sl, Slot::Bind(..)) => {
                    self.merge_partial(&va, &vb, &mut ma, &mut mb)?
                }
                _ => Binding::Maybe,
            };
            if let (Slot::Stack(_), Binding::Maybe) = (sl, &merged) {
                return nc("a stack entry differs between the branches of an if");
            }
            Self::set_slot(&mut out, sl, merged);
        }
        self.par_moves(ma, &mut ca)?;
        self.par_moves(mb, &mut cb)?;
        Ok((out, ca, cb))
    }

    /// Merge a name defined (as a number or boolean) on one path or both
    /// into a partly defined binding with a flag register.
    fn merge_partial(
        &mut self,
        a: &Lk,
        b: &Lk,
        ma: &mut Moves,
        mb: &mut Moves,
    ) -> CResult<Binding> {
        let side = |lk: &Lk| -> (Option<SVal>, Op) {
            match lk {
                Lk::Val(x) if x.is_scalar() => (Some(x.clone()), Op::Imm(1.0)),
                Lk::Partial(v, f) => (Some(v.clone()), Op::R(*f)),
                _ => (None, Op::Imm(0.0)),
            }
        };
        let ((xa, fa), (xb, fb)) = (side(a), side(b));
        let ty = match (xa.as_ref().and_then(ct_of), xb.as_ref().and_then(ct_of)) {
            (Some(x), Some(y)) => ct_join(x, y),
            (Some(x), None) | (None, Some(x)) => Some(x),
            (None, None) => None,
        };
        let Some(t) = ty else {
            return Ok(Binding::Maybe);
        };
        let reg = self.reg_for(t);
        let tag = (t == CT::Dyn).then(|| self.reg());
        let flag = self.reg();
        if let Some(x) = &xa {
            self.moves_into(reg, tag, t, x, ma)?;
        }
        if let Some(x) = &xb {
            self.moves_into(reg, tag, t, x, mb)?;
        }
        ma.pairs.push((flag, fa, false));
        mb.pairs.push((flag, fb, false));
        let v = Carried {
            slot: Slot::Stack(0),
            reg,
            tag,
            ty: t,
        }
        .sval();
        Ok(Binding::Partial(v, flag))
    }

    // --- control flow -----------------------------------------------------

    fn fresh_block(&mut self) -> u32 {
        self.next_block += 1;
        self.next_block
    }

    fn compile_if(&mut self, c: Op, t: PsArray, e: Option<PsArray>) -> CResult<()> {
        let c = match c {
            Op::Imm(v) => {
                if v != 0.0 {
                    return self.exec_proc(&t);
                }
                return match e {
                    Some(e) => self.exec_proc(&e),
                    None => Ok(()),
                };
            }
            Op::R(r) => r,
        };
        let entry = self.st.clone();
        let saved = std::mem::take(&mut self.code);
        let bid = self.block;
        // Compile both branches; whatever happens, put the enclosing
        // block's code and id back (a failed attempt may be rolled back
        // and retried, see `loop_or_unroll`).
        let branches = (|| {
            self.block = self.fresh_block();
            self.branch(&t)?;
            let tcode = std::mem::take(&mut self.code);
            let tstate = std::mem::replace(&mut self.st, entry);
            self.block = self.fresh_block();
            if let Some(e) = e {
                self.branch(&e)?;
            }
            let ecode = std::mem::take(&mut self.code);
            let estate = std::mem::replace(
                &mut self.st,
                State {
                    stack: Vec::new(),
                    scopes: Vec::new(),
                    dead: true,
                },
            );
            Ok((tcode, tstate, ecode, estate))
        })();
        self.block = bid;
        self.code = saved;
        let (tcode, tstate, ecode, estate) = branches?;
        let both_dead = tstate.dead && estate.dead;
        let (mut st, tcode, ecode) = self.join2(tstate, tcode, estate, ecode)?;
        st.dead = both_dead;
        self.emit(Ins::If(c, tcode, ecode))?;
        self.st = st;
        Ok(())
    }

    /// One branch of a run-time `if`. A branch that does not compile
    /// becomes an abort: lanes that take it run in the interpreter.
    fn branch(&mut self, p: &PsArray) -> CResult<()> {
        let cp = self.checkpoint();
        match self.exec_proc(p) {
            Ok(()) => Ok(()),
            Err(NotCompilable(why))
                if !self.strict && why != EXIT_UNROLLED && self.limit.is_none() =>
            {
                let t = &self.trail;
                let at = t[t.len().saturating_sub(3)..].join(" > ");
                self.rollback(cp);
                self.soft.push(if at.is_empty() {
                    why
                } else {
                    format!("{why}, in {at}")
                });
                self.abort_path()
            }
            Err(e) => Err(e),
        }
    }

    /// The state with each carried slot reading its register.
    fn with_carried(entry: &State, carried: &[Carried], maybe: &[Slot]) -> State {
        let mut st = entry.clone();
        for c in carried {
            Self::set_slot(&mut st, c.slot, Binding::Val(c.sval()));
        }
        // Names a loop defines with nothing usable before it: undefined on
        // the first pass round, so a read before the body's own def does not
        // compile (and neither does a read after the loop).
        for sl in maybe {
            Self::set_slot(&mut st, *sl, Binding::Maybe);
        }
        st
    }

    /// `kind` runs before the body each iteration; see [`LoopHead`].
    fn compile_loop(&mut self, head: LoopHead, body: PsArray) -> CResult<()> {
        let entry = self.st.clone();
        let mut carried: Vec<Carried> = Vec::new();
        let mut maybe: Vec<Slot> = Vec::new();
        for _pass in 0..12 {
            let hstate = Self::with_carried(&entry, &carried, &maybe);
            self.st = hstate.clone();
            self.loops.push(LoopCtx {
                carried: carried.clone(),
                exits: Vec::new(),
                depth: hstate.stack.len(),
                unrolled: false,
                mismatch: false,
            });
            let saved = std::mem::take(&mut self.code);
            let saved_emitted = self.emitted;
            let bid = self.block;
            self.block = self.fresh_block();
            let r = self.loop_prefix(&head).and_then(|_| self.exec_proc(&body));
            let ctx = self.loops.pop();
            let mut bcode = std::mem::replace(&mut self.code, saved);
            self.block = bid;
            r?;
            let Some(ctx) = ctx else {
                return nc("internal: loop context lost");
            };
            let end = std::mem::replace(
                &mut self.st,
                State {
                    stack: Vec::new(),
                    scopes: Vec::new(),
                    dead: true,
                },
            );
            // Which slots change across the back edge or at an exit?
            let mut states: Vec<&State> = vec![&hstate];
            if !end.dead {
                states.push(&end);
            }
            states.extend(ctx.exits.iter());
            let slots = Self::slots_of(&states)?;
            let mut grew = false;
            for sl in slots {
                if maybe.contains(&sl) {
                    continue;
                }
                let hv = self.slot_val(&hstate, sl)?;
                let ev = self.slot_val(&entry, sl)?;
                for s in &states[1..] {
                    let sv = self.slot_val(s, sl)?;
                    if sv == hv {
                        self.touch_equal_join(sl, &hv)?;
                        continue;
                    }
                    self.touch_slot(sl);
                    let joined = match (&ev, &sv) {
                        (Lk::Val(x), Lk::Val(y)) => match (ct_of(x), ct_of(y)) {
                            (Some(tx), Some(ty)) => ct_join(tx, ty),
                            _ => None,
                        },
                        _ => None,
                    };
                    let Some(t) = joined else {
                        if let Slot::Bind(..) = sl {
                            maybe.push(sl);
                            carried.retain(|c| c.slot != sl);
                            grew = true;
                            break;
                        }
                        return nc("a loop changes the stack in a way that cannot be compiled");
                    };
                    match carried.iter_mut().find(|c| c.slot == sl) {
                        Some(c) => {
                            if let Some(w) = ct_join(c.ty, t)
                                && w != c.ty
                            {
                                c.ty = w;
                                // Widened (Int to Dyn): an F64 register now.
                                c.reg = self.reg_for(w);
                                if w == CT::Dyn && c.tag.is_none() {
                                    c.tag = Some(self.reg());
                                }
                                grew = true;
                            } else if ct_join(c.ty, t).is_none() {
                                return nc("a loop changes a value's type");
                            }
                        }
                        None => {
                            let reg = self.reg_for(t);
                            let tag = (t == CT::Dyn).then(|| self.reg());
                            carried.push(Carried {
                                slot: sl,
                                reg,
                                tag,
                                ty: t,
                            });
                            grew = true;
                        }
                    }
                }
            }
            if grew {
                self.emitted = saved_emitted;
                continue;
            }
            if ctx.mismatch {
                return nc("a value at a loop's exit does not fit the loop's type for it");
            }
            // Final pass: entry moves, the loop, and the state after it.
            let mut pre = Moves::default();
            for c in &carried {
                let Lk::Val(v) = self.slot_val(&entry, c.slot)? else {
                    return nc("a loop-carried value has no starting value");
                };
                self.moves_into(c.reg, c.tag, c.ty, &v, &mut pre)?;
            }
            // (`par_moves` counts what it adds: append, not `emit`.)
            let mut pcode = Vec::new();
            self.par_moves(pre, &mut pcode)?;
            self.code.extend(pcode);
            if !end.dead {
                let mut back = Moves::default();
                for c in &carried {
                    let Lk::Val(v) = self.slot_val(&end, c.slot)? else {
                        return nc("a loop-carried value is lost");
                    };
                    self.moves_into(c.reg, c.tag, c.ty, &v, &mut back)?;
                }
                self.par_moves(back, &mut bcode)?;
                if let LoopHead::For { c, inc, .. } = &head {
                    bcode.push(Ins::Bin(BinOp::Add, *c, Op::R(*c), *inc));
                }
            }
            self.emit(Ins::Loop(bcode))?;
            let mut post = Self::with_carried(&entry, &carried, &maybe);
            // A `loop` ends only by `exit`; with none, nothing follows.
            post.dead = matches!(head, LoopHead::Loop) && ctx.exits.is_empty();
            self.st = post;
            return Ok(());
        }
        nc("a loop's types did not settle")
    }

    fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            st: self.st.clone(),
            block: self.block,
            code: self.code.len(),
            emitted: self.emitted,
            trail: self.trail.len(),
            loops: self.loops.len(),
            exits: self.loops.last().map_or(0, |l| l.exits.len()),
            frames: self.frames.len(),
        }
    }

    fn rollback(&mut self, cp: Checkpoint) {
        self.st = cp.st;
        self.block = cp.block;
        self.code.truncate(cp.code);
        self.emitted = cp.emitted;
        self.trail.truncate(cp.trail);
        self.loops.truncate(cp.loops);
        if let Some(l) = self.loops.last_mut() {
            l.exits.truncate(cp.exits);
        }
        // A sharing frame is popped by the call that pushed it, error or
        // not, so none is open past a checkpoint taken outside it.
        debug_assert_eq!(self.frames.len(), cp.frames);
        self.frames.truncate(cp.frames);
    }

    /// A counted loop: unrolled when its trips are known and few (the
    /// counter is then a constant, so table reads with it fold away), or
    /// when only unrolling can compile it (a body that changes the stack
    /// depth, like `0 1 2 { ... } for` making three values); otherwise a
    /// real loop (`real`). `trips` holds what each trip pushes first.
    fn loop_or_unroll(
        &mut self,
        trips: Option<Vec<Option<SVal>>>,
        body: PsArray,
        real: impl FnOnce(&mut Self, PsArray) -> CResult<()>,
    ) -> CResult<()> {
        if let Some(t) = &trips
            && t.len() <= UNROLL_SMALL
        {
            let cp = self.checkpoint();
            match self.unrolled(t, &body) {
                Ok(()) => return Ok(()),
                Err(NotCompilable(why)) if why == EXIT_UNROLLED && self.limit.is_none() => {
                    self.rollback(cp)
                }
                Err(e) => return Err(e),
            }
            return real(self, body);
        }
        let cp = self.checkpoint();
        match real(self, body.clone()) {
            Ok(()) => Ok(()),
            Err(e) => {
                let Some(t) = trips else { return Err(e) };
                if self.limit.is_some() {
                    return Err(e);
                }
                self.rollback(cp);
                match self.unrolled(&t, &body) {
                    Ok(()) => Ok(()),
                    // Report the loop's own reason, not the unroller's.
                    Err(_) => Err(e),
                }
            }
        }
    }

    fn unrolled(&mut self, trips: &[Option<SVal>], body: &PsArray) -> CResult<()> {
        self.loops.push(LoopCtx {
            carried: Vec::new(),
            exits: Vec::new(),
            depth: 0,
            unrolled: true,
            mismatch: false,
        });
        let mut r = Ok(());
        for v in trips {
            if self.st.dead {
                break;
            }
            if let Some(v) = v {
                self.push(v.clone());
            }
            r = self.exec_proc(body);
            if r.is_err() {
                break;
            }
        }
        self.loops.pop();
        r
    }

    fn loop_prefix(&mut self, head: &LoopHead) -> CResult<()> {
        match head {
            LoopHead::For { c, ct, inc, lim } => {
                let fin = self.reg();
                self.emit(Ins::ForFin(fin, Op::R(*c), *inc, *lim))?;
                self.emit(Ins::If(fin, vec![Ins::Break], Vec::new()))?;
                // The counter steps as an f64, as `ForControl` does; an
                // integer loop pushes it as an integer.
                if *ct == NT::Int {
                    let v = self.ireg();
                    self.emit(Ins::F2I(v, Op::R(*c)))?;
                    self.push(SVal::Num(Op::R(v), *ct));
                } else {
                    let v = self.reg();
                    self.emit(Ins::Mov(v, Op::R(*c)))?;
                    self.push(SVal::Num(Op::R(v), *ct));
                }
            }
            LoopHead::Repeat { k } => {
                let fin = self.reg();
                self.emit(Ins::Bin(BinOp::Le, fin, Op::R(*k), Op::Imm(0.0)))?;
                self.emit(Ins::If(fin, vec![Ins::Break], Vec::new()))?;
                self.emit(Ins::Bin(BinOp::Sub, *k, Op::R(*k), Op::Imm(1.0)))?;
            }
            LoopHead::Loop => {}
        }
        Ok(())
    }

    fn compile_exit(&mut self) -> CResult<()> {
        let Some(ctx) = self.loops.last() else {
            return nc("exit outside a loop in the kernel");
        };
        if ctx.unrolled {
            return nc(EXIT_UNROLLED);
        }
        // An exit from a loop around a shared body: that body cannot be a
        // subroutine (a `Break` in one would leave the caller's loop).
        let nloops = self.loops.len();
        for f in &mut self.frames {
            if f.loops >= nloops {
                f.unshareable = true;
            }
        }
        if self.st.stack.len() != ctx.depth {
            return nc("exit leaves a different stack depth than the loop started with");
        }
        let carried = ctx.carried.clone();
        let snapshot = self.st.clone();
        let mut mv = Moves::default();
        let mut fits = true;
        for c in &carried {
            let Lk::Val(v) = self.slot_val(&snapshot, c.slot)? else {
                return nc("a loop-carried value is undefined at exit");
            };
            if self.moves_into(c.reg, c.tag, c.ty, &v, &mut mv).is_err() {
                fits = false;
                break;
            }
        }
        if fits {
            let mut code = Vec::new();
            self.par_moves(mv, &mut code)?;
            self.code.extend(code);
        } else if let Some(ctx) = self.loops.last_mut() {
            ctx.mismatch = true;
        }
        self.emit(Ins::Break)?;
        if let Some(ctx) = self.loops.last_mut() {
            ctx.exits.push(snapshot);
        }
        self.st.dead = true;
        Ok(())
    }

    // --- arithmetic -------------------------------------------------------

    fn push_num(&mut self, o: Op, t: NT) {
        self.push(SVal::Num(o, t));
    }

    /// add/sub/mul/min/max: integer when both are, real otherwise.
    fn arith(&mut self, op: BinOp) -> CResult<()> {
        let (b, bt) = self.pop_num()?;
        let (a, at) = self.pop_num()?;
        if (at, bt) == (NT::Int, NT::Int) {
            let iop = match op {
                BinOp::Add => IBinOp::Add,
                BinOp::Sub => IBinOp::Sub,
                BinOp::Mul => IBinOp::Mul,
                BinOp::Min => IBinOp::Min,
                _ => IBinOp::Max,
            };
            return self.ibin(iop, a, b);
        }
        let rt = match (at, bt) {
            (NT::Real, _) | (_, NT::Real) => NT::Real,
            _ => NT::Dyn(0),
        };
        // Into a Dyn result an integer must stay exact (2^53).
        let (a, b) = if matches!(rt, NT::Dyn(_)) {
            (self.as_exact_f(a, at)?, self.as_exact_f(b, bt)?)
        } else {
            (self.as_f(a, at)?, self.as_f(b, bt)?)
        };
        if let (Op::Imm(x), Op::Imm(y), NT::Real) = (a, b, rt) {
            let Some(r) = lane_bin(op, x, y) else {
                return self.abort_path();
            };
            self.push_num(Op::Imm(r), rt);
            return Ok(());
        }
        let d = self.reg();
        self.emit(Ins::Bin(op, d, a, b))?;
        let rt = match rt {
            NT::Dyn(_) => {
                let t = self.reg();
                self.emit(Ins::Bin(BinOp::TagAnd, t, at.tag(), bt.tag()))?;
                // Integer lanes of a Dyn value are exact f64s.
                if !matches!(op, BinOp::Min | BinOp::Max) {
                    self.emit(Ins::CheckInt(Op::R(d), Op::R(t)))?;
                }
                NT::Dyn(t)
            }
            x => x,
        };
        self.push_num(Op::R(d), rt);
        Ok(())
    }

    /// A binary op with a fixed result type and no integer overflow.
    fn bin_to(&mut self, op: BinOp, a: Op, b: Op, out: fn(Op) -> SVal) -> CResult<()> {
        // `false x and` is false, `true x or` is true, and the other
        // constant leaves `x`: a constant guard prunes the branch it gates.
        if matches!(op, BinOp::BAnd | BinOp::BOr) {
            let short = if op == BinOp::BAnd { 0.0 } else { 1.0 };
            match (a, b) {
                (Op::Imm(k), x) | (x, Op::Imm(k)) if !matches!(x, Op::Imm(_)) => {
                    let kb = if k != 0.0 { 1.0 } else { 0.0 };
                    self.push(out(if kb == short { Op::Imm(short) } else { x }));
                    return Ok(());
                }
                _ => {}
            }
        }
        if let (Op::Imm(x), Op::Imm(y)) = (a, b) {
            return match lane_bin(op, x, y) {
                Some(r) => {
                    self.push(out(Op::Imm(r)));
                    Ok(())
                }
                None => self.abort_path(),
            };
        }
        let d = self.reg();
        self.emit(Ins::Bin(op, d, a, b))?;
        self.push(out(Op::R(d)));
        Ok(())
    }

    fn un_to(&mut self, op: UnOp, a: Op, out: impl Fn(Op) -> SVal) -> CResult<()> {
        if let Op::Imm(x) = a {
            return match lane_un(op, x) {
                Some(r) => {
                    self.push(out(Op::Imm(r)));
                    Ok(())
                }
                None => self.abort_path(),
            };
        }
        let d = self.reg();
        self.emit(Ins::Un(op, d, a))?;
        self.push(out(Op::R(d)));
        Ok(())
    }

    /// Require an integer: `false` when the path certainly errors.
    fn need_int(&mut self, t: NT) -> CResult<bool> {
        match t {
            NT::Int => Ok(true),
            NT::Real => {
                self.abort_path()?;
                Ok(false)
            }
            NT::Dyn(r) => {
                self.emit(Ins::NeedInt(Op::R(r)))?;
                Ok(true)
            }
        }
    }

    // --- operators --------------------------------------------------------

    fn apply(&mut self, op: &Operator) -> CResult<()> {
        let name = op.name;
        let real = |o: Op| SVal::Num(o, NT::Real);
        let boolean = SVal::Bool;
        match name {
            // stack
            "pop" => {
                self.pop()?;
            }
            "exch" => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(b);
                self.push(a);
            }
            "dup" => {
                let a = self.pop()?;
                self.push(a.clone());
                self.push(a);
            }
            "copy" => {
                let n = self.pop_const_int("copy's count")?;
                let len = self.st.stack.len();
                if n < 0 || n as usize > len {
                    return nc("copy past the kernel's stack");
                }
                let from = len - n as usize;
                self.touch(from);
                for i in from..len {
                    self.charge()?;
                    let v = self.st.stack[i].clone();
                    self.push(v);
                }
            }
            "index" => {
                let n = self.pop_const_int("index's operand")?;
                let len = self.st.stack.len();
                if n < 0 || n as usize >= len {
                    return nc("index past the kernel's stack");
                }
                self.touch(len - 1 - n as usize);
                let v = self.st.stack[len - 1 - n as usize].clone();
                self.push(v);
            }
            "roll" => {
                let j = self.pop_const_int("roll's shift")?;
                let n = self.pop_const_int("roll's count")?;
                let len = self.st.stack.len();
                if n < 0 || n as usize > len {
                    return nc("roll past the kernel's stack");
                }
                self.touch(len - n as usize);
                if n > 0 {
                    let s = &mut self.st.stack[len - n as usize..];
                    let k = j.rem_euclid(n) as usize;
                    s.rotate_right(k);
                }
            }
            // `count` would see the operands below the procedure's own (the
            // caller's, and the driver's), which the kernel does not have.
            "count" => return nc("count (it sees the whole operand stack)"),
            "mark" => self.push(SVal::Mark),
            "counttomark" => {
                let found = self.st.stack.iter().rposition(|v| *v == SVal::Mark);
                self.touch(found.unwrap_or(0));
                let Some(p) = found else {
                    return nc("counttomark with no mark");
                };
                let n = (self.st.stack.len() - p - 1) as f64;
                self.push_num(Op::Imm(n), NT::Int);
            }
            "cleartomark" => {
                let found = self.st.stack.iter().rposition(|v| *v == SVal::Mark);
                self.touch(found.unwrap_or(0));
                let Some(p) = found else {
                    return nc("cleartomark with no mark");
                };
                self.st.stack.truncate(p);
            }
            "]" => {
                let found = self.st.stack.iter().rposition(|v| *v == SVal::Mark);
                self.touch(found.unwrap_or(0));
                let Some(p) = found else {
                    return nc("] with no mark");
                };
                let items = self.st.stack.split_off(p + 1);
                self.st.stack.pop();
                let b = self.block;
                self.push(SVal::Tuple(Rc::new(RefCell::new(items)), b));
            }
            // arithmetic
            "add" => self.arith(BinOp::Add)?,
            "sub" => self.arith(BinOp::Sub)?,
            "mul" => self.arith(BinOp::Mul)?,
            "min" | ".min" => self.arith(BinOp::Min)?,
            "max" | ".max" => self.arith(BinOp::Max)?,
            "div" => {
                let (b, bt) = self.pop_num()?;
                let (a, at) = self.pop_num()?;
                let (a, b) = (self.as_f(a, at)?, self.as_f(b, bt)?);
                self.bin_to(BinOp::Div, a, b, real)?;
            }
            "idiv" | "mod" => {
                let (b, bt) = self.pop_num()?;
                let (a, at) = self.pop_num()?;
                if let Some(a) = self.as_i(a, at)?
                    && let Some(b) = self.as_i(b, bt)?
                {
                    let op = if name == "idiv" {
                        IBinOp::Idiv
                    } else {
                        IBinOp::Mod
                    };
                    self.ibin(op, a, b)?;
                }
            }
            "neg" | "abs" | "ceiling" | "floor" | "round" | "truncate" => {
                let (a, t) = self.pop_num()?;
                let op = match name {
                    "neg" => UnOp::Neg,
                    "abs" => UnOp::Abs,
                    "ceiling" => UnOp::Ceil,
                    "floor" => UnOp::Floor,
                    "round" => UnOp::Round,
                    _ => UnOp::Trunc,
                };
                if t == NT::Int {
                    // An integer is its own rounding; neg and abs are exact.
                    match op {
                        UnOp::Neg => self.iun(IUnOp::Neg, a)?,
                        UnOp::Abs => self.iun(IUnOp::Abs, a)?,
                        _ => self.push_num(a, t),
                    }
                } else {
                    self.un_to(op, a, move |o| SVal::Num(o, t))?;
                }
            }
            "sqrt" | "sin" | "cos" | "ln" | "log" => {
                let (a, at) = self.pop_num()?;
                let a = self.as_f(a, at)?;
                let op = match name {
                    "sqrt" => UnOp::Sqrt,
                    "sin" => UnOp::Sin,
                    "cos" => UnOp::Cos,
                    "ln" => UnOp::Ln,
                    _ => UnOp::Log,
                };
                self.un_to(op, a, real)?;
            }
            "atan" | "exp" => {
                let (b, bt) = self.pop_num()?;
                let (a, at) = self.pop_num()?;
                let (a, b) = (self.as_f(a, at)?, self.as_f(b, bt)?);
                let op = if name == "atan" {
                    BinOp::Atan
                } else {
                    BinOp::Pow
                };
                self.bin_to(op, a, b, real)?;
            }
            "cvi" => {
                let (a, t) = self.pop_num()?;
                match (t, a) {
                    (NT::Int, _) => self.push_num(a, t),
                    (_, Op::Imm(x)) => match real_to_int(x) {
                        Some(v) => self.push_int(v)?,
                        None => self.abort_path()?,
                    },
                    _ => {
                        let d = self.ireg();
                        self.emit(Ins::F2I(d, a))?;
                        self.push_num(Op::R(d), NT::Int);
                    }
                }
            }
            "cvr" => {
                let (a, t) = self.pop_num()?;
                let a = self.as_f(a, t)?;
                self.push_num(a, NT::Real);
            }
            // relational and logic
            "lt" | "le" | "gt" | "ge" => {
                let (b, bt) = self.pop_num()?;
                let (a, at) = self.pop_num()?;
                if (at, bt) == (NT::Int, NT::Int) {
                    let op = match name {
                        "lt" => IBinOp::Lt,
                        "le" => IBinOp::Le,
                        "gt" => IBinOp::Gt,
                        _ => IBinOp::Ge,
                    };
                    self.ibin(op, a, b)?;
                } else {
                    let op = match name {
                        "lt" => BinOp::Lt,
                        "le" => BinOp::Le,
                        "gt" => BinOp::Gt,
                        _ => BinOp::Ge,
                    };
                    let (a, b) = self.cmp_operands(a, at, b, bt)?;
                    self.bin_to(op, a, b, boolean)?;
                }
            }
            "eq" | "ne" => {
                let b = self.pop()?;
                let a = self.pop()?;
                let op = if name == "eq" { BinOp::Eq } else { BinOp::Ne };
                match (&a, &b) {
                    (SVal::Num(x, NT::Int), SVal::Num(y, NT::Int)) => {
                        let iop = if name == "eq" { IBinOp::Eq } else { IBinOp::Ne };
                        self.ibin(iop, *x, *y)?;
                    }
                    (SVal::Num(x, xt), SVal::Num(y, yt)) => {
                        let (x, y) = self.cmp_operands(*x, *xt, *y, *yt)?;
                        self.bin_to(op, x, y, boolean)?;
                    }
                    (SVal::Bool(x), SVal::Bool(y)) => {
                        self.bin_to(op, *x, *y, boolean)?;
                    }
                    _ => {
                        // Not two run-time numbers or booleans: decided now.
                        let Some(e) = const_eq(&a, &b) else {
                            return nc(format!("{name} on {} and {}", describe(&a), describe(&b)));
                        };
                        let v = f64::from(u8::from(e == (name == "eq")));
                        self.push(SVal::Bool(Op::Imm(v)));
                    }
                }
            }
            "and" | "or" | "xor" => {
                let b = self.pop()?;
                let a = self.pop()?;
                match (&a, &b) {
                    (SVal::Bool(x), SVal::Bool(y)) => {
                        let op = match name {
                            "and" => BinOp::BAnd,
                            "or" => BinOp::BOr,
                            _ => BinOp::BXor,
                        };
                        self.bin_to(op, *x, *y, boolean)?;
                    }
                    (SVal::Num(x, xt), SVal::Num(y, yt)) => {
                        let (x, y, xt, yt) = (*x, *y, *xt, *yt);
                        if let Some(x) = self.as_i(x, xt)?
                            && let Some(y) = self.as_i(y, yt)?
                        {
                            let op = match name {
                                "and" => IBinOp::And,
                                "or" => IBinOp::Or,
                                _ => IBinOp::Xor,
                            };
                            self.ibin(op, x, y)?;
                        }
                    }
                    _ => return nc(format!("{name} on {} and {}", describe(&a), describe(&b))),
                }
            }
            "not" => match self.pop()? {
                SVal::Bool(x) => self.un_to(UnOp::BNot, x, boolean)?,
                SVal::Num(x, t) => {
                    if let Some(x) = self.as_i(x, t)? {
                        self.iun(IUnOp::Not, x)?;
                    }
                }
                other => return nc(format!("not on {}", describe(&other))),
            },
            "bitshift" => {
                let (s, st) = self.pop_num()?;
                let (x, xt) = self.pop_num()?;
                if let Some(x) = self.as_i(x, xt)?
                    && let Some(s) = self.as_i(s, st)?
                {
                    self.ibin(IBinOp::Shift, x, s)?;
                }
            }
            // control
            "if" => {
                let p = self.pop_proc()?;
                let c = self.pop_bool()?;
                self.compile_if(c, p, None)?;
            }
            "ifelse" => {
                let e = self.pop_proc()?;
                let t = self.pop_proc()?;
                let c = self.pop_bool()?;
                self.compile_if(c, t, Some(e))?;
            }
            "exec" => match self.pop()? {
                SVal::Proc(p) => self.exec_proc(&p)?,
                other => self.push(other),
            },
            "for" => {
                let body = self.pop_proc()?;
                let (lim, lt) = self.pop_num()?;
                let (inc, it_) = self.pop_num()?;
                let (init, nt) = self.pop_num()?;
                let trips = match (init, inc, lim) {
                    (Op::Imm(a), Op::Imm(b), Op::Imm(c)) if !matches!(nt, NT::Dyn(_)) => {
                        let ct = if (nt, it_, lt) == (NT::Int, NT::Int, NT::Int) {
                            NT::Int
                        } else {
                            NT::Real
                        };
                        for_trips(a, b, c, UNROLL_MAX).map(|v| {
                            v.into_iter()
                                .map(|x| Some(SVal::Num(Op::Imm(x), ct)))
                                .collect::<Vec<_>>()
                        })
                    }
                    _ => None,
                };
                self.loop_or_unroll(trips, body, |c: &mut Compiler<'a>, body| {
                    let ct = match (nt, it_, lt) {
                        (NT::Int, NT::Int, NT::Int) => NT::Int,
                        (NT::Real, _, _) | (_, NT::Real, _) | (_, _, NT::Real) => NT::Real,
                        _ => {
                            let t = c.reg();
                            c.emit(Ins::Bin(BinOp::TagAnd, t, nt.tag(), it_.tag()))?;
                            c.emit(Ins::Bin(BinOp::TagAnd, t, Op::R(t), lt.tag()))?;
                            NT::Dyn(t)
                        }
                    };
                    // The counter is an f64, as `ForControl` keeps it.
                    let (init, inc, lim) = (c.as_f(init, nt)?, c.as_f(inc, it_)?, c.as_f(lim, lt)?);
                    let r = c.reg();
                    c.emit(Ins::Mov(r, init))?;
                    c.compile_loop(LoopHead::For { c: r, ct, inc, lim }, body)
                })?;
            }
            "repeat" => {
                let body = self.pop_proc()?;
                let (n, t) = self.pop_num()?;
                if !self.need_int(t)? {
                    return Ok(());
                }
                if let Op::Imm(v) = n
                    && v < 0.0
                {
                    return self.abort_path();
                }
                let trips = match n {
                    Op::Imm(v) if v as usize <= UNROLL_MAX => Some(vec![None; v as usize]),
                    _ => None,
                };
                self.loop_or_unroll(trips, body, |c: &mut Compiler<'a>, body| {
                    let n = c.as_f(n, t)?;
                    let ok = c.reg();
                    c.emit(Ins::Bin(BinOp::Ge, ok, n, Op::Imm(0.0)))?;
                    c.emit(Ins::Assert(Op::R(ok)))?;
                    let k = c.reg();
                    c.emit(Ins::Mov(k, n))?;
                    c.compile_loop(LoopHead::Repeat { k }, body)
                })?;
            }
            "forall" => {
                let body = self.pop_proc()?;
                let c = self.pop()?;
                let n = match &c {
                    SVal::Cap(o) => cap_len(o),
                    SVal::Tuple(t, _) => t.borrow().len(),
                    SVal::Row {
                        table, row, cols, ..
                    } => {
                        self.emit(Ins::RowCheck(*table, *row))?;
                        *cols
                    }
                    SVal::LArr(_, n) => *n,
                    other => return nc(format!("forall over {}", describe(other))),
                };
                if n > UNROLL_MAX {
                    return nc("forall over a long array (it is unrolled)");
                }
                // Unrolled: each trip reads its element, then runs the body.
                self.loops.push(LoopCtx {
                    carried: Vec::new(),
                    exits: Vec::new(),
                    depth: 0,
                    unrolled: true,
                    mismatch: false,
                });
                let mut r = Ok(());
                for k in 0..n {
                    if self.st.dead {
                        break;
                    }
                    r = self
                        .get(c.clone(), Op::Imm(k as f64), NT::Int)
                        .and_then(|_| self.exec_proc(&body));
                    if r.is_err() {
                        break;
                    }
                }
                self.loops.pop();
                if let Err(NotCompilable(why)) = r {
                    return nc(if why == EXIT_UNROLLED {
                        "exit inside forall".to_string()
                    } else {
                        why
                    });
                }
            }
            "loop" => {
                let body = self.pop_proc()?;
                self.compile_loop(LoopHead::Loop, body)?;
            }
            "exit" => self.compile_exit()?,
            // names and dictionaries
            "def" => {
                let v = self.pop()?;
                let k = self.pop()?;
                let SVal::Name(n) = k else {
                    return nc(format!("def with a key that is {}", describe(&k)));
                };
                self.touch_def(n.id());
                let Some(top) = self.st.scopes.last_mut() else {
                    return nc("internal: no scope");
                };
                if let ScopeKind::Captured(_) = top.kind {
                    return nc(format!(
                        "def of /{n} into a dictionary from outside the kernel"
                    ));
                }
                top.set(n.id(), Binding::Val(v));
            }
            "load" => {
                let k = self.pop()?;
                let SVal::Name(n) = k else {
                    return nc("load of a non-name");
                };
                self.touch_read(n.id());
                match self.lookup(&self.st, n.id(), self.st.scopes.len() - 1)? {
                    Found::Val(v) => self.push(v),
                    Found::Partial(v, f, uid) => {
                        let v = self.read_partial(&n, v, f, uid)?;
                        self.push(v);
                    }
                    Found::Obj(o) => {
                        if let Value::Operator(_) = o.value {
                            return nc("load of an operator");
                        }
                        let v = self.cap_to_sval(&o)?;
                        self.push(v);
                    }
                    Found::None => return nc(format!("/{n} is not defined")),
                }
            }
            "where" => {
                let k = self.pop()?;
                let SVal::Name(n) = k else {
                    return nc("where with a key that is not a name");
                };
                self.touch_name(n.id());
                let found = self.where_(&n)?;
                match found {
                    Some(d) => {
                        self.push(d);
                        self.push(SVal::Bool(Op::Imm(1.0)));
                    }
                    None => self.push(SVal::Bool(Op::Imm(0.0))),
                }
            }
            "known" => {
                let k = self.pop()?;
                let d = self.pop()?;
                let SVal::Name(n) = k else {
                    return nc("known with a key that is not a name");
                };
                let SVal::CapDict(d) = d else {
                    return nc(format!("known on {}", describe(&d)));
                };
                let v = d.borrow().get_id(n.id()).is_some();
                self.push(SVal::Bool(Op::Imm(f64::from(u8::from(v)))));
            }
            // `bind` inside the kernel would replace names with whatever
            // they mean at that moment, in place; not modelled.
            "bind" => return nc("bind inside the procedure"),
            "dict" => {
                let n = self.pop_const_int("dict's size")?;
                if n < 0 {
                    return self.abort_path();
                }
                self.next_uid += 1;
                let id = self.next_uid;
                self.push(SVal::NewDict(id));
            }
            "begin" => {
                let kind = match self.pop()? {
                    SVal::NewDict(id) => {
                        if self.begun_dicts.contains(&id) {
                            return nc("the same dictionary is begun twice");
                        }
                        self.begun_dicts.push(id);
                        ScopeKind::Local
                    }
                    SVal::CapDict(d) => ScopeKind::Captured(d),
                    other => return nc(format!("begin on {}", describe(&other))),
                };
                self.next_uid += 1;
                let uid = self.next_uid;
                self.st.scopes.push(Scope {
                    uid,
                    kind,
                    names: Vec::new(),
                });
            }
            "end" => {
                if self.st.scopes.len() <= 1 {
                    return nc("end without a matching begin in the kernel");
                }
                self.st.scopes.pop();
            }
            // arrays
            "array" => {
                let n = self.pop_const_int("array's size")?;
                if n < 0 {
                    return self.abort_path();
                }
                if n as usize > MAX_LOCAL_ARRAY {
                    return nc("a local array is too large");
                }
                let id = self.larrays.len() as u32;
                self.larrays.push(n as usize);
                self.emit(Ins::LNew(id))?;
                self.push(SVal::LArr(id, n as usize));
            }
            "length" => {
                let n = match self.pop()? {
                    SVal::Cap(o) => match &o.value {
                        Value::Array(a) => a.len(),
                        Value::String(s) => s.len(),
                        _ => return nc("length of this value"),
                    },
                    SVal::Proc(p) => p.len(),
                    SVal::Tuple(t, _) => t.borrow().len(),
                    SVal::Row {
                        table, row, cols, ..
                    } => {
                        self.emit(Ins::RowCheck(table, row))?;
                        cols
                    }
                    SVal::LArr(_, n) => n,
                    other => return nc(format!("length of {}", describe(&other))),
                };
                self.push_num(Op::Imm(n as f64), NT::Int);
            }
            "get" => {
                let (i, it_) = match self.pop()? {
                    SVal::Num(o, t) => (o, t),
                    SVal::Name(_) => return nc("get with a name key (dictionary get)"),
                    other => return nc(format!("get with an index that is {}", describe(&other))),
                };
                let c = self.pop()?;
                self.get(c, i, it_)?;
            }
            "put" => {
                let v = self.pop()?;
                let (i, it_) = self.pop_num()?;
                let c = self.pop()?;
                self.put(c, i, it_, v)?;
            }
            "aload" => {
                let c = self.pop()?;
                let n = match &c {
                    SVal::Cap(o) => match &o.value {
                        Value::Array(a) => a.len(),
                        _ => return nc("aload of a string"),
                    },
                    SVal::Tuple(t, _) => t.borrow().len(),
                    SVal::Row {
                        table, row, cols, ..
                    } => {
                        if self.tables[*table as usize].strings {
                            // `aload` takes arrays only: a string row is
                            // the interpreter's typecheck.
                            return self.abort_path();
                        }
                        self.emit(Ins::RowCheck(*table, *row))?;
                        *cols
                    }
                    SVal::LArr(_, n) => *n,
                    SVal::Proc(p) => p.len(),
                    other => return nc(format!("aload of {}", describe(other))),
                };
                for k in 0..n {
                    self.get(c.clone(), Op::Imm(k as f64), NT::Int)?;
                    if self.st.dead {
                        return Ok(());
                    }
                }
                self.push(c);
            }
            "astore" => {
                let c = self.pop()?;
                let n = match &c {
                    SVal::Tuple(t, _) => t.borrow().len(),
                    SVal::LArr(_, n) => *n,
                    other => return nc(format!("astore into {}", describe(other))),
                };
                if self.st.stack.len() < n {
                    return nc("astore past the kernel's stack");
                }
                self.touch(self.st.stack.len() - n);
                let vals = self.st.stack.split_off(self.st.stack.len() - n);
                for (k, v) in vals.into_iter().enumerate() {
                    self.put(c.clone(), Op::Imm(k as f64), NT::Int, v)?;
                }
                self.push(c);
            }
            // random numbers (issue #1063), with the pixel's own seed
            "srand" => {
                self.touch_name(RAND_ID);
                self.srand()?
            }
            "rand" => {
                self.touch_name(RAND_ID);
                if let Some(s) = self.seeded_state("rand")? {
                    let a = self.ibin_int(IBinOp::Mul, s, Op::Imm(RAND_MUL as f64))?;
                    let b = self.ibin_int(IBinOp::Add, a, Op::Imm(RAND_INC as f64))?;
                    let n = self.ibin_int(IBinOp::And, b, Op::Imm(RAND_MASK as f64))?;
                    if let Op::R(r) = n {
                        self.seeded.insert(r);
                    }
                    self.set_rand_state(n);
                    self.push_num(n, NT::Int);
                }
            }
            "rrand" => {
                self.touch_name(RAND_ID);
                if let Some(s) = self.seeded_state("rrand")? {
                    // Tagged under a seed override, as `ops/arith.rs` does.
                    if self.it.seed_override.is_some() {
                        let t = self.ibin_int(IBinOp::Or, s, Op::Imm(RRAND_TAG as f64))?;
                        self.push_num(t, NT::Int);
                    } else {
                        self.push_num(s, NT::Int);
                    }
                }
            }
            other => {
                if let Some(kind) = NoiseKind::from_name(other) {
                    return self.noise(kind);
                }
                return nc(format!("the operator {other} is not in the kernel subset"));
            }
        }
        Ok(())
    }

    // --- random numbers (issue #1063) ----------------------------------------

    /// The random state on this path: the slot's integer (see [`RAND_ID`]).
    fn rand_state(&self) -> CResult<Op> {
        match self.st.scopes.first().and_then(|s| s.get(RAND_ID)) {
            Some(Binding::Val(SVal::Num(o, NT::Int))) => Ok(*o),
            _ => nc("internal: the random state is lost"),
        }
    }

    fn set_rand_state(&mut self, o: Op) {
        if let Some(s) = self.st.scopes.first_mut() {
            s.set(RAND_ID, Binding::Val(SVal::Num(o, NT::Int)));
        }
    }

    /// An integer op whose result is an integer (no constant overflow).
    fn ibin_int(&mut self, op: IBinOp, a: Op, b: Op) -> CResult<Op> {
        self.ibin(op, a, b)?;
        match self.pop()? {
            SVal::Num(o, NT::Int) => Ok(o),
            _ => nc("internal: an integer op on the random state overflowed"),
        }
    }

    /// The state `rand` or `rrand` reads. Before any `srand` in the pixel it
    /// is what the previous pixel left, which is not compiled: refused when
    /// no path has seeded, and the lanes that have not retire otherwise (and
    /// then the whole image runs in the interpreter). `None` when the path
    /// certainly errors.
    fn seeded_state(&mut self, what: &str) -> CResult<Option<Op>> {
        match self.rand_state()? {
            Op::Imm(v) if v as i64 == UNSEEDED => nc(format!(
                "{what} before srand in the pixel (it reads the state the previous pixel left)"
            )),
            Op::R(r) if !self.seeded.contains(&r) => {
                self.ibin(IBinOp::Ne, Op::R(r), Op::Imm(UNSEEDED as f64))?;
                match self.pop_bool()? {
                    Op::R(c) => self.emit(Ins::Assert(Op::R(c)))?,
                    Op::Imm(_) => return nc("internal: the random state folded"),
                }
                Ok(Some(Op::R(r)))
            }
            s => Ok(Some(s)),
        }
    }

    /// `srand`, as `ops/arith.rs` does it: an integer operand (a real is a
    /// `typecheck`), masked to 31 bits; under a seed override (`--sweep-seed`)
    /// the forced seed instead. An `rrand` value coming back under an
    /// override is a restore, not a seeding: its lanes retire, so the
    /// interpreter does it (and decides whether the override fired).
    fn srand(&mut self) -> CResult<()> {
        let (o, t) = self.pop_num()?;
        let Some(x) = self.as_i(o, t)? else {
            return Ok(());
        };
        self.uses_rand = true;
        let s = match self.it.seed_override {
            None => self.ibin_int(IBinOp::And, x, Op::Imm(RAND_MASK as f64))?,
            Some(forced) => {
                let hi = self.ibin_int(IBinOp::And, x, Op::Imm(!RAND_MASK as f64))?;
                self.ibin(IBinOp::Ne, hi, Op::Imm(RRAND_TAG as f64))?;
                match self.pop_bool()? {
                    Op::Imm(0.0) => return self.abort_path(),
                    Op::Imm(_) => {}
                    c => self.emit(Ins::Assert(c))?,
                }
                Op::Imm((forced & RAND_MASK) as f64)
            }
        };
        if let Op::R(r) = s {
            self.seeded.insert(r);
        }
        self.set_rand_state(s);
        Ok(())
    }

    fn noise(&mut self, kind: NoiseKind) -> CResult<()> {
        self.uses_noise = true;
        let mut a = [Op::Imm(0.0); 6];
        let mut octtag = Op::Imm(1.0);
        let dims = kind.dims();
        if kind.octaves() {
            let (lac, lt) = self.pop_num()?;
            let (gain, gt) = self.pop_num()?;
            let (oct, ot) = self.pop_num()?;
            if ot == NT::Real {
                return self.abort_path();
            }
            octtag = ot.tag();
            a[dims] = self.as_f(oct, ot)?;
            a[dims + 1] = self.as_f(gain, gt)?;
            a[dims + 2] = self.as_f(lac, lt)?;
        }
        for k in (0..dims).rev() {
            let (v, t) = self.pop_num()?;
            a[k] = self.as_f(v, t)?;
        }
        let d = [self.reg(), self.reg(), self.reg()];
        self.emit(Ins::Noise { kind, d, a, octtag })?;
        for r in d.iter().take(kind.outputs()) {
            self.push_num(Op::R(*r), NT::Real);
        }
        Ok(())
    }

    /// Index `c` at `i`; pushes the element.
    fn get(&mut self, c: SVal, i: Op, it_: NT) -> CResult<()> {
        if it_ == NT::Real {
            return self.abort_path();
        }
        // Indices are read as f64 (with their integer tag).
        let i = self.as_f(i, it_)?;
        match c {
            SVal::Cap(o) => {
                if let Op::Imm(k) = i {
                    let len = cap_len(&o);
                    if k < 0.0 || k as usize >= len {
                        return self.abort_path();
                    }
                    let v = match &o.value {
                        Value::Array(a) => {
                            let Some(el) = a.get(k as usize) else {
                                return self.abort_path();
                            };
                            self.cap_to_sval(&el)?
                        }
                        Value::String(s) => {
                            let b = s.borrow_bytes()[k as usize];
                            SVal::Num(Op::Imm(f64::from(b)), NT::Int)
                        }
                        _ => return nc("get on this value"),
                    };
                    self.push(v);
                    return Ok(());
                }
                let (t, two_level) = self.table_for(&o)?;
                if two_level {
                    let (rows, cols) = (self.tables[t as usize].rows, self.tables[t as usize].cols);
                    // `get` checks the row index now, as the interpreter
                    // does, even if the row is never read.
                    if let NT::Dyn(tag) = it_ {
                        self.emit(Ins::NeedInt(Op::R(tag)))?;
                    }
                    let (lo, hi, ok) = (self.reg(), self.reg(), self.reg());
                    self.emit(Ins::Bin(BinOp::Ge, lo, i, Op::Imm(0.0)))?;
                    self.emit(Ins::Bin(BinOp::Lt, hi, i, Op::Imm(rows as f64)))?;
                    self.emit(Ins::Bin(BinOp::BAnd, ok, Op::R(lo), Op::R(hi)))?;
                    self.emit(Ins::Assert(Op::R(ok)))?;
                    self.push(SVal::Row {
                        table: t,
                        row: i,
                        rtag: it_.tag(),
                        cols,
                    });
                } else {
                    self.gather(t, Op::Imm(0.0), Op::Imm(1.0), i, it_.tag())?;
                }
            }
            SVal::Row {
                table, row, rtag, ..
            } => {
                self.gather(table, row, rtag, i, it_.tag())?;
            }
            SVal::Tuple(t, _) => {
                let items = t.borrow().clone();
                match i {
                    Op::Imm(k) => {
                        if k < 0.0 || k as usize >= items.len() {
                            return self.abort_path();
                        }
                        self.push(items[k as usize].clone());
                    }
                    Op::R(_) => {
                        // Run-time index into a kernel-built array: copy it
                        // into a local array and read that.
                        let id = self.larrays.len() as u32;
                        self.larrays.push(items.len());
                        self.emit(Ins::LNew(id))?;
                        for (k, v) in items.iter().enumerate() {
                            let SVal::Num(o, t) = v else {
                                return nc("run-time index into an array of non-numbers");
                            };
                            let o = self.as_exact_f(*o, *t)?;
                            self.emit(Ins::LPut {
                                arr: id,
                                idx: Op::Imm(k as f64),
                                itag: Op::Imm(1.0),
                                v: o,
                                vtag: t.tag(),
                            })?;
                        }
                        self.lget(id, i, it_)?;
                    }
                }
            }
            SVal::LArr(id, _) => self.lget(id, i, it_)?,
            SVal::Proc(p) => match i {
                Op::Imm(k) if k >= 0.0 && (k as usize) < p.len() => {
                    let Some(el) = p.get(k as usize) else {
                        return self.abort_path();
                    };
                    let v = self.cap_to_sval(&el)?;
                    self.push(v);
                }
                Op::Imm(_) => return self.abort_path(),
                Op::R(_) => return nc("run-time index into a procedure"),
            },
            other => return nc(format!("get on {}", describe(&other))),
        }
        Ok(())
    }

    fn lget(&mut self, id: u32, i: Op, it_: NT) -> CResult<()> {
        let (d, dt) = (self.reg(), self.reg());
        self.emit(Ins::LGet {
            d,
            dtag: dt,
            arr: id,
            idx: i,
            itag: it_.tag(),
        })?;
        self.push_num(Op::R(d), NT::Dyn(dt));
        Ok(())
    }

    fn gather(&mut self, table: u32, row: Op, rtag: Op, col: Op, ctag: Op) -> CResult<()> {
        let d = self.reg();
        let (dtag, nt) = match self.tables[table as usize].kind {
            TableKind::Int => (None, NT::Int),
            TableKind::Real => (None, NT::Real),
            TableKind::Mixed(_) => {
                let t = self.reg();
                (Some(t), NT::Dyn(t))
            }
        };
        self.emit(Ins::Gather {
            d,
            dtag,
            table,
            row,
            rtag,
            col,
            ctag,
        })?;
        if nt == NT::Int {
            // Tables hold f64s; an integer table's elements are exact.
            let di = self.ireg();
            self.emit(Ins::F2I(di, Op::R(d)))?;
            self.push_num(Op::R(di), nt);
            return Ok(());
        }
        self.push_num(Op::R(d), nt);
        Ok(())
    }

    fn put(&mut self, c: SVal, i: Op, it_: NT, v: SVal) -> CResult<()> {
        if it_ == NT::Real {
            return self.abort_path();
        }
        match c {
            SVal::Tuple(t, b) => {
                let Op::Imm(k) = i else {
                    return nc("put at a run-time index into a [ ] array");
                };
                if self.frozen.contains(&(Rc::as_ptr(&t) as usize)) {
                    return nc("put into an array that a branch chose");
                }
                if b != self.block {
                    return nc("put into a [ ] array from inside an if or loop");
                }
                let len = t.borrow().len();
                if k < 0.0 || k as usize >= len {
                    return self.abort_path();
                }
                t.borrow_mut()[k as usize] = v;
            }
            SVal::LArr(id, _) => {
                let SVal::Num(o, vt) = v else {
                    return nc("put of a non-number into a local array");
                };
                let i = self.as_f(i, it_)?;
                let o = self.as_exact_f(o, vt)?;
                self.emit(Ins::LPut {
                    arr: id,
                    idx: i,
                    itag: it_.tag(),
                    v: o,
                    vtag: vt.tag(),
                })?;
            }
            other => {
                return nc(format!(
                    "put into {} (data from outside the kernel is read only)",
                    describe(&other)
                ));
            }
        }
        Ok(())
    }

    /// The snapshot table for a captured array or string: (id, two-level).
    fn table_for(&mut self, o: &Object) -> CResult<(u32, bool)> {
        let key = match &o.value {
            Value::Array(a) => {
                let (p, off, len) = a.identity();
                (0u8, p, off, len)
            }
            Value::String(s) => {
                let (p, off, len) = s.identity();
                (1u8, p, off, len)
            }
            _ => return nc("a table must be an array or string"),
        };
        if let Some(t) = self.table_ids.get(&key) {
            return Ok(*t);
        }
        let (table, two) = match &o.value {
            Value::String(s) => {
                if s.len() > MAX_TABLE_BYTES || self.it.check_alloc(s.len()).is_err() {
                    return nc("a string table is too large");
                }
                let data = s.to_vec();
                (
                    Table {
                        rows: 1,
                        cols: data.len(),
                        data: TableData::U8(data),
                        kind: TableKind::Int,
                        null_rows: Vec::new(),
                        strings: false,
                    },
                    false,
                )
            }
            Value::Array(a) => {
                let items = a.snapshot();
                if items.len() > MAX_TABLE || self.it.check_alloc(items.len() * 8).is_err() {
                    return nc("a table is too large");
                }
                let nums: Option<Vec<(f64, bool)>> = items.iter().map(num_of).collect();
                // An array of equal-length strings: an image read row by
                // row (`readstring`), as one byte table.
                let strs: Option<Vec<crate::object::PsString>> = items
                    .iter()
                    .map(|o| match &o.value {
                        Value::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect();
                if let Some(rows) = strs.filter(|r| !r.is_empty()) {
                    let cols = rows[0].len();
                    if rows.iter().any(|r| r.len() != cols) {
                        return nc("run-time index into an array of strings of different lengths");
                    }
                    let total = rows.len().saturating_mul(cols);
                    if total > MAX_TABLE_BYTES || self.it.check_alloc(total).is_err() {
                        return nc("a string table is too large");
                    }
                    let mut data = Vec::with_capacity(total);
                    for r in &rows {
                        data.extend_from_slice(&r.borrow_bytes());
                    }
                    let t = Table {
                        rows: rows.len(),
                        cols,
                        data: TableData::U8(data),
                        kind: TableKind::Int,
                        null_rows: Vec::new(),
                        strings: true,
                    };
                    let id = self.tables.len() as u32;
                    self.tables.push(t);
                    self.table_ids.insert(key, (id, true));
                    return Ok((id, true));
                }
                if let Some(nums) = nums {
                    (table_of(1, nums.len(), nums), false)
                } else {
                    // An array of equal-length numeric arrays (rows that
                    // are null are allowed, and abort if read).
                    let mut cols = None;
                    let mut rows = Vec::new();
                    let mut nulls = Vec::new();
                    for it in &items {
                        match &it.value {
                            Value::Array(row) => {
                                let r = row.snapshot();
                                if *cols.get_or_insert(r.len()) != r.len() {
                                    return nc(
                                        "run-time index into an array of arrays of different lengths",
                                    );
                                }
                                rows.push(Some(r));
                                nulls.push(false);
                            }
                            Value::Null => {
                                rows.push(None);
                                nulls.push(true);
                            }
                            _ => {
                                return nc(
                                    "run-time index into an array that mixes numbers and other values",
                                );
                            }
                        }
                    }
                    let cols = cols.unwrap_or(0);
                    if items.len().saturating_mul(cols) > MAX_TABLE {
                        return nc("a table is too large");
                    }
                    let mut all = Vec::new();
                    for r in &rows {
                        match r {
                            Some(r) => {
                                for e in r {
                                    let Some(n) = num_of(e) else {
                                        return nc(
                                            "run-time index into an array of arrays of non-numbers",
                                        );
                                    };
                                    all.push(n);
                                }
                            }
                            None => all.extend(std::iter::repeat_n((0.0, false), cols)),
                        }
                    }
                    let mut t = table_of(items.len(), cols, all);
                    if nulls.iter().any(|n| *n) {
                        t.null_rows = nulls;
                    }
                    (t, true)
                }
            }
            _ => return nc("a table must be an array or string"),
        };
        let id = self.tables.len() as u32;
        self.tables.push(table);
        self.table_ids.insert(key, (id, two));
        Ok((id, two))
    }
}

enum Found {
    Val(SVal),
    /// A partly defined binding: value, flag, and the scope it lives in.
    Partial(SVal, Reg, u32),
    Obj(Object),
    None,
}

enum LoopHead {
    /// The counter register, its type, the increment and the limit.
    For {
        c: Reg,
        ct: NT,
        inc: Op,
        lim: Op,
    },
    /// The remaining-count register.
    Repeat {
        k: Reg,
    },
    Loop,
}

/// PostScript `eq` decided at compile time, where it can be: values of
/// different kinds are unequal; names, null and marks by value; strings
/// by content; other composites by identity. `None` when it depends on a
/// run-time value of the same kind (two numbers, two booleans).
fn const_eq(a: &SVal, b: &SVal) -> Option<bool> {
    use SVal::*;
    let text = |v: &SVal| -> Option<Vec<u8>> {
        match v {
            Name(n) => Some(n.as_bytes().to_vec()),
            Cap(o) => match &o.value {
                Value::String(s) => Some(s.to_vec()),
                _ => None,
            },
            _ => None,
        }
    };
    if let (Some(x), Some(y)) = (text(a), text(b)) {
        return Some(x == y);
    }
    Some(match (a, b) {
        (Num(..), Num(..)) | (Bool(_), Bool(_)) => return None,
        // A row chosen at run time is some array of the table: whether two
        // are the same object is a run-time question.
        (Row { .. }, _) | (_, Row { .. }) => return None,
        (Null, Null) | (Mark, Mark) => true,
        _ => a == b,
    })
}

fn num_of(o: &Object) -> Option<(f64, bool)> {
    match o.value {
        Value::Integer(i) if i.unsigned_abs() <= INT_LIMIT_I => Some((i as f64, true)),
        Value::Real(r) => Some((r, false)),
        _ => None,
    }
}

fn table_of(rows: usize, cols: usize, nums: Vec<(f64, bool)>) -> Table {
    let all_int = nums.iter().all(|n| n.1);
    let all_real = nums.iter().all(|n| !n.1);
    let kind = if all_int {
        TableKind::Int
    } else if all_real {
        TableKind::Real
    } else {
        TableKind::Mixed(nums.iter().map(|n| f64::from(u8::from(n.1))).collect())
    };
    Table {
        rows,
        cols,
        data: TableData::F64(nums.into_iter().map(|n| n.0).collect()),
        kind,
        null_rows: Vec::new(),
        strings: false,
    }
}

fn cap_len(o: &Object) -> usize {
    match &o.value {
        Value::Array(a) => a.len(),
        Value::String(s) => s.len(),
        _ => 0,
    }
}

fn type_name(o: &Object) -> &'static str {
    match o.value {
        Value::Integer(_) => "an integer",
        Value::Real(_) => "a real",
        Value::Boolean(_) => "a boolean",
        Value::Mark => "a mark",
        Value::Null => "null",
        Value::Name(_) => "a name",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Dict(_) => "a dictionary",
        Value::Operator(_) => "an operator",
        Value::File(_) => "a file",
        Value::Save(_) => "a save object",
        Value::GState(_) => "a gstate",
    }
}

fn describe(v: &SVal) -> &'static str {
    match v {
        SVal::Num(_, NT::Int) => "an integer",
        SVal::Num(_, NT::Real) => "a real",
        SVal::Num(..) => "a number",
        SVal::Bool(_) => "a boolean",
        SVal::Mark => "a mark",
        SVal::Null => "null",
        SVal::LocalDict => "a kernel dictionary",
        SVal::Name(_) => "a name",
        SVal::Proc(_) => "a procedure",
        SVal::Cap(_) => "an outside array or string",
        SVal::Tuple(..) => "an array",
        SVal::Row { .. } => "a table row",
        SVal::LArr(..) => "a local array",
        SVal::NewDict(_) => "a dictionary",
        SVal::CapDict(_) => "a dictionary",
    }
}
