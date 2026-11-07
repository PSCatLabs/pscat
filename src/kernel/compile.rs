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
use std::collections::HashMap;
use std::rc::Rc;

use super::exec::{INT_LIMIT, lane_bin, lane_un};
use super::ir::{BinOp, Ins, Kernel, NoiseKind, Op, Reg, Table, TableKind, UnOp};
use crate::interp::Interp;
use crate::name::PsName;
use crate::object::{Dict, Object, Operator, PsArray, Value};

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

struct LoopCtx {
    carried: Vec<Carried>,
    exits: Vec<State>,
    depth: usize,
}

/// Procedure nesting allowed at compile time (inlining depth).
const MAX_INLINE: usize = 48;
/// Upper bound on emitted instructions, so a pathological program cannot
/// make the compiler itself expensive.
const MAX_INSNS: usize = 200_000;
/// Largest `N array` a kernel may make.
const MAX_LOCAL_ARRAY: usize = 4096;
/// Largest table a kernel may snapshot (elements).
const MAX_TABLE: usize = 16 << 20;

pub(crate) struct Compiler<'a> {
    it: &'a Interp,
    nregs: u32,
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
}

/// Compile `proc` as a kernel taking `i j` and leaving `ncomp` numbers.
pub(crate) fn compile(it: &Interp, proc: &PsArray, ncomp: usize) -> CResult<(Kernel, bool)> {
    let mut c = Compiler {
        it,
        nregs: 0,
        st: State {
            stack: Vec::new(),
            scopes: vec![Scope {
                uid: 0,
                kind: ScopeKind::Local,
                names: Vec::new(),
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
    };
    let (ri, rj) = (c.reg(), c.reg());
    c.st.stack.push(SVal::Num(Op::R(ri), NT::Int));
    c.st.stack.push(SVal::Num(Op::R(rj), NT::Int));
    c.exec_proc(proc)?;
    let mut outputs = Vec::new();
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
        for v in &c.st.stack {
            match v {
                SVal::Num(o, _) => outputs.push(*o),
                _ => return nc("an output component is not a number"),
            }
        }
    } else {
        outputs = vec![Op::Imm(0.0); ncomp];
    }
    let noise = c.uses_noise;
    Ok((
        Kernel {
            nregs: c.nregs as usize,
            body: c.code,
            in_i: ri,
            in_j: rj,
            outputs,
            tables: c.tables,
            larrays: c.larrays,
            perm: it.noise_perm,
        },
        noise,
    ))
}

impl<'a> Compiler<'a> {
    fn reg(&mut self) -> Reg {
        self.nregs += 1;
        self.nregs - 1
    }

    fn emit(&mut self, i: Ins) -> CResult<()> {
        self.emitted += 1;
        if self.emitted > MAX_INSNS {
            return nc("the kernel is too large");
        }
        self.code.push(i);
        Ok(())
    }

    fn push(&mut self, v: SVal) {
        self.st.stack.push(v);
    }

    fn pop(&mut self) -> CResult<SVal> {
        match self.st.stack.pop() {
            Some(v) => Ok(v),
            None => nc("stack underflow at compile time"),
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

    /// One element of a procedure body.
    fn exec_item(&mut self, obj: &Object) -> CResult<()> {
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
    fn cap_to_sval(&mut self, obj: &Object) -> CResult<SVal> {
        Ok(match &obj.value {
            Value::Integer(i) => {
                if (*i as f64).abs() > INT_LIMIT {
                    return nc("an integer past 2^53");
                }
                SVal::Num(Op::Imm(*i as f64), NT::Int)
            }
            Value::Real(r) => SVal::Num(Op::Imm(*r), NT::Real),
            Value::Boolean(b) => SVal::Bool(Op::Imm(f64::from(u8::from(*b)))),
            Value::Mark => SVal::Mark,
            Value::Name(n) if !obj.executable => SVal::Name(n.clone()),
            Value::Array(a) if obj.executable => SVal::Proc(a.clone()),
            Value::Array(_) | Value::String(_) => SVal::Cap(obj.clone()),
            Value::Dict(d) => SVal::CapDict(d.clone()),
            _ => return nc(format!("unsupported value {}", type_name(obj))),
        })
    }

    fn exec_name(&mut self, n: &PsName) -> CResult<()> {
        match self.lookup(&self.st, n.id(), self.st.scopes.len() - 1)? {
            Found::Val(SVal::Proc(p)) => self.exec_proc(&p),
            Found::Val(v) => {
                self.push(v);
                Ok(())
            }
            Found::Obj(obj) => match &obj.value {
                Value::Operator(op) => self.apply(op),
                Value::Array(a) if obj.executable => self.exec_proc(a),
                Value::Name(n2) if obj.executable => {
                    let n2 = n2.clone();
                    self.exec_name(&n2)
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

    /// Look name `id` up from scope `top` down, then in the program's
    /// dictionary stack.
    fn lookup(&self, st: &State, id: u32, top: usize) -> CResult<Found> {
        for s in st.scopes[..=top].iter().rev() {
            match &s.kind {
                ScopeKind::Local => match s.get(id) {
                    Some(Binding::Val(v)) => return Ok(Found::Val(v.clone())),
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
        reg: Reg,
        tag: Option<Reg>,
        ty: CT,
        v: &SVal,
        out: &mut Vec<(Reg, Op)>,
    ) -> CResult<()> {
        match (v, ty) {
            (SVal::Num(o, nt), CT::Int | CT::Real | CT::Dyn) => {
                out.push((reg, *o));
                if ty == CT::Dyn {
                    out.push((tag.unwrap_or(reg), nt.tag()));
                }
            }
            (SVal::Bool(o), CT::Bool) => out.push((reg, *o)),
            _ => return nc("a value changes type between paths"),
        }
        Ok(())
    }

    /// Emit `moves` as a parallel assignment.
    fn par_moves(&mut self, moves: Vec<(Reg, Op)>, code: &mut Vec<Ins>) -> CResult<()> {
        let moves: Vec<_> = moves.into_iter().filter(|(d, s)| *s != Op::R(*d)).collect();
        let hazard = moves
            .iter()
            .any(|(_, s)| moves.iter().any(|(d, _)| s.uses(*d)));
        if hazard {
            let mut tmps = Vec::new();
            for (_, s) in &moves {
                let t = self.reg();
                code.push(Ins::Mov(t, *s));
                tmps.push(t);
            }
            for ((d, _), t) in moves.iter().zip(tmps) {
                code.push(Ins::Mov(*d, Op::R(t)));
            }
        } else {
            for (d, s) in moves {
                code.push(Ins::Mov(d, s));
            }
        }
        self.emitted += code.len();
        if self.emitted > MAX_INSNS {
            return nc("the kernel is too large");
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
        let (mut ma, mut mb) = (Vec::new(), Vec::new());
        for sl in slots {
            let (va, vb) = (self.slot_val(&a, sl)?, self.slot_val(&b, sl)?);
            if va == vb {
                continue;
            }
            let merged = match (&va, &vb) {
                (Lk::Val(x), Lk::Val(y)) if x.is_scalar() && y.is_scalar() => {
                    let (Some(tx), Some(ty)) = (ct_of(x), ct_of(y)) else {
                        unreachable!("scalars have a type")
                    };
                    match ct_join(tx, ty) {
                        Some(t) => {
                            let reg = self.reg();
                            let tag = (t == CT::Dyn).then(|| self.reg());
                            Self::moves_into(reg, tag, t, x, &mut ma)?;
                            Self::moves_into(reg, tag, t, y, &mut mb)?;
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
        self.block = self.fresh_block();
        self.exec_proc(&t)?;
        let tcode = std::mem::take(&mut self.code);
        let tstate = std::mem::replace(&mut self.st, entry);
        self.block = self.fresh_block();
        if let Some(e) = e {
            self.exec_proc(&e)?;
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
        self.block = bid;
        self.code = saved;
        let both_dead = tstate.dead && estate.dead;
        let (mut st, tcode, ecode) = self.join2(tstate, tcode, estate, ecode)?;
        st.dead = both_dead;
        self.emit(Ins::If(c, tcode, ecode))?;
        self.st = st;
        Ok(())
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
                        continue;
                    }
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
                                if w == CT::Dyn && c.tag.is_none() {
                                    c.tag = Some(self.nregs);
                                    self.nregs += 1;
                                }
                                grew = true;
                            } else if ct_join(c.ty, t).is_none() {
                                return nc("a loop changes a value's type");
                            }
                        }
                        None => {
                            let reg = self.reg();
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
            // Final pass: entry moves, the loop, and the state after it.
            let mut pre = Vec::new();
            for c in &carried {
                let Lk::Val(v) = self.slot_val(&entry, c.slot)? else {
                    return nc("a loop-carried value has no starting value");
                };
                Self::moves_into(c.reg, c.tag, c.ty, &v, &mut pre)?;
            }
            let mut pcode = Vec::new();
            self.par_moves(pre, &mut pcode)?;
            for i in pcode {
                self.emit(i)?;
            }
            if !end.dead {
                let mut back = Vec::new();
                for c in &carried {
                    let Lk::Val(v) = self.slot_val(&end, c.slot)? else {
                        return nc("a loop-carried value is lost");
                    };
                    Self::moves_into(c.reg, c.tag, c.ty, &v, &mut back)?;
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

    fn loop_prefix(&mut self, head: &LoopHead) -> CResult<()> {
        match head {
            LoopHead::For { c, ct, inc, lim } => {
                let fin = self.reg();
                self.emit(Ins::ForFin(fin, Op::R(*c), *inc, *lim))?;
                self.emit(Ins::If(fin, vec![Ins::Break], Vec::new()))?;
                let v = self.reg();
                self.emit(Ins::Mov(v, Op::R(*c)))?;
                self.push(SVal::Num(Op::R(v), *ct));
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
        if self.st.stack.len() != ctx.depth {
            return nc("exit leaves a different stack depth than the loop started with");
        }
        let carried = ctx.carried.clone();
        let snapshot = self.st.clone();
        let mut mv = Vec::new();
        for c in &carried {
            let Lk::Val(v) = self.slot_val(&snapshot, c.slot)? else {
                return nc("a loop-carried value is undefined at exit");
            };
            Self::moves_into(c.reg, c.tag, c.ty, &v, &mut mv)?;
        }
        let mut code = Vec::new();
        self.par_moves(mv, &mut code)?;
        for i in code {
            self.emit(i)?;
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
        let rt = match (at, bt) {
            (NT::Int, NT::Int) => NT::Int,
            (NT::Real, _) | (_, NT::Real) => NT::Real,
            _ => NT::Dyn(0),
        };
        if let (Op::Imm(x), Op::Imm(y), false) = (a, b, matches!(rt, NT::Dyn(_))) {
            let Some(r) = lane_bin(op, x, y) else {
                return self.abort_path();
            };
            if rt == NT::Int && r.abs() > INT_LIMIT {
                return nc("an integer past 2^53");
            }
            self.push_num(Op::Imm(r), rt);
            return Ok(());
        }
        let d = self.reg();
        self.emit(Ins::Bin(op, d, a, b))?;
        let rt = match rt {
            NT::Dyn(_) => {
                let t = self.reg();
                self.emit(Ins::Bin(BinOp::TagAnd, t, at.tag(), bt.tag()))?;
                NT::Dyn(t)
            }
            x => x,
        };
        if rt != NT::Real && !matches!(op, BinOp::Min | BinOp::Max) {
            self.emit(Ins::CheckInt(Op::R(d), rt.tag()))?;
        }
        self.push_num(Op::R(d), rt);
        Ok(())
    }

    /// A binary op with a fixed result type and no integer overflow.
    fn bin_to(&mut self, op: BinOp, a: Op, b: Op, out: fn(Op) -> SVal) -> CResult<()> {
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
        let int = |o: Op| SVal::Num(o, NT::Int);
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
                for i in from..len {
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
                if n > 0 {
                    let s = &mut self.st.stack[len - n as usize..];
                    let k = j.rem_euclid(n) as usize;
                    s.rotate_right(k);
                }
            }
            "count" => {
                let n = self.st.stack.len() as f64;
                self.push_num(Op::Imm(n), NT::Int);
            }
            "mark" => self.push(SVal::Mark),
            "counttomark" => {
                let Some(p) = self.st.stack.iter().rposition(|v| *v == SVal::Mark) else {
                    return nc("counttomark with no mark");
                };
                let n = (self.st.stack.len() - p - 1) as f64;
                self.push_num(Op::Imm(n), NT::Int);
            }
            "cleartomark" => {
                let Some(p) = self.st.stack.iter().rposition(|v| *v == SVal::Mark) else {
                    return nc("cleartomark with no mark");
                };
                self.st.stack.truncate(p);
            }
            "]" => {
                let Some(p) = self.st.stack.iter().rposition(|v| *v == SVal::Mark) else {
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
                let (b, _) = self.pop_num()?;
                let (a, _) = self.pop_num()?;
                self.bin_to(BinOp::Div, a, b, real)?;
            }
            "idiv" | "mod" => {
                let (b, bt) = self.pop_num()?;
                let (a, at) = self.pop_num()?;
                if self.need_int(at)? && self.need_int(bt)? {
                    let op = if name == "idiv" {
                        BinOp::Idiv
                    } else {
                        BinOp::Mod
                    };
                    self.bin_to(op, a, b, int)?;
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
                if t == NT::Int && op != UnOp::Neg && op != UnOp::Abs {
                    self.push_num(a, t);
                } else {
                    self.un_to(op, a, move |o| SVal::Num(o, t))?;
                }
            }
            "sqrt" | "sin" | "cos" | "ln" | "log" => {
                let (a, _) = self.pop_num()?;
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
                let (b, _) = self.pop_num()?;
                let (a, _) = self.pop_num()?;
                let op = if name == "atan" {
                    BinOp::Atan
                } else {
                    BinOp::Pow
                };
                self.bin_to(op, a, b, real)?;
            }
            "cvi" => {
                let (a, t) = self.pop_num()?;
                if t == NT::Int {
                    self.push_num(a, t);
                } else {
                    self.un_to(UnOp::Cvi, a, int)?;
                }
            }
            "cvr" => {
                let (a, _) = self.pop_num()?;
                self.push_num(a, NT::Real);
            }
            // relational and logic
            "lt" | "le" | "gt" | "ge" => {
                let (b, _) = self.pop_num()?;
                let (a, _) = self.pop_num()?;
                let op = match name {
                    "lt" => BinOp::Lt,
                    "le" => BinOp::Le,
                    "gt" => BinOp::Gt,
                    _ => BinOp::Ge,
                };
                self.bin_to(op, a, b, boolean)?;
            }
            "eq" | "ne" => {
                let b = self.pop()?;
                let a = self.pop()?;
                let op = if name == "eq" { BinOp::Eq } else { BinOp::Ne };
                match (&a, &b) {
                    (SVal::Num(x, _), SVal::Num(y, _)) | (SVal::Bool(x), SVal::Bool(y)) => {
                        self.bin_to(op, *x, *y, boolean)?;
                    }
                    (SVal::Num(..), SVal::Bool(_)) | (SVal::Bool(_), SVal::Num(..)) => {
                        let v = f64::from(u8::from(name == "ne"));
                        self.push(SVal::Bool(Op::Imm(v)));
                    }
                    (SVal::Name(x), SVal::Name(y)) => {
                        let v = f64::from(u8::from((x == y) == (name == "eq")));
                        self.push(SVal::Bool(Op::Imm(v)));
                    }
                    _ => return nc(format!("{name} on {} and {}", describe(&a), describe(&b))),
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
                        if self.need_int(xt)? && self.need_int(yt)? {
                            let op = match name {
                                "and" => BinOp::IAnd,
                                "or" => BinOp::IOr,
                                _ => BinOp::IXor,
                            };
                            self.bin_to(op, x, y, int)?;
                        }
                    }
                    _ => return nc(format!("{name} on {} and {}", describe(&a), describe(&b))),
                }
            }
            "not" => match self.pop()? {
                SVal::Bool(x) => self.un_to(UnOp::BNot, x, boolean)?,
                SVal::Num(x, t) => {
                    if self.need_int(t)? {
                        self.un_to(UnOp::INot, x, int)?;
                    }
                }
                other => return nc(format!("not on {}", describe(&other))),
            },
            "bitshift" => {
                let (s, st) = self.pop_num()?;
                let (x, xt) = self.pop_num()?;
                if self.need_int(xt)? && self.need_int(st)? {
                    self.bin_to(BinOp::Shift, x, s, int)?;
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
                let ct = match (nt, it_, lt) {
                    (NT::Int, NT::Int, NT::Int) => NT::Int,
                    (NT::Real, _, _) | (_, NT::Real, _) | (_, _, NT::Real) => NT::Real,
                    _ => {
                        let t = self.reg();
                        self.emit(Ins::Bin(BinOp::TagAnd, t, nt.tag(), it_.tag()))?;
                        self.emit(Ins::Bin(BinOp::TagAnd, t, Op::R(t), lt.tag()))?;
                        NT::Dyn(t)
                    }
                };
                let c = self.reg();
                self.emit(Ins::Mov(c, init))?;
                self.compile_loop(LoopHead::For { c, ct, inc, lim }, body)?;
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
                let ok = self.reg();
                self.emit(Ins::Bin(BinOp::Ge, ok, n, Op::Imm(0.0)))?;
                self.emit(Ins::Assert(Op::R(ok)))?;
                let k = self.reg();
                self.emit(Ins::Mov(k, n))?;
                self.compile_loop(LoopHead::Repeat { k }, body)?;
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
                match self.lookup(&self.st, n.id(), self.st.scopes.len() - 1)? {
                    Found::Val(v) => self.push(v),
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
            "bind" => {
                let p = self.pop_proc()?;
                self.push(SVal::Proc(p));
            }
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
                    SVal::Row { cols, .. } => cols,
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
                    SVal::Row { cols, .. } => *cols,
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
                let vals = self.st.stack.split_off(self.st.stack.len() - n);
                for (k, v) in vals.into_iter().enumerate() {
                    self.put(c.clone(), Op::Imm(k as f64), NT::Int, v)?;
                }
                self.push(c);
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

    fn noise(&mut self, kind: NoiseKind) -> CResult<()> {
        self.uses_noise = true;
        let mut a = [Op::Imm(0.0); 6];
        let mut octtag = Op::Imm(1.0);
        let dims = kind.dims();
        if kind.octaves() {
            let (lac, _) = self.pop_num()?;
            let (gain, _) = self.pop_num()?;
            let (oct, ot) = self.pop_num()?;
            if ot == NT::Real {
                return self.abort_path();
            }
            octtag = ot.tag();
            a[dims] = oct;
            a[dims + 1] = gain;
            a[dims + 2] = lac;
        }
        for k in (0..dims).rev() {
            a[k] = self.pop_num()?.0;
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
                    let cols = self.tables[t as usize].cols;
                    // Range of the row is checked when the row is read.
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
                            self.emit(Ins::LPut {
                                arr: id,
                                idx: Op::Imm(k as f64),
                                itag: Op::Imm(1.0),
                                v: *o,
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
                let data: Vec<f64> = s.to_vec().into_iter().map(f64::from).collect();
                (
                    Table {
                        rows: 1,
                        cols: data.len(),
                        data,
                        kind: TableKind::Int,
                    },
                    false,
                )
            }
            Value::Array(a) => {
                let items = a.snapshot();
                if items.len() > MAX_TABLE {
                    return nc("a table is too large");
                }
                let nums: Option<Vec<(f64, bool)>> = items.iter().map(num_of).collect();
                if let Some(nums) = nums {
                    (table_of(1, nums.len(), nums), false)
                } else {
                    // An array of equal-length numeric arrays.
                    let mut cols = None;
                    let mut all = Vec::new();
                    for it in &items {
                        let Value::Array(row) = &it.value else {
                            return nc(
                                "run-time index into an array that mixes numbers and other values",
                            );
                        };
                        let r = row.snapshot();
                        if *cols.get_or_insert(r.len()) != r.len() {
                            return nc(
                                "run-time index into an array of arrays of different lengths",
                            );
                        }
                        for e in &r {
                            let Some(n) = num_of(e) else {
                                return nc("run-time index into an array of arrays of non-numbers");
                            };
                            all.push(n);
                        }
                        if all.len() > MAX_TABLE {
                            return nc("a table is too large");
                        }
                    }
                    (table_of(items.len(), cols.unwrap_or(0), all), true)
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

fn num_of(o: &Object) -> Option<(f64, bool)> {
    match o.value {
        Value::Integer(i) if (i as f64).abs() <= INT_LIMIT => Some((i as f64, true)),
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
        data: nums.into_iter().map(|n| n.0).collect(),
        kind,
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
        SVal::Num(..) => "a number",
        SVal::Bool(_) => "a boolean",
        SVal::Mark => "a mark",
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
