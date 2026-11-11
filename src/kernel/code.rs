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

/// Set in a destination register for a write that may cover every lane
/// ([`Kernel::fresh`]): the executor then skips the blend that keeps the
/// lanes it does not run.
pub(crate) const FRESH: u32 = 1 << 31;

/// The one register a straight-line instruction the lowering marks
/// [`FRESH`] writes.
pub(crate) fn dest(i: &Ins) -> Option<Reg> {
    match *i {
        Ins::Mov(d, _)
        | Ins::MovI(d, _)
        | Ins::Bin(_, d, ..)
        | Ins::Un(_, d, _)
        | Ins::IBin(_, d, ..)
        | Ins::IUn(_, d, _)
        | Ins::I2F(d, _)
        | Ins::I2FExact(d, _)
        | Ins::F2I(d, _)
        | Ins::Gather { d, .. } => Some(d),
        _ => None,
    }
}

/// A lowered instruction. Fields as in [`Ins`]; operands are words (see
/// [`IMM`]). A block's length counts its instructions, nested ones
/// included, so the instruction after it is at `pc + 1 + len`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum L {
    Mov(Reg, u32),
    /// `Mov` from a register (#1101).
    MovR(Reg, Reg),
    /// The operand's constant holds the integer's bits.
    MovI(Reg, u32),
    Bin(BinOp, Reg, u32, u32),
    /// `Bin` add, sub and mul (#1101): the commonest instructions, with
    /// no second dispatch on the operator, and they cannot fail.
    Add(Reg, u32, u32),
    Sub(Reg, u32, u32),
    Mul(Reg, u32, u32),
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
    /// `d = table[row][col]` where both index tags are known constants and
    /// one index is a good constant, so only the other can be bad:
    /// [`Code::gathers`]`[n]`, then the varying index's operand. Most of a
    /// large kernel's table reads (#1101).
    Gather1(Reg, u32, u32),
}

/// A [`L::Gather1`]: element `x * scale + base` of `table`, for an index
/// `x` that must be below `bound`, and the register for the element's tag
/// (for a table mixing integers and reals). `rows` is set when `x` is the
/// row, so a null row retires the lane.
#[derive(Clone, Copy, Debug)]
pub(crate) struct G1 {
    pub table: u32,
    pub base: u32,
    pub scale: u32,
    pub bound: u32,
    pub rows: bool,
    pub dtag: Option<Reg>,
}

/// A kernel lowered for the CPU executor. Built once per run and shared,
/// read-only, by every worker.
pub(crate) struct Code {
    pub ins: Vec<L>,
    pub consts: Vec<f64>,
    pub ext: Vec<Ins>,
    pub gathers: Vec<G1>,
    /// Where the kernel body is in `ins`.
    pub body: (u32, u32),
}

impl Code {
    pub(crate) fn new(k: &Kernel) -> Code {
        let mut lw = Lower {
            k,
            gathers: Vec::new(),
            ins: Vec::with_capacity(k.size()),
            consts: Vec::new(),
            seen: HashMap::new(),
            ext: Vec::new(),
            subs: Vec::with_capacity(k.subs.len()),
            pos: 1,
            fresh: &[],
        };
        // A subroutine calls only lower-numbered ones: lowering them in
        // order places each callee before its callers need its address.
        for (n, s) in k.subs.iter().enumerate() {
            lw.body(n + 1);
            let start = lw.ins.len() as u32;
            lw.block(&s.body);
            lw.subs.push((start, lw.ins.len() as u32 - start));
        }
        lw.body(0);
        let start = lw.ins.len() as u32;
        lw.block(&k.body);
        let body = (start, lw.ins.len() as u32 - start);
        Code {
            ins: lw.ins,
            consts: lw.consts,
            ext: lw.ext,
            gathers: lw.gathers,
            body,
        }
    }
}

struct Lower<'k> {
    k: &'k Kernel,
    /// The position of the next instruction, numbered as `regalloc` does.
    pos: usize,
    /// The current body's [`Kernel::fresh`] positions not yet passed.
    fresh: &'k [u32],
    gathers: Vec<G1>,
    ins: Vec<L>,
    consts: Vec<f64>,
    /// Each immediate's index by its bits (so 0.0 and -0.0 stay apart).
    seen: HashMap<u64, u32>,
    ext: Vec<Ins>,
    subs: Vec<(u32, u32)>,
}

impl Lower<'_> {
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

    /// `table`'s element indices fit a u32.
    fn fits(&self, table: u32) -> bool {
        self.k
            .tables
            .get(table as usize)
            .is_some_and(|tb| tb.rows.saturating_mul(tb.cols) < u32::MAX as usize)
    }

    /// A constant index below `n`.
    fn good(&self, x: f64, n: usize) -> bool {
        x >= 0.0 && x.fract() == 0.0 && (x as usize) < n
    }

    /// Row `r` of `table` is null.
    fn null(&self, table: u32, r: f64) -> bool {
        self.k.tables[table as usize]
            .null_rows
            .get(r as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Start lowering body `g` (see [`Kernel::fresh`]).
    fn body(&mut self, g: usize) {
        self.pos = 1;
        self.fresh = self.k.fresh.get(g).map_or(&[], Vec::as_slice);
    }

    /// Whether the write at the current position covers every lane.
    fn is_fresh(&mut self, p: usize) -> bool {
        while let Some((&f, rest)) = self.fresh.split_first() {
            if (f as usize) < p {
                self.fresh = rest;
            } else {
                return f as usize == p;
            }
        }
        false
    }

    fn block(&mut self, b: &[Ins]) {
        for i in b {
            self.one(i);
        }
    }

    fn one(&mut self, i: &Ins) {
        let p = self.pos;
        self.pos += 1;
        let fresh = !matches!(i, Ins::If(..) | Ins::Loop(_)) && self.is_fresh(p);
        let fr = |d: Reg| if fresh { d | FRESH } else { d };
        let l = match *i {
            Ins::Mov(d, Op::R(s)) => L::MovR(fr(d), s),
            Ins::Mov(d, a) => L::Mov(fr(d), self.op(a)),
            Ins::MovI(d, v) => L::MovI(fr(d), self.konst(v as u64)),
            Ins::Bin(BinOp::Add, d, a, b) => L::Add(fr(d), self.op(a), self.op(b)),
            Ins::Bin(BinOp::Sub, d, a, b) => L::Sub(fr(d), self.op(a), self.op(b)),
            // An integer tag's `and` of two 0/1 tags is their product.
            Ins::Bin(BinOp::Mul | BinOp::TagAnd, d, a, b) => L::Mul(fr(d), self.op(a), self.op(b)),
            Ins::Bin(op, d, a, b) => L::Bin(op, fr(d), self.op(a), self.op(b)),
            Ins::Un(op, d, a) => L::Un(op, fr(d), self.op(a)),
            Ins::IBin(op, d, a, b) => L::IBin(op, fr(d), self.op(a), self.op(b)),
            Ins::IUn(op, d, a) => L::IUn(op, fr(d), self.op(a)),
            Ins::I2F(d, a) => L::I2F(fr(d), self.op(a)),
            Ins::I2FExact(d, a) => L::I2FExact(fr(d), self.op(a)),
            Ins::F2I(d, a) => L::F2I(fr(d), self.op(a)),
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
                self.pos += 1;
                self.ins[at] = L::Loop((self.ins.len() - at - 1) as u32);
                return;
            }
            Ins::Call(id) => {
                let (s, n) = self.subs[id as usize];
                L::Call(s, n)
            }
            Ins::Break => L::Break,
            Ins::Abort(s) => L::Abort(s),
            Ins::Gather {
                d,
                dtag,
                table,
                row,
                rtag: Op::Imm(rt),
                col,
                ctag: Op::Imm(ct),
            } if rt != 0.0 && ct != 0.0 && self.fits(table) => {
                let tb = &self.k.tables[table as usize];
                let g = match (row, col) {
                    (Op::Imm(r), x) if self.good(r, tb.rows) && !self.null(table, r) => Some((
                        x,
                        G1 {
                            table,
                            base: (r as usize * tb.cols) as u32,
                            scale: 1,
                            bound: tb.cols as u32,
                            rows: false,
                            dtag,
                        },
                    )),
                    (x, Op::Imm(c)) if self.good(c, tb.cols) => Some((
                        x,
                        G1 {
                            table,
                            base: c as u32,
                            scale: tb.cols as u32,
                            bound: tb.rows as u32,
                            rows: true,
                            dtag,
                        },
                    )),
                    _ => None,
                };
                match g {
                    Some((x, g)) => {
                        self.gathers.push(g);
                        L::Gather1(fr(d), self.gathers.len() as u32 - 1, self.op(x))
                    }
                    None => {
                        self.ext.push(i.clone());
                        L::Ext(self.ext.len() as u32 - 1)
                    }
                }
            }
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
