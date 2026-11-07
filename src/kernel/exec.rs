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
//! interpreter, which raises the error for real.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::ir::{BinOp, IBinOp, IUnOp, Ins, Kernel, Op, Reg, TableKind, UnOp};
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
    fn poll(&self, n: u64) -> Result<(), Stop> {
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

/// One worker's machine: a register file and local-array storage.
pub(crate) struct Machine<'k> {
    k: &'k Kernel,
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
    /// Why the first lane was retired.
    pub(crate) why: Option<&'static str>,
}

impl<'k> Machine<'k> {
    pub(crate) fn new(k: &'k Kernel, ctl: &'k RunCtl) -> Self {
        Machine {
            k,
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
            why: None,
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
        let mask = if n == LANES { FULL } else { (1u64 << n) - 1 };
        let (ri, rj) = (self.k.in_i as usize, self.k.in_j as usize);
        for l in 0..LANES {
            // The inputs are integers: I64 registers hold their bits.
            self.regs[ri][l] = f64::from_bits((i0 + l as i64) as u64);
            self.regs[rj][l] = f64::from_bits(j as u64);
        }
        self.alive.clear();
        self.killed = 0;
        let k = self.k;
        self.block(&k.body, mask)?;
        let killed = self.killed;
        let nc = k.outputs.len();
        for (c, o) in k.outputs.iter().enumerate() {
            for l in (0..n).filter(|l| killed >> l & 1 == 0) {
                let v = match *o {
                    Op::R(r) => self.regs[r as usize][l],
                    Op::Imm(x) => x,
                };
                out[l * nc + c] = super::quantize(v);
            }
        }
        let charged = std::mem::take(&mut self.pending);
        self.ctl.poll(charged)?;
        Ok(killed)
    }

    /// Retire `lanes` (see `killed`).
    fn kill(&mut self, lanes: u64, why: &'static str) {
        if lanes == 0 {
            return;
        }
        self.killed |= lanes;
        self.why.get_or_insert(why);
        for a in &mut self.alive {
            *a &= !lanes;
        }
    }

    #[inline(always)]
    fn opnd(&self, o: Op) -> OpRef<'_> {
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
        let dst = &mut self.regs[d as usize];
        if mask == FULL {
            *dst = *t;
        } else {
            for l in 0..LANES {
                if mask >> l & 1 != 0 {
                    dst[l] = t[l];
                }
            }
        }
    }

    /// Run a block for the lanes in `mask`. Returns the lanes still running
    /// at its end (a `Break` inside takes lanes out).
    fn block(&mut self, b: &[Ins], mut mask: u64) -> Result<u64, Stop> {
        for ins in b {
            mask &= !self.killed;
            if mask == 0 {
                break;
            }
            self.pending += u64::from(mask.count_ones());
            match ins {
                Ins::Mov(d, s) => {
                    let t = match self.opnd(*s) {
                        OpRef::Lane(x) => *x,
                        OpRef::Imm(x) => [x; LANES],
                    };
                    self.store(*d, &t, mask);
                }
                Ins::MovI(d, v) => {
                    let t = [f64::from_bits(*v as u64); LANES];
                    self.store(*d, &t, mask);
                }
                Ins::IBin(op, d, a, b) => {
                    let mut t = [0.0; LANES];
                    let mut bad = 0u64;
                    match op {
                        IBinOp::Add | IBinOp::Sub | IBinOp::Mul => {
                            // The common hash/arith case: tight loop.
                            for (l, v) in t.iter_mut().enumerate() {
                                let (x, y) = (self.geti(*a, l), self.geti(*b, l));
                                let r = match op {
                                    IBinOp::Add => x.checked_add(y),
                                    IBinOp::Sub => x.checked_sub(y),
                                    _ => x.checked_mul(y),
                                };
                                match r {
                                    Some(r) => *v = f64::from_bits(r as u64),
                                    None => bad |= 1 << l,
                                }
                            }
                        }
                        _ => {
                            for (l, v) in t.iter_mut().enumerate() {
                                match lane_ibin(*op, self.geti(*a, l), self.geti(*b, l)) {
                                    Some(r) => *v = r.bits(),
                                    None => bad |= 1 << l,
                                }
                            }
                        }
                    }
                    let bad = bad & mask;
                    self.kill(
                        bad,
                        "an integer overflow (the interpreter makes it a real) or a zero divisor",
                    );
                    self.store(*d, &t, mask & !bad);
                }
                Ins::IUn(op, d, a) => {
                    let mut t = [0.0; LANES];
                    let mut bad = 0u64;
                    for (l, v) in t.iter_mut().enumerate() {
                        match lane_iun(*op, self.geti(*a, l)) {
                            Some(r) => *v = f64::from_bits(r as u64),
                            None => bad |= 1 << l,
                        }
                    }
                    let bad = bad & mask;
                    self.kill(bad, "an integer overflow (the interpreter makes it a real)");
                    self.store(*d, &t, mask & !bad);
                }
                Ins::I2F(d, a) => {
                    let mut t = [0.0; LANES];
                    for (l, v) in t.iter_mut().enumerate() {
                        *v = self.geti(*a, l) as f64;
                    }
                    self.store(*d, &t, mask);
                }
                Ins::I2FExact(d, a) => {
                    let mut t = [0.0; LANES];
                    let mut bad = 0u64;
                    for (l, v) in t.iter_mut().enumerate() {
                        let x = self.geti(*a, l);
                        *v = x as f64;
                        // Checked as an integer: converting first would
                        // round 2^53 + 1 to 2^53 and pass.
                        if x.unsigned_abs() > INT_LIMIT_I {
                            bad |= 1 << l;
                        }
                    }
                    let bad = bad & mask;
                    self.kill(
                        bad,
                        "an integer past 2^53 where a value is integer on some lanes only",
                    );
                    self.store(*d, &t, mask & !bad);
                }
                Ins::F2I(d, a) => {
                    let mut t = [0.0; LANES];
                    let mut bad = 0u64;
                    for (l, v) in t.iter_mut().enumerate() {
                        match real_to_int(self.get(*a, l)) {
                            Some(r) => *v = f64::from_bits(r as u64),
                            None => bad |= 1 << l,
                        }
                    }
                    let bad = bad & mask;
                    self.kill(bad, "cvi of a value out of integer range");
                    self.store(*d, &t, mask & !bad);
                }
                Ins::Bin(op, d, a, b) => {
                    let (t, bad) = self.bin(*op, *a, *b, mask);
                    self.kill(
                        bad,
                        "an arithmetic error (division by zero, NaN in a compare)",
                    );
                    self.store(*d, &t, mask & !bad);
                }
                Ins::Un(op, d, a) => {
                    let (t, bad) = self.un(*op, *a, mask);
                    self.kill(
                        bad,
                        "a math error (sqrt of a negative, ln of zero or less, cvi out of range)",
                    );
                    self.store(*d, &t, mask & !bad);
                }
                Ins::ForFin(d, c, inc, lim) => {
                    let mut t = [0.0; LANES];
                    for (l, v) in t.iter_mut().enumerate() {
                        let (c, inc, lim) = (self.get(*c, l), self.get(*inc, l), self.get(*lim, l));
                        let fin = if inc >= 0.0 { c > lim } else { c < lim };
                        *v = f64::from(u8::from(fin));
                    }
                    self.store(*d, &t, mask);
                }
                Ins::CheckInt(v, tag) => {
                    let mut bad = 0;
                    for l in 0..LANES {
                        if mask >> l & 1 != 0
                            && self.get(*tag, l) != 0.0
                            && !within_int(self.get(*v, l))
                        {
                            bad |= 1 << l;
                        }
                    }
                    self.kill(bad, "an integer past 2^53");
                }
                Ins::NeedInt(tag) => {
                    let mut bad = 0;
                    for l in 0..LANES {
                        if mask >> l & 1 != 0 && self.get(*tag, l) == 0.0 {
                            bad |= 1 << l;
                        }
                    }
                    self.kill(bad, "a real where an integer is needed");
                }
                Ins::Assert(c) => {
                    let mut bad = 0;
                    for l in 0..LANES {
                        if mask >> l & 1 != 0 && self.get(*c, l) == 0.0 {
                            bad |= 1 << l;
                        }
                    }
                    self.kill(
                        bad,
                        "a name read where this pixel did not define it, or a bad repeat count",
                    );
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
                    let mut t = [0.0; LANES];
                    let mut tt = [0.0; LANES];
                    let mut bad = 0u64;
                    for l in 0..LANES {
                        if mask >> l & 1 == 0 {
                            continue;
                        }
                        let (r, c) = (self.get(*row, l), self.get(*col, l));
                        if self.get(*rtag, l) == 0.0
                            || self.get(*ctag, l) == 0.0
                            || !(r >= 0.0 && (r as usize) < tb.rows)
                            || !(c >= 0.0 && (c as usize) < tb.cols)
                        {
                            bad |= 1 << l;
                            continue;
                        }
                        if tb.null_rows.get(r as usize).copied().unwrap_or(false) {
                            bad |= 1 << l;
                            continue;
                        }
                        let ix = r as usize * tb.cols + c as usize;
                        t[l] = tb.data.at(ix);
                        if let TableKind::Mixed(tags) = &tb.kind {
                            tt[l] = tags[ix];
                        }
                    }
                    self.kill(
                        bad,
                        "a table index out of range, not an integer, or a null row",
                    );
                    self.store(*d, &t, mask & !bad);
                    if let Some(dt) = dtag {
                        self.store(*dt, &tt, mask & !bad);
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
                    self.kill(
                        bad,
                        "an operation on a table row that is null or out of range",
                    );
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
                    {
                        let (vals, tags) = &self.larr[*arr as usize];
                        for l in 0..LANES {
                            if mask >> l & 1 == 0 {
                                continue;
                            }
                            let i = self.get(*idx, l);
                            if self.get(*itag, l) == 0.0 || !(i >= 0.0 && (i as usize) < vals.len())
                            {
                                bad |= 1 << l;
                                continue;
                            }
                            let i = i as usize;
                            if tags[i][l] == NULL_TAG {
                                bad |= 1 << l;
                                continue;
                            }
                            t[l] = vals[i][l];
                            tt[l] = tags[i][l];
                        }
                    }
                    self.kill(bad, "a local array index out of range, or a null element");
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
                    self.kill(bad, "a local array index out of range");
                }
                Ins::Noise { kind, d, a, octtag } => {
                    let mut outs = [[0.0; LANES]; 3];
                    let mut bad = 0u64;
                    #[allow(clippy::needless_range_loop)] // lanes index three outputs
                    for l in 0..LANES {
                        if mask >> l & 1 == 0 {
                            continue;
                        }
                        if kind.octaves() && self.get(*octtag, l) == 0.0 {
                            bad |= 1 << l;
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
                    self.kill(bad, "a noise operand out of range");
                    for (c, reg) in d.iter().enumerate().take(kind.outputs()) {
                        self.store(*reg, &outs[c], mask & !bad);
                    }
                }
                Ins::If(c, t, e) => {
                    let mut cm = 0u64;
                    let cr = &self.regs[*c as usize];
                    for (l, v) in cr.iter().enumerate() {
                        if *v != 0.0 {
                            cm |= 1 << l;
                        }
                    }
                    let (mt, me) = (mask & cm, mask & !cm);
                    if mt != 0 {
                        self.block(t, mt)?;
                    }
                    if me != 0 {
                        self.block(e, me)?;
                    }
                    mask = self.live(mask);
                }
                Ins::Loop(body) => {
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
                Ins::Break => {
                    if let Some(a) = self.alive.last_mut() {
                        *a &= !mask;
                    }
                    return Ok(0);
                }
                Ins::Abort => {
                    self.kill(mask, "a branch that did not compile, or a certain error");
                    return Ok(0);
                }
            }
        }
        Ok(mask)
    }

    /// `mask` less the lanes that have left the innermost running loop.
    #[inline]
    fn live(&self, mask: u64) -> u64 {
        match self.alive.last() {
            Some(a) => mask & a,
            None => mask,
        }
    }

    /// The op over the batch, and the active lanes it fails on.
    fn bin(&self, op: BinOp, a: Op, b: Op, mask: u64) -> (Lane, u64) {
        let (x, y) = (self.opnd(a), self.opnd(b));
        macro_rules! lanes {
            ($f:expr) => {{
                let f = $f;
                let mut t = [0.0; LANES];
                match (x, y) {
                    (OpRef::Lane(x), OpRef::Lane(y)) => {
                        for l in 0..LANES {
                            t[l] = f(x[l], y[l]);
                        }
                    }
                    (OpRef::Lane(x), OpRef::Imm(y)) => {
                        for l in 0..LANES {
                            t[l] = f(x[l], y);
                        }
                    }
                    (OpRef::Imm(x), OpRef::Lane(y)) => {
                        for l in 0..LANES {
                            t[l] = f(x, y[l]);
                        }
                    }
                    (OpRef::Imm(x), OpRef::Imm(y)) => t = [f(x, y); LANES],
                }
                t
            }};
        }
        let mut bad = 0u64;
        let t = match op {
            BinOp::Add => lanes!(|a: f64, b: f64| a + b),
            BinOp::Sub => lanes!(|a: f64, b: f64| a - b),
            BinOp::Mul => lanes!(|a: f64, b: f64| a * b),
            BinOp::Min => lanes!(f64::min),
            BinOp::Max => lanes!(f64::max),
            BinOp::TagAnd => lanes!(|a: f64, b: f64| a * b),
            BinOp::BAnd => lanes!(|a: f64, b: f64| f64::from(u8::from(a != 0.0 && b != 0.0))),
            BinOp::BOr => lanes!(|a: f64, b: f64| f64::from(u8::from(a != 0.0 || b != 0.0))),
            BinOp::BXor => lanes!(|a: f64, b: f64| f64::from(u8::from((a != 0.0) ^ (b != 0.0)))),
            BinOp::Eq => lanes!(|a: f64, b: f64| f64::from(u8::from(a == b))),
            BinOp::Ne => lanes!(|a: f64, b: f64| f64::from(u8::from(a != b))),
            _ => {
                // The ops that can fail or need per-lane integer work.
                let mut t = [0.0; LANES];
                for (l, v) in t.iter_mut().enumerate() {
                    let (p, q) = (x.at(l), y.at(l));
                    match lane_bin(op, p, q) {
                        Some(r) => *v = r,
                        None => bad |= 1 << l,
                    }
                }
                t
            }
        };
        (t, bad & mask)
    }

    fn un(&self, op: UnOp, a: Op, mask: u64) -> (Lane, u64) {
        let x = self.opnd(a);
        let mut t = [0.0; LANES];
        let mut bad = 0u64;
        match op {
            UnOp::Neg => {
                for (l, v) in t.iter_mut().enumerate() {
                    *v = -x.at(l);
                }
            }
            UnOp::Abs => {
                for (l, v) in t.iter_mut().enumerate() {
                    *v = x.at(l).abs();
                }
            }
            UnOp::Floor => {
                for (l, v) in t.iter_mut().enumerate() {
                    *v = x.at(l).floor();
                }
            }
            _ => {
                for (l, v) in t.iter_mut().enumerate() {
                    match lane_un(op, x.at(l)) {
                        Some(r) => *v = r,
                        None => bad |= 1 << l,
                    }
                }
            }
        }
        (t, bad & mask)
    }
}

#[derive(Clone, Copy)]
enum OpRef<'a> {
    Lane(&'a Lane),
    Imm(f64),
}

impl OpRef<'_> {
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
