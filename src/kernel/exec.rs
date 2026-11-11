//! The lane-batched CPU executor (design: `docs/dev/design/KERNELS.md`).
//!
//! A batch is up to [`LANES`] pixels of one row. Each IR instruction runs
//! as one tight loop over the batch, so its dispatch cost is shared by every
//! pixel, and the plain arithmetic loops vectorise. Control flow is by
//! masks (one bit per lane): an `If` runs a branch only for the lanes that
//! take it (skipping it when none do), and a `Loop` runs until no lane is
//! left in it. A write stores only the active lanes, so a lane that took
//! the other branch, or has left the loop, keeps its values.
//!
//! Exactness: each lane computes the interpreter's own expression in the
//! interpreter's order (see `lane_bin`/`lane_un`), with the same `fmath`
//! transcendentals and the same noise functions. There is no fused
//! multiply-add and no cross-lane arithmetic. A lane that would raise a
//! PostScript error (or takes a branch that did not compile) is retired
//! (`Machine::kill`); the caller then runs those pixels through the
//! interpreter, which raises the error for real. Each retired lane is
//! counted under its reason ([`Why`], or the `Abort`/`Assert` site), which
//! costs nothing until a lane retires.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::code::{Code, FRESH, IMM, L};
use super::ir::{BinOp, IBinOp, IUnOp, Ins, Kernel, Op, Reg, Site, TableData, TableKind, UnOp};
use crate::error::PsError;
use crate::fmath;

pub(crate) const LANES: usize = 16;
const FULL: u64 = if LANES == 64 {
    u64::MAX
} else {
    (1u64 << LANES) - 1
};
/// Integers are held exactly in an f64 up to here.
///
/// The bound is strict: every integer of magnitude *below* 2^53 is exact in
/// an f64, but 2^53 itself is also what 2^53 + 1 rounds to, so a value
/// that reaches it may already have lost its last bit.
pub(crate) const INT_LIMIT: f64 = 9_007_199_254_740_992.0; // 2^53
/// The bound for an i64, checked *before* conversion: every integer of
/// magnitude up to and including 2^53 converts exactly.
pub(crate) const INT_LIMIT_I: u64 = 1 << 53;
/// Element tag of a local array slot that holds PostScript's null.
const NULL_TAG: f64 = 2.0;

type Lane = [f64; LANES];

/// Why an instruction retired a lane, for the instructions whose reason
/// is fixed by their kind and operator. `Abort` and `Assert` carry a site
/// instead ([`Kernel::sites`]); a reason code is a `Why` below [`NWHY`],
/// else `NWHY` plus a site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Why {
    IntOverflow,
    IntDivide,
    IntNeg,
    IntInexact,
    CviRange,
    DivZero,
    CompareNan,
    Atan00,
    ExpUndefined,
    SqrtNeg,
    LnNonPos,
    LogNonPos,
    IntPast53,
    NeedInt,
    TableIndex,
    TableNull,
    RowBad,
    LocalIndex,
    LocalNull,
    LocalPutIndex,
    NoiseOperand,
}

/// How many [`Why`] reasons there are: site codes start here.
pub(crate) const NWHY: u32 = Why::NoiseOperand as u32 + 1;

impl Why {
    pub(crate) fn text(self) -> &'static str {
        match self {
            Why::IntOverflow => {
                "an integer add, sub or mul past 64 bits (the interpreter makes it a real)"
            }
            Why::IntDivide => "idiv or mod by zero (or of the most negative integer by -1)",
            Why::IntNeg => {
                "neg or abs of the most negative integer (the interpreter makes it a real)"
            }
            Why::IntInexact => {
                "an integer past 2^53 converted where the value is integer on some lanes only"
            }
            Why::CviRange => "cvi of a value out of integer range",
            Why::DivZero => "div by zero",
            Why::CompareNan => "an ordered compare of NaN",
            Why::Atan00 => "0 0 atan",
            Why::ExpUndefined => "exp with an undefined result",
            Why::SqrtNeg => "sqrt of a negative",
            Why::LnNonPos => "ln of zero or less",
            Why::LogNonPos => "log of zero or less",
            Why::IntPast53 => "an integer result past 2^53",
            Why::NeedInt => "a real where an integer is needed",
            Why::TableIndex => "a table index out of range or not an integer",
            Why::TableNull => "a table row that is null",
            Why::RowBad => "an operation on a table row that is null or out of range",
            Why::LocalIndex => "a local array index out of range or not an integer",
            Why::LocalNull => "a null element of a local array",
            Why::LocalPutIndex => "a put into a local array out of range or not an integer",
            Why::NoiseOperand => "a noise operand out of range",
        }
    }

    const ALL: [Why; NWHY as usize] = [
        Why::IntOverflow,
        Why::IntDivide,
        Why::IntNeg,
        Why::IntInexact,
        Why::CviRange,
        Why::DivZero,
        Why::CompareNan,
        Why::Atan00,
        Why::ExpUndefined,
        Why::SqrtNeg,
        Why::LnNonPos,
        Why::LogNonPos,
        Why::IntPast53,
        Why::NeedInt,
        Why::TableIndex,
        Why::TableNull,
        Why::RowBad,
        Why::LocalIndex,
        Why::LocalNull,
        Why::LocalPutIndex,
        Why::NoiseOperand,
    ];

    /// The reason a failing `op` gives (`lane_bin`'s `None` cases).
    pub(crate) fn bin(op: BinOp) -> Why {
        match op {
            BinOp::Div => Why::DivZero,
            BinOp::Atan => Why::Atan00,
            BinOp::Pow => Why::ExpUndefined,
            _ => Why::CompareNan,
        }
    }

    /// The reason a failing `op` gives (`lane_un`'s `None` cases).
    pub(crate) fn un(op: UnOp) -> Why {
        match op {
            UnOp::Ln => Why::LnNonPos,
            UnOp::Log => Why::LogNonPos,
            _ => Why::SqrtNeg,
        }
    }
}

/// The text of reason code `code` in kernel `k`.
pub(crate) fn why_text(k: &Kernel, code: usize) -> &str {
    match Why::ALL.get(code) {
        Some(w) => w.text(),
        None => k
            .sites
            .get(code - NWHY as usize)
            .map_or("an unknown reason", String::as_str),
    }
}

/// The reason code of site `s`.
#[inline]
fn site(s: Site) -> u32 {
    NWHY + s
}

/// Why a kernel run stopped early.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stop {
    /// A limit (`--max-time`, `--max-steps`) fired.
    Limit(PsError),
    /// Another worker stopped the run, or the compiled run would not fit
    /// the memory limit: the interpreter does the image instead.
    Cancelled,
}

/// Shared run state: limits, the step count and the stop flag.
pub(crate) struct RunCtl {
    pub deadline_ns: Option<u64>,
    pub max_steps: Option<u64>,
    pub steps: AtomicU64,
    pub stop: AtomicBool,
}

impl RunCtl {
    pub(crate) fn new(deadline_ns: Option<u64>, max_steps: Option<u64>) -> Self {
        RunCtl {
            deadline_ns,
            max_steps,
            steps: AtomicU64::new(0),
            stop: AtomicBool::new(false),
        }
    }

    /// Charge `n` steps and look at the limits.
    pub(crate) fn poll(&self, n: u64) -> Result<(), Stop> {
        let used = self.steps.fetch_add(n, Ordering::Relaxed) + n;
        if self.stop.load(Ordering::Relaxed) {
            return Err(Stop::Cancelled);
        }
        if let Some(max) = self.max_steps
            && used > max
        {
            return Err(Stop::Limit(PsError::StepsExceeded));
        }
        if let Some(d) = self.deadline_ns
            && crate::clock::Clock::mono_ns() >= d
        {
            return Err(Stop::Limit(PsError::TimeExceeded));
        }
        Ok(())
    }
}

/// Loop iterations between looks at the limits.
const POLL_ITERS: u32 = 256;
/// Lane-instructions run between polls at subroutine calls.
const POLL_STEPS: u64 = 1 << 16;

/// One worker's machine: a register file and local-array storage.
pub(crate) struct Machine<'k> {
    k: &'k Kernel,
    /// The kernel as the executor runs it (#1101).
    c: &'k Code,
    regs: Vec<Lane>,
    /// Per local array: element values then element tags, `n` lanes each.
    larr: Vec<(Vec<Lane>, Vec<Lane>)>,
    /// Alive masks of the loops being run, innermost last.
    alive: Vec<u64>,
    ctl: &'k RunCtl,
    /// Steps since the last poll.
    pending: u64,
    /// Lanes of this batch retired because they would raise a PostScript
    /// error (or took a branch that did not compile): their pixels are
    /// run by the interpreter afterwards.
    killed: u64,
    /// Lanes retired so far (every batch this machine ran), by reason code
    /// (see [`Why`]).
    pub(crate) retired: Vec<u64>,
    /// Instructions run (each once per batch, whatever its mask), for the
    /// lane occupancy `PSCAT_KERNEL_TIMING` reports: the steps over this
    /// times `LANES` (#1101).
    pub(crate) dispatched: u64,
}

impl<'k> Machine<'k> {
    pub(crate) fn new(k: &'k Kernel, c: &'k Code, ctl: &'k RunCtl) -> Self {
        Machine {
            k,
            c,
            regs: vec![[0.0; LANES]; k.nregs.max(1)],
            larr: k
                .larrays
                .iter()
                .map(|&n| (vec![[0.0; LANES]; n], vec![[NULL_TAG; LANES]; n]))
                .collect(),
            alive: Vec::new(),
            ctl,
            pending: 0,
            killed: 0,
            retired: vec![0; NWHY as usize + k.sites.len()],
            dispatched: 0,
        }
    }

    /// Run the kernel for pixels `i0..i0+n` of row `j`, writing `ncomp`
    /// bytes per pixel into `out`. Returns the lanes retired to the
    /// interpreter (bit `l` is pixel `i0 + l`); their bytes are left 0.
    pub(crate) fn run_batch(
        &mut self,
        j: i64,
        i0: i64,
        n: usize,
        out: &mut [u8],
    ) -> Result<u64, Stop> {
        debug_assert!(n <= LANES);
        let mut at = [(0, 0); LANES];
        for (l, p) in at.iter_mut().enumerate().take(n) {
            *p = (i0 + l as i64, j);
        }
        let killed = self.run_lanes(&at[..n])?;
        let nc = self.k.outputs.len();
        for l in (0..n).filter(|l| killed >> l & 1 == 0) {
            self.pixel(l, &mut out[l * nc..(l + 1) * nc]);
        }
        Ok(killed)
    }

    /// Run the kernel for the pixels `(i, j)` in `at`, one a lane. Returns
    /// the lanes retired to the interpreter; [`Machine::pixel`] reads the
    /// others' bytes.
    pub(crate) fn run_lanes(&mut self, at: &[(i64, i64)]) -> Result<u64, Stop> {
        let n = at.len();
        debug_assert!(n > 0 && n <= LANES);
        let mask = if n == LANES { FULL } else { (1u64 << n) - 1 };
        let (ri, rj) = (self.k.in_i as usize, self.k.in_j as usize);
        for l in 0..LANES {
            // The inputs are integers: I64 registers hold their bits. A
            // lane past `n` is never run.
            let (i, j) = at[l.min(n - 1)];
            self.regs[ri][l] = f64::from_bits(i as u64);
            self.regs[rj][l] = f64::from_bits(j as u64);
        }
        self.alive.clear();
        self.killed = 0;
        let c = self.c;
        let (b0, bn) = (c.body.0 as usize, c.body.1 as usize);
        self.block(&c.ins[b0..b0 + bn], mask)?;
        let charged = std::mem::take(&mut self.pending);
        self.ctl.poll(charged)?;
        Ok(self.killed)
    }

    /// Lane `l`'s output bytes after [`Machine::run_lanes`].
    pub(crate) fn pixel(&self, l: usize, out: &mut [u8]) {
        for (b, o) in out.iter_mut().zip(&self.k.outputs) {
            let v = match *o {
                Op::R(r) => self.regs[r as usize][l],
                Op::Imm(x) => x,
            };
            *b = super::quantize(v);
        }
    }

    /// The random state lane `l` of the last batch left, if it seeded
    /// (`Kernel::rand`; meaningful for a lane that was not retired).
    pub(crate) fn rand_state(&self, l: usize) -> Option<i64> {
        let s = self.geti(self.k.rand?, l);
        (s >= 0).then_some(s)
    }

    /// Retire `lanes` (see `killed`) for reason `code` (see [`Why`]).
    #[inline]
    fn kill(&mut self, lanes: u64, code: u32) {
        if lanes == 0 {
            return;
        }
        self.kill_slow(lanes, code);
    }

    #[cold]
    #[inline(never)]
    fn kill_slow(&mut self, lanes: u64, code: u32) {
        // Every instruction runs only lanes not yet retired.
        debug_assert_eq!(lanes & self.killed, 0);
        self.killed |= lanes;
        if let Some(n) = self.retired.get_mut(code as usize) {
            *n += u64::from(lanes.count_ones());
        }
        for a in &mut self.alive {
            *a &= !lanes;
        }
    }

    #[inline(always)]
    fn o(&self, o: Op) -> OpRef<'_> {
        match o {
            Op::R(r) => OpRef::Lane(&self.regs[r as usize]),
            Op::Imm(x) => OpRef::Imm(x),
        }
    }

    #[inline(always)]
    fn get(&self, o: Op, l: usize) -> f64 {
        match o {
            Op::R(r) => self.regs[r as usize][l],
            Op::Imm(x) => x,
        }
    }

    /// An integer operand: an I64 register's bits, or an immediate's value.
    #[inline(always)]
    fn geti(&self, o: Op, l: usize) -> i64 {
        match o {
            Op::R(r) => self.regs[r as usize][l].to_bits() as i64,
            Op::Imm(x) => x as i64,
        }
    }

    #[inline(always)]
    fn store(&mut self, d: Reg, t: &Lane, mask: u64) {
        let dst = &mut self.regs[(d & !FRESH) as usize];
        if mask == FULL || d & FRESH != 0 {
            *dst = *t;
        } else {
            // A blend, not a branch per lane: a divergent kernel's masks
            // are unpredictable (#1101).
            let mut m = [0u64; LANES];
            for (l, m) in m.iter_mut().enumerate() {
                *m = 0u64.wrapping_sub(mask >> l & 1);
            }
            for l in 0..LANES {
                dst[l] = f64::from_bits((t[l].to_bits() & m[l]) | (dst[l].to_bits() & !m[l]));
            }
        }
    }

    /// Run a block for the lanes in `mask`. Returns the lanes still running
    /// at its end (a `Break` inside takes lanes out).
    fn block(&mut self, b: &[L], mut mask: u64) -> Result<u64, Stop> {
        // The step and dispatch counts, kept local between the points that
        // read them (a nested block, a poll, the return): moved into `self`
        // before each, so every poll sees the same count as before (#1101).
        let mut steps = 0u64;
        let mut disp = 0u64;
        macro_rules! flush {
            () => {
                self.pending += std::mem::take(&mut steps);
                self.dispatched += std::mem::take(&mut disp);
            };
        }
        let mut pc = 0;
        while let Some(ins) = b.get(pc) {
            pc += 1;
            mask &= !self.killed;
            if mask == 0 {
                break;
            }
            steps += u64::from(mask.count_ones());
            disp += 1;
            match *ins {
                L::Mov(d, s) => {
                    let t = self.w(s).splat();
                    self.store(d, &t, mask);
                }
                L::MovR(d, s) => {
                    let t = self.regs[s as usize];
                    self.store(d, &t, mask);
                }
                L::MovI(d, v) => {
                    let t = [self.c.consts[(v & !IMM) as usize]; LANES];
                    self.store(d, &t, mask);
                }
                L::IBin(op, d, a, b) => {
                    let mut t = [0.0; LANES];
                    let bad = ibin(op, self.w(a), self.w(b), &mut t) & mask;
                    let why = match op {
                        IBinOp::Add | IBinOp::Sub | IBinOp::Mul => Why::IntOverflow,
                        _ => Why::IntDivide,
                    };
                    self.kill(bad, why as u32);
                    self.store(d, &t, mask & !bad);
                }
                L::IUn(op, d, a) => {
                    let mut t = [0.0; LANES];
                    let mut bad = 0u64;
                    for (l, v) in t.iter_mut().enumerate() {
                        match lane_iun(op, self.wi(a, l)) {
                            Some(r) => *v = f64::from_bits(r as u64),
                            None => bad |= 1 << l,
                        }
                    }
                    let bad = bad & mask;
                    self.kill(bad, Why::IntNeg as u32);
                    self.store(d, &t, mask & !bad);
                }
                L::I2F(d, a) => {
                    let x = self.wis(a);
                    let t = x.map(|v| v as f64);
                    self.store(d, &t, mask);
                }
                L::I2FExact(d, a) => {
                    let x = self.wis(a);
                    let t = x.map(|v| v as f64);
                    // Checked as an integer: converting first would round
                    // 2^53 + 1 to 2^53 and pass.
                    let z = x.map(|v| u64::from(v.unsigned_abs() > INT_LIMIT_I));
                    let bad = fails(&z) & mask;
                    self.kill(bad, Why::IntInexact as u32);
                    self.store(d, &t, mask & !bad);
                }
                L::F2I(d, a) => {
                    // `real_to_int` over the batch: a failing lane is retired,
                    // so what it holds does not matter.
                    let mut t = [0.0; LANES];
                    let bad = f2i(self.w(a), &mut t) & mask;
                    self.kill(bad, Why::CviRange as u32);
                    self.store(d, &t, mask & !bad);
                }
                L::Bin(op, d, a, b) => {
                    let mut t = [0.0; LANES];
                    let bad = bin(op, self.w(a), self.w(b), &mut t) & mask;
                    self.kill(bad, Why::bin(op) as u32);
                    self.store(d, &t, mask & !bad);
                }
                L::Add(d, a, b) => {
                    let mut t = [0.0; LANES];
                    flanes(self.w(a), self.w(b), &mut t, |p, q| (p + q, false));
                    self.store(d, &t, mask);
                }
                L::Sub(d, a, b) => {
                    let mut t = [0.0; LANES];
                    flanes(self.w(a), self.w(b), &mut t, |p, q| (p - q, false));
                    self.store(d, &t, mask);
                }
                L::Mul(d, a, b) => {
                    let mut t = [0.0; LANES];
                    flanes(self.w(a), self.w(b), &mut t, |p, q| (p * q, false));
                    self.store(d, &t, mask);
                }
                L::Un(op, d, a) => {
                    let mut t = [0.0; LANES];
                    let bad = un(op, self.w(a), &mut t) & mask;
                    self.kill(bad, Why::un(op) as u32);
                    self.store(d, &t, mask & !bad);
                }
                L::CheckInt(v, tag) => {
                    let bad = check(self.w(v), self.w(tag), |v, tag| {
                        (tag != 0.0) & !within_int(v)
                    });
                    self.kill(bad & mask, Why::IntPast53 as u32);
                }
                L::NeedInt(tag) => {
                    let bad = check(self.w(tag), OpRef::Imm(0.0), |t, _| t == 0.0);
                    self.kill(bad & mask, Why::NeedInt as u32);
                }
                L::Assert(c, s) => {
                    let bad = check(self.w(c), OpRef::Imm(0.0), |t, _| t == 0.0);
                    self.kill(bad & mask, site(s));
                }
                L::Ext(n) => {
                    let c = self.c;
                    self.ext(&c.ext[n as usize], mask);
                }
                L::Gather1(d, g, col) => {
                    let g = self.c.gathers[g as usize];
                    self.gather1(d, g, col, mask);
                }
                L::If(c, tl, el) => {
                    let cm = nonzero(&self.regs[c as usize]);
                    let (tl, el) = (tl as usize, el as usize);
                    let (t, e) = (&b[pc..pc + tl], &b[pc + tl..pc + tl + el]);
                    pc += tl + el;
                    let (mt, me) = (mask & cm, mask & !cm);
                    flush!();
                    if mt != 0 {
                        self.block(t, mt)?;
                    }
                    if me != 0 {
                        self.block(e, me)?;
                    }
                    mask = self.live(mask);
                }
                L::Loop(n) => {
                    let body = &b[pc..pc + n as usize];
                    pc += n as usize;
                    flush!();
                    self.alive.push(mask);
                    let mut iters = 0u32;
                    loop {
                        let m = *self.alive.last().unwrap_or(&0) & !self.killed;
                        if m == 0 {
                            break;
                        }
                        self.block(body, m)?;
                        iters += 1;
                        if iters == POLL_ITERS {
                            iters = 0;
                            let charged = std::mem::take(&mut self.pending);
                            self.ctl.poll(charged)?;
                        }
                    }
                    self.alive.pop();
                    // Lanes that broke out continue after the loop; lanes
                    // that left an *outer* loop meanwhile are still out.
                    mask = self.live(mask);
                }
                L::Call(s, n) => {
                    // A subroutine has no `Break` of its own loops' caller:
                    // the lanes it leaves running are the active ones less
                    // any it retired, as after an `If`.
                    // Calls nest, so a compact call graph can run far more
                    // instructions than it holds with no loop in it: look
                    // at the limits here too.
                    flush!();
                    if self.pending >= POLL_STEPS {
                        let charged = std::mem::take(&mut self.pending);
                        self.ctl.poll(charged)?;
                    }
                    let c = self.c;
                    self.block(&c.ins[s as usize..(s + n) as usize], mask)?;
                    mask = self.live(mask);
                }
                L::Break => {
                    flush!();
                    if let Some(a) = self.alive.last_mut() {
                        *a &= !mask;
                    }
                    return Ok(0);
                }
                L::Abort(s) => {
                    flush!();
                    self.kill(mask, site(s));
                    return Ok(0);
                }
            }
        }
        flush!();
        Ok(mask)
    }

    /// A lowered integer operand over the batch (as [`Machine::wi`]).
    #[inline(always)]
    fn wis(&self, w: u32) -> [i64; LANES] {
        if w & IMM != 0 {
            [self.c.consts[(w & !IMM) as usize] as i64; LANES]
        } else {
            self.regs[w as usize].map(|v| v.to_bits() as i64)
        }
    }

    /// A lowered operand ([`IMM`]).
    #[inline(always)]
    fn w(&self, w: u32) -> OpRef<'_> {
        if w & IMM != 0 {
            OpRef::Imm(self.c.consts[(w & !IMM) as usize])
        } else {
            OpRef::Lane(&self.regs[w as usize])
        }
    }

    /// A lowered integer operand: an I64 register's bits, or an
    /// immediate's value (as [`Machine::geti`]).
    #[inline(always)]
    fn wi(&self, w: u32, l: usize) -> i64 {
        if w & IMM != 0 {
            self.c.consts[(w & !IMM) as usize] as i64
        } else {
            self.regs[w as usize][l].to_bits() as i64
        }
    }

    /// [`L::Gather1`]: as [`Ins::Gather`] with one good constant index, so
    /// only the other (`x`) can be bad.
    #[inline(never)]
    fn gather1(&mut self, d: Reg, g: super::code::G1, x: u32, mask: u64) {
        let tb = &self.k.tables[g.table as usize];
        let mut t = [0.0; LANES];
        let mut tt = [0.0; LANES];
        let tags = match (g.dtag, &tb.kind) {
            (Some(_), TableKind::Mixed(tags)) => Some(&tags[..]),
            _ => None,
        };
        let xv = self.w(x);
        let bad = match &tb.data {
            TableData::F64(v) => gather1(&g, xv, v, tags, &mut t, &mut tt),
            TableData::U8(v) => gather1(&g, xv, v, tags, &mut t, &mut tt),
        } & mask;
        let mut null = 0u64;
        if g.rows && !tb.null_rows.is_empty() {
            let mut m = mask & !bad;
            while m != 0 {
                let l = m.trailing_zeros() as usize;
                m &= m - 1;
                if tb
                    .null_rows
                    .get(xv.at(l) as usize)
                    .copied()
                    .unwrap_or(false)
                {
                    null |= 1 << l;
                }
            }
        }
        self.kill(bad, Why::TableIndex as u32);
        self.kill(null, Why::TableNull as u32);
        let ok = mask & !bad & !null;
        self.store(d, &t, ok);
        if let Some(dt) = g.dtag {
            self.store(dt, &tt, ok);
        }
    }

    /// Run one of the wide, rarely run instructions ([`L::Ext`]) from its
    /// IR form for the lanes in `mask`.
    fn ext(&mut self, ins: &Ins, mask: u64) {
        match ins {
            Ins::ForFin(d, c, inc, lim) => {
                let mut t = [0.0; LANES];
                for (l, v) in t.iter_mut().enumerate() {
                    let (c, inc, lim) = (self.get(*c, l), self.get(*inc, l), self.get(*lim, l));
                    let fin = if inc >= 0.0 { c > lim } else { c < lim };
                    *v = f64::from(u8::from(fin));
                }
                self.store(*d, &t, mask);
            }
            Ins::Gather {
                d,
                dtag,
                table,
                row,
                rtag,
                col,
                ctag,
            } => {
                let tb = &self.k.tables[*table as usize];
                let (rv, rt) = (self.o(*row), self.o(*rtag));
                let (cv, ct) = (self.o(*col), self.o(*ctag));
                let mut t = [0.0; LANES];
                let mut tt = [0.0; LANES];
                let tags = match (dtag, &tb.kind) {
                    (Some(_), TableKind::Mixed(tags)) => Some(&tags[..]),
                    _ => None,
                };
                let bad = match &tb.data {
                    TableData::F64(v) => gather(tb, rv, rt, cv, ct, v, tags, &mut t, &mut tt),
                    TableData::U8(v) => gather(tb, rv, rt, cv, ct, v, tags, &mut t, &mut tt),
                } & mask;
                let mut null = 0u64;
                if !tb.null_rows.is_empty() {
                    let mut m = mask & !bad;
                    while m != 0 {
                        let l = m.trailing_zeros() as usize;
                        m &= m - 1;
                        if tb
                            .null_rows
                            .get(rv.at(l) as usize)
                            .copied()
                            .unwrap_or(false)
                        {
                            null |= 1 << l;
                        }
                    }
                }
                let ok = mask & !bad & !null;
                self.kill(bad, Why::TableIndex as u32);
                self.kill(null, Why::TableNull as u32);
                self.store(*d, &t, ok);
                if let Some(dt) = dtag {
                    self.store(*dt, &tt, ok);
                }
            }
            Ins::RowCheck(table, row) => {
                let tb = &self.k.tables[*table as usize];
                let mut bad = 0u64;
                for l in 0..LANES {
                    if mask >> l & 1 == 0 {
                        continue;
                    }
                    let r = self.get(*row, l);
                    if !(r >= 0.0 && (r as usize) < tb.rows)
                        || tb.null_rows.get(r as usize).copied().unwrap_or(false)
                    {
                        bad |= 1 << l;
                    }
                }
                self.kill(bad, Why::RowBad as u32);
            }
            Ins::LNew(a) => {
                let (vals, tags) = &mut self.larr[*a as usize];
                for (v, t) in vals.iter_mut().zip(tags.iter_mut()) {
                    for l in 0..LANES {
                        if mask >> l & 1 != 0 {
                            v[l] = 0.0;
                            t[l] = NULL_TAG;
                        }
                    }
                }
            }
            Ins::LGet {
                d,
                dtag,
                arr,
                idx,
                itag,
            } => {
                let mut t = [0.0; LANES];
                let mut tt = [0.0; LANES];
                let mut bad = 0u64;
                let mut null = 0u64;
                {
                    let (vals, tags) = &self.larr[*arr as usize];
                    for l in 0..LANES {
                        if mask >> l & 1 == 0 {
                            continue;
                        }
                        let i = self.get(*idx, l);
                        if self.get(*itag, l) == 0.0 || !(i >= 0.0 && (i as usize) < vals.len()) {
                            bad |= 1 << l;
                            continue;
                        }
                        let i = i as usize;
                        if tags[i][l] == NULL_TAG {
                            null |= 1 << l;
                            continue;
                        }
                        t[l] = vals[i][l];
                        tt[l] = tags[i][l];
                    }
                }
                self.kill(bad, Why::LocalIndex as u32);
                self.kill(null, Why::LocalNull as u32);
                let bad = bad | null;
                self.store(*d, &t, mask & !bad);
                self.store(*dtag, &tt, mask & !bad);
            }
            Ins::LPut {
                arr,
                idx,
                itag,
                v,
                vtag,
            } => {
                let n = self.larr[*arr as usize].0.len();
                let mut bad = 0u64;
                for l in 0..LANES {
                    if mask >> l & 1 == 0 {
                        continue;
                    }
                    let i = self.get(*idx, l);
                    if self.get(*itag, l) == 0.0 || !(i >= 0.0 && (i as usize) < n) {
                        bad |= 1 << l;
                        continue;
                    }
                    let (val, tag) = (self.get(*v, l), self.get(*vtag, l));
                    let (vals, tags) = &mut self.larr[*arr as usize];
                    vals[i as usize][l] = val;
                    tags[i as usize][l] = tag;
                }
                self.kill(bad, Why::LocalPutIndex as u32);
            }
            Ins::Noise { kind, d, a, octtag } => {
                let mut outs = [[0.0; LANES]; 3];
                let mut bad = 0u64;
                let mut real_oct = 0u64;
                #[allow(clippy::needless_range_loop)] // lanes index three outputs
                for l in 0..LANES {
                    if mask >> l & 1 == 0 {
                        continue;
                    }
                    if kind.octaves() && self.get(*octtag, l) == 0.0 {
                        real_oct |= 1 << l;
                        continue;
                    }
                    let mut args = [0.0; 6];
                    for (x, o) in args.iter_mut().zip(a.iter()) {
                        *x = self.get(*o, l);
                    }
                    match crate::ops::noise::kernel_noise(&self.k.perm, *kind, &args) {
                        Ok(r) => {
                            for c in 0..3 {
                                outs[c][l] = r[c];
                            }
                        }
                        Err(_) => bad |= 1 << l,
                    }
                }
                self.kill(bad, Why::NoiseOperand as u32);
                self.kill(real_oct, Why::NeedInt as u32);
                let bad = bad | real_oct;
                for (c, reg) in d.iter().enumerate().take(kind.outputs()) {
                    self.store(*reg, &outs[c], mask & !bad);
                }
            }
            _ => unreachable!("lowered inline: {ins:?}"),
        }
    }

    /// `mask` less the lanes that have left the innermost running loop.
    #[inline]
    fn live(&self, mask: u64) -> u64 {
        match self.alive.last() {
            Some(a) => mask & a,
            None => mask,
        }
    }
}

/// The op over the batch into `t`, and the lanes it fails on (the caller
/// masks them).
#[inline(always)]
fn bin(op: BinOp, x: OpRef<'_>, y: OpRef<'_>, t: &mut Lane) -> u64 {
    let c = |v: bool| f64::from(u8::from(v));
    let ok = |v: f64| (v, false);
    // What a failing lane holds does not matter: it is retired, and the
    // caller stores only the others. Selecting a value for it would keep
    // the loop from vectorising.
    let ord = |v: bool, p: f64, q: f64| (c(v), p.is_nan() | q.is_nan());
    match op {
        BinOp::Add => flanes(x, y, t, |p, q| ok(p + q)),
        BinOp::Sub => flanes(x, y, t, |p, q| ok(p - q)),
        BinOp::Mul | BinOp::TagAnd => flanes(x, y, t, |p, q| ok(p * q)),
        BinOp::Min => flanes(x, y, t, |p, q| ok(p.min(q))),
        BinOp::Max => flanes(x, y, t, |p, q| ok(p.max(q))),
        BinOp::BAnd => flanes(x, y, t, |p, q| ok(c(p != 0.0 && q != 0.0))),
        BinOp::BOr => flanes(x, y, t, |p, q| ok(c(p != 0.0 || q != 0.0))),
        BinOp::BXor => flanes(x, y, t, |p, q| ok(c((p != 0.0) ^ (q != 0.0)))),
        BinOp::Eq => flanes(x, y, t, |p, q| ok(c(p == q))),
        BinOp::Ne => flanes(x, y, t, |p, q| ok(c(p != q))),
        BinOp::Div => flanes(x, y, t, |p, q| (p / q, q == 0.0)),
        BinOp::Lt => flanes(x, y, t, |p, q| ord(p < q, p, q)),
        BinOp::Le => flanes(x, y, t, |p, q| ord(p <= q, p, q)),
        BinOp::Gt => flanes(x, y, t, |p, q| ord(p > q, p, q)),
        BinOp::Ge => flanes(x, y, t, |p, q| ord(p >= q, p, q)),
        BinOp::Atan | BinOp::Pow => flanes(x, y, t, |p, q| match lane_bin(op, p, q) {
            Some(r) => (r, false),
            None => (0.0, true),
        }),
    }
}

/// A unary op over the batch into `t`, and the lanes it fails on.
#[inline(always)]
fn un(op: UnOp, x: OpRef<'_>, t: &mut Lane) -> u64 {
    let ok = |v: f64| (v, false);
    let y = OpRef::Imm(0.0);
    match op {
        UnOp::Neg => flanes(x, y, t, |v, _| ok(-v)),
        UnOp::Abs => flanes(x, y, t, |v, _| ok(v.abs())),
        UnOp::Floor => flanes(x, y, t, |v, _| ok(v.floor())),
        UnOp::Ceil => flanes(x, y, t, |v, _| ok(v.ceil())),
        UnOp::Trunc => flanes(x, y, t, |v, _| ok(v.trunc())),
        UnOp::Round => flanes(x, y, t, |v, _| {
            let f = v.floor();
            ok(if v - f >= 0.5 { f + 1.0 } else { f })
        }),
        UnOp::BNot => flanes(x, y, t, |v, _| ok(f64::from(u8::from(v == 0.0)))),
        // `fmath::sqrt` is `f64::sqrt`; a negative lane fails.
        UnOp::Sqrt => flanes(x, y, t, |v, _| (v.sqrt(), v < 0.0)),
        _ => flanes(x, y, t, |v, _| match lane_un(op, v) {
            Some(r) => (r, false),
            None => (0.0, true),
        }),
    }
}

/// A table read over the batch ([`Ins::Gather`]): the elements into `t`
/// (and their tags into `tt`), and the lanes whose row or column is out of
/// range or not an integer. Every lane loads, from element 0 where its
/// index is bad, so the loop has no branch on the lanes; the index is
/// computed in f64, which vectorises (NEON has no 64-bit multiply), and is
/// exact: a good lane's row and column are integers below the table's size
/// (#1101).
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn gather<E: Copy + Into<f64>>(
    tb: &super::ir::Table,
    rv: OpRef<'_>,
    rt: OpRef<'_>,
    cv: OpRef<'_>,
    ct: OpRef<'_>,
    v: &[E],
    tags: Option<&[f64]>,
    t: &mut Lane,
    tt: &mut Lane,
) -> u64 {
    if v.is_empty() {
        return FULL;
    }
    let (mut b0, mut b1, mut b2, mut b3) = ([0.0; LANES], [0.0; LANES], [0.0; LANES], [0.0; LANES]);
    let (rv, rt) = (rv.lane(&mut b0), rt.lane(&mut b1));
    let (cv, ct) = (cv.lane(&mut b2), ct.lane(&mut b3));
    // `(r as usize) < rows` for an r >= 0 is `r < rows` (NaN fails both).
    let (rows, cols) = (tb.rows as f64, tb.cols as f64);
    let m = |b: bool| 0u64.wrapping_sub(u64::from(b));
    let mut okm = [0u64; LANES];
    let mut ix = [0u64; LANES];
    for l in 0..LANES {
        let (r, c) = (rv[l], cv[l]);
        let ok = m(rt[l] != 0.0)
            & m(ct[l] != 0.0)
            & m(r >= 0.0)
            & m(r < rows)
            & m(c >= 0.0)
            & m(c < cols);
        okm[l] = ok;
        ix[l] = ((r * cols + c) as u64) & ok;
    }
    // A bad lane's index is 0; the clamp only tells the compiler so.
    let last = (v.len() - 1) as u64;
    for l in 0..LANES {
        t[l] = v[ix[l].min(last) as usize].into();
    }
    if let Some(tags) = tags
        && !tags.is_empty()
    {
        let last = (tags.len() - 1) as u64;
        for l in 0..LANES {
            tt[l] = tags[ix[l].min(last) as usize];
        }
    }
    if okm.iter().fold(!0u64, |a, b| a & b) != 0 {
        return 0;
    }
    okm.iter()
        .enumerate()
        .fold(0, |bad, (l, &ok)| bad | (!ok & 1) << l)
}

/// [`gather`] with one index a good constant: element `x * scale + base`,
/// and only `x` is checked (#1101).
#[inline(always)]
fn gather1<E: Copy + Into<f64>>(
    g: &super::code::G1,
    xv: OpRef<'_>,
    v: &[E],
    tags: Option<&[f64]>,
    t: &mut Lane,
    tt: &mut Lane,
) -> u64 {
    if v.is_empty() {
        return FULL;
    }
    let mut buf = [0.0; LANES];
    let xv = xv.lane(&mut buf);
    let bound = f64::from(g.bound);
    let (scale, base) = (u64::from(g.scale), u64::from(g.base));
    let m = |b: bool| 0u64.wrapping_sub(u64::from(b));
    let mut okm = [0u64; LANES];
    let mut ix = [0u64; LANES];
    for l in 0..LANES {
        let x = xv[l];
        let ok = m(x >= 0.0) & m(x < bound);
        okm[l] = ok;
        // Wrapping: a bad lane's x may be huge (the cast saturates), and
        // its index is masked to 0 anyway; a good lane's cannot wrap.
        ix[l] = (x as u64).wrapping_mul(scale).wrapping_add(base) & ok;
    }
    let last = (v.len() - 1) as u64;
    for l in 0..LANES {
        t[l] = v[ix[l].min(last) as usize].into();
    }
    if let Some(tags) = tags
        && !tags.is_empty()
    {
        let last = (tags.len() - 1) as u64;
        for l in 0..LANES {
            tt[l] = tags[ix[l].min(last) as usize];
        }
    }
    if okm.iter().fold(!0u64, |a, b| a & b) != 0 {
        return 0;
    }
    okm.iter()
        .enumerate()
        .fold(0, |bad, (l, &ok)| bad | (!ok & 1) << l)
}

/// `cvi` of a real over the batch ([`real_to_int`]): the bits into `t`,
/// and the lanes out of range.
#[inline(always)]
fn f2i(x: OpRef<'_>, t: &mut Lane) -> u64 {
    flanes(x, OpRef::Imm(0.0), t, |v, _| {
        let r = v.trunc();
        let ok = r.is_finite() && r >= i64::MIN as f64 && r <= i64::MAX as f64;
        (f64::from_bits((r as i64) as u64), !ok)
    })
}

/// `f` over the batch as a check (it writes nothing): the lanes where it
/// is true.
#[inline(always)]
fn check(x: OpRef<'_>, y: OpRef<'_>, f: fn(f64, f64) -> bool) -> u64 {
    let mut t = [0.0; LANES];
    flanes(x, y, &mut t, |p, q| (0.0, f(p, q)))
}

#[derive(Clone, Copy)]
enum OpRef<'a> {
    Lane(&'a Lane),
    Imm(f64),
}

/// The lanes where `z` is 1 (each 0 or 1), as a mask. Almost always none
/// fail: that is one OR across the batch, and the mask is built only when
/// some lane fails (#1101).
#[inline(always)]
fn fails(z: &[u64; LANES]) -> u64 {
    if z.iter().fold(0, |a, b| a | b) == 0 {
        return 0;
    }
    fails_slow(z)
}

#[cold]
#[inline(never)]
fn fails_slow(z: &[u64; LANES]) -> u64 {
    z.iter().enumerate().fold(0, |m, (l, &b)| m | b << l)
}

/// The lanes of `x` that are not 0.0, as a mask: branch-free, so it
/// vectorises (an `If`'s condition).
#[inline(always)]
fn nonzero(x: &Lane) -> u64 {
    let mut z = [0u64; LANES];
    for l in 0..LANES {
        z[l] = 0u64.wrapping_sub(u64::from(x[l] != 0.0)) & (1u64 << l);
    }
    z.iter().fold(0, |a, b| a | b)
}

/// An integer op over the batch: register bits out in `t`, and the lanes
/// it fails on (`lane_ibin`'s `None`). The common ops run branch-free, so
/// the add and the bitwise ops vectorise (#1101).
#[inline(always)]
fn ibin(op: IBinOp, x: OpRef<'_>, y: OpRef<'_>, t: &mut Lane) -> u64 {
    let b = |v: i64| v as u64;
    let c = |v: bool| f64::from(u8::from(v)).to_bits();
    match op {
        // Overflow: both operands' signs differ from the result's.
        IBinOp::Add => ilanes(x, y, t, |p, q| {
            let r = p.wrapping_add(q);
            (b(r), ((p ^ r) & (q ^ r)) < 0)
        }),
        IBinOp::Sub => ilanes(x, y, t, |p, q| {
            let r = p.wrapping_sub(q);
            (b(r), ((p ^ q) & (p ^ r)) < 0)
        }),
        IBinOp::Mul => ilanes(x, y, t, |p, q| {
            let (r, o) = p.overflowing_mul(q);
            (b(r), o)
        }),
        IBinOp::And => ilanes(x, y, t, |p, q| (b(p & q), false)),
        IBinOp::Or => ilanes(x, y, t, |p, q| (b(p | q), false)),
        IBinOp::Xor => ilanes(x, y, t, |p, q| (b(p ^ q), false)),
        IBinOp::Min => ilanes(x, y, t, |p, q| (b(p.min(q)), false)),
        IBinOp::Max => ilanes(x, y, t, |p, q| (b(p.max(q)), false)),
        IBinOp::Lt => ilanes(x, y, t, |p, q| (c(p < q), false)),
        IBinOp::Le => ilanes(x, y, t, |p, q| (c(p <= q), false)),
        IBinOp::Gt => ilanes(x, y, t, |p, q| (c(p > q), false)),
        IBinOp::Ge => ilanes(x, y, t, |p, q| (c(p >= q), false)),
        IBinOp::Eq => ilanes(x, y, t, |p, q| (c(p == q), false)),
        IBinOp::Ne => ilanes(x, y, t, |p, q| (c(p != q), false)),
        IBinOp::Idiv | IBinOp::Mod | IBinOp::Shift => {
            ilanes(x, y, t, |p, q| match lane_ibin(op, p, q) {
                Some(r) => (r.bits().to_bits(), false),
                None => (0, true),
            })
        }
    }
}

/// `f` over the batch's integer operands (an I64 register's bits, or an
/// immediate's value as `v as i64`), writing bits into `t`; returns the
/// lanes where `f` says it failed. The loops read the operands in place,
/// one per operand shape, so they vectorise; the failing lanes are found
/// again only when some lane failed.
#[inline(always)]
fn ilanes(
    x: OpRef<'_>,
    y: OpRef<'_>,
    t: &mut Lane,
    f: impl Fn(i64, i64) -> (u64, bool) + Copy,
) -> u64 {
    let i = |v: f64| v.to_bits() as i64;
    let mut any = false;
    match (x, y) {
        (OpRef::Lane(x), OpRef::Lane(y)) => {
            for l in 0..LANES {
                let (r, o) = f(i(x[l]), i(y[l]));
                t[l] = f64::from_bits(r);
                any |= o;
            }
        }
        (OpRef::Lane(x), OpRef::Imm(y)) => {
            let y = y as i64;
            for l in 0..LANES {
                let (r, o) = f(i(x[l]), y);
                t[l] = f64::from_bits(r);
                any |= o;
            }
        }
        (OpRef::Imm(x), OpRef::Lane(y)) => {
            let x = x as i64;
            for l in 0..LANES {
                let (r, o) = f(x, i(y[l]));
                t[l] = f64::from_bits(r);
                any |= o;
            }
        }
        (OpRef::Imm(x), OpRef::Imm(y)) => {
            let (r, o) = f(x as i64, y as i64);
            *t = [f64::from_bits(r); LANES];
            any = o;
        }
    }
    if !any {
        return 0;
    }
    let mut m = 0u64;
    for l in 0..LANES {
        m |= u64::from(f(x.int_at(l), y.int_at(l)).1) << l;
    }
    m
}

/// As [`ilanes`] for f64 operands, writing f64 values.
#[inline(always)]
fn flanes(
    x: OpRef<'_>,
    y: OpRef<'_>,
    t: &mut Lane,
    f: impl Fn(f64, f64) -> (f64, bool) + Copy,
) -> u64 {
    let mut any = false;
    match (x, y) {
        (OpRef::Lane(x), OpRef::Lane(y)) => {
            for l in 0..LANES {
                let (r, o) = f(x[l], y[l]);
                t[l] = r;
                any |= o;
            }
        }
        (OpRef::Lane(x), OpRef::Imm(y)) => {
            for l in 0..LANES {
                let (r, o) = f(x[l], y);
                t[l] = r;
                any |= o;
            }
        }
        (OpRef::Imm(x), OpRef::Lane(y)) => {
            for l in 0..LANES {
                let (r, o) = f(x, y[l]);
                t[l] = r;
                any |= o;
            }
        }
        (OpRef::Imm(x), OpRef::Imm(y)) => {
            let (r, o) = f(x, y);
            *t = [r; LANES];
            any = o;
        }
    }
    if !any {
        return 0;
    }
    let mut m = 0u64;
    for l in 0..LANES {
        m |= u64::from(f(x.at(l), y.at(l)).1) << l;
    }
    m
}

impl<'a> OpRef<'a> {
    /// The operand as a lane array: a register in place, an immediate
    /// repeated into `buf`.
    #[inline(always)]
    fn lane<'b>(&self, buf: &'b mut Lane) -> &'b Lane
    where
        'a: 'b,
    {
        match *self {
            OpRef::Lane(x) => x,
            OpRef::Imm(x) => {
                *buf = [x; LANES];
                buf
            }
        }
    }

    /// Lane `l` as an integer operand ([`Machine::geti`]).
    #[inline(always)]
    fn int_at(&self, l: usize) -> i64 {
        match self {
            OpRef::Lane(x) => x[l].to_bits() as i64,
            OpRef::Imm(x) => *x as i64,
        }
    }

    /// The operand as a whole lane array (an immediate repeated).
    #[inline(always)]
    fn splat(&self) -> Lane {
        match self {
            OpRef::Lane(x) => **x,
            OpRef::Imm(x) => [*x; LANES],
        }
    }

    #[inline(always)]
    fn at(&self, l: usize) -> f64 {
        match self {
            OpRef::Lane(x) => x[l],
            OpRef::Imm(x) => *x,
        }
    }
}

/// One lane of an integer op, `None` where PostScript would raise an
/// error or promote to a real. Compares give 0.0/1.0 *as bits of an f64*
/// (`ibin_out` turns the result into register bits). Also the compiler's
/// constant folder.
pub(crate) fn lane_ibin(op: IBinOp, x: i64, y: i64) -> Option<IOut> {
    use IBinOp::*;
    Some(match op {
        Add => IOut::I(x.checked_add(y)?),
        Sub => IOut::I(x.checked_sub(y)?),
        Mul => IOut::I(x.checked_mul(y)?),
        Min => IOut::I(x.min(y)),
        Max => IOut::I(x.max(y)),
        Idiv => IOut::I(x.checked_div(y)?),
        Mod => IOut::I(x.checked_rem(y)?),
        And => IOut::I(x & y),
        Or => IOut::I(x | y),
        Xor => IOut::I(x ^ y),
        Shift => IOut::I(if y >= 64 {
            0
        } else if y <= -64 {
            x >> 63
        } else if y >= 0 {
            x.wrapping_shl(y as u32)
        } else {
            x >> (-y) as u32
        }),
        Lt => IOut::B(x < y),
        Le => IOut::B(x <= y),
        Gt => IOut::B(x > y),
        Ge => IOut::B(x >= y),
        Eq => IOut::B(x == y),
        Ne => IOut::B(x != y),
    })
}

/// An integer op's result: an integer, or a boolean.
#[derive(Clone, Copy, Debug)]
pub(crate) enum IOut {
    I(i64),
    B(bool),
}

impl IOut {
    /// As register bits.
    fn bits(self) -> f64 {
        match self {
            IOut::I(v) => f64::from_bits(v as u64),
            IOut::B(b) => f64::from(u8::from(b)),
        }
    }
}

pub(crate) fn lane_iun(op: IUnOp, x: i64) -> Option<i64> {
    match op {
        IUnOp::Neg => x.checked_neg(),
        IUnOp::Abs => x.checked_abs(),
        IUnOp::Not => Some(!x),
    }
}

/// `cvi` of a real, as `ops/convert.rs` does it.
pub(crate) fn real_to_int(r: f64) -> Option<i64> {
    let t = r.trunc();
    if !t.is_finite() || t < i64::MIN as f64 || t > i64::MAX as f64 {
        return None;
    }
    Some(t as i64)
}

/// An integer-valued f64 that is certainly exact (false for NaN): the
/// result of exact integer inputs that stays below 2^53 did not round.
#[inline]
fn within_int(x: f64) -> bool {
    x.abs() < INT_LIMIT
}

/// One lane of a binary op, `None` where PostScript would raise an error.
/// Also the compiler's constant folder, so folding and running agree.
pub(crate) fn lane_bin(op: BinOp, a: f64, b: f64) -> Option<f64> {
    let bool_f = |c: bool| f64::from(u8::from(c));
    Some(match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Min => a.min(b),
        BinOp::Max => a.max(b),
        BinOp::TagAnd => a * b,
        BinOp::Div => {
            if b == 0.0 {
                return None;
            }
            a / b
        }
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let ord = a.partial_cmp(&b)?;
            use std::cmp::Ordering::*;
            bool_f(match op {
                BinOp::Lt => ord == Less,
                BinOp::Le => ord != Greater,
                BinOp::Gt => ord == Greater,
                _ => ord != Less,
            })
        }
        BinOp::Eq => bool_f(a == b),
        BinOp::Ne => bool_f(a != b),
        BinOp::BAnd => bool_f(a != 0.0 && b != 0.0),
        BinOp::BOr => bool_f(a != 0.0 || b != 0.0),
        BinOp::BXor => bool_f((a != 0.0) ^ (b != 0.0)),
        BinOp::Atan => {
            if a == 0.0 && b == 0.0 {
                return None;
            }
            let mut angle = fmath::atan2(a, b).to_degrees();
            if angle < 0.0 {
                angle += 360.0;
            }
            angle
        }
        BinOp::Pow => {
            let r = fmath::powf(a, b);
            if r.is_nan() {
                return None;
            }
            r
        }
    })
}

/// One lane of a unary op, `None` where PostScript would raise an error.
pub(crate) fn lane_un(op: UnOp, x: f64) -> Option<f64> {
    Some(match op {
        UnOp::Neg => -x,
        UnOp::Abs => x.abs(),
        UnOp::Floor => x.floor(),
        UnOp::Ceil => x.ceil(),
        UnOp::Trunc => x.trunc(),
        UnOp::Round => {
            let f = x.floor();
            if x - f >= 0.5 { f + 1.0 } else { f }
        }
        UnOp::Sqrt => {
            if x < 0.0 {
                return None;
            }
            fmath::sqrt(x)
        }
        UnOp::Sin => fmath::sin(x.to_radians()),
        UnOp::Cos => fmath::cos(x.to_radians()),
        UnOp::Ln => {
            if x <= 0.0 {
                return None;
            }
            fmath::ln(x)
        }
        UnOp::Log => {
            if x <= 0.0 {
                return None;
            }
            fmath::log10(x)
        }
        UnOp::BNot => f64::from(u8::from(x == 0.0)),
    })
}
