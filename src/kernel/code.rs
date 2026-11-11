//! The executor's compact form of a kernel (#1101).
//!
//! The IR ([`Ins`]) is a tree of up-to-128-byte instructions, with each
//! nested block a separate allocation. A large kernel (fractal_clouds'
//! golden_coast: 170k instructions) walks nearly all of it for every batch,
//! so that stride streamed tens of MB from memory per batch. The executor
//! runs this lowered form instead: one flat array of 16-byte instructions,
//! with nested blocks inline after the `If` or `Loop` that owns them and the
//! subroutines placed before the body. Rarely run, wide instructions keep
//! their IR form in a side table (`L::Ext`).
//!
//! An operand is a register number, or [`IMM`] plus an index into
//! [`Code::consts`], which holds each immediate's f64 *value* exactly as
//! `Op::Imm` did: an integer op still reads it as `v as i64`, and a register
//! by its bits, so the two read kinds stay as the IR defines them.

use std::collections::HashMap;

use super::ir::{BinOp, IBinOp, IUnOp, Ins, Kernel, Op, Reg, Site, UnOp};

/// Set in an operand for an immediate: the rest indexes [`Code::consts`].
pub(crate) const IMM: u32 = 1 << 31;

/// A lowered instruction. Fields as in [`Ins`]; operands are words (see
/// [`IMM`]). A block's length counts its instructions, nested ones
/// included, so the instruction after it is at `pc + 1 + len`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum L {
    Mov(Reg, u32),
    /// The operand's constant holds the integer's bits.
    MovI(Reg, u32),
    Bin(BinOp, Reg, u32, u32),
    Un(UnOp, Reg, u32),
    IBin(IBinOp, Reg, u32, u32),
    IUn(IUnOp, Reg, u32),
    I2F(Reg, u32),
    I2FExact(Reg, u32),
    F2I(Reg, u32),
    CheckInt(u32, u32),
    NeedInt(u32),
    Assert(u32, Site),
    /// Condition register, then- and else-block lengths; the then-block
    /// follows, then the else-block.
    If(Reg, u32, u32),
    /// Body length; the body follows.
    Loop(u32),
    /// A subroutine's start and length in [`Code::ins`].
    Call(u32, u32),
    Break,
    Abort(Site),
    /// [`Code::ext`]`[n]`, run from its IR form.
    Ext(u32),
}

/// A kernel lowered for the CPU executor. Built once per run and shared,
/// read-only, by every worker.
pub(crate) struct Code {
    pub ins: Vec<L>,
    pub consts: Vec<f64>,
    pub ext: Vec<Ins>,
    /// Where the kernel body is in `ins`.
    pub body: (u32, u32),
}

impl Code {
    pub(crate) fn new(k: &Kernel) -> Code {
        let mut lw = Lower {
            ins: Vec::with_capacity(k.size()),
            consts: Vec::new(),
            seen: HashMap::new(),
            ext: Vec::new(),
            subs: Vec::with_capacity(k.subs.len()),
        };
        // A subroutine calls only lower-numbered ones: lowering them in
        // order places each callee before its callers need its address.
        for s in &k.subs {
            let start = lw.ins.len() as u32;
            lw.block(&s.body);
            lw.subs.push((start, lw.ins.len() as u32 - start));
        }
        let start = lw.ins.len() as u32;
        lw.block(&k.body);
        let body = (start, lw.ins.len() as u32 - start);
        Code {
            ins: lw.ins,
            consts: lw.consts,
            ext: lw.ext,
            body,
        }
    }
}

struct Lower {
    ins: Vec<L>,
    consts: Vec<f64>,
    /// Each immediate's index by its bits (so 0.0 and -0.0 stay apart).
    seen: HashMap<u64, u32>,
    ext: Vec<Ins>,
    subs: Vec<(u32, u32)>,
}

impl Lower {
    fn konst(&mut self, bits: u64) -> u32 {
        let n = self.consts.len() as u32;
        let i = *self.seen.entry(bits).or_insert(n);
        if i == n {
            self.consts.push(f64::from_bits(bits));
        }
        debug_assert!(i < IMM);
        i | IMM
    }

    fn op(&mut self, o: Op) -> u32 {
        match o {
            Op::R(r) => {
                debug_assert!(r < IMM);
                r
            }
            Op::Imm(x) => self.konst(x.to_bits()),
        }
    }

    fn block(&mut self, b: &[Ins]) {
        for i in b {
            self.one(i);
        }
    }

    fn one(&mut self, i: &Ins) {
        let l = match *i {
            Ins::Mov(d, a) => L::Mov(d, self.op(a)),
            Ins::MovI(d, v) => L::MovI(d, self.konst(v as u64)),
            Ins::Bin(op, d, a, b) => L::Bin(op, d, self.op(a), self.op(b)),
            Ins::Un(op, d, a) => L::Un(op, d, self.op(a)),
            Ins::IBin(op, d, a, b) => L::IBin(op, d, self.op(a), self.op(b)),
            Ins::IUn(op, d, a) => L::IUn(op, d, self.op(a)),
            Ins::I2F(d, a) => L::I2F(d, self.op(a)),
            Ins::I2FExact(d, a) => L::I2FExact(d, self.op(a)),
            Ins::F2I(d, a) => L::F2I(d, self.op(a)),
            Ins::CheckInt(v, t) => L::CheckInt(self.op(v), self.op(t)),
            Ins::NeedInt(t) => L::NeedInt(self.op(t)),
            Ins::Assert(c, s) => L::Assert(self.op(c), s),
            Ins::If(c, ref t, ref e) => {
                let at = self.ins.len();
                self.ins.push(L::Break);
                let t0 = self.ins.len();
                self.block(t);
                let tl = (self.ins.len() - t0) as u32;
                let e0 = self.ins.len();
                self.block(e);
                let el = (self.ins.len() - e0) as u32;
                self.ins[at] = L::If(c, tl, el);
                return;
            }
            Ins::Loop(ref body) => {
                let at = self.ins.len();
                self.ins.push(L::Break);
                self.block(body);
                self.ins[at] = L::Loop((self.ins.len() - at - 1) as u32);
                return;
            }
            Ins::Call(id) => {
                let (s, n) = self.subs[id as usize];
                L::Call(s, n)
            }
            Ins::Break => L::Break,
            Ins::Abort(s) => L::Abort(s),
            Ins::ForFin(..)
            | Ins::Gather { .. }
            | Ins::RowCheck(..)
            | Ins::LNew(_)
            | Ins::LGet { .. }
            | Ins::LPut { .. }
            | Ins::Noise { .. } => {
                self.ext.push(i.clone());
                L::Ext(self.ext.len() as u32 - 1)
            }
        };
        self.ins.push(l);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_lowered_instruction_is_16_bytes() {
        assert_eq!(std::mem::size_of::<super::L>(), 16);
    }
}
