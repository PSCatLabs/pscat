//! A debug-build check of storage kinds (`ir.rs`): every operand an
//! instruction reads is the kind it expects. A missing int-to-real
//! conversion would otherwise read i64 bits as an f64 and give plausible
//! garbage; this makes it a deterministic failure in the tests.

use super::ir::{Ins, Kernel, Op};

/// `kinds[r]` is true for an I64 register.
pub(crate) fn kinds(k: &Kernel, kinds: &[bool]) -> Result<(), String> {
    let ki = |o: &Op| match o {
        Op::R(r) => kinds.get(*r as usize).copied(),
        Op::Imm(_) => None,
    };
    // Reads an f64 (an immediate is either).
    let f = |o: &Op, what: &str| -> Result<(), String> {
        if ki(o) == Some(true) {
            return Err(format!("{what} reads an I64 register as an f64 ({o:?})"));
        }
        Ok(())
    };
    let i = |o: &Op, what: &str| -> Result<(), String> {
        if ki(o) == Some(false) {
            return Err(format!(
                "{what} reads an F64 register as an integer ({o:?})"
            ));
        }
        Ok(())
    };
    let dk = |r: u32, want: bool, what: &str| -> Result<(), String> {
        if kinds.get(r as usize).copied() != Some(want) {
            return Err(format!("{what} writes r{r} of the wrong kind"));
        }
        Ok(())
    };
    fn walk(b: &[Ins], g: &mut dyn FnMut(&Ins) -> Result<(), String>) -> Result<(), String> {
        for ins in b {
            g(ins)?;
            match ins {
                Ins::If(_, t, e) => {
                    walk(t, g)?;
                    walk(e, g)?;
                }
                Ins::Loop(body) => walk(body, g)?,
                _ => {}
            }
        }
        Ok(())
    }
    let mut g = |ins: &Ins| match ins {
        Ins::Mov(d, a) => {
            // A copy keeps the kind; an immediate is an f64 value.
            match a {
                Op::R(s) => dk(*d, kinds.get(*s as usize).copied().unwrap_or(false), "Mov"),
                Op::Imm(_) => dk(*d, false, "Mov of an immediate"),
            }
        }
        Ins::MovI(d, _) => dk(*d, true, "MovI"),
        Ins::Bin(_, d, a, b) => {
            f(a, "Bin")?;
            f(b, "Bin")?;
            dk(*d, false, "Bin")
        }
        Ins::Un(_, d, a) => {
            f(a, "Un")?;
            dk(*d, false, "Un")
        }
        Ins::IBin(op, d, a, b) => {
            i(a, "IBin")?;
            i(b, "IBin")?;
            dk(*d, !op.is_compare(), "IBin")
        }
        Ins::IUn(_, d, a) => {
            i(a, "IUn")?;
            dk(*d, true, "IUn")
        }
        Ins::I2F(d, a) | Ins::I2FExact(d, a) => {
            i(a, "I2F")?;
            dk(*d, false, "I2F")
        }
        Ins::F2I(d, a) => {
            f(a, "F2I")?;
            dk(*d, true, "F2I")
        }
        Ins::ForFin(d, a, b, c) => {
            f(a, "ForFin")?;
            f(b, "ForFin")?;
            f(c, "ForFin")?;
            dk(*d, false, "ForFin")
        }
        Ins::CheckInt(a, b) => {
            f(a, "CheckInt")?;
            f(b, "CheckInt")
        }
        Ins::NeedInt(a) | Ins::Assert(a, _) | Ins::RowCheck(_, a) => f(a, "a check"),
        Ins::Gather {
            d,
            dtag,
            row,
            rtag,
            col,
            ctag,
            ..
        } => {
            for o in [row, rtag, col, ctag] {
                f(o, "Gather")?;
            }
            dk(*d, false, "Gather")?;
            match dtag {
                Some(t) => dk(*t, false, "Gather"),
                None => Ok(()),
            }
        }
        Ins::LGet {
            d, dtag, idx, itag, ..
        } => {
            f(idx, "LGet")?;
            f(itag, "LGet")?;
            dk(*d, false, "LGet")?;
            dk(*dtag, false, "LGet")
        }
        Ins::LPut {
            idx, itag, v, vtag, ..
        } => {
            for o in [idx, itag, v, vtag] {
                f(o, "LPut")?;
            }
            Ok(())
        }
        Ins::Noise { kind, d, a, octtag } => {
            for o in a {
                f(o, "Noise")?;
            }
            f(octtag, "Noise")?;
            for r in d.iter().take(kind.outputs()) {
                dk(*r, false, "Noise")?;
            }
            Ok(())
        }
        Ins::If(c, ..) => {
            if kinds.get(*c as usize).copied() == Some(true) {
                return Err("If reads an I64 condition".into());
            }
            Ok(())
        }
        Ins::Loop(_) | Ins::Call(_) | Ins::LNew(_) | Ins::Break | Ins::Abort(_) => Ok(()),
    };
    walk(&k.body, &mut g)?;
    for s in &k.subs {
        walk(&s.body, &mut g)?;
    }
    for o in &k.outputs {
        f(o, "an output")?;
    }
    if let Some(o) = &k.rand {
        i(o, "the random state")?;
    }
    Ok(())
}
