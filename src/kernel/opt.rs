//! Clean-up passes over a compiled kernel, before register allocation.
//!
//! - [`prune_subs`] drops the subroutines nothing calls (the compiler makes
//!   some in loop passes and attempts it then throws away).
//! - [`dce`] removes instructions whose results nothing reads. Only
//!   instructions that cannot retire a lane go: one that can (a division,
//!   an ordered compare, a checked integer op, a gather, a check) stands
//!   for the error the interpreter would raise, so it stays even when its
//!   value is unused. [`dce_body`] does the same for one subroutine as
//!   it is made, so the size cap counts what it will keep.

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

/// Remove pure instructions whose results nothing needs, and `If`s left
/// with nothing in either branch.
///
/// A register is needed when an instruction that is not pure reads it (or
/// an `If` tests it, or it is an output), or a pure instruction writing a
/// needed register reads it: marked from those roots in one pass, so a
/// long chain of dead moves (merges of a name nothing reads, through many
/// joins) goes at once.
pub(crate) fn dce(k: &mut Kernel) {
    let roots: Vec<u32> = k
        .outputs
        .iter()
        .chain(&k.rand)
        .filter_map(|o| match o {
            Op::R(r) => Some(*r),
            _ => None,
        })
        .collect();
    let mut blocks: Vec<&mut Vec<Ins>> = vec![&mut k.body];
    blocks.extend(k.subs.iter_mut().map(|s| &mut s.body));
    dce_blocks(k.nregs, &mut blocks, &roots);
}

/// [`dce`] on one subroutine's body, made as the compiler goes (#1091):
/// what it leaves (`results`) is needed, as is what its instructions that
/// are not pure read. The final [`dce`] can remove more (a result no call
/// site reads), never less. Its registers are numbered densely first, so
/// the cost is the body's size, not the kernel's register count.
pub(crate) fn dce_body(body: &mut Vec<Ins>, results: &[u32]) {
    let mut dense: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut orig: Vec<u32> = Vec::new();
    super::ir::walk_regs_mut(body, &mut |r, _| {
        *r = *dense.entry(*r).or_insert_with(|| {
            orig.push(*r);
            orig.len() as u32 - 1
        });
    });
    // A result the body never mentions needs nothing in it.
    let roots: Vec<u32> = results
        .iter()
        .filter_map(|r| dense.get(r).copied())
        .collect();
    dce_blocks(orig.len(), &mut [body], &roots);
    super::ir::walk_regs_mut(body, &mut |r, _| *r = orig[*r as usize]);
}

fn dce_blocks(n: usize, blocks: &mut [&mut Vec<Ins>], roots: &[u32]) {
    for _ in 0..8 {
        // For each register, what its pure definitions read.
        let mut deps: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut need = vec![false; n];
        let mut work: Vec<u32> = Vec::new();
        let root = |r: u32, need: &mut Vec<bool>, work: &mut Vec<u32>| {
            if !need[r as usize] {
                need[r as usize] = true;
                work.push(r);
            }
        };
        let mut scan = |b: &[Ins], need: &mut Vec<bool>, work: &mut Vec<u32>| {
            super::ir::walk(b, &mut |i| {
                if pure(i) {
                    let (mut d, mut rs) = (Vec::new(), Vec::new());
                    i.regs(&mut |r, w| if w { d.push(r) } else { rs.push(r) });
                    for x in d {
                        deps[x as usize].extend_from_slice(&rs);
                    }
                } else {
                    i.regs(&mut |r, w| {
                        if !w {
                            root(r, need, work);
                        }
                    });
                }
            })
        };
        for b in blocks.iter() {
            scan(b, &mut need, &mut work);
        }
        for &r in roots {
            root(r, &mut need, &mut work);
        }
        while let Some(r) = work.pop() {
            for &x in &std::mem::take(&mut deps[r as usize]) {
                root(x, &mut need, &mut work);
            }
        }
        let mut changed = false;
        for b in blocks.iter_mut() {
            changed |= sweep(b, &need);
        }
        // Another round only if an emptied `If` freed its condition.
        if !changed {
            break;
        }
    }
}

/// Remove what [`dce`] found unneeded; true if an `If` went.
fn sweep(b: &mut Vec<Ins>, need: &[bool]) -> bool {
    let mut ifs = false;
    for i in b.iter_mut() {
        for nb in i.blocks_mut() {
            ifs |= sweep(nb, need);
        }
    }
    b.retain(|i| match i {
        Ins::If(_, t, e) if t.is_empty() && e.is_empty() => {
            ifs = true;
            false
        }
        _ if pure(i) => {
            let mut live = false;
            i.regs(&mut |r, w| live |= w && need[r as usize]);
            live
        }
        _ => true,
    });
    ifs
}
