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

use super::ir::{Ins, Kernel, Op, Reg};

/// Pack `k`'s registers; returns the new register count.
pub(crate) fn allocate(k: &mut Kernel) {
    let n = k.nregs;
    if n == 0 {
        return;
    }
    // First and last position of each register.
    let mut first = vec![usize::MAX; n];
    let mut last = vec![0usize; n];
    let mut loops: Vec<(usize, usize)> = Vec::new();
    let mut pos = 1usize;
    walk(
        &k.body,
        &mut pos,
        &mut |r, p| {
            let r = r as usize;
            first[r] = first[r].min(p);
            last[r] = last[r].max(p);
        },
        &mut loops,
    );
    let end = pos + 1;
    // The inputs are written before the body; the outputs read after it.
    for r in [k.in_i, k.in_j] {
        first[r as usize] = 0;
    }
    for o in k.outputs.iter().chain(&k.rand) {
        if let Op::R(r) = o {
            last[*r as usize] = end;
            first[*r as usize] = first[*r as usize].min(end);
        }
    }
    // Widen across loops until nothing changes (an outer loop may widen an
    // interval into an inner one's range and back).
    loop {
        let mut changed = false;
        for &(ls, le) in &loops {
            for r in 0..n {
                if first[r] == usize::MAX {
                    continue;
                }
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
    let mut order: Vec<usize> = (0..n).filter(|&r| first[r] != usize::MAX).collect();
    order.sort_by_key(|&r| (first[r], last[r]));
    let mut map = vec![0u32; n];
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
        map[r] = reg;
        active.push(std::cmp::Reverse((last[r], reg)));
    }
    rename(&mut k.body, &map);
    k.in_i = map[k.in_i as usize];
    k.in_j = map[k.in_j as usize];
    for o in k.outputs.iter_mut().chain(&mut k.rand) {
        if let Op::R(r) = o {
            *r = map[*r as usize];
        }
    }
    k.nregs = count.max(1) as usize;
}

/// Remove moves into registers nothing reads. A name a loop defines is
/// carried round it in a register (with a defined-flag where it may be
/// undefined) in case the next trip or the code after the loop reads it;
/// for a loop's scratch names (`/k exch def`) nothing does, and their
/// back-edge moves would otherwise run on every trip.
///
/// A register is live if any instruction other than a move mentions it,
/// it is an input, an output or the random state a pixel leaves (both
/// read after the body), or it is the source of a move into a live
/// register. Int-to-real conversions count as moves: `I2FExact` retires a
/// lane only to keep the converted value exact, so with the value unread it
/// guards nothing (and an unused carried name would otherwise send pixels
/// to the interpreter). Plain moves have no side effects at all, so
/// dropping the rest changes nothing a kernel computes.
pub(crate) fn drop_dead_moves(k: &mut Kernel) {
    fn scan(b: &[Ins], live: &mut [bool], movs: &mut Vec<(Reg, Reg)>) {
        for ins in b {
            match ins {
                Ins::Mov(d, Op::R(s)) | Ins::I2F(d, Op::R(s)) | Ins::I2FExact(d, Op::R(s)) => {
                    movs.push((*d, *s))
                }
                Ins::Mov(_, Op::Imm(_))
                | Ins::I2F(_, Op::Imm(_))
                | Ins::I2FExact(_, Op::Imm(_))
                | Ins::MovI(..) => {}
                Ins::If(c, t, e) => {
                    live[*c as usize] = true;
                    scan(t, live, movs);
                    scan(e, live, movs);
                }
                Ins::Loop(body) => scan(body, live, movs),
                other => each_reg(other, |r| live[r as usize] = true),
            }
        }
    }
    fn prune(b: &mut Vec<Ins>, live: &[bool]) {
        b.retain(|ins| match ins {
            Ins::Mov(d, _) | Ins::MovI(d, _) | Ins::I2F(d, _) | Ins::I2FExact(d, _) => {
                live[*d as usize]
            }
            _ => true,
        });
        for ins in b {
            match ins {
                Ins::If(_, t, e) => {
                    prune(t, live);
                    prune(e, live);
                }
                Ins::Loop(body) => prune(body, live),
                _ => {}
            }
        }
    }
    let mut live = vec![false; k.nregs];
    let mut movs = Vec::new();
    scan(&k.body, &mut live, &mut movs);
    for r in [k.in_i, k.in_j] {
        live[r as usize] = true;
    }
    for o in k.outputs.iter().chain(&k.rand) {
        if let Op::R(r) = o {
            live[*r as usize] = true;
        }
    }
    // Liveness flows from a move's destination to its source. A worklist
    // over those edges visits each register once, so a long chain of merge
    // moves (60,000 `c { /x 1 def } if`) costs linear time, not a scan of
    // every move per link.
    let mut srcs: Vec<Vec<Reg>> = vec![Vec::new(); k.nregs];
    for &(d, s) in &movs {
        srcs[d as usize].push(s);
    }
    let mut work: Vec<Reg> = (0..k.nregs as Reg).filter(|&r| live[r as usize]).collect();
    while let Some(d) = work.pop() {
        for &s in &srcs[d as usize] {
            if !live[s as usize] {
                live[s as usize] = true;
                work.push(s);
            }
        }
    }
    prune(&mut k.body, &live);
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
        Ins::LNew(_) | Ins::Break | Ins::Abort | Ins::If(..) | Ins::Loop(_) => {}
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
            Ins::LNew(_) | Ins::Break | Ins::Abort => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kernel of `body`, reading inputs 0 and 1 and writing `out`.
    fn kernel(nregs: usize, body: Vec<Ins>, out: Reg) -> Kernel {
        Kernel {
            nregs,
            body,
            in_i: 0,
            in_j: 1,
            outputs: vec![Op::R(out)],
            rand: None,
            tables: Vec::new(),
            larrays: Vec::new(),
            perm: [0; 256],
        }
    }

    /// A chain of moves, each feeding the next in program order, live from
    /// the output back to the input: liveness flows against program order,
    /// and every link must be found. (Its cost is checked through
    /// `kernelcompile` in `tests/kernels.rs`.)
    #[test]
    fn a_live_move_chain_against_program_order_is_kept() {
        let n = 1000u32;
        let body: Vec<Ins> = (1..n).map(|r| Ins::Mov(r + 1, Op::R(r))).collect();
        let mut k = kernel(n as usize + 1, body, n);
        drop_dead_moves(&mut k);
        assert_eq!(k.body.len(), n as usize - 1, "a live move was dropped");
    }

    /// The random state a pixel leaves is read after the body, like an
    /// output: a move into it stays.
    #[test]
    fn a_move_into_the_random_state_is_kept() {
        let body = vec![
            Ins::MovI(2, 7),
            Ins::Mov(3, Op::R(2)),
            Ins::I2F(4, Op::R(0)),
        ];
        let mut k = kernel(5, body, 4);
        k.rand = Some(Op::R(3));
        drop_dead_moves(&mut k);
        assert_eq!(k.body.len(), 3, "{:?}", k.body);
    }

    /// Moves and int-to-real conversions into registers nothing reads go;
    /// whatever feeds the output or a check stays.
    #[test]
    fn only_unread_moves_and_conversions_are_dropped() {
        let body = vec![
            Ins::Mov(2, Op::R(0)), // read by the Assert: kept
            Ins::Assert(Op::R(2)),
            Ins::I2FExact(3, Op::R(1)), // feeds only 4, which nothing reads
            Ins::Mov(4, Op::R(3)),
            Ins::Loop(vec![Ins::MovI(5, 1), Ins::Break]), // 5 is unread
            Ins::I2F(6, Op::R(1)),                        // the output
        ];
        let mut k = kernel(7, body, 6);
        drop_dead_moves(&mut k);
        assert_eq!(k.body.len(), 4, "{:?}", k.body);
        assert!(matches!(k.body[0], Ins::Mov(2, _)));
        assert!(matches!(k.body[1], Ins::Assert(_)));
        assert!(matches!(&k.body[2], Ins::Loop(b) if matches!(b[..], [Ins::Break])));
        assert!(matches!(k.body[3], Ins::I2F(6, _)));
    }
}
