//! The kernel IR: structured, register-based, one f64 per lane per
//! register. See `docs/dev/design/KERNELS.md`.
//!
//! Every value is an f64. Booleans are 0.0 or 1.0. An integer is held
//! exactly as an f64 while it stays within ±2^53. Past that the kernel
//! aborts to the interpreter, which keeps i64 semantics. Whether a value
//! is an integer is known statically in most places; where branches merge
//! an integer and a real, the compiler carries a *tag* register (1.0 =
//! integer, 0.0 = real) next to it.

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
    /// `idiv`, `mod` on integers (operands already checked to be integers).
    Idiv,
    Mod,
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
    /// Bitwise and/or/xor on integers.
    IAnd,
    IOr,
    IXor,
    /// `bitshift` on integers.
    Shift,
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
    /// Bitwise not on an integer.
    INot,
    /// `cvi` on a real: truncate; aborts when not finite or past ±2^53.
    Cvi,
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
    Mov(Reg, Op),
    Bin(BinOp, Reg, Op, Op),
    Un(UnOp, Reg, Op),
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
