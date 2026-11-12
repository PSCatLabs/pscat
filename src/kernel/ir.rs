//! The kernel IR: structured, register-based, one f64 per lane per
//! register. See `docs/dev/design/KERNELS.md`.
//!
//! A register is 64 bits per lane, in one of two *storage kinds* (fixed
//! per register by the compiler, checked by `check.rs` in debug builds):
//!
//! - **I64**: a value statically known to be a PostScript integer, as i64
//!   bits, with the interpreter's exact i64 arithmetic (`IBin`, `IUn`);
//!   an overflow, which the interpreter would promote to a real, retires
//!   the lane.
//! - **F64**: a real, a boolean (0.0 or 1.0), a tag or flag, or a value
//!   whose integer-ness differs per lane (*Dyn*): an f64, with a *tag*
//!   register (1.0 = integer) beside it. A Dyn integer is held exactly,
//!   so it must stay within ±2^53 (`CheckInt`, `I2FExact`).
//!
//! `Op::Imm` is always a *value* (an integer immediate is read as `v as
//! i64` by integer ops), never bits; an integer constant past 2^53 is
//! put in a register with `MovI`.

pub(crate) type Reg = u32;
/// An index into [`Kernel::sites`].
pub(crate) type Site = u32;

/// An instruction operand: a register or an immediate.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Op {
    R(Reg),
    Imm(f64),
}

/// Immediates are equal when their bits are: 0.0 and -0.0 are different
/// values (`-1 exp` of them is inf and -inf), so the compiler must never
/// take one for the other (a branch join that kept one of them on every
/// lane: Codex review 3). A NaN equals the same NaN.
impl PartialEq for Op {
    fn eq(&self, o: &Op) -> bool {
        match (self, o) {
            (Op::R(a), Op::R(b)) => a == b,
            (Op::Imm(a), Op::Imm(b)) => a.to_bits() == b.to_bits(),
            _ => false,
        }
    }
}

impl Op {
    pub(crate) fn uses(&self, r: Reg) -> bool {
        matches!(self, Op::R(x) if *x == r)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BinOp {
    Add,
    Sub,
    Mul,
    /// `div`: aborts on a zero divisor.
    Div,
    Min,
    Max,
    /// Relational: abort on a NaN operand (PostScript's `undefinedresult`).
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    /// Boolean and/or/xor on 0/1 values.
    BAnd,
    BOr,
    BXor,
    /// `num den atan`, degrees in [0, 360); aborts when both are 0.
    Atan,
    /// `base exp exp`; aborts on NaN.
    Pow,
    /// The integer tag of a result from two operand tags (both 0/1).
    TagAnd,
    /// A Dyn `mul`'s result `a` with tag `b`: a lane whose integer (`b`
    /// 1.0) is not exact in an f64 fails, as `Ins::CheckInt`, and an
    /// integer lane's -0.0 becomes +0.0, since an integer has no negative
    /// zero (#1102): `a + 0.0` there, `a` untouched on a real lane (whose
    /// -0.0 or NaN must keep its bits; arithmetic on a NaN may not).
    IntZeroCheck,
    /// A Dyn `neg`: `-a`, plus 0.0 on an integer lane, so an integer 0
    /// negated is 0.
    NegIntZero,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnOp {
    Neg,
    Abs,
    Floor,
    Ceil,
    Round,
    Trunc,
    /// Aborts on a negative operand.
    Sqrt,
    Sin,
    Cos,
    /// Abort on an operand <= 0.
    Ln,
    Log,
    /// Boolean not on 0/1.
    BNot,
}

/// Integer ops on i64 bits (results i64 bits, compares 0.0/1.0 f64).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IBinOp {
    /// Checked: an overflow retires the lane.
    Add,
    Sub,
    Mul,
    Min,
    Max,
    /// Checked: a zero divisor retires the lane.
    Idiv,
    Mod,
    And,
    Or,
    Xor,
    /// `bitshift`.
    Shift,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl IBinOp {
    /// Writes a boolean (F64) rather than an integer.
    pub(crate) fn is_compare(self) -> bool {
        matches!(
            self,
            IBinOp::Lt | IBinOp::Le | IBinOp::Gt | IBinOp::Ge | IBinOp::Eq | IBinOp::Ne
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IUnOp {
    /// Checked.
    Neg,
    Abs,
    /// Bitwise not.
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoiseKind {
    G2,
    G3,
    V2,
    V3,
    G2D,
    Fbm2,
    Fbm3,
    VFbm2,
    VFbm3,
    Ridged2,
    Ridged3,
}

impl NoiseKind {
    pub(crate) fn from_name(name: &str) -> Option<NoiseKind> {
        Some(match name {
            "gnoise2" => NoiseKind::G2,
            "gnoise3" => NoiseKind::G3,
            "vnoise2" => NoiseKind::V2,
            "vnoise3" => NoiseKind::V3,
            "gnoise2d" => NoiseKind::G2D,
            "fbm2" => NoiseKind::Fbm2,
            "fbm3" => NoiseKind::Fbm3,
            "vfbm2" => NoiseKind::VFbm2,
            "vfbm3" => NoiseKind::VFbm3,
            "ridged2" => NoiseKind::Ridged2,
            "ridged3" => NoiseKind::Ridged3,
            _ => return None,
        })
    }

    /// Coordinate count.
    pub(crate) fn dims(self) -> usize {
        match self {
            NoiseKind::G2 | NoiseKind::V2 | NoiseKind::G2D => 2,
            NoiseKind::Fbm2 | NoiseKind::VFbm2 | NoiseKind::Ridged2 => 2,
            _ => 3,
        }
    }

    /// Takes `oct gain lac` after the coordinates.
    pub(crate) fn octaves(self) -> bool {
        !matches!(
            self,
            NoiseKind::G2 | NoiseKind::G3 | NoiseKind::V2 | NoiseKind::V3 | NoiseKind::G2D
        )
    }

    pub(crate) fn outputs(self) -> usize {
        if self == NoiseKind::G2D { 3 } else { 1 }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Ins {
    /// Copy 64 bits (either kind), or write an immediate's f64 value.
    Mov(Reg, Op),
    /// Write an integer constant's i64 bits.
    MovI(Reg, i64),
    Bin(BinOp, Reg, Op, Op),
    Un(UnOp, Reg, Op),
    IBin(IBinOp, Reg, Op, Op),
    IUn(IUnOp, Reg, Op),
    /// Integer to real, as the interpreter's `as f64` (may round).
    I2F(Reg, Op),
    /// Integer to an exact f64 (into a Dyn slot): retires the lane past 2^53.
    I2FExact(Reg, Op),
    /// `cvi` of a real: truncate; retires the lane when not finite or past i64.
    F2I(Reg, Op),
    /// `d = if inc >= 0 { c > limit } else { c < limit }`: `for`'s
    /// termination test, exactly `ForControl::finished`.
    ForFin(Reg, Op, Op, Op),
    /// Abort where `tag` is 1 (an integer) and `|v| > 2^53`.
    CheckInt(Op, Op),
    /// Abort where `tag` is 0 (a real where an integer is needed).
    NeedInt(Op),
    /// Abort where the boolean operand is false. The site
    /// ([`Kernel::sites`]) says what failed, for the retire report.
    Assert(Op, Site),
    /// `d = table[row][col]` (and its integer tag into `dtag` when the
    /// table mixes integers and reals). Aborts where `row` or `col` is
    /// out of range or not an integer (per `rtag`/`ctag`).
    Gather {
        d: Reg,
        dtag: Option<Reg>,
        table: u32,
        row: Op,
        rtag: Op,
        col: Op,
        ctag: Op,
    },
    /// Retire the lanes whose `row` of `table` is out of range or null (an
    /// operation on the row itself, such as `length`, reads no element).
    RowCheck(u32, Op),
    /// Per-lane local array (`N array`): fill every element with null.
    LNew(u32),
    /// `d = arr[idx]`, with its tag; aborts on a null element, a non-integer
    /// or out-of-range index.
    LGet {
        d: Reg,
        dtag: Reg,
        arr: u32,
        idx: Op,
        itag: Op,
    },
    /// `arr[idx] = v` (tag `vtag`); aborts as `LGet` on the index.
    LPut {
        arr: u32,
        idx: Op,
        itag: Op,
        v: Op,
        vtag: Op,
    },
    /// A native noise call. `a` holds the coordinates, then (for an
    /// octave sum) `oct gain lac`; `octtag` is the tag of `oct`.
    Noise {
        kind: NoiseKind,
        d: [Reg; 3],
        a: [Op; 6],
        octtag: Op,
    },
    If(Reg, Vec<Ins>, Vec<Ins>),
    Loop(Vec<Ins>),
    /// Run shared subroutine `id` ([`Kernel::subs`]) for the active lanes,
    /// as if its body stood here. Its parameters are written just before
    /// and its results copied out just after; no `Break` in it leaves it.
    Call(u32),
    /// Leave the innermost loop (the active lanes stop iterating).
    Break,
    /// Every active lane would raise a PostScript error here, or took a
    /// branch that did not compile: the site ([`Kernel::sites`]) says which.
    Abort(Site),
}

impl Ins {
    /// Visit the registers this instruction itself (not a nested block)
    /// reads (`false`) or writes (`true`).
    pub(crate) fn regs_mut(&mut self, f: &mut dyn FnMut(&mut Reg, bool)) {
        let rd = |o: &mut Op, f: &mut dyn FnMut(&mut Reg, bool)| {
            if let Op::R(r) = o {
                f(r, false);
            }
        };
        match self {
            Ins::Mov(d, a)
            | Ins::Un(_, d, a)
            | Ins::IUn(_, d, a)
            | Ins::I2F(d, a)
            | Ins::I2FExact(d, a)
            | Ins::F2I(d, a) => {
                rd(a, f);
                f(d, true);
            }
            Ins::MovI(d, _) => f(d, true),
            Ins::Bin(_, d, a, b) | Ins::IBin(_, d, a, b) => {
                rd(a, f);
                rd(b, f);
                f(d, true);
            }
            Ins::ForFin(d, a, b, c) => {
                rd(a, f);
                rd(b, f);
                rd(c, f);
                f(d, true);
            }
            Ins::CheckInt(a, b) => {
                rd(a, f);
                rd(b, f);
            }
            Ins::NeedInt(a) | Ins::Assert(a, _) | Ins::RowCheck(_, a) => rd(a, f),
            Ins::Gather {
                d,
                dtag,
                row,
                rtag,
                col,
                ctag,
                ..
            } => {
                for x in [row, rtag, col, ctag] {
                    rd(x, f);
                }
                f(d, true);
                if let Some(t) = dtag {
                    f(t, true);
                }
            }
            Ins::LGet {
                d, dtag, idx, itag, ..
            } => {
                rd(idx, f);
                rd(itag, f);
                f(d, true);
                f(dtag, true);
            }
            Ins::LPut {
                idx, itag, v, vtag, ..
            } => {
                for x in [idx, itag, v, vtag] {
                    rd(x, f);
                }
            }
            Ins::Noise { kind, d, a, octtag } => {
                for x in a.iter_mut() {
                    rd(x, f);
                }
                rd(octtag, f);
                for r in d.iter_mut().take(kind.outputs()) {
                    f(r, true);
                }
            }
            Ins::If(c, ..) => f(c, false),
            Ins::Loop(_) | Ins::Call(_) | Ins::LNew(_) | Ins::Break | Ins::Abort(_) => {}
        }
    }

    /// As [`Ins::regs_mut`], reading only.
    pub(crate) fn regs(&self, f: &mut dyn FnMut(Reg, bool)) {
        let rd = |o: &Op, f: &mut dyn FnMut(Reg, bool)| {
            if let Op::R(r) = o {
                f(*r, false);
            }
        };
        match self {
            Ins::Mov(d, a)
            | Ins::Un(_, d, a)
            | Ins::IUn(_, d, a)
            | Ins::I2F(d, a)
            | Ins::I2FExact(d, a)
            | Ins::F2I(d, a) => {
                rd(a, f);
                f(*d, true);
            }
            Ins::MovI(d, _) => f(*d, true),
            Ins::Bin(_, d, a, b) | Ins::IBin(_, d, a, b) => {
                rd(a, f);
                rd(b, f);
                f(*d, true);
            }
            Ins::ForFin(d, a, b, c) => {
                rd(a, f);
                rd(b, f);
                rd(c, f);
                f(*d, true);
            }
            Ins::CheckInt(a, b) => {
                rd(a, f);
                rd(b, f);
            }
            Ins::NeedInt(a) | Ins::Assert(a, _) | Ins::RowCheck(_, a) => rd(a, f),
            Ins::Gather {
                d,
                dtag,
                row,
                rtag,
                col,
                ctag,
                ..
            } => {
                for x in [row, rtag, col, ctag] {
                    rd(x, f);
                }
                f(*d, true);
                if let Some(t) = dtag {
                    f(*t, true);
                }
            }
            Ins::LGet {
                d, dtag, idx, itag, ..
            } => {
                rd(idx, f);
                rd(itag, f);
                f(*d, true);
                f(*dtag, true);
            }
            Ins::LPut {
                idx, itag, v, vtag, ..
            } => {
                for x in [idx, itag, v, vtag] {
                    rd(x, f);
                }
            }
            Ins::Noise { kind, d, a, octtag } => {
                for x in a {
                    rd(x, f);
                }
                rd(octtag, f);
                for r in d.iter().take(kind.outputs()) {
                    f(*r, true);
                }
            }
            Ins::If(c, ..) => f(*c, false),
            Ins::Loop(_) | Ins::Call(_) | Ins::LNew(_) | Ins::Break | Ins::Abort(_) => {}
        }
    }

    /// The nested blocks of an `If` or `Loop`.
    pub(crate) fn blocks(&self) -> Vec<&Vec<Ins>> {
        match self {
            Ins::If(_, t, e) => vec![t, e],
            Ins::Loop(b) => vec![b],
            _ => Vec::new(),
        }
    }

    pub(crate) fn blocks_mut(&mut self) -> Vec<&mut Vec<Ins>> {
        match self {
            Ins::If(_, t, e) => vec![t, e],
            Ins::Loop(b) => vec![b],
            _ => Vec::new(),
        }
    }
}

/// Visit every instruction of `b`, nested blocks included (outer first).
pub(crate) fn walk(b: &[Ins], f: &mut dyn FnMut(&Ins)) {
    for i in b {
        f(i);
        for nb in i.blocks() {
            walk(nb, f);
        }
    }
}

/// Visit every register occurrence in `b`, nested blocks included.
pub(crate) fn walk_regs_mut(b: &mut [Ins], f: &mut dyn FnMut(&mut Reg, bool)) {
    for i in b {
        i.regs_mut(f);
        for nb in i.blocks_mut() {
            walk_regs_mut(nb, f);
        }
    }
}

/// A shared subroutine: a procedure body compiled once and run by `Call`
/// from each place it is used with the same symbolic context (#1064).
#[derive(Clone, Debug)]
pub(crate) struct Sub {
    pub body: Vec<Ins>,
    /// Written by the caller just before each `Call`.
    pub params: Vec<Reg>,
    /// Read by the caller just after each `Call`.
    pub results: Vec<Reg>,
}

/// A table captured from the program's data at compile time: an array or
/// string of numbers (one row), or an array of equal-length numeric arrays.
#[derive(Clone, Debug)]
pub(crate) struct Table {
    pub rows: usize,
    pub cols: usize,
    pub data: TableData,
    pub kind: TableKind,
    /// Rows that are `null` in the program's array (reading one is the
    /// interpreter's `typecheck`, so the kernel aborts). Empty when none.
    pub null_rows: Vec<bool>,
    /// Built from strings (each row a string): `aload` of a row is a
    /// `typecheck`, as for any string.
    pub strings: bool,
    /// A one-level array of booleans, held as 0 and 1 (`Real` kind): a
    /// gather from it is a boolean, as a boolean register holds it.
    pub bools: bool,
}

/// A table's elements: f64s, or bytes (a string: an image read with
/// `readstring` is a table of bytes, kept at a byte each).
#[derive(Clone, Debug)]
pub(crate) enum TableData {
    F64(Vec<f64>),
    U8(Vec<u8>),
}

impl TableData {
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn at(&self, i: usize) -> f64 {
        match self {
            TableData::F64(v) => v[i],
            TableData::U8(v) => f64::from(v[i]),
        }
    }

    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    pub(crate) fn len(&self) -> usize {
        match self {
            TableData::F64(v) => v.len(),
            TableData::U8(v) => v.len(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum TableKind {
    Int,
    Real,
    /// Per-element tags, 1.0 for an integer.
    Mixed(Vec<f64>),
}

/// A compiled kernel: plain data, `Send`, shared by the worker threads.
#[derive(Clone, Debug)]
pub(crate) struct Kernel {
    pub nregs: usize,
    pub body: Vec<Ins>,
    /// Shared subroutines, run by `Ins::Call`. A subroutine calls only
    /// subroutines of lower index.
    pub subs: Vec<Sub>,
    /// Which body owns each register: 0 for `body`, `s + 1` for `subs[s]`
    /// (empty when there are no subroutines). A subroutine's registers
    /// occur only in its own body, except that its callers write its
    /// parameters and read its results.
    pub owner: Vec<u32>,
    /// Register holding `i` (column) and `j` (row).
    pub in_i: Reg,
    pub in_j: Reg,
    /// The output components, in order. A [`Kernel::typed`] kernel's has
    /// `n` values then their `n` tags.
    pub outputs: Vec<Op>,
    /// The outputs are typed values (`kernelarray`, #1102), not components
    /// to paint: `outputs` holds the `n` values, then a tag for each (as
    /// `NT::tag`: 1.0 an integer, 0.0 a real, or an F64 register holding
    /// that per lane). An integer value whose tag is the immediate 1.0 is
    /// an I64 register's bits (or an immediate); under a tag register it is
    /// an F64 holding the integer exactly (a Dyn value).
    pub typed: bool,
    /// The kernel calls `srand` (issue #1063): the random state a pixel
    /// leaves (an integer, I64), or -1 where it never seeded. A pixel that
    /// seeds draws only from its own seed, so pixels stay independent; the
    /// state the last pixel leaves is the interpreter's state afterwards.
    pub rand: Option<Op>,
    pub tables: Vec<Table>,
    /// Sizes of the per-lane local arrays.
    pub larrays: Vec<usize>,
    pub perm: [u8; 256],
    /// What each `Abort` or `Assert` site stands for, with the named
    /// procedures it was compiled in: the report of why pixels retired.
    pub sites: Vec<String>,
    /// Per body (0 the kernel's, `s + 1` subroutine `s`'s), the positions
    /// (numbered as `regalloc` numbers them) of the writes that start a
    /// register's live interval. On the lanes such a write leaves alone the
    /// register holds nothing live, so the executor may write every lane
    /// (#1101). Empty when the registers were not allocated.
    pub fresh: Vec<Vec<u32>>,
}

impl Kernel {
    /// Instruction count, counting nested blocks and each subroutine once.
    pub(crate) fn size(&self) -> usize {
        count(&self.body) + self.subs.iter().map(|s| count(&s.body)).sum::<usize>()
    }
}

/// Instruction count of a block, nested blocks included.
pub(crate) fn count(b: &[Ins]) -> usize {
    b.iter()
        .map(|i| match i {
            Ins::If(_, t, e) => 1 + count(t) + count(e),
            Ins::Loop(body) => 1 + count(body),
            _ => 1,
        })
        .sum()
}

#[cfg(test)]
mod size_tests {
    /// The size cap's memory bound (`MAX_INSNS` in compile.rs) assumes
    /// 128 bytes an instruction.
    #[test]
    fn an_instruction_is_at_most_128_bytes() {
        assert!(std::mem::size_of::<super::Ins>() <= 128);
    }
}
