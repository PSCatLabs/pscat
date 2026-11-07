//! Clean-up passes over a compiled kernel, before register allocation.
//!
//! - [`prune_subs`] drops the subroutines nothing calls (the compiler makes
//!   some in loop passes and attempts it then throws away).
//! - [`dce`] removes instructions whose results nothing reads. Only
//!   instructions that cannot retire a lane go: one that can (a division,
//!   an ordered compare, a checked integer op, a gather, a check) stands
//!   for the error the interpreter would raise, so it stays even when its
//!   value is unused.

use super::ir::{BinOp, IBinOp, IUnOp, Ins, Kernel, Op, UnOp};

/// Remove the subroutines no call reaches, renumbering the rest.
pub(crate) fn prune_subs(k: &mut Kernel) {
    if k.subs.is_empty() {
        return;
    }
    let n = k.subs.len();
    let calls = |b: &[Ins]| {
        let mut v = Vec::new();
        super::ir::walk(b, &mut |i| {
            if let Ins::Call(id) = i {
                v.push(*id as usize);
            }
        });
        v
    };
    let mut reach = vec![false; n];
    for id in calls(&k.body) {
        reach[id] = true;
    }
    // A subroutine calls only lower-numbered ones: callers come first.
    for s in (0..n).rev() {
        if reach[s] {
            for id in calls(&k.subs[s].body) {
                reach[id] = true;
            }
        }
    }
    if reach.iter().all(|&r| r) {
        return;
    }
    let mut new_id = vec![u32::MAX; n];
    let mut next = 0u32;
    for s in 0..n {
        if reach[s] {
            new_id[s] = next;
            next += 1;
        }
    }
    let renum = |b: &mut Vec<Ins>| renumber(b, &new_id);
    renum(&mut k.body);
    let subs = std::mem::take(&mut k.subs);
    for (s, mut sub) in subs.into_iter().enumerate() {
        if reach[s] {
            renum(&mut sub.body);
            k.subs.push(sub);
        }
    }
    for o in &mut k.owner {
        if *o != 0 {
            let id = new_id[*o as usize - 1];
            // A dropped subroutine's registers occur nowhere now.
            *o = if id == u32::MAX { 0 } else { id + 1 };
        }
    }
}

fn renumber(b: &mut [Ins], new_id: &[u32]) {
    for i in b {
        if let Ins::Call(id) = i {
            *id = new_id[*id as usize];
        }
        for nb in i.blocks_mut() {
            renumber(nb, new_id);
        }
    }
}

/// Can `i` change nothing but its destination register (no lane retired)?
fn pure(i: &Ins) -> bool {
    match i {
        Ins::Mov(..) | Ins::MovI(..) | Ins::I2F(..) | Ins::ForFin(..) => true,
        Ins::Bin(op, ..) => matches!(
            op,
            BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Min
                | BinOp::Max
                | BinOp::TagAnd
                | BinOp::BAnd
                | BinOp::BOr
                | BinOp::BXor
                | BinOp::Eq
                | BinOp::Ne
        ),
        Ins::Un(op, ..) => matches!(
            op,
            UnOp::Neg
                | UnOp::Abs
                | UnOp::Floor
                | UnOp::Ceil
                | UnOp::Round
                | UnOp::Trunc
                | UnOp::Sin
                | UnOp::Cos
                | UnOp::BNot
        ),
        Ins::IBin(op, ..) => !matches!(
            op,
            IBinOp::Add | IBinOp::Sub | IBinOp::Mul | IBinOp::Idiv | IBinOp::Mod
        ),
        Ins::IUn(op, ..) => *op == IUnOp::Not,
        _ => false,
    }
}

/// Remove pure instructions whose results are never read, and `If`s left
/// with nothing in either branch, until none is left.
pub(crate) fn dce(k: &mut Kernel) {
    for _ in 0..64 {
        let mut uses = vec![0u32; k.nregs];
        let mut count = |b: &[Ins]| {
            super::ir::walk(b, &mut |i| {
                i.regs(&mut |r, w| {
                    if !w {
                        uses[r as usize] += 1;
                    }
                })
            })
        };
        count(&k.body);
        for s in &k.subs {
            count(&s.body);
        }
        for o in k.outputs.iter().chain(&k.rand) {
            if let Op::R(r) = o {
                uses[*r as usize] += 1;
            }
        }
        let mut changed = sweep(&mut k.body, &uses);
        for s in &mut k.subs {
            changed |= sweep(&mut s.body, &uses);
        }
        if !changed {
            break;
        }
    }
}

fn sweep(b: &mut Vec<Ins>, uses: &[u32]) -> bool {
    let mut changed = false;
    for i in b.iter_mut() {
        for nb in i.blocks_mut() {
            changed |= sweep(nb, uses);
        }
    }
    let before = b.len();
    b.retain(|i| match i {
        Ins::If(_, t, e) => !(t.is_empty() && e.is_empty()),
        _ if pure(i) => {
            let mut live = false;
            i.regs(&mut |r, w| live |= w && uses[r as usize] != 0);
            live
        }
        _ => true,
    });
    changed || b.len() != before
}
