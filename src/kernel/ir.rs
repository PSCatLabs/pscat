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

/// An instruction operand: a register or an immediate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Op {
    R(Reg),
    Imm(f64),
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
    /// Abort where the boolean operand is false.
    Assert(Op),
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
    /// Leave the innermost loop (the active lanes stop iterating).
    Break,
    /// Every active lane would raise a PostScript error here.
    Abort,
}

/// A table captured from the program's data at compile time: an array or
/// string of numbers (one row), or an array of equal-length numeric arrays.
#[derive(Clone, Debug)]
pub(crate) struct Table {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f64>,
    pub kind: TableKind,
    /// Rows that are `null` in the program's array (reading one is the
    /// interpreter's `typecheck`, so the kernel aborts). Empty when none.
    pub null_rows: Vec<bool>,
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
    /// Register holding `i` (column) and `j` (row).
    pub in_i: Reg,
    pub in_j: Reg,
    /// The output components, in order.
    pub outputs: Vec<Op>,
    pub tables: Vec<Table>,
    /// Sizes of the per-lane local arrays.
    pub larrays: Vec<usize>,
    pub perm: [u8; 256],
}

impl Kernel {
    /// Instruction count, counting nested blocks.
    pub(crate) fn size(&self) -> usize {
        fn count(b: &[Ins]) -> usize {
            b.iter()
                .map(|i| match i {
                    Ins::If(_, t, e) => 1 + count(t) + count(e),
                    Ins::Loop(body) => 1 + count(body),
                    _ => 1,
                })
                .sum()
        }
        count(&self.body)
    }
}
