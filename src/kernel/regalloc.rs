//! Register allocation for a compiled kernel: the compiler hands out a
//! fresh register for every value, and a large kernel ends up with tens
//! of thousands (each `LANES` f64s per worker). Most live briefly, so
//! they are packed into far fewer by a linear scan over live intervals.
//!
//! **Intervals.** Instructions are numbered in program order (a nested
//! block's instructions inside its parent's range). A register's interval
//! runs from its first to its last occurrence. A loop runs its body again,
//! and a lane may leave it part way through an iteration (`exit`), so a
//! register that occurs both inside a loop and outside it is kept live for
//! the whole loop: a value from before the loop is read again on the next
//! trip, and a value read after the loop may have been written on any
//! earlier trip (a lane that exits early never reaches the later write).
//!
//! **Masks.** Sharing a register between two values whose intervals do not
//! overlap is safe under masked execution because every read is dominated
//! by a write on the same lanes: the compiler's temporaries are written
//! where they are made; merge and loop-carried registers are written on
//! every incoming path; a partly defined name is read only behind an
//! `Assert` of its flag.

//!
//! **Subroutines** (`Ins::Call`, #1064). Each body (the kernel's own, and
//! each subroutine's) is packed on its own, into a region of registers
//! disjoint from every body that may be running when it is: a
//! subroutine's region lies past the regions of all its callers. So a
//! call clobbers nothing its caller keeps, whatever is live across it. A
//! subroutine's parameters are live from its start and its results to its
//! end, as the kernel's inputs and outputs are for the kernel's body.

use std::collections::HashSet;

use super::ir::{Ins, Kernel, Op, Reg};

/// Pack `k`'s registers. Leaves `k` as it is if its subroutine structure
/// is not what the compiler makes (a register shared between bodies other
/// than as a parameter or a result).
pub(crate) fn allocate(k: &mut Kernel) {
    let n = k.nregs;
    if n == 0 {
        return;
    }
    let groups = k.subs.len() + 1;
    let owner: Vec<usize> = (0..n)
        .map(|r| k.owner.get(r).copied().unwrap_or(0) as usize)
        .collect();
    if owner.iter().any(|&g| g >= groups) {
        debug_assert!(false, "a register owned by no body");
        return;
    }
    let params: Vec<HashSet<Reg>> = std::iter::once(HashSet::new())
        .chain(k.subs.iter().map(|s| s.params.iter().copied().collect()))
        .collect();
    let results: Vec<HashSet<Reg>> = std::iter::once(HashSet::new())
        .chain(k.subs.iter().map(|s| s.results.iter().copied().collect()))
        .collect();
    // Check the structure, and find each body's callers.
    let mut callers: Vec<Vec<usize>> = vec![Vec::new(); groups];
    for g in 0..groups {
        let body = if g == 0 { &k.body } else { &k.subs[g - 1].body };
        let mut ok = true;
        super::ir::walk(body, &mut |ins| {
            if let Ins::Call(id) = ins {
                let c = *id as usize + 1;
                // A subroutine calls only subroutines made before it.
                if c >= groups || (g != 0 && c >= g) {
                    ok = false;
                } else if !callers[c].contains(&g) {
                    callers[c].push(g);
                }
            }
            ins.regs(&mut |r, w| {
                let o = owner[r as usize];
                let shared = if w {
                    params[o].contains(&r)
                } else {
                    results[o].contains(&r)
                };
                if o != g && (o == 0 || !shared) {
                    ok = false;
                }
            });
        });
        if !ok {
            debug_assert!(false, "registers shared between bodies");
            return;
        }
    }
    // Pack each body on its own.
    let mut local = vec![0u32; n];
    let mut count = Vec::with_capacity(groups);
    for g in 0..groups {
        let (body, entry, exit): (&Vec<Ins>, Vec<Reg>, Vec<Reg>) = if g == 0 {
            let outs = k
                .outputs
                .iter()
                .chain(&k.rand)
                .filter_map(|o| match o {
                    Op::R(r) => Some(*r),
                    Op::Imm(_) => None,
                })
                .collect();
            (&k.body, vec![k.in_i, k.in_j], outs)
        } else {
            let s = &k.subs[g - 1];
            (&s.body, s.params.clone(), s.results.clone())
        };
        count.push(pack(body, &entry, &exit, &owner, g, &mut local));
    }
    // Lay the regions out: each past every caller's (callers come first:
    // the kernel's body, then subroutines from the last made).
    let mut off = vec![0u32; groups];
    for g in (1..groups).rev() {
        off[g] = callers[g]
            .iter()
            .map(|&c| off[c] + count[c])
            .max()
            .unwrap_or(count[0]);
    }
    let map: Vec<u32> = (0..n).map(|r| off[owner[r]] + local[r]).collect();
    let total = (0..groups).map(|g| off[g] + count[g]).max().unwrap_or(0);
    rename(&mut k.body, &map);
    for s in &mut k.subs {
        rename(&mut s.body, &map);
        for r in s.params.iter_mut().chain(s.results.iter_mut()) {
            *r = map[*r as usize];
        }
    }
    k.in_i = map[k.in_i as usize];
    k.in_j = map[k.in_j as usize];
    for o in k.outputs.iter_mut().chain(&mut k.rand) {
        if let Op::R(r) = o {
            *r = map[*r as usize];
        }
    }
    let nregs = total.max(1) as usize;
    if !k.subs.is_empty() {
        let mut own = vec![0u32; nregs];
        for r in 0..n {
            own[map[r] as usize] = owner[r] as u32;
        }
        k.owner = own;
    }
    k.nregs = nregs;
}

/// Linear scan over the registers of body `g` (those `owner` gives it):
/// sets their `local` numbers, returns how many it used.
fn pack(
    body: &[Ins],
    entry: &[Reg],
    exit: &[Reg],
    owner: &[usize],
    g: usize,
    local: &mut [u32],
) -> u32 {
    let n = owner.len();
    // First and last position of each register.
    let mut first = vec![usize::MAX; n];
    let mut last = vec![0usize; n];
    let mut loops: Vec<(usize, usize)> = Vec::new();
    let mut pos = 1usize;
    walk(
        body,
        &mut pos,
        &mut |r, p| {
            let r = r as usize;
            if owner[r] == g {
                first[r] = first[r].min(p);
                last[r] = last[r].max(p);
            }
        },
        &mut loops,
    );
    let end = pos + 1;
    // The inputs are written before the body; the outputs read after it.
    for &r in entry {
        first[r as usize] = 0;
    }
    for &r in exit {
        last[r as usize] = end;
        first[r as usize] = first[r as usize].min(end);
    }
    let regs: Vec<usize> = (0..n)
        .filter(|&r| owner[r] == g && first[r] != usize::MAX)
        .collect();
    // Widen across loops until nothing changes (an outer loop may widen an
    // interval into an inner one's range and back).
    loop {
        let mut changed = false;
        for &(ls, le) in &loops {
            for &r in &regs {
                let inside = first[r] <= le && last[r] >= ls;
                let outside = first[r] < ls || last[r] > le;
                if inside && outside {
                    let (f, l) = (first[r].min(ls), last[r].max(le));
                    if f != first[r] || l != last[r] {
                        first[r] = f;
                        last[r] = l;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Linear scan.
    let mut order = regs;
    order.sort_by_key(|&r| (first[r], last[r]));
    let mut active: std::collections::BinaryHeap<std::cmp::Reverse<(usize, u32)>> =
        std::collections::BinaryHeap::new();
    let mut free: Vec<u32> = Vec::new();
    let mut count = 0u32;
    for r in order {
        while let Some(std::cmp::Reverse((e, reg))) = active.peek().copied() {
            if e < first[r] {
                active.pop();
                free.push(reg);
            } else {
                break;
            }
        }
        let reg = free.pop().unwrap_or_else(|| {
            count += 1;
            count - 1
        });
        local[r] = reg;
        active.push(std::cmp::Reverse((last[r], reg)));
    }
    count
}

/// Visit every register occurrence with its position; record loop ranges.
fn walk(
    b: &[Ins],
    pos: &mut usize,
    f: &mut impl FnMut(Reg, usize),
    loops: &mut Vec<(usize, usize)>,
) {
    for ins in b {
        let p = *pos;
        *pos += 1;
        match ins {
            Ins::If(c, t, e) => {
                f(*c, p);
                walk(t, pos, f, loops);
                walk(e, pos, f, loops);
            }
            Ins::Loop(body) => {
                let start = p;
                walk(body, pos, f, loops);
                loops.push((start, *pos));
                *pos += 1;
            }
            other => each_reg(other, |r| f(r, p)),
        }
    }
}

fn each_op(o: &Op, f: &mut impl FnMut(Reg)) {
    if let Op::R(r) = o {
        f(*r);
    }
}

/// Every register a straight-line instruction reads or writes.
fn each_reg(ins: &Ins, mut f: impl FnMut(Reg)) {
    match ins {
        Ins::Mov(d, a)
        | Ins::Un(_, d, a)
        | Ins::IUn(_, d, a)
        | Ins::I2F(d, a)
        | Ins::I2FExact(d, a)
        | Ins::F2I(d, a) => {
            f(*d);
            each_op(a, &mut f);
        }
        Ins::MovI(d, _) => f(*d),
        Ins::Bin(_, d, a, b) | Ins::IBin(_, d, a, b) => {
            f(*d);
            each_op(a, &mut f);
            each_op(b, &mut f);
        }
        Ins::ForFin(d, a, b, c) => {
            f(*d);
            each_op(a, &mut f);
            each_op(b, &mut f);
            each_op(c, &mut f);
        }
        Ins::CheckInt(a, b) => {
            each_op(a, &mut f);
            each_op(b, &mut f);
        }
        Ins::NeedInt(a) | Ins::Assert(a) | Ins::RowCheck(_, a) => each_op(a, &mut f),
        Ins::Gather {
            d,
            dtag,
            row,
            rtag,
            col,
            ctag,
            ..
        } => {
            f(*d);
            if let Some(t) = dtag {
                f(*t);
            }
            for o in [row, rtag, col, ctag] {
                each_op(o, &mut f);
            }
        }
        Ins::LGet {
            d, dtag, idx, itag, ..
        } => {
            f(*d);
            f(*dtag);
            each_op(idx, &mut f);
            each_op(itag, &mut f);
        }
        Ins::LPut {
            idx, itag, v, vtag, ..
        } => {
            for o in [idx, itag, v, vtag] {
                each_op(o, &mut f);
            }
        }
        Ins::Noise { kind, d, a, octtag } => {
            for r in d.iter().take(kind.outputs()) {
                f(*r);
            }
            for o in a {
                each_op(o, &mut f);
            }
            each_op(octtag, &mut f);
        }
        Ins::LNew(_) | Ins::Break | Ins::Abort | Ins::If(..) | Ins::Loop(_) | Ins::Call(_) => {}
    }
}

fn rename(b: &mut [Ins], map: &[u32]) {
    let m = |r: &mut Reg| *r = map[*r as usize];
    let mo = |o: &mut Op| {
        if let Op::R(r) = o {
            *r = map[*r as usize];
        }
    };
    for ins in b {
        match ins {
            Ins::Mov(d, a)
            | Ins::Un(_, d, a)
            | Ins::IUn(_, d, a)
            | Ins::I2F(d, a)
            | Ins::I2FExact(d, a)
            | Ins::F2I(d, a) => {
                m(d);
                mo(a);
            }
            Ins::MovI(d, _) => m(d),
            Ins::Bin(_, d, a, b) | Ins::IBin(_, d, a, b) => {
                m(d);
                mo(a);
                mo(b);
            }
            Ins::ForFin(d, a, b, c) => {
                m(d);
                mo(a);
                mo(b);
                mo(c);
            }
            Ins::CheckInt(a, b) => {
                mo(a);
                mo(b);
            }
            Ins::NeedInt(a) | Ins::Assert(a) | Ins::RowCheck(_, a) => mo(a),
            Ins::Gather {
                d,
                dtag,
                row,
                rtag,
                col,
                ctag,
                ..
            } => {
                m(d);
                if let Some(t) = dtag {
                    m(t);
                }
                mo(row);
                mo(rtag);
                mo(col);
                mo(ctag);
            }
            Ins::LGet {
                d, dtag, idx, itag, ..
            } => {
                m(d);
                m(dtag);
                mo(idx);
                mo(itag);
            }
            Ins::LPut {
                idx, itag, v, vtag, ..
            } => {
                mo(idx);
                mo(itag);
                mo(v);
                mo(vtag);
            }
            Ins::Noise { kind, d, a, octtag } => {
                for r in d.iter_mut().take(kind.outputs()) {
                    m(r);
                }
                for o in a.iter_mut() {
                    mo(o);
                }
                mo(octtag);
            }
            Ins::If(c, t, e) => {
                m(c);
                rename(t, map);
                rename(e, map);
            }
            Ins::Loop(body) => rename(body, map),
            Ins::LNew(_) | Ins::Break | Ins::Abort | Ins::Call(_) => {}
        }
    }
}
